//! Registered derivations — provenance V1 §18, made re-executable.
//!
//! §18 requires a derived fact to *name* the source digests it came from, and
//! stops there. Naming is necessary and not sufficient: a citation nobody
//! follows is indistinguishable from a citation nobody could follow. So a
//! derivation here is the same shape as a matcher in `o7-closure-matcher` — an
//! identity pair resolving to exactly one pure function, whose bytes are hashed
//! so the pair cannot quietly come to mean something else.
//!
//! DELIBERATELY ONE ENTRY. `carries_finding` is the fact §18 names and the only
//! one any consumer needs today. This is a registry because that is the shape
//! the problem has, not a framework for derivations in general; a second entry
//! arrives when a second consumer does.

use o7_closure_canonical::digest_of_canonical_bytes;
use serde_json::Value;

/// A derivation: total, pure, and a function of the named sources only.
///
/// A bare `fn` pointer for the reason `MatcherFn` is one — it cannot capture an
/// environment, so its inputs are exactly what it is handed.
///
/// The `Value`s it is handed are **not** retained artifacts. Each is a view
/// materialised by `check_derived` out of one validated source, carrying
/// exactly the fields [`DerivationEntry::sources`] declares and nothing else.
/// See [`DerivationInput`] for why the parameter type did not change instead.
pub type DerivationFn = fn(sources: &[Value]) -> Option<Value>;

/// One field a derivation reads, under both of the names the contracts give it.
///
/// Redaction policy §7.5 records an asymmetry that is easy to read past: a
/// complete §8 projection is keyed in the CANONICAL vocabulary, a reduced source
/// record is keyed in §5.3's DECODED one, and the two are not the same set of
/// names. §8 of the same document then requires that a fact whose every input
/// survived redaction remain usable — which cannot be satisfied without knowing
/// that `/stableId` and `/id` are the same field.
///
/// §7.5 also refuses a global correspondence table, and it is right to: a third
/// mapping maintained alongside two contracts is a place for them to disagree
/// silently. So the correspondence is not global. Each derivation names the two
/// spellings of the fields IT reads, next to the rule that reads them, and
/// nothing else acquires a translation.
///
/// WHAT KEEPS THE DECLARATION HONEST, since it is not the hashed implementation
/// bytes. `check_derived` materialises the view from this declaration in BOTH
/// representations and hands the rule nothing else. A declaration that omits a
/// field the rule reads therefore breaks the complete-projection case as well as
/// the reduced one; a `canonical` name that is wrong breaks both; a `decoded`
/// name that is wrong breaks the reduced case. Each of the three is a
/// preregistered witness in `tests/correction_b4d.rs` and
/// `tests/derivation_source_view.rs`, so the declaration is checked by execution
/// rather than by review.
///
/// WHY NOT CHANGE `DerivationFn` TO TAKE THE SOURCES INSTEAD. §18's identity
/// rule: a registered derivation's file bytes ARE its identity, and a signature
/// change would force `review-carries-finding/2` for a change that alters no
/// behaviour of the rule. Keeping the rule reading a plain object, and making
/// the CALLER responsible for producing a faithful one, leaves the identity
/// where the contract puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivationInput {
    /// The §8 projection member, and the name the rule itself reads.
    pub canonical: &'static str,
    /// The §5.3 decoded source field, and therefore the `retainedFields` key a
    /// reduced record carries it under.
    pub decoded: &'static str,
}

/// One source slot of a derivation: the SURFACE it takes, and the fields it
/// reads out of that surface.
///
/// WHY THE KIND IS HERE AND NOT INFERRED. A slot was declared by field names
/// alone, and `check_derived` read those names out of whatever artifact was
/// cited. That makes the guard check a proxy — "the field is readable here" —
/// while the property anyone relies on is "this is the surface the rule is
/// about". The two came apart in both directions: `/stableId` is carried by
/// four of the five §8 projections, so an issue comment or a check run filled
/// `review-carries-finding`'s review slot and the fact was recomputed and
/// admitted; and `/pullRequestReviewId` is carried by exactly one, so the
/// comment slot happened to be bounded by a coincidence in a five-row table
/// rather than by a rule. `tests/correction_g1.rs` holds both directions.
///
/// THE EXPECTATION COMES FROM THE RULE, NEVER FROM THE ARTIFACT. This is the
/// registry — the contract side — and the artifact supplies only what it is.
/// A slot that asked the cited artifact what kind it ought to be would be
/// self-certification with extra steps, which is the shape this crate exists to
/// refuse.
///
/// ONE NAME FOR TWO MEMBERS. The kind is matched against `/sourceKind` for a
/// complete §8 projection and against `/locatorKind` for a §7 reduced record,
/// via [`o7_closure_provenance_artifact_surface`]. That is one field only
/// because §8's source kinds and §7.3's locator kinds are the same vocabulary —
/// an invariant `tests/correction_g1.rs` executes rather than assumes, because
/// if they ever drift this single field silently becomes a proxy for two
/// different properties and the defect above returns inside its own fix.
///
/// [`o7_closure_provenance_artifact_surface`]: crate::artifact::ArtifactKind::surface
#[derive(Debug, Clone, Copy)]
pub struct DerivationSource {
    /// The §8 `sourceKind` / §7.3 `locatorKind` this slot takes.
    pub kind: &'static str,
    /// The fields the rule reads out of it, under both spellings.
    pub inputs: &'static [DerivationInput],
}

/// One registered derivation.
#[derive(Debug, Clone, Copy)]
pub struct DerivationEntry {
    pub id: &'static str,
    pub version: &'static str,
    /// The sources the derivation consumes, in order: what surface each slot
    /// takes and what the rule reads out of it. Its length is the arity —
    /// declared once, so a rule cannot take a number of sources different from
    /// the number it declares slots for.
    pub sources: &'static [DerivationSource],
    /// The exact bytes of the file defining `derive`, embedded at compile time.
    pub implementation_source: &'static str,
    /// SHA-256 of [`Self::implementation_source`], the identity of
    /// `(id, version)`.
    pub implementation_digest: &'static str,
    pub derive: DerivationFn,
}

/// `carries_finding`: a submitted review carries a finding when a review comment
/// belongs to it.
///
/// Provenance V1 §18's own example, and the reason the rule exists: this value
/// is not a GitHub field, so an adapter that supplies it is asserting rather
/// than reporting.
mod carries_finding_v1;

pub const REGISTRY: &[DerivationEntry] = &[DerivationEntry {
    id: "review-carries-finding",
    version: "1",
    sources: &[
        // 0 — the submitted review. §8.2 `stableId`; §5.3 `/id`.
        DerivationSource {
            kind: "github-submitted-review",
            inputs: &[DerivationInput {
                canonical: "/stableId",
                decoded: "/id",
            }],
        },
        // 1 — the review comment. §8.4 `pullRequestReviewId`;
        // §5.3 `/pull_request_review_id`.
        DerivationSource {
            kind: "github-review-comment",
            inputs: &[DerivationInput {
                canonical: "/pullRequestReviewId",
                decoded: "/pull_request_review_id",
            }],
        },
    ],
    implementation_source: include_str!("derivations/carries_finding_v1.rs"),
    implementation_digest:
        "sha256:9990c9990875b746b0afcc3bda59538b34aeaaef87f9efeb98a03c5d9d7e902e",
    derive: carries_finding_v1::derive,
}];

impl DerivationEntry {
    /// How many source snapshots the derivation consumes, in order.
    ///
    /// Derived from [`Self::sources`] rather than declared beside it: two
    /// statements of one number are two chances to disagree, and the one that
    /// would have been wrong is the one nothing reads.
    #[must_use]
    pub const fn arity(&self) -> usize {
        self.sources.len()
    }
}

/// Resolve an identity pair to exactly one derivation. Fails closed.
#[must_use]
pub fn resolve(id: &str, version: &str) -> Option<&'static DerivationEntry> {
    REGISTRY
        .iter()
        .find(|entry| entry.id == id && entry.version == version)
}

/// The bytes of the file defining a derivation are the ones bound to its
/// `(id, version)`.
///
/// The same mechanism as `o7-closure-matcher`'s implementation binding, and the
/// same reason: `matcher.version` changes whenever behaviour changes for ANY
/// input, no finite vector set discharges "any", and every edit moves the file's
/// bytes. `include_str!` and `mod` read the same path in the same build, so the
/// hashed bytes are the code that runs.
///
/// WHAT THIS DOES NOT ESTABLISH, and it is the same residual Slice A closed for
/// matchers and this slice leaves open for derivations: the expected value lives
/// in this tree, two lines from the `include_str!` that supplies the bytes it
/// judges, so one commit can edit both. Slice A's answer was to record the
/// digest in the durable artifact instead, and no artifact carries a derivation
/// digest yet. Until one does, this check catches drift and not intent.
pub fn verify_implementation(entry: &DerivationEntry) -> Result<(), DerivationDrift> {
    let computed = digest_of_canonical_bytes(entry.implementation_source.as_bytes());
    if computed.as_str() == entry.implementation_digest {
        return Ok(());
    }
    Err(DerivationDrift {
        id: entry.id,
        version: entry.version,
        expected: entry.implementation_digest,
        computed: computed.as_str().to_owned(),
    })
}

/// A registered derivation is not the code bound to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationDrift {
    pub id: &'static str,
    pub version: &'static str,
    pub expected: &'static str,
    pub computed: String,
}

impl std::fmt::Display for DerivationDrift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "derivation {:?} version {:?} is not the implementation bound to it: the source \
             file hashes to {}, the version is bound to {}. A behaviour change needs a new \
             version, never a new digest on this one",
            self.id, self.version, self.computed, self.expected
        )
    }
}
