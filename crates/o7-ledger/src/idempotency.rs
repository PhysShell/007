//! Idempotency: a `(scope, key)` pair is bound to the digest of the request that
//! created it. A replay with the same digest returns the prior result; a replay
//! with the SAME key but a DIFFERENT digest is a conflict and changes nothing.
//! This is deliberately NOT `INSERT OR IGNORE`: two different requests must never
//! be collapsed into one identity.

use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use crate::LedgerError;

/// Idempotency scopes used in PR 1.
pub const SCOPE_CREATE_CONVERSATION: &str = "create-conversation";
pub const SCOPE_CREATE_RUN: &str = "create-run";
pub const SCOPE_APPEND_USER_MESSAGE: &str = "append-user-message";

/// Lowercase hex SHA-256 of the canonical request bytes.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        // Writing hex into a String is infallible; ignore the formatter Result
        // rather than unwrap (the tree forbids unwrap/expect/panic).
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Outcome of an idempotency check inside a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdemOutcome {
    /// No prior record — the caller should perform the operation and then
    /// [`record`] the result within the same transaction.
    Fresh,
    /// A prior record with a matching digest exists; its `result_reference` is
    /// returned and the operation must NOT run again.
    Replayed(String),
}

/// Check `(scope, key)` against `request_digest`.
///
/// # Errors
/// [`LedgerError::IdempotencyConflict`] if the key exists with a different
/// digest; SQLite errors are propagated.
pub fn check(
    tx: &Transaction<'_>,
    scope: &str,
    key: &str,
    request_digest: &str,
) -> Result<IdemOutcome, LedgerError> {
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT request_digest, result_reference FROM idempotency_record \
             WHERE scope = ?1 AND key = ?2",
            (scope, key),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    match existing {
        Some((stored_digest, result_reference)) => {
            if stored_digest == request_digest {
                Ok(IdemOutcome::Replayed(result_reference))
            } else {
                Err(LedgerError::IdempotencyConflict {
                    scope: scope.to_owned(),
                    key: key.to_owned(),
                })
            }
        }
        None => Ok(IdemOutcome::Fresh),
    }
}

/// Persist a fresh idempotency record within the caller's transaction.
///
/// # Errors
/// Propagates SQLite errors (including a primary-key clash, which would indicate
/// a caller bug — [`check`] must run first in the same transaction).
pub fn record(
    tx: &Transaction<'_>,
    scope: &str,
    key: &str,
    request_digest: &str,
    result_reference: &str,
    created_at: i64,
) -> Result<(), LedgerError> {
    tx.execute(
        "INSERT INTO idempotency_record \
         (scope, key, request_digest, result_reference, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        (scope, key, request_digest, result_reference, created_at),
    )?;
    Ok(())
}
