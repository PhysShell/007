//! VB-4 — the compile-gated deterministic fault seam. Compiled ONLY under the non-default
//! `fault-injection` feature: the frozen confinement matrix builds a dedicated real-sandboy artifact
//! WITH it so the setup-failure oracles can drive genuine stage failures against the REAL composed
//! launch state machine. A production/default/release build never compiles this file, so no
//! fault-point names or parser reach the binary, and a fault-enabled artifact carries a DISTINCT
//! backend identity (see `main.rs`).
//!
//! The seam is structurally narrow: ONE typed [`FaultId`], ONE parser at the monitor entry boundary
//! ([`monitor_faults`]), at most one injected fault per launch. The control variable is read ONCE in
//! the unconfined monitor and REMOVED before the fork, so it is never inherited by the launch child
//! or the target — the confined target can never select or modify a fault point.

/// Exactly one named injection point in the composed VB-4 launch state machine. The launch-level
/// points are tripped by the `trip!` macro in `super`; the mechanism-internal points are mapped to
/// the per-module fault knobs so the REAL install genuinely fails at that stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaultId {
    // cgroup (monitor-owned)
    CgroupCreate,
    CgroupVerify,
    Kill,
    Drain,
    // Landlock (child)
    LandlockCreate,
    LandlockAddRule,
    LandlockRestrict,
    LandlockSelfCheck,
    // fd scrub + env (child)
    FdVerify,
    EnvVerify,
    // seccomp (child)
    SeccompNoNewPrivs,
    SeccompApply,
    SeccompSelfCheck,
    // protocol (monitor/child)
    ReadyWrite,
    ReadyValidate,
    Release,
    Execveat,
}

impl FaultId {
    fn parse(s: &str) -> Option<FaultId> {
        Some(match s {
            "cgroup_create" => FaultId::CgroupCreate,
            "cgroup_verify" => FaultId::CgroupVerify,
            "kill" => FaultId::Kill,
            "drain" => FaultId::Drain,
            "landlock_create" => FaultId::LandlockCreate,
            "landlock_add_rule" => FaultId::LandlockAddRule,
            "landlock_restrict" => FaultId::LandlockRestrict,
            "landlock_self_check" => FaultId::LandlockSelfCheck,
            "fd_verify" => FaultId::FdVerify,
            "env_verify" => FaultId::EnvVerify,
            "seccomp_no_new_privs" => FaultId::SeccompNoNewPrivs,
            "seccomp_apply" => FaultId::SeccompApply,
            "seccomp_self_check" => FaultId::SeccompSelfCheck,
            "ready_write" => FaultId::ReadyWrite,
            "ready_validate" => FaultId::ReadyValidate,
            "release" => FaultId::Release,
            "execveat" => FaultId::Execveat,
            _ => return None,
        })
    }
}

/// The (at most one) selected fault, plus an "invalid selection" flag so a malformed/unknown fault
/// aborts the launch rather than running the target.
#[derive(Clone, Copy)]
pub(crate) struct Faults {
    id: Option<FaultId>,
    invalid: bool,
}

/// Parse the fault selection ONCE, in the unconfined monitor, before any fork, and REMOVE the control
/// variable so it is never inherited by the child or the target.
pub(crate) fn monitor_faults() -> Faults {
    let raw = std::env::var("O7_FAULT_POINT").ok();
    std::env::remove_var("O7_FAULT_POINT");
    match raw {
        None => Faults {
            id: None,
            invalid: false,
        },
        Some(s) => match FaultId::parse(&s) {
            Some(id) => Faults {
                id: Some(id),
                invalid: false,
            },
            None => Faults {
                id: None,
                invalid: true,
            },
        },
    }
}

impl Faults {
    pub(crate) fn tripped(&self, id: FaultId) -> bool {
        self.id == Some(id)
    }

    /// An unknown/malformed fault selection: the launch must abort (never run the target).
    pub(crate) fn is_invalid(&self) -> bool {
        self.invalid
    }

    /// Map a mechanism-internal fault to the Landlock module's fault knob, so the REAL install fails
    /// at exactly that stage.
    pub(crate) fn landlock(&self) -> crate::landlock::Faults {
        let mut f = crate::landlock::Faults::default();
        match self.id {
            Some(FaultId::LandlockCreate) => f.create = true,
            Some(FaultId::LandlockAddRule) => f.add_rule = true,
            Some(FaultId::LandlockRestrict) => f.restrict_self = true,
            Some(FaultId::LandlockSelfCheck) => f.selfcheck_outside_notdenied = true,
            _ => {}
        }
        f
    }

    pub(crate) fn seccomp(&self) -> crate::seccomp::Fault {
        match self.id {
            Some(FaultId::SeccompNoNewPrivs) => crate::seccomp::Fault::NoNewPrivs,
            Some(FaultId::SeccompApply) => crate::seccomp::Fault::Apply,
            Some(FaultId::SeccompSelfCheck) => crate::seccomp::Fault::OmitSocketNative,
            _ => crate::seccomp::Fault::None,
        }
    }

    pub(crate) fn cgroup(&self) -> crate::cgroup::Faults {
        let mut f = crate::cgroup::Faults::default();
        match self.id {
            Some(FaultId::Kill) => f.kill = true,
            Some(FaultId::Drain) => f.drain = true,
            _ => {}
        }
        f
    }
}
