//! A test helper that reports how many descriptors ABOVE stdio (fd > 2) it inherited at startup,
//! so a test can prove that an unrelated process spawned CONCURRENTLY with a Sandboy launch never
//! inherits a control-transport descriptor. It prints one integer (the inherited count, with its
//! own directory handle discounted) and exits 0. A race-free CLOEXEC transport yields `0`; a
//! transport with an inheritable window would occasionally yield a positive count.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

fn main() {
    // Count fds > 2 open at startup. Reading /proc/self/fd itself holds exactly one directory
    // descriptor (> 2) during enumeration, so discount it.
    let mut above_stdio = 0usize;
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            if let Some(fd) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i32>().ok())
            {
                if fd > 2 {
                    above_stdio += 1;
                }
            }
        }
    }
    println!("{}", above_stdio.saturating_sub(1));
}
