//! Opaque identifiers for the canonical run-event protocol.
//!
//! These mirror the ledger's id discipline (opaque strings the reducer never parses
//! meaning out of) but carry NO generation dependency: ids are minted by the effectful
//! integration layer and flow INTO the pure reducer, never out of it. Keeping the pure
//! core free of `uuid`/persistence deps is what lets run truth be recomputed anywhere.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Borrow the underlying string form.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Wrap an already-existing id string (minted upstream, or read back from
            /// the ledger). The pure core never invents an id.
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
    };
}

id_type!(
    /// Identifies a run — the unit the canonical event `sequence` is scoped to and the
    /// unit a verdict is reduced for.
    RunId
);
id_type!(
    /// Identifies a single canonical run event.
    RunEventId
);
id_type!(
    /// Identifies a gate (a required-or-optional obligation, e.g. a build/test/lint
    /// check) whose outcome the reducer folds into the verdict.
    GateId
);
