//! A test helper that reports how many descriptors ABOVE stdio (fd > 2) it inherited at startup,
//! so a test can prove that a process spawned CONCURRENTLY with a Sandboy launch never inherits a
//! control-transport descriptor. It prints one non-negative integer (the inherited count, with its
//! own directory handle discounted) and exits 0 ON SUCCESS ONLY. If it cannot enumerate or parse
//! its own descriptor table it exits NON-ZERO and prints nothing — so an enumeration failure can
//! never masquerade as "no leaks". A race-free CLOEXEC transport yields `0`; an inheritable-window
//! transport would occasionally yield a positive count.
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
    let mut above_stdio: usize = 0;
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
        if fd > 2 {
            above_stdio += 1;
        }
    }
    // Reading /proc/self/fd itself holds exactly one directory descriptor (> 2) during enumeration,
    // so discount it.
    println!("{}", above_stdio.saturating_sub(1));
    0
}
