//! TEST-ONLY confined-TARGET probe for the Vertical B confinement matrix. It is sealed and
//! executed AS the confined target; it attempts a specific escape and records the concrete
//! OUTCOME (success, or the exact errno) to a marker file inside the writable worktree, so an
//! acceptance test reads a real oracle instead of a bare `is_err()`. `unsafe` stays forbidden —
//! `std::fs` / `std::net` / `std::env` plus `nix`'s SAFE syscall wrappers (`setsid`, `setpgid`,
//! `execv`) — the same discipline the rest of the tree uses.
//!
//! Escape subcommands that ENUMERATE kernel state (`fds`) fail CLOSED: any enumeration/parse/
//! readlink error exits non-zero and writes NO success marker, so a probe error can never
//! masquerade as "no leak". `setsid` and `setpgid` are SEPARATE subcommands, each starting from the
//! target's initial process state, so neither result depends on the other having run first.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

use std::ffi::CString;
use std::io::{self, Write as _};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fs") if args.len() >= 5 => fs_probe(&args[1], &args[2], &args[3], &args[4]),
        Some("net") if args.len() >= 2 => net_probe(&args[1]),
        Some("env") if args.len() >= 2 => env_probe(&args[1]),
        Some("setsid") if args.len() >= 2 => setsid_probe(&args[1]),
        Some("setpgid") if args.len() >= 2 => setpgid_probe(&args[1]),
        Some("fds") if args.len() >= 2 => fds_probe(&args[1]),
        // `ran MARKER`: write MARKER as the VERY FIRST action, so "the target started" is proven
        // even if it is killed immediately after — never conflated with "reached a later step".
        Some("ran") if args.len() >= 2 => ran_probe(&args[1]),
        // `exec TARGET SECONDARY PRIMARY`: attempt to execve an executable that is NOT under
        // `allow_exec`. On denial the call returns and we record the errno to PRIMARY; on escape the
        // exec succeeds, replaces us, and the executed command creates SECONDARY (which must never
        // appear under real Landlock).
        Some("exec") if args.len() >= 4 => exec_probe(&args[1], &args[2], &args[3]),
        _ => {
            eprintln!(
                "usage: sandbox_probe <fs WT CREATE OVERWRITE TRUNCATE | net MARKER | env MARKER | \
                 setsid MARKER | setpgid MARKER | fds MARKER | ran MARKER | \
                 exec TARGET SECONDARY PRIMARY>"
            );
            64
        }
    }
}

/// `Ok(())` → `OK`; `Err` → `ERR:<errno>` so the test can assert the EXACT kernel errno.
fn outcome(result: &io::Result<()>) -> String {
    match result {
        Ok(()) => "OK".to_owned(),
        Err(e) => format!("ERR:{}", e.raw_os_error().unwrap_or(-1)),
    }
}

/// As [`outcome`], for a `nix` result whose error is a raw `Errno` (its `i32` value is the errno).
fn nix_outcome<T>(result: nix::Result<T>) -> String {
    match result {
        Ok(_) => "OK".to_owned(),
        Err(e) => format!("ERR:{}", e as i32),
    }
}

/// Write a probe's result marker, or exit ERROR (2). A marker that could NOT be written is an
/// infrastructure ERROR, never an implicit "the escape was denied": an acceptance test reading a
/// missing/partial marker must not mistake a failed write (worktree not writable, `ENOSPC`, or the
/// very confinement rules under test) for a confinement success. Mirrors the `fds`/`ran` discipline.
/// Returns the process exit code to propagate (0 on success, 2 on write failure).
fn write_marker(subcmd: &str, path: impl AsRef<std::path::Path>, body: impl AsRef<[u8]>) -> i32 {
    let path = path.as_ref();
    match std::fs::write(path, body) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!(
                "sandbox_probe {subcmd}: cannot write marker {}: {e}",
                path.display()
            );
            2
        }
    }
}

/// Filesystem / Landlock: a write INSIDE the writable worktree should succeed; every write OUTSIDE
/// it should be denied (`EACCES`/`EPERM`). Records THREE distinct outside operations, because
/// Landlock governs them with SEPARATE access rights and a partial ruleset could pass one while
/// leaking another:
/// - `create`: making a new regular file (`MAKE_REG`);
/// - `overwrite`: a NON-truncating write to a PRE-EXISTING file (`WRITE_FILE`);
/// - `truncate`: opening a PRE-EXISTING file `O_TRUNC` (`TRUNCATE`, ABI 3).
///
/// The test additionally checks the pre-existing files' bytes/size are UNCHANGED, so a wrong errno
/// cannot mask actual corruption.
fn fs_probe(worktree: &str, create: &str, overwrite: &str, truncate: &str) -> i32 {
    let marker = std::path::Path::new(worktree).join("fs.result");
    let inside = std::path::Path::new(worktree).join("inside.txt");
    let inside_res = std::fs::write(&inside, b"inside");
    // MAKE_REG: create a brand-new file outside the worktree.
    let create_res = std::fs::write(create, b"created");
    // WRITE_FILE: open an EXISTING outside file for writing (no truncation) and overwrite bytes.
    let overwrite_res = std::fs::OpenOptions::new()
        .write(true)
        .open(overwrite)
        .and_then(|mut f| f.write_all(b"HACKED"));
    // TRUNCATE: open an EXISTING outside file with O_TRUNC (empties it) — an ABI-3 right distinct
    // from WRITE_FILE.
    let truncate_res = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(truncate)
        .map(|_| ());
    let report = format!(
        "inside={}\ncreate={}\noverwrite={}\ntruncate={}\n",
        outcome(&inside_res),
        outcome(&create_res),
        outcome(&overwrite_res),
        outcome(&truncate_res),
    );
    write_marker("fs", &marker, report)
}

/// Network / seccomp: creating an IPv4/IPv6 socket should be denied. Records each outcome with its
/// exact errno.
fn net_probe(marker: &str) -> i32 {
    let udp4 = std::net::UdpSocket::bind("127.0.0.1:0").map(|_| ());
    let tcp4 = std::net::TcpListener::bind("127.0.0.1:0").map(|_| ());
    let udp6 = std::net::UdpSocket::bind("[::1]:0").map(|_| ());
    let report = format!(
        "udp4={}\ntcp4={}\nudp6={}\n",
        outcome(&udp4),
        outcome(&tcp4),
        outcome(&udp6),
    );
    write_marker("net", marker, report)
}

/// FAIL-CLOSED count of INHERITED socket descriptors (`/proc/self/fd/<n>` → `socket:[...]`),
/// excluding std streams. Any enumeration/parse/readlink error is a hard error, never a silent `0`
/// — an error can never masquerade as "no inherited sockets".
fn count_inherited_sockets() -> io::Result<usize> {
    let mut n = 0;
    for entry in std::fs::read_dir("/proc/self/fd")? {
        let entry = entry?;
        let name = entry.file_name();
        let fd = name
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-numeric fd entry"))?;
        if fd <= 2 {
            continue;
        }
        // The readdir handle itself resolves to the directory (not `socket:`), so it is naturally
        // excluded from the socket count; any readlink error IS fatal.
        let target = std::fs::read_link(entry.path())?;
        if target.to_string_lossy().starts_with("socket:") {
            n += 1;
        }
    }
    Ok(n)
}

/// Inherited-descriptor oracle: report the socket-descriptor count, fail-closed.
fn fds_probe(marker: &str) -> i32 {
    match count_inherited_sockets() {
        Ok(n) => match std::fs::write(marker, format!("inherited_sockets={n}\n")) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("sandbox_probe fds: cannot write marker: {e}");
                2
            }
        },
        Err(e) => {
            eprintln!("sandbox_probe fds: enumeration failed: {e}");
            2
        }
    }
}

/// Liveness witness: write MARKER as the first thing this process does. Used by the setup-failure
/// matrix where the ONLY question is "did the target run at all"; must not be conflated with a
/// probe that writes its marker only after several successful steps.
fn ran_probe(marker: &str) -> i32 {
    match std::fs::write(marker, b"ran") {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("sandbox_probe ran: cannot write marker: {e}");
            2
        }
    }
}

/// Environment: record the EXACT set of variable NAMES the confined target received (sorted).
fn env_probe(marker: &str) -> i32 {
    let mut names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
    names.sort();
    write_marker("env", marker, names.join(","))
}

/// Process tree / seccomp: a confined target must NOT leave the owned set by starting a new session
/// (`setsid`). INDEPENDENT of `setpgid` — this process has done nothing else first.
fn setsid_probe(marker: &str) -> i32 {
    let report = format!("setsid={}\n", nix_outcome(nix::unistd::setsid()));
    write_marker("setsid", marker, report)
}

/// Process tree / seccomp: a confined target must NOT leave the owned set by starting a new process
/// group (`setpgid`). INDEPENDENT of `setsid` — this process is not a session leader from a prior
/// `setsid`, so a stand-in (no seccomp) returns `OK`, and only a real filter yields `EPERM`.
fn setpgid_probe(marker: &str) -> i32 {
    let report = format!(
        "setpgid={}\n",
        nix_outcome(nix::unistd::setpgid(
            nix::unistd::Pid::from_raw(0),
            nix::unistd::Pid::from_raw(0),
        ))
    );
    write_marker("setpgid", marker, report)
}

/// Filesystem / Landlock EXECUTE: attempt to `execve` a `target` that is NOT under `allow_exec`.
/// Under real Landlock the exec is denied and returns `EACCES`/`EPERM`, which we record to
/// `primary`; `secondary` is never created. On escape the exec succeeds, our image is replaced, and
/// the executed command creates `secondary` — the observable proof the kernel did NOT refuse it.
fn exec_probe(target: &str, secondary: &str, primary: &str) -> i32 {
    let Ok(path) = CString::new(target) else {
        return 64;
    };
    // `env /bin/dash -c 'touch <secondary>'`: if the exec is allowed, this creates `secondary`.
    let argv: Vec<CString> = ["env", "/bin/dash", "-c", &format!("touch {secondary}")]
        .into_iter()
        .filter_map(|s| CString::new(s).ok())
        .collect();
    // `execv` only returns on FAILURE; on success our process image is replaced.
    match nix::unistd::execv(&path, &argv) {
        Ok(_) => 0,
        Err(e) => write_marker("exec", primary, format!("exec=ERR:{}\n", e as i32)),
    }
}
