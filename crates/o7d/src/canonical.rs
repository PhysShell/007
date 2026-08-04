//! Q-Deck R1 fourth/fifth/sixth corrective rounds: classify an existing
//! command's bound child run's own canonical record BEFORE ever deciding to
//! redrive or recover it.
//!
//! Ledger status alone can never authorize re-invoking the provider a
//! second time — a stale or not-yet-caught-up ledger projection can show
//! `queued`/`running`/`interrupted` for a child run whose canonical
//! `events.jsonl` is already fully sealed. Nor is a record's mere
//! `run_id`/`conversation_id` match enough (fifth corrective round): the
//! record must be tied to the EXACT accepted `Command` that dispatched it.
//! Nor, once dispatched, is an unsealed outcome alone enough to redrive
//! (sixth corrective round): the actual classification logic —
//! [`ChildRecordState`]/[`DispatchProgress`] and the check itself — lives in
//! the root `o7` crate's `recovery` module (`o7::recovery::classify_command_child`)
//! so `o7d`'s own redrive decision and `o7 recover`'s read-only operator
//! discovery reporting share ONE classifier, never two that could drift.
//! This module is now a thin, `o7d`-specific wrapper: it resolves the
//! record directory from server-owned execution configuration and the
//! already-durably-bound `run_id`, then delegates.

use std::ffi::OsStr;
use std::io::Read as _;
use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{self, Mode, OFlags};

pub(crate) use o7::recovery::{ChildRecordState, DispatchProgress};

use crate::state::ExecutionConfig;

/// Q-Deck A0 corrective round 5 (Codex P1 / CodeRabbit Major, Part 2B): the
/// maximum `events.jsonl` size this server will read for an HTTP-triggered
/// candidate replay (admission preflight, run-detail projection) — checked
/// via `metadata` BEFORE any read, never after. A canonical record this
/// large is already pathological; refusing it here bounds the worst case
/// for an unauthenticated or low-trust caller hammering either endpoint.
const MAX_EVENTS_JSONL_BYTES: u64 = 8 * 1024 * 1024;

/// The maximum size of any SINGLE resolved artifact (a patch, a gate log,
/// a candidate receipt) during one HTTP-triggered candidate replay.
const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

/// The maximum TOTAL bytes hydrated (summed across every artifact resolved)
/// during one HTTP-triggered candidate replay — bounds a record with many
/// individually-small-enough artifacts that would otherwise sum to an
/// unbounded amount of memory.
const MAX_TOTAL_HYDRATED_BYTES: u64 = 128 * 1024 * 1024;

/// Q-Deck A0 corrective round 5: an [`o7_run::replay::ArtifactResolver`]
/// that enforces [`MAX_ARTIFACT_BYTES`]/[`MAX_TOTAL_HYDRATED_BYTES`] before
/// ever reading an artifact's bytes — scoped to ONLY the two HTTP-triggered
/// candidate-replay call sites in this module (`parent_candidate_state_usable`,
/// `candidate_projection`). Deliberately NOT applied to the shared
/// `o7::events::RecordDirResolver` every other consumer (`o7 replay`, `o7
/// recover`, this server's own pre-redrive classification) uses — those
/// are operator-invoked, not reachable by an untrusted HTTP caller, so an
/// unbounded read there is a different, already-accepted risk profile this
/// round does not change.
struct BoundedRecordDirResolver {
    dir_fd: OwnedFd,
    total_read: AtomicU64,
}

impl BoundedRecordDirResolver {
    fn new(dir_fd: OwnedFd) -> Self {
        Self {
            dir_fd,
            total_read: AtomicU64::new(0),
        }
    }
}

fn confined_locator(locator: &str) -> bool {
    let p = std::path::Path::new(locator);
    !locator.is_empty()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Q-Deck A0 corrective round 6 (Part 1 counterexample #2 / Part 2): opens
/// the record directory itself, no-follow, as a directory descriptor.
/// Every subsequent open (`events.jsonl`, each artifact locator) happens
/// relative to THIS descriptor via `openat`, never by re-resolving a path
/// string — there is no second, independent path resolution between a
/// check and a read left to race.
///
/// Only the FINAL path component (`run_id`, already proven to be a single
/// confined component by [`confine_run_id_component`]) is genuinely
/// variable here; `NOFOLLOW` on it rejects a symlinked record directory.
/// The leading components (`exec.runs_dir`, the target label) are entirely
/// server-configuration-controlled, never attacker input, so the kernel
/// resolving symlinks within THEM (ordinary path resolution, unavoidable
/// short of walking from a fixed root) is not a risk this round changes.
fn open_record_dir(dir: &std::path::Path) -> std::io::Result<OwnedFd> {
    fs::open(
        dir,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)
}

/// Opens `name` relative to `dir_fd`, no-follow, refusing anything that is
/// not a plain regular file. `O_NONBLOCK` means a FIFO cannot hang this
/// open even though the file-type check a moment later refuses it anyway.
/// The size returned comes from `fstat` on the EXACT descriptor just
/// opened — never a separate, independently-resolved `stat` call — so
/// there is no window between "check" and "this is the file being read"
/// for a TOCTOU swap to land in.
fn open_regular_file_nofollow(dir_fd: impl AsFd, name: &OsStr) -> std::io::Result<(OwnedFd, u64)> {
    // `?` on these two, not a `String`-formatting map_err: preserving the
    // underlying `std::io::Error` (and critically its `ErrorKind`, e.g.
    // `NotFound` for a missing `events.jsonl`) is what lets
    // `candidate_projection` keep distinguishing "no record at all" from
    // "a record exists but is broken" — collapsing everything to a string
    // here would silently erase that distinction.
    let fd = fs::openat(
        dir_fd,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let stat = fs::fstat(&fd).map_err(std::io::Error::from)?;
    if !fs::FileType::from_raw_mode(stat.st_mode).is_file() {
        return Err(std::io::Error::other(format!(
            "{name:?} is not a regular file — a symlink, FIFO, socket, device, or directory is \
             never valid record content"
        )));
    }
    let size = u64::try_from(stat.st_size)
        .map_err(|_| std::io::Error::other(format!("{name:?}: fstat reported a negative size")))?;
    Ok((fd, size))
}

/// Walks `locator`'s components relative to `dir_fd`, opening every
/// intermediate component as its own no-follow directory descriptor (never
/// `base.join(locator)` followed by a single path-based open), and returns
/// the final regular-file descriptor plus its `fstat`-observed size.
/// [`confined_locator`] has already proven every component is a `Normal`
/// component (no absolute path, no `..`, non-empty) before this runs.
fn open_locator_nofollow(dir_fd: &OwnedFd, locator: &str) -> Result<(OwnedFd, u64), String> {
    let path = std::path::Path::new(locator);
    let mut components: Vec<&OsStr> = path.components().map(|c| c.as_os_str()).collect();
    let Some(last) = components.pop() else {
        return Err("empty locator".to_owned());
    };
    let mut intermediate: Option<OwnedFd> = None;
    for component in components {
        let parent: &OwnedFd = intermediate.as_ref().unwrap_or(dir_fd);
        let next = fs::openat(
            parent,
            component,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| format!("open intermediate directory {component:?}: {e}"))?;
        intermediate = Some(next);
    }
    let parent: &OwnedFd = intermediate.as_ref().unwrap_or(dir_fd);
    open_regular_file_nofollow(parent, last).map_err(|e| format!("open {last:?}: {e}"))
}

/// Reads the ALREADY-OPENED descriptor `fd`, whose `fstat`-observed size
/// was `declared_size`, capped at `limit` bytes — inclusive: a file of
/// EXACTLY `limit` bytes is accepted, `limit + 1` is refused. Reads
/// `limit + 1` bytes through the SAME descriptor the size came from (never
/// a fresh open): if more than `limit` bytes are actually observed, the
/// file grew after `fstat` (a sparse file, a concurrent writer, a device
/// masquerading as a small file) and is refused rather than silently
/// truncated or trusted on the strength of a now-stale declared size.
fn read_bounded_from_fd(fd: OwnedFd, declared_size: u64, limit: u64) -> Result<Vec<u8>, String> {
    if declared_size > limit {
        return Err(format!(
            "declared size {declared_size} exceeds the {limit}-byte limit"
        ));
    }
    let mut file = std::fs::File::from(fd);
    let mut buf = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read: {e}"))?;
    let observed = buf.len() as u64;
    if observed > limit {
        return Err(format!(
            "grew past the {limit}-byte limit during read (observed at least {observed} bytes)"
        ));
    }
    Ok(buf)
}

impl o7_run::replay::ArtifactResolver for BoundedRecordDirResolver {
    fn resolve(
        &self,
        a: &o7_run::event::ArtifactRef,
    ) -> Result<Vec<u8>, o7_run::replay::ArtifactError> {
        let fail = |reason: String| o7_run::replay::ArtifactError {
            locator: a.locator.clone(),
            reason,
        };
        if !confined_locator(&a.locator) {
            return Err(fail(
                "locator escapes the record directory (absolute, `..`, or empty) — refused \
                 before I/O"
                    .to_owned(),
            ));
        }
        let (fd, size) = open_locator_nofollow(&self.dir_fd, &a.locator).map_err(fail)?;
        if size > MAX_ARTIFACT_BYTES {
            return Err(fail(format!(
                "artifact is {size} bytes, exceeding the {MAX_ARTIFACT_BYTES}-byte per-artifact \
                 limit for an HTTP-triggered candidate replay"
            )));
        }
        // `checked_add` inside `fetch_update`, never a plain `fetch_add`:
        // a plain add silently wraps on overflow with no signal at all —
        // here, overflow instead fails the resolve outright. A repeated
        // reference to the same artifact is never deduplicated; it always
        // consumes budget again, so there is no way to evade the total by
        // referencing one artifact many times.
        let prev_total = self
            .total_read
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |prev| {
                prev.checked_add(size)
            })
            .map_err(|_| fail("total hydrated byte counter overflowed".to_owned()))?;
        let running_total = prev_total + size;
        if running_total > MAX_TOTAL_HYDRATED_BYTES {
            return Err(fail(format!(
                "total hydrated bytes for this replay would reach {running_total}, exceeding \
                 the {MAX_TOTAL_HYDRATED_BYTES}-byte total budget for an HTTP-triggered \
                 candidate replay"
            )));
        }
        read_bounded_from_fd(fd, size, MAX_ARTIFACT_BYTES).map_err(fail)
    }
}

/// Read `events.jsonl` relative to the already-opened, no-follow record
/// directory descriptor `dir_fd` — refusing (before any read) anything
/// that is not a plain regular file, or a regular file larger than
/// [`MAX_EVENTS_JSONL_BYTES`] (inclusive: exactly the limit is accepted).
/// Q-Deck A0 corrective round 5, Part 2B; round 6, Part 2 (descriptor-
/// based, TOCTOU-proof rewrite — see [`open_regular_file_nofollow`] and
/// [`read_bounded_from_fd`]).
fn read_bounded_events_jsonl(dir_fd: &OwnedFd) -> std::io::Result<String> {
    let (fd, size) = open_regular_file_nofollow(dir_fd, OsStr::new("events.jsonl"))?;
    if size > MAX_EVENTS_JSONL_BYTES {
        return Err(std::io::Error::other(format!(
            "events.jsonl is {size} bytes, exceeding the {MAX_EVENTS_JSONL_BYTES}-byte limit \
             for an HTTP-triggered candidate replay"
        )));
    }
    let buf =
        read_bounded_from_fd(fd, size, MAX_EVENTS_JSONL_BYTES).map_err(std::io::Error::other)?;
    String::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// The run-store `target` label a command's child run record lives under —
/// `o7d` never passes `--target` when spawning `o7 continue`
/// (`spawn_continue`), so this MUST replicate `continue_run`'s own
/// default-target derivation exactly (`src/main.rs`, root crate): the
/// canonicalized configured repo's own final path component.
fn child_target(exec: &ExecutionConfig) -> std::io::Result<String> {
    let repo = exec.repo.canonicalize()?;
    Ok(repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "target".to_owned()))
}

/// Q-Deck A0 corrective round 3 (Codex P1, Part 1): whether `id` is valid
/// as the run-id component of a filesystem path — non-empty, not
/// absolute, no `.`/`..`, no embedded separator: exactly one normal
/// `std::path::Component`. `Path::new(id).components()` already collapses
/// `.`/redundant separators and classifies `..`/an absolute root as their
/// own non-`Normal` variants, so this is a complete, exhaustive check, not
/// a denylist of specific substrings.
///
/// `pub(crate)`, not merely a private helper inside [`child_record_dir`]:
/// `routes::create_command` calls this directly too, so a structurally
/// invalid `parent_run_id` gets its own stable `400`, distinct from a
/// well-formed-but-nonexistent one (`404`) or a well-formed, existing, but
/// candidate-unusable one (`409`) — three different failure shapes that
/// must not collapse into the same status code.
#[must_use]
pub(crate) fn is_confined_run_id_component(id: &str) -> bool {
    let mut components = std::path::Path::new(id).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn confine_run_id_component(id: &str) -> std::io::Result<()> {
    if is_confined_run_id_component(id) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("run id {id:?} is not a valid single path component"),
        ))
    }
}

/// The exact record directory a child run's canonical stream lives in —
/// computed ONLY from server-owned execution configuration (`exec`) and
/// `run_id`, never from anything else in the HTTP request. Q-Deck A0
/// corrective round 3 (Codex P1, Part 1): `run_id` is confined to a
/// single normal path component BEFORE it is ever joined into a path —
/// checked HERE, inside the one shared path-construction primitive every
/// caller goes through (including `parent_candidate_state_usable`, whose
/// own `run_id` comes directly from raw, not-yet-ledger-validated HTTP
/// request text), so no caller can forget it and no filesystem I/O is
/// ever attempted against an unconfined path.
pub(crate) fn child_record_dir(exec: &ExecutionConfig, run_id: &str) -> std::io::Result<PathBuf> {
    confine_run_id_component(run_id)?;
    Ok(exec.runs_dir.join(child_target(exec)?).join(run_id))
}

/// Classify `run_id`'s own canonical record against the EXACT accepted
/// `command` this redrive/recovery decision is about — the shared decision
/// point behind Q-Deck R1's redrive path
/// (`routes.rs::redrive_or_recover_locked`).
pub(crate) fn classify_child_record(
    exec: &ExecutionConfig,
    run_id: &str,
    command: &o7_ledger::Command,
) -> ChildRecordState {
    match child_record_dir(exec, run_id) {
        Ok(dir) => o7::recovery::classify_command_child(&dir, run_id, command),
        Err(e) => ChildRecordState::Invalid(format!("resolving the record directory: {e}")),
    }
}

/// Q-Deck A0 corrective round 2 (Part 5); repository-bound corrective
/// round 3 (Codex P1, Part 4): whether `parent_run_id`'s own canonical
/// record proves it is USABLE as a command-continuation parent — checked
/// BEFORE a new command is ever durably created, not merely discovered
/// later when `o7 continue` itself fails after acceptance. A parent that
/// predates Q-Deck A0 (a genuine R1-only run, sealed and otherwise
/// perfectly valid, but with no `candidate_state` contract obligation at
/// all) is EXPLICITLY not usable — A0 makes a valid, semantically
/// verified candidate receipt mandatory for every continuation once live,
/// never a silent fallback to R1's old zero-carryover behavior. Returns
/// `Err(reason)` (a SERVER-side diagnostic string, never sent to the
/// client verbatim — same discipline as `ApiError::Internal`'s stable
/// code) for every one of: no canonical record at all; full replay
/// failure (chain/digest/reducer/artifact/candidate-semantic — the SAME
/// authoritative `verify_prefix` every other consumer uses); not sealed;
/// no candidate-state contract obligation; no candidate-state evidence
/// captured; the parent's own candidate contract was captured against a
/// DIFFERENT repository than this server is configured for (Part 4 — a
/// server pointed at a different checkout sharing the same `target`
/// basename must never treat that checkout's old record as usable). A
/// `Blocked`/`Fail`/`Error` sealed verdict is itself a legitimate,
/// replay-valid record (§8: canonical replay validity is not the same
/// question as continuation eligibility) — its own verdict is untouched
/// by this check; only the SEPARATE candidate-continuation-eligibility
/// question is answered here.
///
/// # Errors
/// A diagnostic `String` (server-side only) describing why the parent is
/// not usable.
pub(crate) fn parent_candidate_state_usable(
    exec: &ExecutionConfig,
    parent_run_id: &str,
) -> Result<(), String> {
    let dir = child_record_dir(exec, parent_run_id)
        .map_err(|e| format!("resolving parent record directory: {e}"))?;
    let dir_fd =
        open_record_dir(&dir).map_err(|e| format!("opening the parent's record directory: {e}"))?;
    let text = read_bounded_events_jsonl(&dir_fd)
        .map_err(|e| format!("reading the parent's canonical record: {e}"))?;
    let events = o7::events::from_jsonl(&text)
        .map_err(|e| format!("parsing the parent's canonical record: {e}"))?;
    let resolver = BoundedRecordDirResolver::new(dir_fd);
    let (state, _artifacts_verified) = o7_run::replay::verify_prefix(&events, &resolver)
        .map_err(|e| format!("the parent's canonical record failed full replay: {e}"))?;
    if state.verdict.is_none() {
        return Err("the parent run is not sealed".to_owned());
    }
    let Some(obligation) = state
        .contract
        .as_ref()
        .and_then(|c| c.candidate_state.as_ref())
    else {
        return Err(
            "the parent run predates Q-Deck A0 (no candidate_state contract obligation) — a \
             valid candidate receipt is mandatory for every command continuation"
                .to_owned(),
        );
    };
    if state.candidate_state.is_none() {
        return Err(
            "the parent run declares a candidate_state obligation but never captured a receipt"
                .to_owned(),
        );
    }
    // Q-Deck A0 corrective round 3 (Codex P1, Part 4): the parent's OWN
    // verified candidate contract must have been captured against THIS
    // exact server's configured repository — `verify_prefix` (above)
    // already proved the CAPTURED RECEIPT agrees with its OWN contract
    // (internal self-consistency); it has no way to know whether that
    // contract's `repository_id` matches the repository THIS server is
    // configured for right now. Without this check, pointing the server
    // at a different checkout sharing the same `target` basename would
    // let a stale record from the OLD repository pass admission — the
    // mismatch would only surface later, in
    // `materialize_parent_candidate_state`, after the command already
    // durably attached a child run.
    let this_repo_id = o7::worktree::repository_identity(&exec.repo)
        .map_err(|e| format!("resolving this server's own configured repository identity: {e}"))?;
    if obligation.repository_id != this_repo_id {
        return Err(
            "the parent's candidate-state contract was captured against a different \
             repository than this server is currently configured for"
                .to_owned(),
        );
    }
    // Q-Deck A0 corrective round 8 (fresh exact-head Codex P1,
    // `#discussion_r3710144132`): `verify_prefix` above proves the
    // receipt's OWN internal consistency, never that `base_commit` is
    // still a real, resolvable object in `exec.repo` right now. A pruned
    // base (its retaining branch deleted, reflog expired, real `git gc`
    // run — reproduced live) previously still passed this whole function,
    // and `create_command` durably attached a child before the resulting
    // `o7 continue` crashed inside `worktree::add`'s own `git worktree
    // add`, leaving a stuck, never-dispatched, unrecoverable-by-redrive
    // child behind. Checked here, using the SAME hardened primitive
    // `main.rs`'s own materialization path already calls
    // (`worktree::verify_commit_exists`, `git cat-file -e`) — never a
    // second, lighter reimplementation — and inside this function's own
    // existing bounded `spawn_blocking` admission path, so it costs
    // nothing beyond what full replay already spends there.
    o7::worktree::verify_commit_exists(&exec.repo, &obligation.base_commit).map_err(|e| {
        format!(
            "the parent's candidate-state base commit no longer resolves in this server's \
             configured repository: {e}"
        )
    })?;
    // `verify_prefix` (above) already ran full candidate semantic
    // verification on `state.candidate_state` as part of its own authoritative
    // pass — its mere presence here, post-success, IS the proof it is usable.
    Ok(())
}

/// Q-Deck A0 (`docs/q-deck/a0-candidate-state.md` §8), corrected corrective
/// round 2 (Part 3): a read-only, best-effort candidate-state projection
/// for `routes::get_run` — never mutates anything, never gates a redrive
/// decision (that stays exactly `classify_child_record`/
/// `o7::recovery::classify_command_child`, above, untouched by this).
///
/// The ORIGINAL implementation of this function had two defects, both fixed
/// here: (1) it read the raw JSON's `actual_tree_oid` field, a name that
/// stopped existing when corrective round 1 reshaped
/// `CandidateStateMaterialized` to carry `materialized_tree_oid` instead —
/// this projection's own tree-OID field had been silently, permanently
/// `None` ever since; (2) it trusted whatever the last matching raw JSON
/// line claimed, with NO chain/digest/reducer/artifact/candidate-semantic
/// verification at all — a tampered or truncated record could be projected
/// as "materialized" even though full replay would reject it. Both are
/// fixed by running the SAME authoritative `o7_run::replay::verify_prefix`
/// every other production consumer uses, and reading the tree OID back out
/// of the typed, already-verified `RunState` — never raw JSON. A run whose
/// full replay fails is projected as `"verification_failed"`, never
/// `"materialized"` — this projection must not claim semantic lineage is
/// verified when it is not.
///
/// Returns `(candidate_source_run_id, candidate_tree_oid,
/// materialization_status)`; all `None`/`"not_applicable"` for a run with no
/// canonical record yet, or one that never attempted materialization (a
/// top-level `o7 run`, or a continuation still pre-dispatch).
pub(crate) fn candidate_projection(
    exec: &ExecutionConfig,
    run_id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let dir = match child_record_dir(exec, run_id) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "[o7d] candidate projection for run {run_id}: resolving record directory: {e}"
            );
            return (None, None, Some("failed".to_owned()));
        }
    };
    // Q-Deck A0 corrective round 5 (CodeRabbit Major, Part 4): only a
    // missing record (a run with no canonical record yet, or one that
    // never captured/materialized anything) projects as `not_applicable`.
    // Any OTHER I/O error (permission denied, a genuine device/read
    // failure) is a `failed` status, not silently folded into the exact
    // same "there's simply nothing here" value.
    //
    // Q-Deck A0 corrective round 6 (Part 2): the record directory itself
    // missing (no canonical record has ever been written for this run)
    // and `events.jsonl` missing INSIDE an existing directory both surface
    // as `ErrorKind::NotFound` — exactly as the old path-based `metadata`
    // call collapsed both cases, preserved here for behavioral parity.
    let dir_fd = match open_record_dir(&dir) {
        Ok(fd) => fd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (None, None, Some("not_applicable".to_owned()))
        }
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: opening record directory: {e}");
            return (None, None, Some("failed".to_owned()));
        }
    };
    let text = match read_bounded_events_jsonl(&dir_fd) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (None, None, Some("not_applicable".to_owned()))
        }
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: reading events.jsonl: {e}");
            return (None, None, Some("failed".to_owned()));
        }
    };
    let events = match o7::events::from_jsonl(&text) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: malformed events.jsonl: {e}");
            return (None, None, Some("verification_failed".to_owned()));
        }
    };
    let resolver = BoundedRecordDirResolver::new(dir_fd);
    let (state, _artifacts_verified) = match o7_run::replay::verify_prefix(&events, &resolver) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: replay failed: {e}");
            return (None, None, Some("verification_failed".to_owned()));
        }
    };
    match &state.candidate_materialized {
        Some(mat) => (
            Some(mat.source_run_id.as_str().to_owned()),
            Some(mat.materialized_tree_oid.clone()),
            Some("materialized".to_owned()),
        ),
        None => (None, None, Some("not_applicable".to_owned())),
    }
}

/// Q-Deck A0 corrective round 6 (Part 2): adversarial, descriptor-level
/// tests for the bounded, no-follow, TOCTOU-proof read path. At `c979bc1`
/// these checks were path-based (`std::fs::metadata` then a SEPARATE
/// `std::fs::read`/`read_to_string`) — a symlink from `events.jsonl` to
/// `/dev/zero` reports `metadata().len() == 0` (character devices always
/// do), passes the size gate, and the subsequent read follows the symlink
/// and reads until OOM. Every test here that would otherwise risk an
/// unbounded read is wrapped in an explicit wall-clock timeout so a
/// regression fails the test loudly rather than hanging or exhausting this
/// shared VPS's own memory.
// Q-Deck A0 corrective round 8 (fresh CodeRabbit finding): every
// `unwrap`/`expect`/index in this module operates ONLY on bytes and paths
// this same test just constructed (fixture files, in-memory buffers,
// literals) — never on untrusted input reaching this process from a real
// caller. A panic here is this test failing loudly on its own broken
// assumption, not a production fault path this lint would otherwise guard.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod bounded_read_tests {
    use super::*;
    use o7_run::event::{ArtifactKind, ArtifactRef, Digest256};
    use o7_run::replay::ArtifactResolver as _;
    use std::time::Duration;

    fn artifact_ref(locator: &str) -> ArtifactRef {
        ArtifactRef {
            kind: ArtifactKind::Diff,
            locator: locator.to_owned(),
            digest: Digest256::of_bytes(b""),
        }
    }

    /// Every genuinely risky call in this module — anything that could, on
    /// a regression, read unboundedly — runs on a blocking thread with a
    /// hard wall-clock cap. A regression fails the assertion (or times out
    /// the test itself) instead of ever actually exhausting memory.
    fn run_with_timeout<F, R>(f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("operation did not complete within the timeout — likely an unbounded read")
    }

    #[test]
    fn events_jsonl_symlink_to_dev_zero_is_rejected_quickly() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.path().join("events.jsonl")).unwrap();
        let dir_path = dir.path().to_owned();
        let result = run_with_timeout(move || {
            let dir_fd = open_record_dir(&dir_path).unwrap();
            read_bounded_events_jsonl(&dir_fd)
        });
        assert!(result.is_err(), "a symlink to /dev/zero must never be read");
    }

    #[test]
    fn artifact_symlink_to_dev_zero_is_rejected_quickly() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.path().join("evil.patch")).unwrap();
        let dir_path = dir.path().to_owned();
        let result: Result<Vec<u8>, _> = run_with_timeout(move || {
            let dir_fd = open_record_dir(&dir_path).unwrap();
            let resolver = BoundedRecordDirResolver::new(dir_fd);
            resolver.resolve(&artifact_ref("evil.patch"))
        });
        assert!(result.is_err(), "a symlink to /dev/zero must never be read");
    }

    #[test]
    fn symlink_to_an_oversized_regular_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.bin");
        // Sparse: a huge LOGICAL size with no real disk/time cost.
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_ARTIFACT_BYTES + 1)
            .unwrap();
        std::os::unix::fs::symlink(&big, dir.path().join("evil.patch")).unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let resolver = BoundedRecordDirResolver::new(dir_fd);
        let result = resolver.resolve(&artifact_ref("evil.patch"));
        assert!(
            result.is_err(),
            "a symlink target is refused by NOFOLLOW regardless of its own size"
        );
    }

    #[test]
    fn fifo_is_rejected_without_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("events.jsonl");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("spawn mkfifo");
        assert!(
            status.success(),
            "mkfifo must succeed for this test to be meaningful"
        );
        let dir_path = dir.path().to_owned();
        let result = run_with_timeout(move || {
            let dir_fd = open_record_dir(&dir_path).unwrap();
            read_bounded_events_jsonl(&dir_fd)
        });
        assert!(
            result.is_err(),
            "a FIFO must never be treated as record content"
        );
    }

    #[test]
    fn socket_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("events.jsonl");
        let _listener = std::os::unix::net::UnixListener::bind(&sock_path).unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let result = read_bounded_events_jsonl(&dir_fd);
        assert!(
            result.is_err(),
            "a socket must never be treated as record content"
        );
    }

    /// A device special file can't be created without root, and hard-
    /// linking a real one (`/dev/null`) into a tempdir fails cross-device
    /// (`/dev` and `TMPDIR` are different filesystems here). This tests the
    /// file-type check itself directly against the real `/dev/null`
    /// device — proving `FileType::from_raw_mode(..).is_file()` correctly
    /// refuses a character device, independent of how a caller's fd came
    /// to point at one (symlink-NOFOLLOW already covers that path above).
    #[test]
    fn file_type_check_rejects_a_real_character_device() {
        let fd = fs::open(
            "/dev/null",
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .unwrap();
        let stat = fs::fstat(&fd).unwrap();
        assert!(
            !fs::FileType::from_raw_mode(stat.st_mode).is_file(),
            "/dev/null must never be classified as a regular file"
        );
    }

    #[test]
    fn toctou_path_replacement_after_open_does_not_affect_already_opened_fd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("events.jsonl"), "original\n").unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let (fd, _size) = open_regular_file_nofollow(&dir_fd, OsStr::new("events.jsonl")).unwrap();
        // Swap the path AFTER the descriptor is already open — a real
        // TOCTOU attacker's move. Descriptor-based reads never re-resolve
        // the path, so this must have no effect on what gets read.
        std::fs::remove_file(dir.path().join("events.jsonl")).unwrap();
        std::os::unix::fs::symlink("/dev/zero", dir.path().join("events.jsonl")).unwrap();
        let content = read_bounded_from_fd(fd, 9, MAX_EVENTS_JSONL_BYTES).unwrap();
        assert_eq!(content, b"original\n");
    }

    #[test]
    fn post_fstat_growth_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("growing.txt");
        std::fs::write(&path, "12345").unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let (fd, declared_size) =
            open_regular_file_nofollow(&dir_fd, OsStr::new("growing.txt")).unwrap();
        assert_eq!(declared_size, 5);
        // Grow the file (via the ORIGINAL path, a separate handle) after
        // the size was fstat'd but before the bounded read runs.
        std::fs::write(&path, "1234567890").unwrap();
        let result = read_bounded_from_fd(fd, declared_size, 5);
        assert!(
            result.is_err(),
            "a file that grew past its fstat'd size must be rejected, not silently truncated"
        );
    }

    #[test]
    fn sparse_file_oversized_logical_length_is_rejected_before_any_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_EVENTS_JSONL_BYTES + 1)
            .unwrap();
        std::fs::rename(&path, dir.path().join("events.jsonl")).unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let result = run_with_timeout(move || read_bounded_events_jsonl(&dir_fd));
        assert!(
            result.is_err(),
            "an oversized declared (logical) size must be refused before any read at all"
        );
    }

    #[test]
    fn exact_limit_is_accepted_inclusive() {
        let fd = tempfile::tempfile().unwrap();
        let owned: std::os::fd::OwnedFd = fd.into();
        let content = read_bounded_from_fd(owned, 0, 10);
        assert!(content.is_ok());
        // A real exact-limit boundary, using the actual event-log constant
        // via a sparse (all-zero, valid-UTF8) file — fast, no huge write.
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join("events.jsonl"))
            .unwrap()
            .set_len(MAX_EVENTS_JSONL_BYTES)
            .unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let text = run_with_timeout(move || read_bounded_events_jsonl(&dir_fd))
            .expect("exactly the limit must be accepted");
        assert_eq!(text.len() as u64, MAX_EVENTS_JSONL_BYTES);
    }

    #[test]
    fn limit_plus_one_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, vec![b'a'; 11]).unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let (fd, size) = open_regular_file_nofollow(&dir_fd, OsStr::new("f.bin")).unwrap();
        let result = read_bounded_from_fd(fd, size, 10);
        assert!(
            result.is_err(),
            "declared size one over the limit must be rejected"
        );
    }

    #[test]
    fn repeated_artifact_reference_always_consumes_budget_never_bypassed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), vec![b'x'; 1000]).unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let resolver = BoundedRecordDirResolver::new(dir_fd);
        // Seed the running total close to the ceiling so a SECOND read of
        // the exact same artifact tips it over — proving repeated
        // references are never deduplicated to evade the total budget.
        resolver
            .total_read
            .store(MAX_TOTAL_HYDRATED_BYTES - 1500, Ordering::SeqCst);
        assert!(resolver.resolve(&artifact_ref("a.patch")).is_ok());
        let result = resolver.resolve(&artifact_ref("a.patch"));
        assert!(
            result.is_err(),
            "a second read of the SAME artifact must still consume budget and can still exceed it"
        );
    }

    #[test]
    fn total_budget_overflow_is_rejected_not_wrapped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.patch"), b"x").unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let resolver = BoundedRecordDirResolver::new(dir_fd);
        resolver.total_read.store(u64::MAX, Ordering::SeqCst);
        let result = resolver.resolve(&artifact_ref("a.patch"));
        assert!(
            result.is_err(),
            "checked_add overflow must fail the resolve, never silently wrap the counter"
        );
    }

    #[test]
    fn malformed_locators_are_rejected_before_any_io() {
        let dir = tempfile::tempdir().unwrap();
        let dir_fd = open_record_dir(dir.path()).unwrap();
        let resolver = BoundedRecordDirResolver::new(dir_fd);
        for bad in ["../escape.txt", "/etc/passwd", "", "a/../../b"] {
            let result = resolver.resolve(&artifact_ref(bad));
            assert!(
                result.is_err(),
                "locator {bad:?} must be refused before any I/O"
            );
        }
    }
}
