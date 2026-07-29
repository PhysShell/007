//! A test helper that reports the `/proc/self/fd/<n>` READLINK TARGET of every descriptor it
//! inherited ABOVE stdio (fd > 2), one per line, EXCLUDING its own `/proc/self/fd` enumeration
//! handle — identified by its EXACT descriptor (`Dir`'s `dirfd`), not by a readlink-target guess.
//! The concurrent-sibling test uses these targets to prove that a process spawned CONCURRENTLY
//! with a Sandboy launch never inherits the control-transport SOCKET (`socket:[…]`) — the object
//! type is encoded in the readlink target, so the check can be specific to the transport rather
//! than counting unrelated plumbing (e.g. the non-CLOEXEC Cargo jobserver `pipe:[…]`). It exits 0
//! ON SUCCESS ONLY; if it cannot enumerate/parse/readlink its own descriptor table it exits
//! NON-ZERO and prints nothing — so an enumeration failure can never masquerade as "no leak". A
//! race-free CLOEXEC transport leaks no socket line.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs::File;
use std::os::fd::AsRawFd as _;

use nix::dir::Dir;

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
    for entry in dir.iter() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("fd_probe: directory entry error: {e}");
                return 2;
            }
        };
        let name = entry.file_name();
        let bytes = name.to_bytes();
        // `.`/`..` are directory bookkeeping, never descriptors; skip them. (std `read_dir` hides
        // these, `fdopendir` iteration does not.)
        if bytes == b"." || bytes == b".." {
            continue;
        }
        // Every other /proc/self/fd entry is a numeric descriptor; a non-numeric name is impossible
        // and fails closed rather than being silently skipped.
        let Some(fd) = name.to_str().ok().and_then(|n| n.parse::<i32>().ok()) else {
            eprintln!("fd_probe: non-numeric fd entry {name:?}");
            return 2;
        };
        if fd <= 2 {
            continue;
        }
        // The probe's OWN enumeration handle is not inherited by anyone; exclude it by EXACT
        // descriptor identity, never by a readlink-target guess (a genuinely inherited descriptor
        // that also points at some `/proc/<pid>/fd` must NOT be hidden).
        if fd == dirfd {
            continue;
        }
        // A readlink failure on a live descriptor is a real error, never a silent "no leak".
        let link = format!("/proc/self/fd/{fd}");
        let target = match std::fs::read_link(&link) {
            Ok(target) => target.to_string_lossy().into_owned(),
            Err(e) => {
                eprintln!("fd_probe: readlink {link}: {e}");
                return 2;
            }
        };
        println!("{target}");
    }
    0
}
