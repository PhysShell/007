//! Process identity and process-group membership.
//!
//! A raw PID is not a durable identity (PID reuse), so [`ProcessIdentity`] also
//! carries the kernel start-time. This is enough for *in-supervisor* lifecycle
//! ownership (the window in which the supervisor is alive); durable, across-crash
//! identity is explicitly out of scope for PR 2 (see `docs/architecture/`).

use std::fs;

/// Identifies a process by PID, its process group, and its kernel start-time
/// (jiffies since boot) to disambiguate a reused PID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: i32,
    pub process_group: i32,
    pub start_time_ticks: u64,
}

impl ProcessIdentity {
    /// Read a live process's identity from `/proc/<pid>/stat`. Returns `None` if
    /// the process is gone or the stat line cannot be parsed.
    #[must_use]
    pub fn read(pid: i32) -> Option<Self> {
        let raw = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (process_group, start_time_ticks) = parse_stat(&raw)?;
        Some(Self {
            pid,
            process_group,
            start_time_ticks,
        })
    }

    /// Enumerate every live process whose process group equals `pgid`.
    ///
    /// This is the AUTHORITATIVE membership check, so a failure to read `/proc`
    /// (e.g. `/proc` unavailable — including any non-Linux host) is an ERROR, not
    /// an empty set: cleanup must never treat "unknown" as "gone". Individual
    /// entries that vanish mid-scan are skipped (a process may exit), which is
    /// safe — a real member cannot disappear without actually exiting.
    ///
    /// # Errors
    /// Returns the `/proc` read error if the directory cannot be enumerated.
    pub fn enumerate_group(pgid: i32) -> std::io::Result<Vec<Self>> {
        let mut members = Vec::new();
        for entry in fs::read_dir("/proc")? {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(pid) = name.parse::<i32>() else {
                continue;
            };
            if let Some(identity) = Self::read(pid) {
                if identity.process_group == pgid {
                    members.push(identity);
                }
            }
        }
        Ok(members)
    }
}

/// Parse `(process_group, start_time_ticks)` out of a `/proc/<pid>/stat` line.
///
/// The `comm` (field 2) is wrapped in parentheses and may itself contain spaces
/// and parentheses, so fields are read *after the final `)`*: there, field 3
/// (state) is index 0, so `pgrp` (field 5) is index 2 and `starttime` (field 22)
/// is index 19.
fn parse_stat(stat: &str) -> Option<(i32, u64)> {
    let close = stat.rfind(')')?;
    let tail = stat.get(close + 1..)?;
    let fields: Vec<&str> = tail.split_whitespace().collect();
    let pgrp = fields.get(2)?.parse::<i32>().ok()?;
    let starttime = fields.get(19)?.parse::<u64>().ok()?;
    Some((pgrp, starttime))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::parse_stat;

    #[test]
    fn parses_pgrp_and_starttime_with_tricky_comm() {
        // comm contains spaces and a parenthesis; fields after the last ')':
        // state ppid pgrp session ... (starttime is the 20th, index 19).
        let after = "R 100 4242 100 0 -1 0 0 0 0 0 1 2 3 4 20 0 1 0 987654 0 0";
        let line = format!("321 (weird )comm) {after}");
        let (pgrp, start) = parse_stat(&line).unwrap();
        assert_eq!(pgrp, 4242);
        assert_eq!(start, 987654);
    }
}
