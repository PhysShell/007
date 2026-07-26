//! A test helper that reports the `/proc/self/fd/<n>` READLINK TARGET of every descriptor it
//! inherited ABOVE stdio (fd > 2), one per line, EXCLUDING its own `/proc/<pid>/fd` enumeration
//! handle (whose target ends in `/fd`). The concurrent-sibling test uses these targets to prove
//! that a process spawned CONCURRENTLY with a Sandboy launch never inherits the control-transport
//! SOCKET (`socket:[…]`) — the object type is encoded in the readlink target, so the check can be
//! specific to the transport rather than counting unrelated plumbing (e.g. the non-CLOEXEC Cargo
//! jobserver `pipe:[…]`). It exits 0 ON SUCCESS ONLY; if it cannot enumerate/parse/readlink its own
//! descriptor table it exits NON-ZERO and prints nothing — so an enumeration failure can never
//! masquerade as "no leak". A race-free CLOEXEC transport leaks no socket line.
#![allow(clippy::print_stdout, clippy::print_stderr)]

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let entries = match std::fs::read_dir("/proc/self/fd") {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("fd_probe: cannot enumerate /proc/self/fd: {e}");
            return 2;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("fd_probe: directory entry error: {e}");
                return 2;
            }
        };
        // Every /proc/self/fd entry is a numeric descriptor; a non-numeric name is unexpected and
        // fails closed rather than being silently skipped.
        let name = entry.file_name();
        let Some(fd) = name.to_str().and_then(|n| n.parse::<i32>().ok()) else {
            eprintln!("fd_probe: non-numeric fd entry {name:?}");
            return 2;
        };
        if fd <= 2 {
            continue;
        }
        // A readlink failure on a live descriptor is a real error, never a silent "no leak".
        let target = match std::fs::read_link(entry.path()) {
            Ok(target) => target.to_string_lossy().into_owned(),
            Err(e) => {
                eprintln!("fd_probe: readlink /proc/self/fd/{fd}: {e}");
                return 2;
            }
        };
        // The probe's OWN `/proc/<pid>/fd` enumeration handle (target ends in `/fd`) is not
        // inherited; everything else above stdio IS. Identify by target, not by count.
        if target.ends_with("/fd") {
            continue;
        }
        println!("{target}");
    }
    0
}
