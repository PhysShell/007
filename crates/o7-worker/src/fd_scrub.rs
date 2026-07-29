//! Exact-descriptor scrubbing for the test-harness fake backend.
//!
//! A process that must close every INHERITED descriptor before starting a child enumerates
//! `/proc/self/fd`. The enumeration itself holds one descriptor open — the directory handle — and
//! that descriptor appears in the listing like any other. If the close-set is built as "every fd
//! above stdio" the enumeration handle is swept in with the rest; the `Dir`/`closedir` drop then
//! closes it, and a close loop that also closes that number double-closes it — harmless only until
//! the number is recycled, at which point the loop closes an unrelated fresh descriptor.
//!
//! [`inherited_fds_to_scrub`] removes that hazard by knowing the scan's OWN descriptor by exact
//! identity — `Dir`'s `dirfd`, not a `readlink`-target guess — and excluding precisely it. The
//! caller drops the `Dir` (releasing `dirfd`) BEFORE closing the returned set, so the enumeration
//! handle is closed exactly once, by its own drop, while every genuinely inherited descriptor is
//! still scrubbed. Enumeration/parse failures return `Err` so the caller can FAIL CLOSED rather
//! than start the child with an un-scrubbed table.

use std::io;
use std::os::fd::AsRawFd as _;

use nix::dir::Dir;

/// The inherited descriptors to close: every fd above stdio (`fd > 2`) EXCEPT the enumeration
/// handle `dir` itself, identified by its exact `dirfd`. The `.`/`..` bookkeeping entries are
/// skipped; any other non-numeric entry — impossible under `/proc/self/fd` — fails closed via an
/// `InvalidData` error. The caller MUST drop `dir` before closing the returned descriptors, so the
/// scan handle is released exactly once (by its own drop) and never double-closed.
pub fn inherited_fds_to_scrub(dir: &mut Dir) -> io::Result<Vec<i32>> {
    let dirfd = dir.as_raw_fd();
    let mut scrub = Vec::new();
    for entry in dir.iter() {
        let entry = entry.map_err(io::Error::from)?;
        let name = entry.file_name();
        let bytes = name.to_bytes();
        // `.`/`..` are directory bookkeeping, never descriptors. (std `read_dir` hides these;
        // `fdopendir` iteration does not.)
        if bytes == b"." || bytes == b".." {
            continue;
        }
        match name.to_str().ok().and_then(|n| n.parse::<i32>().ok()) {
            // The scan's own directory handle is not inherited by the child; exclude it by EXACT
            // identity so it is never double-closed. Everything else above stdio is scrubbed —
            // including a genuinely inherited descriptor that happens to point at some
            // `/proc/<pid>/fd`, which a target-string guess would wrongly hide.
            Some(fd) if fd > 2 && fd != dirfd => scrub.push(fd),
            Some(_) => {}
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("non-numeric /proc/self/fd entry {name:?}"),
                ))
            }
        }
    }
    Ok(scrub)
}
