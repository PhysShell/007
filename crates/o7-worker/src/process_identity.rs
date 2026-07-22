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
    /// This is the AUTHORITATIVE membership check, so it fails CLOSED: anything it
    /// cannot positively resolve is an ERROR, never an empty/short set — cleanup must
    /// never treat "unknown" as "gone". Concretely:
    ///   * a top-level `/proc` read failure, or a directory-ENTRY I/O error, propagates;
    ///   * a per-PID `stat` read that fails with `NotFound` is a confirmed exit race
    ///     and is the ONLY thing skipped (the PID vanished, so it is genuinely gone);
    ///   * any other `stat` I/O error (EACCES, EIO, …) propagates — the scanner cannot
    ///     distinguish such a PID from a live member, so it must not drop it;
    ///   * a `stat` that reads successfully but does not parse is a membership failure
    ///     (the PID exists but we cannot rule it out of the group).
    ///
    /// # Errors
    /// Returns an I/O error if `/proc` cannot be enumerated, a member's `stat` cannot
    /// be read for any reason other than a confirmed exit, or an existing PID's `stat`
    /// cannot be parsed.
    pub fn enumerate_group(pgid: i32) -> std::io::Result<Vec<Self>> {
        Self::enumerate_group_with(&RealProc, pgid)
    }

    /// The injectable core of [`enumerate_group`]. Splitting the `/proc` access behind
    /// [`ProcSource`] lets tests inject faults (EIO on a live member's `stat`, a
    /// malformed `stat`) that a real `/proc` cannot be made to produce on demand, and
    /// assert the scan fails closed instead of under-counting.
    pub(crate) fn enumerate_group_with(
        source: &dyn ProcSource,
        pgid: i32,
    ) -> std::io::Result<Vec<Self>> {
        let mut members = Vec::new();
        for pid in source.pids()? {
            let raw = match source.stat(pid) {
                Ok(raw) => raw,
                // A confirmed exit race is the ONLY safe skip: the PID is gone.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                // Any other error ("permission denied", "input/output error", …) is
                // NOT proof the PID exited. Fail closed rather than drop a live member.
                Err(err) => return Err(err),
            };
            let Some((process_group, start_time_ticks)) = parse_stat(&raw) else {
                // The PID exists (its `stat` read succeeded) but the line is
                // unparseable, so we cannot prove it is NOT in the group. Treat it as a
                // membership failure, never as "not a member".
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unparseable /proc/{pid}/stat; cannot prove group membership"),
                ));
            };
            if process_group == pgid {
                members.push(Self {
                    pid,
                    process_group,
                    start_time_ticks,
                });
            }
        }
        Ok(members)
    }
}

/// Injectable access to the process table, so the authoritative membership scan can
/// be tested against `/proc` faults it could never be made to exhibit on demand.
pub(crate) trait ProcSource {
    /// The numeric PID entries of the process table. A directory-entry I/O error is
    /// propagated (never silently skipped); non-numeric entries (`self`, `cpuinfo`, …)
    /// are not PIDs and are dropped.
    ///
    /// # Errors
    /// Any I/O error encountered listing the table or iterating its entries.
    fn pids(&self) -> std::io::Result<Vec<i32>>;

    /// The raw `/proc/<pid>/stat` contents. A `NotFound` error means the PID exited
    /// between listing and reading (a benign exit race).
    ///
    /// # Errors
    /// Any I/O error reading the entry (including `NotFound` for an exited PID).
    fn stat(&self, pid: i32) -> std::io::Result<String>;
}

/// The real, `/proc`-backed [`ProcSource`].
struct RealProc;

impl ProcSource for RealProc {
    fn pids(&self) -> std::io::Result<Vec<i32>> {
        let mut pids = Vec::new();
        for entry in fs::read_dir("/proc")? {
            // A directory-entry error is a real I/O fault, not "no more entries" —
            // propagate it instead of silently skipping (which could hide a member).
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            // Non-numeric names are the kernel's own `/proc` files, never PIDs.
            let Ok(pid) = name.parse::<i32>() else {
                continue;
            };
            pids.push(pid);
        }
        Ok(pids)
    }

    fn stat(&self, pid: i32) -> std::io::Result<String> {
        fs::read_to_string(format!("/proc/{pid}/stat"))
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
    use super::{parse_stat, ProcSource, ProcessIdentity};
    use std::collections::BTreeMap;
    use std::io;

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

    /// EIO / ENOENT / EACCES as raw errnos, so injected `stat` faults are the exact
    /// `io::Error`s a real `/proc` read would yield.
    const EIO: i32 = 5;
    const ENOENT: i32 = 2;
    const EACCES: i32 = 13;

    /// A scriptable `stat` outcome for one PID.
    enum StatScript {
        /// A well-formed `/proc/<pid>/stat` line for `(pgrp, starttime)`.
        Line(String),
        /// The read fails with this raw OS errno (e.g. EIO, ENOENT, EACCES).
        OsError(i32),
    }

    /// A scriptable process table for fault injection. Each listed PID maps to the
    /// outcome its `stat` read should produce; a listing error can also be forced.
    struct FakeProc {
        pids: Vec<i32>,
        pids_error: Option<i32>,
        stats: BTreeMap<i32, StatScript>,
    }

    impl FakeProc {
        fn new(pids: Vec<i32>) -> Self {
            Self {
                pids,
                pids_error: None,
                stats: BTreeMap::new(),
            }
        }
        fn with_stat(mut self, pid: i32, pgrp: i32, start: u64) -> Self {
            let line = format!(
                "{pid} (proc) S 1 {pgrp} {pid} 0 -1 0 0 0 0 0 1 2 3 4 20 0 1 0 {start} 0 0"
            );
            self.stats.insert(pid, StatScript::Line(line));
            self
        }
        fn with_stat_errno(mut self, pid: i32, errno: i32) -> Self {
            self.stats.insert(pid, StatScript::OsError(errno));
            self
        }
        fn with_raw_stat(mut self, pid: i32, raw: &str) -> Self {
            self.stats.insert(pid, StatScript::Line(raw.to_owned()));
            self
        }
    }

    impl ProcSource for FakeProc {
        fn pids(&self) -> io::Result<Vec<i32>> {
            match self.pids_error {
                Some(errno) => Err(io::Error::from_raw_os_error(errno)),
                None => Ok(self.pids.clone()),
            }
        }
        fn stat(&self, pid: i32) -> io::Result<String> {
            match self.stats.get(&pid) {
                Some(StatScript::Line(line)) => Ok(line.clone()),
                Some(StatScript::OsError(errno)) => Err(io::Error::from_raw_os_error(*errno)),
                // Unlisted PID == the entry disappeared before we read it (exit race).
                None => Err(io::Error::from_raw_os_error(ENOENT)),
            }
        }
    }

    #[test]
    fn live_member_stat_eio_is_a_membership_error_not_empty() {
        // pgid 4242 has one provable member (pid 100) and one member (pid 200) whose
        // `stat` returns EIO — the scanner cannot prove 200's group, so it must NOT
        // report a clean/short set. Cleanup would therefore fail closed (never empty).
        let source = FakeProc::new(vec![100, 200])
            .with_stat(100, 4242, 111)
            .with_stat_errno(200, EIO);
        let result = ProcessIdentity::enumerate_group_with(&source, 4242);
        assert!(
            result.is_err(),
            "a live member whose stat returns EIO must fail the scan, got {result:?}"
        );
    }

    #[test]
    fn permission_denied_stat_also_fails_closed() {
        // EACCES is not a confirmed exit either — it must propagate, not be skipped.
        let source = FakeProc::new(vec![200]).with_stat_errno(200, EACCES);
        assert!(ProcessIdentity::enumerate_group_with(&source, 4242).is_err());
    }

    #[test]
    fn confirmed_exit_race_is_skipped_but_other_members_are_kept() {
        // pid 300 vanished (ENOENT) — a benign exit race, skipped. pid 100 remains a
        // member; pid 400 is a different group. The scan still succeeds and reports 100.
        let source = FakeProc::new(vec![100, 300, 400])
            .with_stat(100, 4242, 111)
            .with_stat_errno(300, ENOENT)
            .with_stat(400, 9999, 222);
        let members = ProcessIdentity::enumerate_group_with(&source, 4242).unwrap();
        assert_eq!(members.len(), 1);
        let member = members.first().unwrap();
        assert_eq!(member.pid, 100);
        assert_eq!(member.process_group, 4242);
    }

    #[test]
    fn unparseable_stat_for_existing_pid_is_a_membership_error() {
        // pid 100 exists (stat read OK) but the line is garbage — we cannot prove it is
        // outside the group, so the scan fails closed rather than dropping it.
        let source = FakeProc::new(vec![100]).with_raw_stat(100, "totally not a stat line");
        let result = ProcessIdentity::enumerate_group_with(&source, 4242);
        assert!(
            result.is_err(),
            "unparseable existing stat must error, got {result:?}"
        );
    }

    #[test]
    fn directory_listing_error_propagates() {
        let mut source = FakeProc::new(Vec::new());
        source.pids_error = Some(EACCES);
        assert!(ProcessIdentity::enumerate_group_with(&source, 4242).is_err());
    }
}
