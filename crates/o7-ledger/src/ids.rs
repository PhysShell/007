//! Typed identifiers. Each is an opaque UUID string — the ledger never parses
//! meaning out of an id, it only stores and compares them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Mint a fresh random (UUIDv4) identifier.
            #[must_use]
            pub fn generate() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Borrow the underlying string form.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Wrap an already-existing id string (e.g. read back from the DB).
            #[must_use]
            pub fn from_raw(raw: String) -> Self {
                Self(raw)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_owned()))
            }
        }
    };
}

id_type!(
    /// Identifies a conversation — the top-level container and the unit the
    /// event `sequence` is scoped to.
    ConversationId
);
id_type!(
    /// Identifies a run (a unit of agent work within a conversation).
    RunId
);
id_type!(
    /// Identifies one attempt of a run.
    AttemptId
);
id_type!(
    /// Identifies a single event.
    EventId
);
