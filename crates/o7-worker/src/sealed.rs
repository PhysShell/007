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

/// A held, sealed (immutable) copy of an object's bytes.
#[derive(Debug, Clone)]
pub struct SealedObject {
    fd: Arc<File>,
    digest: Digest256,
}

impl SealedObject {
    /// Open `source`, read its bytes THROUGH the held fd (bounded), stage them into a sealed
    /// memfd, verify the seals, read them back, and require the digest to match `expected`.
    ///
    /// # Errors
    /// [`SealError`] on open/read failure, oversize, a sealing failure, or a digest mismatch.
    pub fn stage_from_path(source: &Path, expected: &Digest256) -> Result<Self, SealError> {
        let src = File::open(source).map_err(|e| SealError::Open(e.to_string()))?;
        // Read back THROUGH the held fd, not the path — the bytes we stage are the ones we hold.
        let held = format!("/proc/self/fd/{}", src.as_raw_fd());
        let mut f = File::open(&held).map_err(|e| SealError::Open(e.to_string()))?;
        let mut bytes = Vec::new();
        let mut limited = std::io::Read::take(&mut f, MAX_OBJECT_BYTES as u64 + 1);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| SealError::Open(e.to_string()))?;
        if bytes.len() > MAX_OBJECT_BYTES {
            return Err(SealError::TooLarge);
        }
        Self::stage_from_bytes(&bytes, expected)
    }

    /// Acquire `source` into a sealed memfd WITHOUT a pre-known digest — the digest is COMPUTED
    /// from the sealed bytes. Used for the target: the parent seals the exact object it will
    /// hand the backend, then binds the launch to `digest()`, closing the hash→exec TOCTOU.
    ///
    /// # Errors
    /// [`SealError`] on open/read failure, oversize, or a sealing failure.
    pub fn stage(source: &Path) -> Result<Self, SealError> {
        let src = File::open(source).map_err(|e| SealError::Open(e.to_string()))?;
        let held = format!("/proc/self/fd/{}", src.as_raw_fd());
        let mut f = File::open(&held).map_err(|e| SealError::Open(e.to_string()))?;
        let mut bytes = Vec::new();
        let mut limited = std::io::Read::take(&mut f, MAX_OBJECT_BYTES as u64 + 1);
        limited
            .read_to_end(&mut bytes)
            .map_err(|e| SealError::Open(e.to_string()))?;
        if bytes.len() > MAX_OBJECT_BYTES {
            return Err(SealError::TooLarge);
        }
        let digest = Digest256::of_bytes(&bytes);
        Self::stage_from_bytes(&bytes, &digest)
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
