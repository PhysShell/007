//! A test helper that reports the `/proc/self/fd/<n>` READLINK TARGET of every descriptor it
//! inherited ABOVE stdio (fd > 2), one per line, EXCLUDING its own `/proc/self/fd` enumeration
//! handle — identified by its EXACT descriptor (`Dir`'s `dirfd`), not by a readlink-target guess.
//! The concurrent-sibling test uses these targets to prove that a process spawned CONCURRENTLY
//! with a Sandboy launch never inherits the control-transport SOCKET (`socket:[…]`) — the object
//! type is encoded in the readlink target, so the check can be specific to the transport rather
//! than counting unrelated plumbing (e.g. the non-CLOEXEC Cargo jobserver `pipe:[…]`).
//!
//! Output is BUFFERED: the whole scan is collected through [`collect_targets`] and published to
//! stdout only after it fully succeeds. It exits 0 ON SUCCESS ONLY; if it cannot
//! enumerate/parse/readlink its own descriptor table — or cannot write/flush stdout — it exits
//! NON-ZERO and prints nothing (no partial prefix), so a scan failure can never masquerade as a
//! shorter "no leak" listing. A race-free CLOEXEC transport leaks no socket line.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs::File;
use std::io::{self, Write as _};
use std::os::fd::AsRawFd as _;

use nix::dir::Dir;
use o7_worker::fd_scrub::collect_targets;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    // Open the enumeration handle as a real fd-bearing directory object so we know the EXACT
    // descriptor our own scan occupies. `File::open` yields an O_CLOEXEC fd; `Dir::from` wraps that
    // SAME fd with `fdopendir`, and `dirfd` returns it unchanged — so `dirfd` is precisely the fd
    // the iteration reads through, never a guess.
    let dir_file = match File::open("/proc/self/fd") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("fd_probe: cannot open /proc/self/fd: {e}");
            return 2;
        }
    };
    let mut dir = match Dir::from(dir_file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("fd_probe: cannot fdopendir /proc/self/fd: {e}");
            return 2;
        }
    };
    let dirfd = dir.as_raw_fd();
    // Adapt the nix directory iteration to the file-name-bytes stream `collect_targets` consumes and
    // resolve each descriptor with a real readlink. `collect_targets` returns the COMPLETE list or
    // the first error — it publishes nothing, so a mid-scan failure cannot leak a partial prefix.
    let names = dir.iter().map(|res| {
        res.map(|entry| entry.file_name().to_bytes().to_vec())
            .map_err(io::Error::from)
    });
    let targets = match collect_targets(names, dirfd, readlink_proc_fd) {
        Ok(targets) => targets,
        Err(e) => {
            eprintln!("fd_probe: {e}");
            return 2;
        }
    };
    // Only now that the scan fully succeeded, render the whole result and publish it in ONE write.
    // A stdout write or flush failure is itself an error (exit 2), never a silent truncation.
    let mut buf = String::with_capacity(targets.iter().map(|t| t.len() + 1).sum());
    for target in &targets {
        buf.push_str(target);
        buf.push('\n');
    }
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    if let Err(e) = lock.write_all(buf.as_bytes()) {
        eprintln!("fd_probe: stdout write failed: {e}");
        return 2;
    }
    if let Err(e) = lock.flush() {
        eprintln!("fd_probe: stdout flush failed: {e}");
        return 2;
    }
    0
}

/// Resolve `/proc/self/fd/<fd>` to its readlink target string. A failure aborts the whole scan.
fn readlink_proc_fd(fd: i32) -> io::Result<String> {
    std::fs::read_link(format!("/proc/self/fd/{fd}")).map(|t| t.to_string_lossy().into_owned())
}
