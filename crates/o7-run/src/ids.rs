//! Opaque identifiers for the canonical run-event protocol.
//!
//! These mirror the ledger's id discipline (opaque strings the reducer never parses
//! meaning out of) but carry NO generation dependency: ids are minted by the effectful
//! integration layer and flow INTO the pure reducer, never out of it.
//!
//! Values deserialized from storage are UNTRUSTED: an empty id is rejected on
//! deserialization (and by [`IdError`]-returning constructors), so a blank identifier
//! cannot slip in as a valid one. `from_raw` is the trusted, in-process constructor used
//! by upstream minting and tests.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A rejected identifier (currently: empty).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} identifier: must be non-empty")]
pub struct IdError {
    pub kind: &'static str,
}

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        pub struct $name(String);

        impl $name {
            /// Construct a validated id (untrusted input path).
            ///
            /// # Errors
            /// [`IdError`] if the value is empty.
            pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
                let raw = raw.into();
                if raw.is_empty() {
                    Err(IdError { kind: $kind })
                } else {
                    Ok(Self(raw))
                }
            }

            /// Borrow the underlying string form.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Wrap an already-existing id string minted upstream (trusted, in-process).
            /// The pure core never invents an id.
            #[must_use]
            pub fn from_raw(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let raw = String::deserialize(deserializer)?;
                Self::new(raw).map_err(serde::de::Error::custom)
            }
        }
    };
}

id_type!(
    /// Identifies a run — the unit the canonical event `sequence` is scoped to and the
    /// unit a verdict is reduced for.
    RunId,
    "run"
);
id_type!(
    /// Identifies a single canonical run event.
    RunEventId,
    "run-event"
);
id_type!(
    /// Identifies a gate (a required-or-optional obligation whose outcome the reducer folds
    /// into the verdict).
    GateId,
    "gate"
);
