//! C-2: `fd_probe` buffers its output. Production `run()` and this regression drive the SAME
//! `collect_targets` seam (no parallel test-only reimplementation) — `run()` publishes to stdout
//! only after `collect_targets` returns the COMPLETE list, so a mid-scan parse/readlink/enumeration
//! failure exits 2 with EMPTY stdout instead of leaking a partial prefix.

// Restriction-lint allowance (AGENTS.md §4). Invariant: every `unwrap()` here is on the `Ok` arm of
// a `collect_targets` call whose inputs are constructed IN THIS TEST to succeed — a `None`/`Err`
// there is an outright bug in the fixture, so panicking (a test failure) is the sound handling. No
// `expect` is used, so only `unwrap_used` is allowed; every value the test reasons about is asserted.
#![allow(clippy::unwrap_used)]

use std::io;

use o7_worker::fd_scrub::collect_targets;

/// The core C-2 contract, as a deterministic seam: descriptor `3` resolves to a target, descriptor
/// `4`'s readlink fails.
///
/// - GREEN companion (non-vacuity): with both readlinks succeeding, `3`'s target IS a real,
///   publishable line — exactly what the old eager `fd_probe` printed before it reached `4`.
/// - RED (buffered): when `4`'s readlink fails, the ENTIRE scan fails — `collect_targets` returns
///   `Err` and no targets, so `run()` has nothing to publish (empty stdout), where the old eager
///   code would already have emitted `3`'s line.
#[test]
fn a_late_readlink_failure_fails_the_whole_scan_so_nothing_is_published() {
    let names = || vec![Ok(b"3".to_vec()), Ok(b"4".to_vec())];

    let all_ok = collect_targets(names(), 99, |fd| Ok(format!("socket:[{fd}]"))).unwrap();
    assert_eq!(
        all_ok,
        vec!["socket:[3]".to_string(), "socket:[4]".to_string()],
        "with both readlinks succeeding, entry 3 is a real publishable target (the non-vacuity anchor)"
    );

    let readlink_fails_on_4 = |fd: i32| -> io::Result<String> {
        if fd == 3 {
            Ok("socket:[3]".to_string())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "readlink denied",
            ))
        }
    };
    let result = collect_targets(names(), 99, readlink_fails_on_4);
    assert!(
        result.is_err(),
        "a late readlink failure must fail the whole buffered scan (no partial prefix), got {result:?}"
    );
}

/// The additional GREEN: a fully successful scan yields every target, in order, published only
/// after the whole scan completes — the exit-0 happy path.
#[test]
fn a_fully_successful_scan_yields_every_target_in_order() {
    let targets = collect_targets(vec![Ok(b"5".to_vec()), Ok(b"6".to_vec())], 99, |fd| {
        Ok(format!("pipe:[{fd}]"))
    })
    .unwrap();
    assert_eq!(
        targets,
        vec!["pipe:[5]".to_string(), "pipe:[6]".to_string()]
    );
}

/// An enumeration error (a directory-entry read failure) also fails the whole scan — fail closed.
#[test]
fn an_enumeration_error_fails_the_whole_scan() {
    let names: Vec<io::Result<Vec<u8>>> =
        vec![Ok(b"3".to_vec()), Err(io::Error::other("getdents failed"))];
    assert!(collect_targets(names, 99, |fd| Ok(format!("x{fd}"))).is_err());
}

/// A non-numeric entry fails closed rather than being silently skipped.
#[test]
fn a_non_numeric_entry_fails_closed() {
    assert!(collect_targets(vec![Ok(b"notanfd".to_vec())], 99, |fd| Ok(format!("t{fd}"))).is_err());
}

/// The H-2 exact-`dirfd` exclusion is preserved (unchanged by C-2): stdio, the exact `dirfd`, and
/// the `.`/`..` bookkeeping entries are never reported as targets.
#[test]
fn stdio_the_exact_dirfd_and_dot_entries_are_excluded() {
    let names = vec![
        Ok(b".".to_vec()),
        Ok(b"..".to_vec()),
        Ok(b"0".to_vec()),
        Ok(b"1".to_vec()),
        Ok(b"2".to_vec()),
        Ok(b"7".to_vec()), // == dirfd passed below
    ];
    let targets = collect_targets(names, 7, |fd| Ok(format!("t{fd}"))).unwrap();
    assert!(
        targets.is_empty(),
        "stdio, the exact dirfd, and dot entries must all be excluded; got {targets:?}"
    );
}
