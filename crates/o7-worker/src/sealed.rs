//! A genuinely immutable held object: bytes staged into an anonymous `memfd` and sealed, so a
//! same-UID process cannot mutate the inode after acquisition (a held-but-mutable fd could be
//! written in place; a sealed memfd cannot). Addressable as `/proc/<owner_pid>/fd/<n>` for the
//! object's lifetime.
//!
//! `unsafe` stays forbidden: `memfd_create`, `fcntl(F_ADD_SEALS/F_GET_SEALS)`, and
//! `File::from(OwnedFd)` are all safe APIs.

use std::ffi::CString;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::os::unix::io::AsRawFd as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nix::fcntl::{fcntl, FcntlArg, SealFlag};
use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

use o7_sandbox_protocol::ids::Digest256;

/// Upper bound on a staged object (defends against an unbounded read of a huge/again-growing
/// source).
pub const MAX_OBJECT_BYTES: usize = 256 * 1024 * 1024;

/// The full seal set an immutable object must carry.
fn required_seals() -> SealFlag {
    SealFlag::F_SEAL_WRITE | SealFlag::F_SEAL_GROW | SealFlag::F_SEAL_SHRINK | SealFlag::F_SEAL_SEAL
}

/// Why an object could not be sealed/acquired.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    /// The source object could not be opened or read.
    #[error("could not open/read source object: {0}")]
    Open(String),
    /// The source exceeds [`MAX_OBJECT_BYTES`].
    #[error("object exceeds the {MAX_OBJECT_BYTES}-byte maximum")]
    TooLarge,
    /// Staging/sealing failed (memfd, add-seals, get-seals).
    #[error("could not stage/seal object: {0}")]
    Seal(String),
    /// The seals did not fully take.
    #[error("sealed object is not fully immutable")]
    NotSealed,
    /// The sealed bytes do not match the expected digest.
    #[error("sealed object digest does not match expected")]
    DigestMismatch,
}

/// A TARGET-staging failure tagged by the PHASE it occurred in. The phase — NOT the [`SealError`]
/// variant — is the authority for boundary classification, because the same variant means different
/// things in different phases. [`SealError::NotSealed`], for instance, is a fail-closed TARGET
/// refusal when the SOURCE object is unacceptable (an input `/proc/<pid>/fd/<n>` object that is not a
/// fully sealed memfd), but an INFRASTRUCTURE failure when the worker's OWN staging memfd could not
/// be sealed. [`SealedObject::stage`] erases which phase produced a bare `SealError`, so it wraps
/// each phase here instead — classifying by variant cannot tell these two apart.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TargetStageError {
    /// The SOURCE object was rejected during hardened acquisition — a symlink final component, a
    /// FIFO/device/socket/directory, an over-size object, or an input `/proc/fd` object that is not
    /// a fully sealed memfd. This is the intended fail-closed TARGET refusal.
    #[error("target source rejected: {0}")]
    SourceRejected(#[source] SealError),
    /// The source was acceptable, but staging it into the worker's OWN sealed memfd failed
    /// (memfd/seal/digest). This is an INFRASTRUCTURE failure, never a target refusal.
    #[error("target staging failed: {0}")]
    StagingFailed(#[source] SealError),
}

/// A held, sealed (immutable) copy of an object's bytes.
#[derive(Debug, Clone)]
pub struct SealedObject {
    fd: Arc<File>,
    digest: Digest256,
}

/// Whether `path` is a `/proc/<pid|self|thread-self>/fd/<n>` descriptor path — the frozen PR-3
/// execution contract. Such a path is a procfs MAGIC SYMLINK to a held object, so it must be
/// acquired by FOLLOWING it (never `O_NOFOLLOW`) into the exact object it points at, in this PID
/// namespace. Every other path is an ordinary filesystem path, hardened with `O_NOFOLLOW`.
/// Whether `path` is a `/proc/<pid|self|thread-self>/fd/<n>` descriptor path — a caller-held
/// sealed launch SOURCE (authorized by its seals at acquisition), NOT an ordinary exec allowance.
pub(crate) fn is_proc_fd_path(path: &Path) -> bool {
    use std::path::Component;
    fn normal<'a>(c: &Component<'a>) -> Option<&'a str> {
        match c {
            Component::Normal(s) => s.to_str(),
            _ => None,
        }
    }
    fn all_digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }
    let comps: Vec<Component<'_>> = path.components().collect();
    match comps.as_slice() {
        [Component::RootDir, proc, pid, fd, n] => {
            normal(proc) == Some("proc")
                && normal(fd) == Some("fd")
                && matches!(normal(pid), Some(s) if s == "self" || s == "thread-self" || all_digits(s))
                && matches!(normal(n), Some(s) if all_digits(s))
        }
        _ => false,
    }
}

/// Read all of `file`'s bytes under [`MAX_OBJECT_BYTES`], returning [`SealError::TooLarge`] if it
/// exceeds the cap (read `cap + 1` so an object that grew past the cap is caught, not truncated).
fn read_capped(mut file: File) -> Result<Vec<u8>, SealError> {
    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, MAX_OBJECT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SealError::Open(e.to_string()))?;
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(SealError::TooLarge);
    }
    Ok(bytes)
}

/// Acquire the bytes of an ORDINARY filesystem `source`, hardened exactly as the verifier does:
/// open `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC` (a symlink final component fails closed; a FIFO/
/// device never blocks), prove the OPEN descriptor is a REGULAR file (rejecting FIFO/device/
/// socket/directory/pseudo-file), and read the bytes from that same descriptor under a size cap
/// with a drift check — so the bytes staged are the bytes proven.
fn acquire_ordinary(source: &Path) -> Result<Vec<u8>, SealError> {
    use rustix::fs::{self as rfs, Mode, OFlags};
    let fd = rfs::open(
        source,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| {
        if e == rustix::io::Errno::LOOP {
            SealError::Open("the path is a symlink (O_NOFOLLOW)".to_owned())
        } else {
            SealError::Open(format!("open failed: {e}"))
        }
    })?;
    let st = rfs::fstat(&fd).map_err(|e| SealError::Open(format!("fstat failed: {e}")))?;
    if (st.st_mode & 0o170_000) != 0o100_000 {
        return Err(SealError::Open(
            "not a regular file (fifo, device, socket, or directory)".to_owned(),
        ));
    }
    let size =
        u64::try_from(st.st_size).map_err(|_| SealError::Open("negative size".to_owned()))?;
    if size == 0 {
        return Err(SealError::Open(
            "empty or pseudo-file (reported size 0)".to_owned(),
        ));
    }
    if size > MAX_OBJECT_BYTES as u64 {
        return Err(SealError::TooLarge);
    }
    // Read under a cap of size+1 and require an EXACT match, catching a file that grew between
    // stat and read (or a pseudo-file whose content exceeds its reported size).
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    let read = std::io::Read::take(File::from(fd), size + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SealError::Open(format!("read failed: {e}")))? as u64;
    if read != size {
        return Err(SealError::Open(format!(
            "size drift: stat reported {size} bytes but the file yielded {read}"
        )));
    }
    Ok(bytes)
}

/// Acquire the bytes of a `/proc/<pid>/fd/<n>` descriptor `source` — the frozen PR-3 execution
/// path. FOLLOW the procfs magic symlink (NOT `O_NOFOLLOW`: the path itself is a magic symlink,
/// and a universal no-follow would break the one executable contract PR-3 requires) into the
/// exact held object, require it to be a REGULAR object, and require it to be a FULLY SEALED
/// memfd (so a mutable file merely exposed through `/proc/<pid>/fd` cannot pose as an immutable
/// object), then read its exact bytes.
fn acquire_proc_fd(source: &Path) -> Result<Vec<u8>, SealError> {
    use rustix::fs::{self as rfs, Mode, OFlags};
    let fd = rfs::open(source, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty())
        .map_err(|e| SealError::Open(format!("proc-fd open failed: {e}")))?;
    let st = rfs::fstat(&fd).map_err(|e| SealError::Open(format!("fstat failed: {e}")))?;
    if (st.st_mode & 0o170_000) != 0o100_000 {
        return Err(SealError::Open(
            "proc-fd target is not a regular object".to_owned(),
        ));
    }
    // Require the full seal set: only an immutable sealed memfd may be acquired via /proc/fd.
    let got = fcntl(fd.as_raw_fd(), FcntlArg::F_GET_SEALS)
        .map_err(|_| SealError::NotSealed)
        .and_then(|g| {
            if g & required_seals().bits() == required_seals().bits() {
                Ok(())
            } else {
                Err(SealError::NotSealed)
            }
        });
    got?;
    read_capped(File::from(fd))
}

impl SealedObject {
    /// Acquire `source` (hardened) and stage it into a sealed memfd, requiring the sealed bytes'
    /// digest to match `expected`. An ordinary path is opened `O_NOFOLLOW`; a `/proc/<pid>/fd/<n>`
    /// path follows the magic symlink into the exact held sealed object.
    ///
    /// # Errors
    /// [`SealError`] on open/read failure, oversize, a sealing failure, or a digest mismatch.
    pub fn stage_from_path(source: &Path, expected: &Digest256) -> Result<Self, SealError> {
        let bytes = Self::acquire(source)?;
        Self::stage_from_bytes(&bytes, expected)
    }

    /// Acquire `source` (hardened) into a sealed memfd WITHOUT a pre-known digest — the digest is
    /// COMPUTED from the sealed bytes. Used for the target: the parent seals the exact object it
    /// will hand the backend, then binds the launch to `digest()`, closing the hash→exec TOCTOU.
    ///
    /// The failure is PHASE-TAGGED ([`TargetStageError`]) so the boundary can classify a SOURCE
    /// rejection (a fail-closed target refusal) apart from a worker STAGING failure (infrastructure),
    /// even when both surface the same underlying [`SealError`] variant (e.g. `NotSealed`).
    ///
    /// # Errors
    /// [`TargetStageError::SourceRejected`] if `source` is unacceptable (hardened open, oversize, or
    /// an input `/proc/fd` object that is not fully sealed); [`TargetStageError::StagingFailed`] if
    /// the worker's own memfd could not be created/sealed or its digest did not bind.
    pub(crate) fn stage(source: &Path) -> Result<Self, TargetStageError> {
        let bytes = Self::acquire(source).map_err(TargetStageError::SourceRejected)?;
        let digest = Digest256::of_bytes(&bytes);
        Self::stage_from_bytes(&bytes, &digest).map_err(TargetStageError::StagingFailed)
    }

    /// Hardened acquisition of `source`'s bytes: the `/proc/<pid>/fd/<n>` execution contract
    /// follows the magic symlink into the exact sealed object; every other path is an ordinary
    /// filesystem path opened `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC` and proven a regular file.
    fn acquire(source: &Path) -> Result<Vec<u8>, SealError> {
        if is_proc_fd_path(source) {
            acquire_proc_fd(source)
        } else {
            acquire_ordinary(source)
        }
    }

    /// Stage `bytes` directly into a sealed memfd (used when the caller already holds the exact
    /// bytes, e.g. an already-sealed PR-3 target read back once).
    ///
    /// # Errors
    /// [`SealError`] on a sealing failure or a digest mismatch.
    pub fn stage_from_bytes(bytes: &[u8], expected: &Digest256) -> Result<Self, SealError> {
        let name = CString::new("o7-sealed").map_err(|e| SealError::Seal(e.to_string()))?;
        // CLOEXEC so the sealed object does NOT leak into spawned children (the backend/target
        // reach it via `/proc/<owner_pid>/fd/<n>`, which resolves the OWNER's table regardless);
        // ALLOW_SEALING so we can seal it immutable.
        let owned = memfd_create(
            &name,
            MemFdCreateFlag::MFD_CLOEXEC | MemFdCreateFlag::MFD_ALLOW_SEALING,
        )
        .map_err(|e| SealError::Seal(e.to_string()))?;
        let mut mf = File::from(owned);
        mf.write_all(bytes)
            .map_err(|e| SealError::Seal(e.to_string()))?;
        let raw = mf.as_raw_fd();
        fcntl(raw, FcntlArg::F_ADD_SEALS(required_seals()))
            .map_err(|e| SealError::Seal(e.to_string()))?;
        let got = fcntl(raw, FcntlArg::F_GET_SEALS).map_err(|e| SealError::Seal(e.to_string()))?;
        if got & required_seals().bits() != required_seals().bits() {
            return Err(SealError::NotSealed);
        }
        // Read the SEALED bytes back and bind the digest to them.
        let back = std::fs::read(format!("/proc/self/fd/{raw}"))
            .map_err(|e| SealError::Seal(e.to_string()))?;
        let digest = Digest256::of_bytes(&back);
        if digest != *expected {
            return Err(SealError::DigestMismatch);
        }
        Ok(Self {
            fd: Arc::new(mf),
            digest,
        })
    }

    /// The exec/read path of the sealed object: `/proc/<owner_pid>/fd/<n>`.
    #[must_use]
    pub fn descriptor_path(&self) -> PathBuf {
        PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            self.fd.as_raw_fd()
        ))
    }

    /// The digest of the sealed bytes.
    #[must_use]
    pub fn digest(&self) -> &Digest256 {
        &self.digest
    }
}
