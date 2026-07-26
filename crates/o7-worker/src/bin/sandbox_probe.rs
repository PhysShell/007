//! TEST-ONLY confined-TARGET probe for the Vertical B confinement matrix. It is sealed and
//! executed AS the confined target; it attempts a specific escape and records the concrete
//! OUTCOME (success, or the exact errno) to a marker file inside the writable worktree, so an
//! acceptance test reads a real oracle instead of a bare `is_err()`. `unsafe` stays forbidden —
//! only `std::fs` / `std::net` / `std::env`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fs") if args.len() >= 4 => fs_probe(&args[1], &args[2], &args[3]),
        Some("net") if args.len() >= 2 => net_probe(&args[1]),
        Some("env") if args.len() >= 2 => env_probe(&args[1]),
        _ => {
            eprintln!("usage: sandbox_probe <fs WT OUTSIDE MARKER | net MARKER | env MARKER>");
            64
        }
    }
}

/// `Ok(())` → `OK`; `Err` → `ERR:<errno>` so the test can assert the EXACT kernel errno.
fn outcome(result: &std::io::Result<()>) -> String {
    match result {
        Ok(()) => "OK".to_owned(),
        Err(e) => format!("ERR:{}", e.raw_os_error().unwrap_or(-1)),
    }
}

/// Filesystem / Landlock: a write INSIDE the writable worktree should succeed; a write OUTSIDE it
/// should be denied (`EACCES`/`EPERM`). Records both outcomes.
fn fs_probe(worktree: &str, outside: &str, marker: &str) -> i32 {
    let inside = std::path::Path::new(worktree).join("inside.txt");
    let inside_res = std::fs::write(&inside, b"inside");
    let outside_res = std::fs::write(outside, b"outside");
    let report = format!(
        "inside={}\noutside={}\n",
        outcome(&inside_res),
        outcome(&outside_res)
    );
    let _ = std::fs::write(marker, report);
    0
}

/// Network / seccomp: creating an IPv4/IPv6 socket should be denied. Records each outcome.
fn net_probe(marker: &str) -> i32 {
    let udp4 = std::net::UdpSocket::bind("127.0.0.1:0").map(|_| ());
    let tcp4 = std::net::TcpListener::bind("127.0.0.1:0").map(|_| ());
    let udp6 = std::net::UdpSocket::bind("[::1]:0").map(|_| ());
    let report = format!(
        "udp4={}\ntcp4={}\nudp6={}\n",
        outcome(&udp4),
        outcome(&tcp4),
        outcome(&udp6)
    );
    let _ = std::fs::write(marker, report);
    0
}

/// Environment: record the EXACT set of variable NAMES the confined target received (sorted).
fn env_probe(marker: &str) -> i32 {
    let mut names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
    names.sort();
    let _ = std::fs::write(marker, names.join(","));
    0
}
