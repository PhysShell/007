//! Exact-`dirfd` `/proc/self/fd` scan helpers for the test-harness binaries.
//!
//! A process scanning `/proc/self/fd` holds one descriptor open for the enumeration itself — the
//! directory handle — and that descriptor appears in the listing like any other. Both helpers here
//! identify the scan's OWN descriptor by exact identity (`Dir`'s `dirfd`, not a `readlink`-target
//! guess) so it is never mistaken for an inherited fd.
//!
//! - [`inherited_fds_to_scrub`] builds the close-set for the fake backend: every fd above stdio
//!   EXCEPT the exact `dirfd`. If the close-set swept in the enumeration handle, the `Dir`/`closedir`
//!   drop would close it once and a close loop would double-close that number (closing an unrelated
//!   fresh descriptor once it is recycled). The caller drops the `Dir` (releasing `dirfd`) BEFORE
//!   closing the returned set, so the handle is closed exactly once by its own drop.
//! - [`collect_targets`] resolves the readlink target of every inherited descriptor for `fd_probe`,
//!   returning the COMPLETE list or the FIRST error — so the caller publishes only after a fully
//!   successful scan and never leaks a partial prefix on a mid-scan failure.
//!
//! Both fail closed: enumeration/parse (and, for `collect_targets`, readlink) failures return `Err`
//! rather than silently scrubbing nothing / emitting a truncated result.

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

/// Fully collect the `readlink` targets of every inherited descriptor — `fd > 2`, EXCLUDING the
/// exact enumeration handle `dirfd` (and stdio and the `.`/`..` bookkeeping entries) — resolving
/// each descriptor with `readlink`. `names` yields each `/proc/self/fd` entry's file-name bytes.
///
/// Returns the COMPLETE target list, or the FIRST enumeration/parse/readlink error. Nothing is
/// published here: the caller writes the targets to stdout only on `Ok`, so a failure part-way
/// through the scan never leaves a partial prefix on stdout. The exact-`dirfd` exclusion matches
/// [`inherited_fds_to_scrub`] — a genuinely inherited descriptor that merely points at some
/// `/proc/<pid>/fd` is kept, not hidden.
pub fn collect_targets(
    names: impl IntoIterator<Item = io::Result<Vec<u8>>>,
    dirfd: i32,
    readlink: impl Fn(i32) -> io::Result<String>,
) -> io::Result<Vec<String>> {
    let mut targets = Vec::new();
    for name in names {
        let name = name?;
        let bytes = name.as_slice();
        // `.`/`..` are directory bookkeeping, never descriptors.
        if bytes == b"." || bytes == b".." {
            continue;
        }
        // Every other entry is a numeric descriptor; a non-numeric name is impossible under
        // `/proc/self/fd` and fails closed rather than being silently skipped.
        let Some(fd) = std::str::from_utf8(bytes)
            .ok()
            .and_then(|n| n.parse::<i32>().ok())
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "non-numeric /proc/self/fd entry {:?}",
                    String::from_utf8_lossy(bytes)
                ),
            ));
        };
        // Stdio and the scan's own enumeration handle are not inherited descriptors to report.
        if fd <= 2 || fd == dirfd {
            continue;
        }
        // A readlink failure on a live descriptor is a real error, never a silent "no leak"; it
        // aborts the whole scan so no partial prefix is ever published.
        targets.push(readlink(fd)?);
    }
    Ok(targets)
}
