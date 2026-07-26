//! TEMPORARY DIAGNOSTIC BUILD — DO NOT MERGE. Reverted after ONE hosted CI run.
//!
//! Normally this reports how many descriptors above stdio (fd > 2) it inherited. For the Vertical B
//! concurrent-sibling leak investigation it ALSO dumps, for every inherited fd > 2, the fd number,
//! its `/proc/self/fd/<n>` readlink target (which encodes the object TYPE — `socket:[…]`,
//! `pipe:[…]`, a path, `anon_inode:[…]`), and the `flags:` line from `/proc/self/fdinfo/<n>`. Line 1
//! is the LEAK COUNT (fds > 2 that are NOT this probe's own `/proc/<pid>/fd` enumeration handle);
//! the remaining lines are one diagnostic record per inherited fd > 2. It exits 0 ON SUCCESS ONLY;
//! an enumeration/parse failure exits non-zero and prints nothing to stdout, so an error can never
//! masquerade as "no leaks".
#![allow(clippy::print_stdout, clippy::print_stderr)]

fn main() {
    std::process::exit(run());
}

/// The `flags:` octal from `/proc/self/fdinfo/<fd>` (so CLOEXEC — bit `O_CLOEXEC`/`02000000` — is
/// visible), or a placeholder if unreadable.
fn fdinfo_flags(fd: i32) -> String {
    match std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}")) {
        Ok(text) => text
            .lines()
            .find(|l| l.starts_with("flags:"))
            .map(str::trim)
            .unwrap_or("flags:?")
            .to_owned(),
        Err(e) => format!("flags:<err {e}>"),
    }
}

fn run() -> i32 {
    let entries = match std::fs::read_dir("/proc/self/fd") {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("fd_probe: cannot enumerate /proc/self/fd: {e}");
            return 2;
        }
    };
    // Collect (fd, readlink-target) for every descriptor above stdio, fail-closed.
    let mut records: Vec<(i32, String)> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("fd_probe: directory entry error: {e}");
                return 2;
            }
        };
        let name = entry.file_name();
        let Some(fd) = name.to_str().and_then(|n| n.parse::<i32>().ok()) else {
            eprintln!("fd_probe: non-numeric fd entry {name:?}");
            return 2;
        };
        if fd <= 2 {
            continue;
        }
        let link = match std::fs::read_link(entry.path()) {
            Ok(target) => target.to_string_lossy().into_owned(),
            // A readlink error on a live fd is a real failure, not "no leak".
            Err(e) => {
                eprintln!("fd_probe: readlink /proc/self/fd/{fd}: {e}");
                return 2;
            }
        };
        records.push((fd, link));
    }
    // The probe's OWN `/proc/<pid>/fd` enumeration handle (readlink ends with `/fd`) is not a leak;
    // everything else above stdio IS an inherited descriptor. Identify by target, not by count.
    let leaks = records
        .iter()
        .filter(|(_, link)| !link.ends_with("/fd"))
        .count();
    println!("{leaks}");
    for (fd, link) in &records {
        let own = if link.ends_with("/fd") {
            " (own-dirfd)"
        } else {
            ""
        };
        println!("fd={fd} link={link} {}{own}", fdinfo_flags(*fd));
    }
    0
}
