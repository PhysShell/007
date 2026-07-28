//! H-2: the `/proc/self/fd` scrub must know its OWN enumeration descriptor by EXACT identity.
//!
//! RED (the pre-H-2 hazard): a naive "every fd above stdio" collect INCLUDES the enumeration
//! handle's own `dirfd` — the very descriptor the `Dir`/`closedir` drop then closes — so a close
//! loop that also closes that number double-closes it (and closes an unrelated fresh descriptor
//! once the number is recycled). GREEN: [`inherited_fds_to_scrub`] excludes the EXACT `dirfd` from
//! the close-set while every ordinary inherited descriptor survives.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::File;
use std::os::fd::AsRawFd as _;

use nix::dir::Dir;

use o7_worker::fd_scrub::inherited_fds_to_scrub;

/// A dirfd-UNAWARE collect: every fd above stdio, `dirfd` included. This is the pre-H-2 behavior the
/// fix removes. Returns `(dirfd, all_fds_above_stdio)`.
fn naive_collect_above_stdio() -> (i32, Vec<i32>) {
    let dir_file = File::open("/proc/self/fd").unwrap();
    let mut dir = Dir::from(dir_file).unwrap();
    let dirfd = dir.as_raw_fd();
    let mut all = Vec::new();
    for entry in dir.iter() {
        let entry = entry.unwrap();
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        if let Some(fd) = entry
            .file_name()
            .to_str()
            .ok()
            .and_then(|n| n.parse::<i32>().ok())
        {
            if fd > 2 {
                all.push(fd);
            }
        }
    }
    (dirfd, all)
}

#[test]
fn scrub_set_excludes_the_exact_enumeration_dirfd_while_neighbors_survive() {
    // Two ordinary inherited-style descriptors that must NOT be hidden by the exclusion. Held open
    // across both collects below so their numbers stay live.
    let neighbor_a = File::open("/proc/self/status").unwrap();
    let neighbor_b = File::open("/proc/self/status").unwrap();
    let fd_a = neighbor_a.as_raw_fd();
    let fd_b = neighbor_b.as_raw_fd();

    // RED: a dirfd-unaware collect includes its OWN enumeration handle — non-vacuous proof of the
    // double-close hazard (that descriptor is the one `Dir`/`closedir` closes on drop).
    let (naive_dirfd, naive_all) = naive_collect_above_stdio();
    assert!(naive_dirfd > 2, "the enumeration handle sits above stdio");
    assert!(
        naive_all.contains(&naive_dirfd),
        "a dirfd-unaware collect must include its own enumeration handle {naive_dirfd} (the \
         double-close hazard); collected {naive_all:?}"
    );

    // GREEN: the real helper excludes the EXACT dirfd, by identity, and keeps the neighbors.
    let dir_file = File::open("/proc/self/fd").unwrap();
    let mut dir = Dir::from(dir_file).unwrap();
    let dirfd = dir.as_raw_fd();
    let scrub = inherited_fds_to_scrub(&mut dir).unwrap();

    assert!(
        !scrub.contains(&dirfd),
        "the scrub set must exclude the enumeration handle's exact descriptor {dirfd}; got {scrub:?}"
    );
    assert!(
        scrub.contains(&fd_a) && scrub.contains(&fd_b),
        "ordinary inherited descriptors ({fd_a}, {fd_b}) must not be hidden by the exclusion; got \
         {scrub:?}"
    );

    // Mirror the production ordering: the enumeration handle is released before anyone closes the
    // set, so `dirfd` is closed exactly once (by this drop) and the loop below would never touch it.
    drop(dir);
    drop(neighbor_a);
    drop(neighbor_b);
}
