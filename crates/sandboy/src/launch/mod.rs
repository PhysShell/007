//! VB-4 — the PRODUCTION confined-launch state machine: one unconfined monitor + one launch child
//! that becomes the target via `execveat`, with a two-party proof and a pause-for-GO barrier.
//!
//! The unconfined **monitor** solely owns the fd-0 control/report socket, the cgroup leaf + teardown
//! authority, the absolute deadline, child supervision, and report publication. It forks EXACTLY ONE
//! child; that SAME child later `execveat`s the sealed target — no prober, no second install. Proof
//! split: the monitor proves the child is placed in its cgroup leaf, the deadline is armed, and
//! teardown authority is available; the child proves monitor-only fds are absent, the env is built
//! byte-exactly from the launch spec, and Landlock / fd-scrub / seccomp are installed and
//! effect-checked. The child writes ONE `READY_TO_EXEC` record bound to the launch nonce and the
//! policy / launch-spec / target digests, its pid and cgroup identity, and its dimension proofs; the
//! monitor folds it with its own proof into a [`VerifiedLaunchPlan`], the ONLY constructor of a
//! report. Only after a valid GO does the monitor release the child, which closes the proof/release
//! fds and `execveat`s. Once the cgroup exists, teardown failure is authoritative and dominates any
//! earlier setup error.

use std::ffi::CString;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::net::Shutdown;
use std::os::fd::{AsRawFd as _, RawFd};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use o7_sandbox_protocol::frame::encode;
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{Enforcement, SandboxPolicy};
use o7_sandbox_protocol::report::SandboxReport;
use o7_sandbox_protocol::{LaunchRequest, SCHEMA_VERSION};

use crate::seccomp::{self, ForkResult};

// ---- compile-gated fault seam (Stage 3 fills `fault`; here it is a zero-cost no-op) --------------

#[cfg(feature = "fault-injection")]
pub(crate) mod fault;
#[cfg(feature = "fault-injection")]
pub(crate) use fault::{monitor_faults, Faults};

/// In a production/default build the fault seam carries nothing and every mapping is a no-op, so no
/// fault-point names or parser reach the binary.
#[cfg(not(feature = "fault-injection"))]
#[derive(Clone, Copy, Default)]
pub(crate) struct Faults;

/// Parse the fault selection once, in the unconfined monitor, before any fork. In a production build
/// this is a no-op that never reads `O7_FAULT_POINT`; the `fault-injection` build overrides it.
#[cfg(not(feature = "fault-injection"))]
pub(crate) fn monitor_faults() -> Faults {
    Faults
}

#[cfg(not(feature = "fault-injection"))]
impl Faults {
    pub(crate) fn landlock(&self) -> crate::landlock::Faults {
        crate::landlock::Faults::default()
    }
    pub(crate) fn seccomp(&self) -> crate::seccomp::Fault {
        crate::seccomp::Fault::default()
    }
    pub(crate) fn cgroup(&self) -> crate::cgroup::Faults {
        crate::cgroup::Faults::default()
    }
    pub(crate) fn staging(&self) -> crate::staging::Faults {
        crate::staging::Faults::default()
    }
    pub(crate) fn runtime(&self) -> crate::runtime::Faults {
        crate::runtime::Faults::default()
    }
}

/// Fault trip point. In production this expands to NOTHING (no fault-point name in the binary); under
/// `fault-injection` it runs `$on_fault` when that point is the selected one.
macro_rules! trip {
    ($faults:expr, $id:ident, $on_fault:expr) => {{
        #[cfg(feature = "fault-injection")]
        {
            if $faults.tripped($crate::launch::fault::FaultId::$id) {
                $on_fault
            }
        }
    }};
}

// ---- inputs + proof objects ---------------------------------------------------------------------

/// The report-binding identity the monitor received.
pub(crate) struct Bindings {
    pub(crate) identity: BackendIdentity,
    pub(crate) backend_digest: Digest256,
    pub(crate) policy_digest: Digest256,
    pub(crate) launch_nonce: LaunchNonce,
    pub(crate) launch_spec_digest: Digest256,
    pub(crate) target_digest: Digest256,
}

/// What the launch CHILD proves and returns over the one-shot proof pipe.
struct ChildProof {
    ok: bool,
    stage: String,
    child_pid: i32,
    cgroup_path: String,
    nonce_hex: String,
    policy_digest: String,
    launch_spec_digest: String,
    target_digest: String,
    /// Digest of the PRIVATE EXECUTION INODE the child actually `execveat`s — proven equal to the
    /// sealed source and bound here so the monitor confirms it matches the launch-spec target digest.
    exec_digest: String,
    /// The execution inode's IDENTITY `dev:ino` — names the exact object that ran.
    exec_inode: String,
    /// The versioned runtime profile the child resolved + applied (audit binding).
    runtime_profile: String,
    /// A digest over the FULLY-RESOLVED runtime read policy (interpreter + library-root +
    /// support-file paths AND opened-object identities) — commits the proof to the exact objects
    /// granted, not merely the profile name.
    runtime_binding: String,
    fs: bool,
    net: bool,
    env: bool,
    fd: bool,
    placement: bool,
}

/// What the MONITOR proves about the cgroup leaf and the armed deadline.
struct MonitorProof {
    leaf_path: String,
    child_in_leaf: bool,
    deadline_armed: bool,
    teardown_authority: bool,
}

fn enf(ok: bool) -> Enforcement {
    if ok {
        Enforcement::Enforced
    } else {
        Enforcement::NotEnforced
    }
}

/// The immutable combined plan. Its [`report`](Self::report) is the ONLY place a `SandboxReport` is
/// produced, so a dimension can never be flipped to `Enforced` by an independent field assignment.
struct VerifiedLaunchPlan {
    identity: BackendIdentity,
    backend_digest: Digest256,
    policy_digest: Digest256,
    launch_nonce: LaunchNonce,
    launch_spec_digest: Digest256,
    filesystem: Enforcement,
    network: Enforcement,
    env: Enforcement,
    process_tree: Enforcement,
    timeout: Enforcement,
}

impl VerifiedLaunchPlan {
    /// Fold the child proof + monitor proof + bindings into the plan. Every binding is checked; a
    /// mismatch downgrades the affected dimension (never a silent Enforced).
    fn assemble(
        bindings: &Bindings,
        child: &ChildProof,
        monitor: &MonitorProof,
    ) -> VerifiedLaunchPlan {
        let bindings_match = child.nonce_hex == bindings.launch_nonce.as_str()
            && child.policy_digest == bindings.policy_digest.as_str()
            && child.launch_spec_digest == bindings.launch_spec_digest.as_str()
            && child.target_digest == bindings.target_digest.as_str()
            // The PRIVATE EXECUTION INODE the child ran must carry the exact launch-spec target
            // digest — the object-capability is bound to the approved bytes, not merely "some memfd".
            && child.exec_digest == bindings.target_digest.as_str();
        let child_ok = child.ok && bindings_match;
        // process_tree needs BOTH the child's inherited placement AND the monitor's independent
        // observation that the child sits in the monitor-owned leaf.
        let placement = child_ok
            && child.placement
            && monitor.child_in_leaf
            && monitor.leaf_path == child.cgroup_path;
        VerifiedLaunchPlan {
            identity: bindings.identity.clone(),
            backend_digest: bindings.backend_digest.clone(),
            policy_digest: bindings.policy_digest.clone(),
            launch_nonce: bindings.launch_nonce.clone(),
            launch_spec_digest: bindings.launch_spec_digest.clone(),
            // Filesystem is Enforced only when the child ALSO bound the execution-inode identity and
            // the resolved runtime-policy digest — so the report commits to the exact object that ran
            // and the exact runtime objects granted, not merely to "some confined process".
            filesystem: enf(child_ok
                && child.fs
                && !child.exec_inode.is_empty()
                && !child.runtime_binding.is_empty()),
            // A leaked inherited socket defeats network deny-all, so fd-scrub gates the network dim.
            network: enf(child_ok && child.net && child.fd),
            env: enf(child_ok && child.env),
            process_tree: enf(placement),
            timeout: enf(monitor.deadline_armed && monitor.teardown_authority),
        }
    }

    fn report(&self) -> SandboxReport {
        SandboxReport {
            schema_version: SCHEMA_VERSION,
            backend: self.identity.clone(),
            backend_digest: self.backend_digest.clone(),
            policy_digest: self.policy_digest.clone(),
            launch_nonce: self.launch_nonce.clone(),
            launch_spec_digest: self.launch_spec_digest.clone(),
            filesystem: self.filesystem,
            network: self.network,
            env: self.env,
            process_tree: self.process_tree,
            timeout: self.timeout,
        }
    }

    fn fully_enforced(&self) -> bool {
        matches!(self.filesystem, Enforcement::Enforced)
            && matches!(self.network, Enforcement::Enforced)
            && matches!(self.env, Enforcement::Enforced)
            && matches!(self.process_tree, Enforcement::Enforced)
            && matches!(self.timeout, Enforcement::Enforced)
    }
}

// ---- exit codes ---------------------------------------------------------------------------------

pub(crate) mod exit {
    /// Report sent but the parent did NOT authorize (NACK/EOF) — the target never ran.
    pub(crate) const NACK: i32 = 67;
    /// The report was NOT fully enforced (a proof failed); the target never ran.
    pub(crate) const REFUSED: i32 = 71;
    /// Internal launch plumbing failed (pipe/fork/encode).
    pub(crate) const PLUMBING: i32 = 70;
    /// The confined target ran to completion and was reaped; the tree was torn down cleanly.
    pub(crate) const OK: i32 = 0;
    /// Teardown of the cgroup tree FAILED — authoritative, dominates any earlier error.
    pub(crate) const TEARDOWN: i32 = 77;
}

const DRAIN_BOUND: Duration = Duration::from_secs(3);
const READY_BOUND: Duration = Duration::from_secs(10);

// ---- the monitor entry --------------------------------------------------------------------------

/// Run the full confined launch. Owns the report publication and returns the process exit code.
pub(crate) fn run_confined_launch(
    sock: &mut UnixStream,
    policy: &SandboxPolicy,
    request: &LaunchRequest,
    target_path: &Path,
    bindings: Bindings,
    faults: &Faults,
) -> i32 {
    // An unknown/malformed fault selection must abort the launch (never run the target).
    #[cfg(feature = "fault-injection")]
    if faults.is_invalid() {
        return refuse_before_child(sock, &bindings, "invalid_fault");
    }

    // 1. Open the sealed target BEFORE anything — its path resolves the worker's fd table, which a
    //    confined child could not open post-Landlock. The child inherits and `execveat`s this fd.
    let target = match File::open(target_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("sandboy: cannot open sealed target {target_path:?}: {e}");
            return refuse_before_child(sock, &bindings, "target_open");
        }
    };

    // 2. Establish the monitor-owned cgroup leaf (the monitor moves ITSELF in; the child inherits it).
    trip!(
        faults,
        CgroupCreate,
        return refuse_before_child(sock, &bindings, "cgroup_create")
    );
    let leaf = match crate::cgroup::Leaf::establish() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("sandboy: cannot establish cgroup leaf: {e}");
            return refuse_before_child(sock, &bindings, "cgroup_create");
        }
    };

    // 3. Pipes: proof (child → monitor) and release (monitor → child), both CLOEXEC (raw fds).
    let (proof_r, proof_w) = match seccomp::make_pipe_cloexec() {
        Ok(p) => p,
        Err(_) => return teardown_and(&leaf, faults, exit::PLUMBING, "pipe"),
    };
    let (release_r, release_w) = match seccomp::make_pipe_cloexec() {
        Ok(p) => p,
        Err(_) => {
            let _ = seccomp::close_fd(proof_r);
            let _ = seccomp::close_fd(proof_w);
            return teardown_and(&leaf, faults, exit::PLUMBING, "pipe");
        }
    };

    // 4. Arm the absolute deadline (the timeout proof is the ARMED timer + the teardown path).
    let deadline_at = Instant::now() + policy.timeout;

    // 5. Fork EXACTLY ONE launch child.
    let sock_fd = sock.as_raw_fd();
    let target_fd = target.as_raw_fd();
    match seccomp::fork_raw() {
        Err(_) => {
            let _ = seccomp::close_fd(proof_r);
            let _ = seccomp::close_fd(proof_w);
            let _ = seccomp::close_fd(release_r);
            let _ = seccomp::close_fd(release_w);
            teardown_and(&leaf, faults, exit::PLUMBING, "fork")
        }
        Ok(ForkResult::Child) => launch_child(ChildCtx {
            policy,
            request,
            bindings: &bindings,
            leaf_path: leaf.path().to_owned(),
            target_fd,
            target_path,
            sock_fd,
            proof_w,
            release_r,
            close_first: [proof_r, release_w],
            faults,
        }),
        Ok(ForkResult::Parent(child_pid)) => {
            // The monitor closes the CHILD's ends so its proof read sees EOF when the child is done.
            let _ = seccomp::close_fd(proof_w);
            let _ = seccomp::close_fd(release_r);
            let code = monitor_after_fork(MonitorCtx {
                sock,
                bindings: &bindings,
                leaf: &leaf,
                child_pid,
                deadline_at,
                proof_r,
                release_w,
                faults,
            });
            let _ = seccomp::close_fd(proof_r);
            let _ = seccomp::close_fd(release_w);
            drop(target);
            code
        }
    }
}

// ---- the monitor role (post-fork parent) --------------------------------------------------------

struct MonitorCtx<'a> {
    sock: &'a mut UnixStream,
    bindings: &'a Bindings,
    leaf: &'a crate::cgroup::Leaf,
    child_pid: i32,
    deadline_at: Instant,
    proof_r: RawFd,
    release_w: RawFd,
    faults: &'a Faults,
}

fn monitor_after_fork(mut ctx: MonitorCtx<'_>) -> i32 {
    // 5a. Prove the monitor-side invariants (independent of the child's self-report).
    let child_in_leaf = ctx.leaf.contains(ctx.child_pid).unwrap_or(false);
    let kill_path = format!("/sys/fs/cgroup{}/cgroup.kill", ctx.leaf.path());
    let teardown_authority = File::options().write(true).open(&kill_path).is_ok();
    let monitor_proof = MonitorProof {
        leaf_path: ctx.leaf.path().to_owned(),
        child_in_leaf,
        deadline_armed: true,
        teardown_authority,
    };

    // 5b. Read the child's READY_TO_EXEC (bounded); a failed/absent proof downgrades the report.
    let plan = {
        #[cfg(feature = "fault-injection")]
        if ctx.faults.tripped(fault::FaultId::ReadyValidate) {
            forced_downgrade(ctx.bindings, &monitor_proof, "ready_validate")
        } else {
            assemble_from_pipe(&ctx, &monitor_proof)
        }
        #[cfg(not(feature = "fault-injection"))]
        assemble_from_pipe(&ctx, &monitor_proof)
    };
    report_and_run(&mut ctx, plan)
}

fn assemble_from_pipe(ctx: &MonitorCtx<'_>, monitor_proof: &MonitorProof) -> VerifiedLaunchPlan {
    match read_child_proof(ctx.proof_r, ctx.child_pid) {
        Ok(child) => VerifiedLaunchPlan::assemble(ctx.bindings, &child, monitor_proof),
        Err(stage) => forced_downgrade(ctx.bindings, monitor_proof, &stage),
    }
}

/// Publish the report built from the plan, then run the GO/release/supervise/teardown sequence.
fn report_and_run(ctx: &mut MonitorCtx<'_>, plan: VerifiedLaunchPlan) -> i32 {
    let fully = plan.fully_enforced();
    let frame = match encode(&plan.report()) {
        Ok(f) => f,
        Err(_) => return teardown_and(ctx.leaf, ctx.faults, exit::PLUMBING, "encode"),
    };
    if ctx.sock.write_all(&frame).is_err()
        || ctx.sock.flush().is_err()
        || ctx.sock.shutdown(Shutdown::Write).is_err()
    {
        return teardown_and(ctx.leaf, ctx.faults, exit::PLUMBING, "report_write");
    }
    // GO barrier.
    let mut go = [0u8; 1];
    let authorized = matches!(ctx.sock.read_exact(&mut go), Ok(())) && go[0] == b'G';
    if !authorized {
        // The child already reported (and, on a downgrade, exited); REAP it before teardown so its
        // zombie does not linger in the leaf and stall the drain — then the leaf drains at once.
        reap_child_bounded(ctx.child_pid);
        return teardown_and(ctx.leaf, ctx.faults, exit::NACK, "nack");
    }
    if !fully {
        eprintln!("sandboy: refusing to release — not fully enforced");
        reap_child_bounded(ctx.child_pid);
        return teardown_and(ctx.leaf, ctx.faults, exit::REFUSED, "refused");
    }

    // 6. Release the SAME child with one token (one-shot). Then supervise under the deadline.
    trip!(ctx.faults, Release, {
        // Post-GO, PRE-execution fault: the child never receives the token, so the target never runs.
        write_fault_witness("release");
        return teardown_and(ctx.leaf, ctx.faults, exit::PLUMBING, "release");
    });
    if seccomp::write_fd(ctx.release_w, b"G")
        .map(|n| n == 1)
        .unwrap_or(false)
    {
        // ok
    } else {
        return teardown_and(ctx.leaf, ctx.faults, exit::PLUMBING, "release");
    }
    // Close the monitor's release-WRITE end NOW: the child, after taking the single token, reads once
    // more and requires EOF (a one-shot channel — nothing may follow). That EOF only arrives once the
    // last writer closes, so without this the child blocks forever and the target never `execveat`s.
    let _ = seccomp::close_fd(ctx.release_w);
    let _t_sup = Instant::now();
    let target_status = supervise(ctx.child_pid, ctx.deadline_at);

    // 7. Teardown DOMINATES.
    let _t_td = Instant::now();
    let _r = ctx.leaf.teardown(DRAIN_BOUND, ctx.faults.cgroup());
    match _r {
        Ok(()) => target_status,
        Err(e) => {
            eprintln!(
                "sandboy: teardown failed at {}: {} (exit {})",
                e.stage(),
                e.message(),
                e.exit_code()
            );
            // MONITOR-owned witness for a DEADLINE-teardown fault (kill/drain). Teardown DOMINATES the
            // target's own status — the exit is TEARDOWN.
            write_fault_witness(e.stage());
            exit::TEARDOWN
        }
    }
}

/// Reap the launch child (bounded ~2s) so a SELF-EXITED child's zombie leaves the cgroup leaf before
/// the drain observation, rather than lingering and stalling it. Instant on a downgrade (the child has
/// already exited); a still-running child returns `None` and is left for `cgroup.kill` in teardown.
fn reap_child_bounded(pid: i32) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        match seccomp::try_wait(pid) {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// Wait for the child; on the absolute deadline, the teardown (cgroup.kill in the caller) reaps it.
fn supervise(child_pid: i32, deadline_at: Instant) -> i32 {
    loop {
        match seccomp::try_wait(child_pid) {
            Ok(Some(_status)) => return exit::OK,
            Ok(None) => {}
            Err(_) => return exit::OK,
        }
        if Instant::now() >= deadline_at {
            return exit::OK; // teardown kills the tree
        }
        std::thread::sleep(
            deadline_at
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20)),
        );
    }
}

/// TEST-ONLY (`fault-injection`): record the reached fault `stage` to the MONITOR-owned witness path
/// `O7_FAULT_WITNESS`, if set, so a stage-specific oracle proves the launch failed at EXACTLY this
/// stage (not by a generic error). Written by the unconfined monitor (or the pre-exec child for
/// `execveat`), NEVER by the confined target: a setup fault means the target never runs, and the
/// target's env is rebuilt from the launch spec, so `O7_FAULT_WITNESS` can never reach it. Unlike
/// `O7_FAULT_POINT`, this benign path is not removed from the env.
#[cfg(feature = "fault-injection")]
fn write_fault_witness(stage: &str) {
    if let Some(p) = std::env::var_os("O7_FAULT_WITNESS") {
        let _ = std::fs::write(p, stage.as_bytes());
    }
}
#[cfg(not(feature = "fault-injection"))]
fn write_fault_witness(_stage: &str) {}

fn forced_downgrade(
    bindings: &Bindings,
    monitor: &MonitorProof,
    stage: &str,
) -> VerifiedLaunchPlan {
    eprintln!("sandboy: launch not enforced at {stage}");
    // MONITOR-owned witness: the fine stage the launch was downgraded at (the child's reported stage
    // for a child-side fault, or the monitor's own stage for ready_validate / ready_eof).
    write_fault_witness(stage);
    let empty = ChildProof {
        ok: false,
        stage: stage.to_owned(),
        child_pid: 0,
        cgroup_path: String::new(),
        nonce_hex: String::new(),
        policy_digest: String::new(),
        launch_spec_digest: String::new(),
        target_digest: String::new(),
        exec_digest: String::new(),
        exec_inode: String::new(),
        runtime_profile: String::new(),
        runtime_binding: String::new(),
        fs: false,
        net: false,
        env: false,
        fd: false,
        placement: false,
    };
    VerifiedLaunchPlan::assemble(bindings, &empty, monitor)
}

/// Send a downgraded report BEFORE any child/cgroup exists (target-open / early cgroup failure), then
/// expect the NACK. Nothing to tear down.
fn refuse_before_child(sock: &mut UnixStream, bindings: &Bindings, stage: &str) -> i32 {
    let monitor = MonitorProof {
        leaf_path: String::new(),
        child_in_leaf: false,
        deadline_armed: false,
        teardown_authority: false,
    };
    let plan = forced_downgrade(bindings, &monitor, stage);
    if let Ok(frame) = encode(&plan.report()) {
        let _ = sock.write_all(&frame);
        let _ = sock.flush();
        let _ = sock.shutdown(Shutdown::Write);
        let mut go = [0u8; 1];
        let _ = sock.read_exact(&mut go);
    }
    exit::REFUSED
}

fn teardown_and(leaf: &crate::cgroup::Leaf, faults: &Faults, on_ok: i32, why: &str) -> i32 {
    match leaf.teardown(DRAIN_BOUND, faults.cgroup()) {
        Ok(()) => on_ok,
        Err(e) => {
            eprintln!(
                "sandboy: teardown failed at {}: {} (dominates {why})",
                e.stage(),
                e.message()
            );
            // MONITOR-owned witness for a TEARDOWN fault (kill/drain): the exact teardown stage. The
            // teardown failure DOMINATES `why` (the triggering error) — the exit code is TEARDOWN.
            write_fault_witness(e.stage());
            exit::TEARDOWN
        }
    }
}

// ---- the child role (post-fork child; never returns) --------------------------------------------

struct ChildCtx<'a> {
    policy: &'a SandboxPolicy,
    request: &'a LaunchRequest,
    bindings: &'a Bindings,
    leaf_path: String,
    target_fd: RawFd,
    target_path: &'a Path,
    sock_fd: RawFd,
    proof_w: RawFd,
    release_r: RawFd,
    close_first: [RawFd; 2],
    faults: &'a Faults,
}

/// The launch child: close monitor-only fds → chdir → Landlock → env → fd-scrub+ledger → seccomp →
/// self-check → prove placement → READY → block release → close proof/release → final ledger →
/// `execveat`. On any failure it signals the stage and `_exit`s; the monitor tears the cgroup down.
fn launch_child(ctx: ChildCtx<'_>) -> i32 {
    // A) Close the monitor-only ends we inherited (proof-READ, release-WRITE, control socket), and put
    //    /dev/null on stdin. The full scrub (step G) closes anything else.
    for fd in ctx.close_first {
        let _ = seccomp::close_fd(fd);
    }
    let _ = seccomp::close_fd(ctx.sock_fd);
    if let Ok(devnull) = File::open("/dev/null") {
        let _ = seccomp::dup2_fd(devnull.as_raw_fd(), 0);
    }

    // B) chdir to the launch-spec cwd BEFORE confinement (afterwards the path may be unreachable).
    let cwd = std::path::PathBuf::from(std::ffi::OsString::from_vec(ctx.request.cwd.clone()));
    let _ = std::env::set_current_dir(&cwd);

    // B') Read our OWN cgroup placement NOW — inherited from the monitor's leaf; `/proc/self/cgroup`
    //     becomes unreadable once Landlock restricts paths.
    let cgroup_path = crate::cgroup::cgroup_path_of(std::process::id() as i32).unwrap_or_default();
    let placement_built = cgroup_path == ctx.leaf_path;

    // C) Read the sealed source bytes ONCE (pre-confinement): used both to materialize the private
    //    execution object and to parse the target ELF for the runtime read policy.
    let source = std::fs::read(format!("/proc/self/fd/{}", ctx.target_fd)).unwrap_or_default();

    // The FINE stage of the FIRST failing setup step, reported in the child proof so the monitor can
    // write a stage-specific fault witness (`O7_FAULT_WITNESS`). `None` on a clean launch.
    let mut fault_stage: Option<String> = None;

    // D) Materialize the PRIVATE, RULEABLE EXECUTION OBJECT (userns → private tmpfs → O_TMPFILE →
    //    exact copy → digest recheck → close writers → non-dumpable). Its bytes are proven EQUAL to
    //    the sealed source AND to hash to the launch-spec target digest. Landlock cannot authorize a
    //    pathless `memfd` narrowly, so we run an object of the kind it CAN control.
    let exec_obj = match crate::staging::materialize(
        &source,
        &ctx.bindings.target_digest,
        &ctx.faults.staging(),
    ) {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("sandboy: private execution object failed: {e}");
            fault_stage.get_or_insert_with(|| e.stage.as_str().to_owned());
            None
        }
    };
    // The sealed source memfd is no longer needed — its bytes now live in the execution inode.
    let _ = seccomp::close_fd(ctx.target_fd);

    // E) Resolve the RUNTIME read policy (interpreter + library roots) from the target ELF against the
    //    versioned platform profile — a separate authority class from `allow_exec`.
    let runtime = match crate::runtime::resolve(
        &source,
        &crate::runtime::PROFILE_V1,
        &ctx.faults.runtime(),
    ) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("sandboy: runtime policy unresolved: {e}");
            fault_stage.get_or_insert_with(|| e.stage().to_owned());
            None
        }
    };
    drop(source);

    // F) Build the target argv + env from the launch SPEC, sanitizing dynamic-linker controls.
    let (argv, envp, env_built) = build_argv_envp(&ctx, runtime.as_ref());

    // G) Fail-closed fd scrub + ledger BEFORE confinement: only stdio + exec fd + rule fd +
    //    proof-write + release-read may remain.
    let (exec_fd, rule_fd) = match &exec_obj {
        Some(o) => (o.exec_fd, o.rule_fd),
        None => (-1, -1),
    };
    let fd_built = if exec_obj.is_some() {
        scrub_and_verify(&[0, 1, 2, exec_fd, rule_fd, ctx.proof_w, ctx.release_r])
    } else {
        false
    };

    // H) Landlock: rule the private execution inode + `allow_exec` + the runtime read policy, then
    //    restrict. Close the `O_PATH` rule fd afterwards — it must not reach the target.
    // `runtime_binding` is returned BY the install, computed from the exact ruled objects — never
    // re-resolved from a pathname afterwards (that would let a swap bind a different object than the
    // one the rule protects).
    let (fs_ok, runtime_binding) = match (&exec_obj, &runtime) {
        (Some(o), Some(rt)) => match install_landlock(&ctx, o.rule_fd, rt) {
            Ok(binding) => (true, binding.as_str().to_owned()),
            Err(stage) => {
                fault_stage.get_or_insert_with(|| stage.to_owned());
                (false, String::new())
            }
        },
        // exec_obj / runtime already failed above and recorded their fine stage.
        _ => (false, String::new()),
    };
    if rule_fd >= 0 {
        let _ = seccomp::close_fd(rule_fd);
    }

    // I) seccomp network/process deny + effect-check.
    let net_ok = match install_seccomp_checked(&ctx) {
        Ok(()) => true,
        Err(stage) => {
            fault_stage.get_or_insert_with(|| stage.to_owned());
            false
        }
    };

    // Identity bindings the child proves: the execution inode's digest + identity, and the resolved
    // runtime read policy (profile version + a digest over every granted object's identity).
    let exec_digest = exec_obj
        .as_ref()
        .map(|o| o.digest.as_str().to_owned())
        .unwrap_or_default();
    let exec_inode = exec_obj
        .as_ref()
        .map(|o| format!("{}:{}", o.inode_id.0, o.inode_id.1))
        .unwrap_or_default();
    let runtime_profile = runtime
        .as_ref()
        .map(|r| r.profile_version.to_owned())
        .unwrap_or_default();
    // `runtime_binding` was bound at install time (step H) from the exact ruled objects.

    let (fd_ok, env_ok, placement) =
        apply_launch_faults(ctx.faults, fd_built, env_built, placement_built);
    // Capture the fd/env/placement fault stage: a dimension the launch BUILT but a fault then flipped.
    #[cfg(feature = "fault-injection")]
    if fault_stage.is_none() {
        if fd_built && !fd_ok {
            fault_stage = Some("fd_verify".to_owned());
        } else if env_built && !env_ok {
            fault_stage = Some("env_verify".to_owned());
        } else if placement_built && !placement {
            fault_stage = Some("cgroup_verify".to_owned());
        }
    }

    // J) READY_TO_EXEC — one immutable record. `stage` is the FINE fault stage when a fault was hit,
    //    else the coarse first-failed dimension (unchanged 7b.2 behaviour on a clean/naturally-failing
    //    launch).
    let all_ok = fs_ok && net_ok && env_ok && fd_ok && placement;
    let proof = ChildProof {
        ok: all_ok,
        stage: fault_stage
            .clone()
            .unwrap_or_else(|| first_failed_stage(fs_ok, net_ok, env_ok, fd_ok, placement)),
        child_pid: std::process::id() as i32,
        cgroup_path,
        nonce_hex: ctx.bindings.launch_nonce.as_str().to_owned(),
        policy_digest: ctx.bindings.policy_digest.as_str().to_owned(),
        launch_spec_digest: ctx.bindings.launch_spec_digest.as_str().to_owned(),
        target_digest: ctx.bindings.target_digest.as_str().to_owned(),
        exec_digest,
        exec_inode,
        runtime_profile,
        runtime_binding,
        fs: fs_ok,
        net: net_ok,
        env: env_ok,
        fd: fd_ok,
        placement,
    };
    trip!(ctx.faults, ReadyWrite, seccomp::exit_now(90));
    if write_child_proof(ctx.proof_w, &proof).is_err() || !all_ok {
        seccomp::exit_now(90);
    }

    // K) Block on the one-shot release token (reject extra bytes).
    let mut token = [0u8; 1];
    match seccomp::read_fd(ctx.release_r, &mut token) {
        Ok(1) if token[0] == b'G' => {}
        _ => seccomp::exit_now(91),
    }
    if !matches!(seccomp::read_fd(ctx.release_r, &mut [0u8; 1]), Ok(0)) {
        seccomp::exit_now(91); // a one-shot channel: nothing may follow the single token
    }

    // L) Close proof + release fds, then re-scrub: the final fd set the target inherits must be
    //    EXACTLY {stdio, private execution fd}.
    let _ = seccomp::close_fd(ctx.proof_w);
    let _ = seccomp::close_fd(ctx.release_r);
    if !scrub_and_verify(&[0, 1, 2, exec_fd]) {
        seccomp::exit_now(92);
    }

    // M) Become the target: `execveat` the read-only private execution inode. The `execveat` fault
    //    fires HERE, in the pre-exec child (still the trusted launcher — the target image never runs),
    //    so recording the witness here is monitor-side by trust, never target-manufactured.
    trip!(ctx.faults, Execveat, {
        write_fault_witness("execveat");
        seccomp::exit_now(93);
    });
    let argv_ptrs: Vec<*const libc::c_char> = argv
        .iter()
        .map(|c| c.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();
    let envp_ptrs: Vec<*const libc::c_char> = envp
        .iter()
        .map(|c| c.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();
    let e = crate::landlock::execveat_replace(exec_fd, &argv_ptrs, &envp_ptrs);
    eprintln!("sandboy: execveat failed: {e}");
    seccomp::exit_now(93)
}

fn first_failed_stage(fs: bool, net: bool, env: bool, fd: bool, placement: bool) -> String {
    if !fs {
        "landlock_self_check"
    } else if !fd {
        "fd_verify"
    } else if !net {
        "seccomp_self_check"
    } else if !env {
        "env_verify"
    } else if !placement {
        "cgroup_verify"
    } else {
        "ok"
    }
    .to_owned()
}

// ---- child helpers ------------------------------------------------------------------------------

/// Apply the launch-level (fd/env/placement) dimension faults. In production this is the identity.
#[cfg(feature = "fault-injection")]
fn apply_launch_faults(
    faults: &Faults,
    fd: bool,
    env: bool,
    placement: bool,
) -> (bool, bool, bool) {
    (
        fd && !faults.tripped(fault::FaultId::FdVerify),
        env && !faults.tripped(fault::FaultId::EnvVerify),
        placement && !faults.tripped(fault::FaultId::CgroupVerify),
    )
}
#[cfg(not(feature = "fault-injection"))]
fn apply_launch_faults(
    _faults: &Faults,
    fd: bool,
    env: bool,
    placement: bool,
) -> (bool, bool, bool) {
    (fd, env, placement)
}

/// Install Landlock for the worktree + `allow_exec` + the private execution object (`rule_fd`, its
/// `O_PATH`) + the runtime read policy, then restrict + differentially self-check. Returns whether the
/// install was proven.
/// Returns the runtime read-policy binding (computed from the EXACT ruled objects) on a proven
/// install, or `Err(fine-stage)` if not enforced — the fine `InstallError` stage (`create_ruleset`,
/// `add_rule`, `restrict_self`, `self_check_outside_inconclusive`, …) so a fault oracle can prove the
/// launch failed at EXACTLY that Landlock stage. The binding is the ONLY runtime commitment
/// `READY_TO_EXEC` makes — no pathname is re-resolved after the rules attach.
fn install_landlock(
    ctx: &ChildCtx<'_>,
    rule_fd: RawFd,
    runtime: &crate::runtime::RuntimePolicy,
) -> Result<o7_sandbox_protocol::ids::Digest256, &'static str> {
    match crate::landlock::install_filesystem(
        &ctx.policy.worktree,
        &ctx.policy.allow_exec,
        std::env::temp_dir().as_path(),
        Some(rule_fd),
        runtime,
        ctx.faults.landlock(),
    ) {
        Ok(proof) => Ok(proof.runtime_binding),
        Err(e) => {
            eprintln!("sandboy: landlock not enforced: {e}");
            Err(e.stage())
        }
    }
}

/// `Ok(())` when seccomp is installed AND the network-deny EFFECT is verified. `Err(fine-stage)`
/// distinguishes an install CRASH (`no_new_privs`/`apply`) from a SELF-CHECK failure
/// (`seccomp_self_check`: the filter installed but the effect check found network not actually denied)
/// — the report-downgrade (Class B) case.
fn install_seccomp_checked(ctx: &ChildCtx<'_>) -> Result<(), &'static str> {
    // NO blanket `execve` deny: exec confinement is Landlock's job (the narrow path allowlist +
    // the exact execution-object rule), so legitimate `allow_exec` subprocesses (`/bin/dash`,
    // `/bin/sleep`) still run while a non-allowed path exec is kernel-denied.
    match seccomp::install_seccomp(ctx.faults.seccomp()) {
        Ok(()) => {
            if seccomp::network_is_denied() {
                Ok(())
            } else {
                Err("seccomp_self_check")
            }
        }
        Err(e) => {
            eprintln!("sandboy: seccomp not enforced: {e}");
            Err(e.stage())
        }
    }
}

/// Build the target's argv + envp as C strings from the launch specification, validating env names
/// against the policy allowlist AND stripping dynamic-linker control variables the runtime profile
/// forbids (`LD_PRELOAD`, `LD_LIBRARY_PATH`, …) so they can never reach the target. `env_ok` is false
/// if any spec name is not allowlisted.
fn build_argv_envp(
    ctx: &ChildCtx<'_>,
    runtime: Option<&crate::runtime::RuntimePolicy>,
) -> (Vec<CString>, Vec<CString>, bool) {
    // `request.argv` carries the target's arguments as argv[1..]; the conventional program-name
    // argv[0] is synthesized here from the sealed target descriptor path (matching the reference
    // launcher, which sets argv[0] via `Command::new(target)`), then the request args follow.
    let argv0 = CString::new(ctx.target_path.as_os_str().as_bytes()).ok();
    let argv: Vec<CString> = argv0
        .into_iter()
        .chain(
            ctx.request
                .argv
                .iter()
                .filter_map(|a| CString::new(a.clone()).ok()),
        )
        .collect();
    let empty = Vec::new();
    let forbidden: &[Vec<u8>] = runtime
        .map(|r| r.forbidden_env.as_slice())
        .unwrap_or(&empty);
    let mut env_ok = true;
    let mut envp = Vec::new();
    for entry in &ctx.request.env {
        // Strip any dynamic-linker control variable the profile forbids — it never reaches the
        // target, whatever the allowlist says.
        if forbidden
            .iter()
            .any(|f| f.as_slice() == entry.name.as_slice())
        {
            continue;
        }
        let allowed = ctx
            .policy
            .env_allowlist
            .iter()
            .any(|n| n.as_encoded_bytes() == entry.name.as_slice());
        if !allowed {
            env_ok = false;
            continue;
        }
        let mut kv = entry.name.clone();
        kv.push(b'=');
        kv.extend_from_slice(&entry.value);
        match CString::new(kv) {
            Ok(c) => envp.push(c),
            Err(_) => env_ok = false,
        }
    }
    (argv, envp, env_ok)
}

/// Close every open fd not in `keep`, then independently re-verify the ledger. The enumeration is
/// FILESYSTEM-FREE (`close_range` + `fcntl`), so — unlike `/proc/self/fd` — it keeps working after
/// Landlock has restricted path access, which is exactly where the child scrubs (post-confinement,
/// pre-`execveat`). One `close_range` collapses the whole high fd space above the highest kept fd;
/// the few remaining low fds are closed individually.
fn scrub_and_verify(keep: &[RawFd]) -> bool {
    let keep_max = keep.iter().copied().max().unwrap_or(2);
    // Collapse EVERYTHING above the highest kept fd in one syscall. After this, nothing numbered
    // above `keep_max` can be open, which is what makes the bounded ledger scan below complete.
    if seccomp::close_range_from(keep_max + 1).is_err() {
        return false;
    }
    for fd in 0..=keep_max {
        if !keep.contains(&fd) {
            let _ = seccomp::close_fd(fd);
        }
    }
    verify_ledger(keep)
}

/// The fd ledger as an executable contract: the open fd set must equal EXACTLY `allowed` — every
/// allowed fd is open, no other fd is, and none of them is a socket. Relies on the caller having
/// `close_range`d everything above `max(allowed)` (so a bounded `0..=max` scan is a COMPLETE census,
/// no `/proc` needed).
fn verify_ledger(allowed: &[RawFd]) -> bool {
    let scan_max = allowed.iter().copied().max().unwrap_or(2);
    for fd in 0..=scan_max {
        let open = seccomp::fd_is_open(fd);
        // A stray fd (open but not allowed) OR a required fd that vanished — both fail closed.
        if open != allowed.contains(&fd) {
            return false;
        }
        if open && seccomp::fd_is_socket(fd) {
            return false;
        }
    }
    true
}

// ---- READY_TO_EXEC serialization (in-process, our code on both ends) ----------------------------

const READY_MAGIC: &[u8; 4] = b"O7R1";
const READY_MAX: usize = 64 * 1024;

fn write_child_proof(pipe: RawFd, p: &ChildProof) -> Result<(), ()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(READY_MAGIC);
    buf.push(u8::from(p.ok));
    put_str(&mut buf, &p.stage);
    buf.extend_from_slice(&p.child_pid.to_le_bytes());
    put_str(&mut buf, &p.cgroup_path);
    put_str(&mut buf, &p.nonce_hex);
    put_str(&mut buf, &p.policy_digest);
    put_str(&mut buf, &p.launch_spec_digest);
    put_str(&mut buf, &p.target_digest);
    put_str(&mut buf, &p.exec_digest);
    put_str(&mut buf, &p.exec_inode);
    put_str(&mut buf, &p.runtime_profile);
    put_str(&mut buf, &p.runtime_binding);
    buf.extend_from_slice(&[
        u8::from(p.fs),
        u8::from(p.net),
        u8::from(p.env),
        u8::from(p.fd),
        u8::from(p.placement),
    ]);
    let mut framed = (buf.len() as u32).to_le_bytes().to_vec();
    framed.extend_from_slice(&buf);
    write_all_fd(pipe, &framed)
}

fn read_child_proof(pipe: RawFd, child_pid: i32) -> Result<ChildProof, String> {
    let mut len = [0u8; 4];
    read_exact_fd(pipe, &mut len).map_err(|_| "ready_eof".to_owned())?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > READY_MAX {
        return Err("ready_malformed".to_owned());
    }
    let mut body = vec![0u8; n];
    read_exact_fd(pipe, &mut body).map_err(|_| "ready_eof".to_owned())?;
    let mut r = Reader { buf: &body, pos: 0 };
    if r.take(4)? != READY_MAGIC {
        return Err("ready_malformed".to_owned());
    }
    let ok = r.u8()? == 1;
    let stage = r.string()?;
    let pid = r.i32()?;
    if pid != child_pid {
        return Err("ready_pid".to_owned());
    }
    let cgroup_path = r.string()?;
    let nonce_hex = r.string()?;
    let policy_digest = r.string()?;
    let launch_spec_digest = r.string()?;
    let target_digest = r.string()?;
    let exec_digest = r.string()?;
    let exec_inode = r.string()?;
    let runtime_profile = r.string()?;
    let runtime_binding = r.string()?;
    let fs = r.u8()? == 1;
    let net = r.u8()? == 1;
    let env = r.u8()? == 1;
    let fd = r.u8()? == 1;
    let placement = r.u8()? == 1;
    if !ok {
        return Err(stage);
    }
    Ok(ChildProof {
        ok,
        stage,
        child_pid: pid,
        cgroup_path,
        nonce_hex,
        policy_digest,
        launch_spec_digest,
        target_digest,
        exec_digest,
        exec_inode,
        runtime_profile,
        runtime_binding,
        fs,
        net,
        env,
        fd,
        placement,
    })
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u16).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// A bounds-checked reader over the READY body (explicit lifetime — no elided-lifetime path).
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or("ready_malformed".to_owned())?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or("ready_malformed".to_owned())?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, String> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| "ready_malformed".to_owned())
    }
    fn i32(&mut self) -> Result<i32, String> {
        let b: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "ready_malformed".to_owned())?;
        Ok(i32::from_le_bytes(b))
    }
    fn string(&mut self) -> Result<String, String> {
        let l: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| "ready_malformed".to_owned())?;
        let n = u16::from_le_bytes(l) as usize;
        let bytes = self.take(n)?.to_vec();
        String::from_utf8(bytes).map_err(|_| "ready_malformed".to_owned())
    }
}

fn write_all_fd(fd: RawFd, mut buf: &[u8]) -> Result<(), ()> {
    while !buf.is_empty() {
        match seccomp::write_fd(fd, buf) {
            Ok(0) => return Err(()),
            Ok(n) => buf = buf.get(n..).unwrap_or(&[]),
            Err(e) if e == libc::EINTR => {}
            Err(_) => return Err(()),
        }
    }
    Ok(())
}

/// Read exactly `buf.len()` bytes from `fd` within `READY_BOUND`; EOF is an error.
fn read_exact_fd(fd: RawFd, buf: &mut [u8]) -> Result<(), ()> {
    let start = Instant::now();
    let mut off = 0;
    while off < buf.len() {
        if start.elapsed() > READY_BOUND {
            return Err(());
        }
        let Some(slice) = buf.get_mut(off..) else {
            return Err(());
        };
        match seccomp::read_fd(fd, slice) {
            Ok(0) => return Err(()),
            Ok(n) => off += n,
            Err(e) if e == libc::EINTR => {}
            Err(_) => return Err(()),
        }
    }
    Ok(())
}
