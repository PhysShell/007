//! The §5.4 rows that step 1 of `docs/tasks/a1-v0.md` can already make
//! executable.
//!
//! §5.4 opens with "Happy path is not acceptance" and freezes the required
//! outcome of every row. The full matrix is step 8; these are the rows whose
//! subject is the wire seed itself — encoding, versions, unknown fields, null
//! policy, per-object bounds, and framed identity. Each test names the row it
//! discharges.
//!
//! Rows deliberately **not** here, because their subject does not exist yet:
//! anything needing closure traversal (step 2), the reducer and `state_version`
//! (step 3), receipt congruence (step 4), or a provider (step 5). A test that
//! asserted them now would be asserting against a stub.

// Test fixtures are valid by construction; a fixture that is not should fail
// loudly rather than be papered over with a substituted value. Same allowance
// as `crates/o7-worktree/tests/stateroot_temp.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_a1_contracts::bounds::{
    MAX_ARTIFACT_REFS, MAX_CONTROL_ARTIFACT_BYTES, MAX_REACHABLE_CLOSURE_BYTES,
};
use o7_a1_contracts::{
    parse_artifact, typed_media_type, validate_document, AdapterVersion, ArtifactKindV1,
    ArtifactRef, ArtifactRefs, BudgetPolicy, BudgetPolicyError, CommitId, EnvelopeError,
    EnvelopeV1, EnvelopeVersion, Id, MessageKindV1, MessageKindVersion, ModelIdentity, Optional,
    ParseError, ProducerRole, Timestamp, WireDigest,
};

fn id(s: &str) -> Id {
    Id::parse(s).expect("fixture id")
}

fn digest(seed: &str) -> WireDigest {
    WireDigest::of_bytes(seed.as_bytes())
}

fn adapter(s: &str) -> AdapterVersion {
    AdapterVersion::parse(s).expect("fixture adapter version")
}

fn model(s: &str) -> ModelIdentity {
    ModelIdentity::parse(s).expect("fixture model identity")
}

fn receipt_ref() -> ArtifactRef {
    ArtifactRef {
        kind: ArtifactKindV1::ProviderExecutionReceipt,
        media_type: typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
        digest: digest("receipt"),
        size: 512,
    }
}

/// A controller-produced envelope: no provider fields, no receipt ref.
fn controller_envelope() -> EnvelopeV1 {
    EnvelopeV1 {
        envelope_version: EnvelopeVersion::default(),
        message_kind: MessageKindV1::WorkOrder,
        message_kind_version: MessageKindVersion::default(),
        message_id: id("msg-1"),
        root_goal_id: id("goal-1"),
        task_id: id("task-1"),
        campaign_id: id("camp-1"),
        round_id: Optional::present(id("round-1")),
        causation_id: Optional::absent(),
        correlation_id: id("corr-1"),
        producer_role: ProducerRole::Controller,
        producer_execution_id: id("exec-controller-1"),
        producer_adapter_version: adapter("o7-controller/0.1.0"),
        model_identity: Optional::absent(),
        prompt_digest: Optional::absent(),
        tool_policy_digest: Optional::absent(),
        contract_digest: digest("contract"),
        expected_input_head: CommitId::parse(&"a".repeat(40)).ok().into(),
        payload_digest: WireDigest::of_bytes(b"{}"),
        artifact_refs: ArtifactRefs::empty(),
        provider_execution_receipt_ref: Optional::absent(),
        created_at: Timestamp::parse("2026-08-09T20:00:00Z").expect("fixture timestamp"),
    }
}

/// A coder-produced envelope: provider fields and a receipt ref, per §3.0.
fn coder_envelope() -> EnvelopeV1 {
    EnvelopeV1 {
        message_kind: MessageKindV1::CoderReport,
        producer_role: ProducerRole::Coder,
        producer_execution_id: id("exec-coder-1"),
        model_identity: Optional::present(model("claude-opus-5")),
        prompt_digest: Optional::present(digest("prompt")),
        tool_policy_digest: Optional::present(digest("tool-policy")),
        provider_execution_receipt_ref: Optional::present(receipt_ref()),
        ..controller_envelope()
    }
}

// ---------------------------------------------------------------------------
// Row: unsupported envelope_version / message_kind_version
//      "refused, never best-effort parsed (FD-1.6)"
// ---------------------------------------------------------------------------

// The version is a type, so an unsupported one has no in-memory
// representation: the only way to attempt it is over the wire, which is also
// the only way it ever arrives.
#[test]
fn an_unsupported_envelope_version_is_refused() {
    let bytes = envelope_json_with("envelope_version", serde_json::json!(2));
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    // ...and through every other door as well.
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());
}

#[test]
fn an_unsupported_message_kind_version_is_refused() {
    let bytes = envelope_json_with("message_kind_version", serde_json::json!(7));
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());
}

/// A serialized controller envelope with one member replaced.
fn envelope_json_with(key: &str, value: serde_json::Value) -> Vec<u8> {
    let mut v = serde_json::to_value(controller_envelope()).expect("fixture must serialize");
    if let Some(map) = v.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
    v.to_string().into_bytes()
}

// ---------------------------------------------------------------------------
// Row: unknown field present — "rejected at parse time (FD-1.6)"
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_envelope_field_is_rejected_at_parse_time() {
    let mut value = serde_json::to_value(controller_envelope()).expect("fixture must serialize");
    if let Some(map) = value.as_object_mut() {
        map.insert("shadow_authority".to_owned(), serde_json::json!(true));
    }
    let bytes = value.to_string().into_bytes();
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES),
        Err(ParseError::SchemaMismatch { .. })
    ));
}

#[test]
fn an_unrecognised_enum_variant_fails_closed() {
    // FD-1.6: no `#[serde(other)]` catch-all anywhere, so a future kind is
    // refused rather than absorbed into a "we did not understand this" variant.
    let mut value = serde_json::to_value(controller_envelope()).expect("fixture must serialize");
    if let Some(map) = value.as_object_mut() {
        map.insert("message_kind".to_owned(), serde_json::json!("future_kind"));
    }
    let bytes = value.to_string().into_bytes();
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
}

// ---------------------------------------------------------------------------
// Row: explicit JSON null in an optional field — "rejected (FD-1.3)"
// ---------------------------------------------------------------------------

#[test]
fn an_explicit_null_in_an_optional_field_is_rejected() {
    // The field is genuinely optional — absent parses fine — so this is the
    // null itself being refused, not a missing required value.
    let mut value = serde_json::to_value(controller_envelope()).expect("fixture must serialize");
    if let Some(map) = value.as_object_mut() {
        map.insert("causation_id".to_owned(), serde_json::Value::Null);
    }
    let bytes = value.to_string().into_bytes();
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES),
        Err(ParseError::ExplicitNull { .. })
    ));
}

#[test]
fn the_same_optional_field_absent_is_accepted() {
    let env = controller_envelope();
    let bytes = serde_json::to_vec(&env).expect("fixture must serialize");
    assert!(!String::from_utf8_lossy(&bytes).contains("causation_id"));
    let parsed: EnvelopeV1 =
        parse_artifact(&bytes, MAX_CONTROL_ARTIFACT_BYTES).expect("absent optional must parse");
    assert_eq!(parsed, env);
}

// ---------------------------------------------------------------------------
// Row: malformed CoderReport / ReviewerReport
//      "parse rejection, no acceptance, no dispatch"
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_payload_is_rejected() {
    for bad in [
        &b"not json at all"[..],
        &b"{"[..],
        &b"[]"[..],
        &b"{} trailing"[..],
        &[0xff, 0xfe][..],
    ] {
        assert!(
            validate_document(bad, MAX_CONTROL_ARTIFACT_BYTES).is_err(),
            "expected rejection for {bad:?}"
        );
    }
}

#[test]
fn a_leading_bom_is_refused_rather_than_stripped() {
    // Stripping would mean the payload digest names bytes nobody parsed.
    assert_eq!(
        validate_document("\u{feff}{}".as_bytes(), MAX_CONTROL_ARTIFACT_BYTES),
        Err(ParseError::LeadingBom)
    );
}

// ---------------------------------------------------------------------------
// Row: payload exceeding a per-object bound
//      "rejected, never truncated (FD-1.4)"
// ---------------------------------------------------------------------------

#[test]
fn a_payload_over_its_bound_is_rejected_not_truncated() {
    let oversized = format!(r#"{{"a":"{}"}}"#, "x".repeat(64)).into_bytes();
    let err = validate_document(&oversized, 16);
    assert!(matches!(
        err,
        Err(ParseError::PayloadTooLarge { max: 16, .. })
    ));
    // Nothing partial came back: the failure carries no value at all.
    assert!(err.is_err());
}

#[test]
fn too_many_artifact_refs_are_rejected() {
    let env = controller_envelope();
    let too_many: Vec<ArtifactRef> = (0..=MAX_ARTIFACT_REFS)
        .map(|i| ArtifactRef {
            kind: ArtifactKindV1::Diff,
            media_type: "text/x-diff".to_owned(),
            digest: digest(&format!("blob-{i}")),
            size: 1,
        })
        .collect();
    // The bound is on the type, so an over-long list cannot even be built.
    assert!(ArtifactRefs::new(too_many.clone()).is_err());
    let within: Vec<ArtifactRef> = too_many.iter().take(MAX_ARTIFACT_REFS).cloned().collect();
    assert!(ArtifactRefs::new(within).is_ok());

    // ...and it is refused on the wire, including through the direct door.
    let mut value = serde_json::to_value(env).expect("fixture must serialize");
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "artifact_refs".to_owned(),
            serde_json::to_value(&too_many).expect("refs must serialize"),
        );
    }
    let bytes = value.to_string().into_bytes();
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());
}

// ---------------------------------------------------------------------------
// Row: campaign policy budget above the protocol hard maximum
//      "refused at campaign creation (FD-1.5)"
// ---------------------------------------------------------------------------

#[test]
fn a_campaign_budget_above_the_hard_maximum_is_refused() {
    let policy = BudgetPolicy {
        max_provider_turns: 8,
        max_wall_time_seconds: 3600,
        evidence_budget_bytes: MAX_REACHABLE_CLOSURE_BYTES + 1,
        closure_object_budget: 512,
    };
    assert!(matches!(
        policy.validate(),
        Err(BudgetPolicyError::EvidenceBudgetTooLarge { .. })
    ));
}

// ---------------------------------------------------------------------------
// Rows: provider-produced artifact with no receipt ref — "rejected (§3.0)"
//       controller-derived artifact carrying a receipt ref — "rejected (FD-11)"
// ---------------------------------------------------------------------------

#[test]
fn a_provider_artifact_without_a_receipt_ref_is_rejected() {
    let mut env = coder_envelope();
    env.provider_execution_receipt_ref = Optional::absent();
    assert!(matches!(
        env.validate(),
        Err(EnvelopeError::MissingProviderField {
            field: "provider_execution_receipt_ref",
            ..
        })
    ));
}

#[test]
fn a_controller_artifact_carrying_a_receipt_ref_is_rejected() {
    let mut env = controller_envelope();
    env.provider_execution_receipt_ref = Optional::present(receipt_ref());
    assert!(matches!(
        env.validate(),
        Err(EnvelopeError::ForbiddenProviderField {
            field: "provider_execution_receipt_ref",
            ..
        })
    ));
}

#[test]
fn a_controller_artifact_carrying_provider_evidence_is_rejected() {
    let mut env = controller_envelope();
    env.model_identity = Optional::present(model("claude-opus-5"));
    assert!(matches!(
        env.validate(),
        Err(EnvelopeError::ForbiddenProviderField {
            field: "model_identity",
            ..
        })
    ));
}

#[test]
fn well_formed_envelopes_of_both_shapes_validate() {
    assert_eq!(controller_envelope().validate(), Ok(()));
    assert_eq!(coder_envelope().validate(), Ok(()));
}

// ---------------------------------------------------------------------------
// Framed identity (FD-1.2) — the basis on which FD-6 later distinguishes a
// replay from an IdConflict. The reducer rows themselves are step 3; what is
// testable now is that the identity actually discriminates.
// ---------------------------------------------------------------------------

#[test]
fn the_framed_digest_is_stable_and_excludes_created_at() {
    let env = controller_envelope();
    let before = env.framed_digest();
    let mut redelivered = env.clone();
    redelivered.created_at = Timestamp::parse("2099-01-01T00:00:00Z").expect("fixture timestamp");
    // §5.4: "redelivery with a different created_at | replay". A clock
    // disagreeing must not change what an artifact *is*.
    assert_eq!(before, redelivered.framed_digest());
}

#[test]
fn a_different_expected_input_head_is_a_different_identity() {
    // §5.4: "same payload bytes, different expected_input_head | IdConflict,
    // not a replay". The reducer decides that; the digest has to make it
    // *possible* to decide.
    let env = controller_envelope();
    let mut moved = env.clone();
    moved.expected_input_head = CommitId::parse(&"b".repeat(40)).ok().into();
    assert_eq!(env.payload_digest, moved.payload_digest);
    assert_ne!(env.framed_digest(), moved.framed_digest());
}

#[test]
fn every_framed_field_changes_the_identity() {
    let base = controller_envelope();
    let d = base.framed_digest();

    let mut kind = base.clone();
    kind.message_kind = MessageKindV1::CoderReport;
    assert_ne!(d, kind.framed_digest(), "message_kind");

    let mut msg = base.clone();
    msg.message_id = id("msg-2");
    assert_ne!(d, msg.framed_digest(), "message_id");

    let mut role = base.clone();
    role.producer_role = ProducerRole::Human;
    assert_ne!(d, role.framed_digest(), "producer_role");

    let mut payload = base.clone();
    payload.payload_digest = WireDigest::of_bytes(b"other");
    assert_ne!(d, payload.framed_digest(), "payload_digest");

    let mut contract = base.clone();
    contract.contract_digest = digest("other-contract");
    assert_ne!(d, contract.framed_digest(), "contract_digest");

    let mut round = base.clone();
    round.round_id = Optional::absent();
    assert_ne!(d, round.framed_digest(), "round_id absent vs present");

    let mut refs = base.clone();
    refs.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::Diff,
        media_type: "text/x-diff".to_owned(),
        digest: digest("blob"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    assert_ne!(d, refs.framed_digest(), "artifact_refs");
}

#[test]
fn a_ref_under_a_different_declared_media_type_is_a_different_reference() {
    // FD-1.7 / FD-2.5: "the same bytes under a different declared type are a
    // different reference".
    let mut a = controller_envelope();
    a.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::Diff,
        media_type: "text/x-diff".to_owned(),
        digest: digest("blob"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    let mut b = a.clone();
    b.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::Diff,
        media_type: "application/octet-stream".to_owned(),
        digest: digest("blob"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    assert_ne!(a.framed_digest(), b.framed_digest());
}

#[test]
fn the_envelope_binds_the_exact_stored_payload_bytes() {
    let mut env = controller_envelope();
    env.payload_digest = WireDigest::of_bytes(br#"{"status":"candidate_produced"}"#);
    assert!(env.binds_payload(br#"{"status":"candidate_produced"}"#));
    // FD-1.1: bytes are the artifact. A re-serialized payload with different
    // whitespace is a *different* payload, not an equivalent one.
    assert!(!env.binds_payload(br#"{ "status": "candidate_produced" }"#));
}

#[test]
fn a_ref_to_an_envelope_bearing_artifact_charges_both_halves() {
    // FD-1.8: size = stored envelope bytes + stored payload bytes.
    assert_eq!(EnvelopeV1::ref_size(400, 1024), 1424);
}

// ---------------------------------------------------------------------------
// Regressions for the review of b2ba165. Each of these passed the old test
// suite while the defect was live, which is the more useful thing to record.
// ---------------------------------------------------------------------------

/// The admission path must run the cross-field rules, not only the schema.
///
/// No single field of this envelope is wrong — a provider report with no receipt
/// ref is refused only by a rule that reads `producer_role` and
/// `provider_execution_receipt_ref` together. There is deliberately no public
/// helper that stops before that rule: the earlier `parse_payload` was one, and
/// a comment recommending the safer sibling is not an admission boundary.
#[test]
fn the_admission_path_enforces_cross_field_rules() {
    let mut env = coder_envelope();
    env.provider_execution_receipt_ref = Optional::absent();
    let bytes = serde_json::to_vec(&env).expect("fixture must serialize");

    // The cross-field rule now runs *inside* deserialization, so it surfaces as
    // a schema mismatch rather than a separate post-parse verdict — earlier,
    // and on every route rather than only this one.
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());

    // The same envelope with its receipt ref restored is admissible, so the
    // rejection above is the cross-field rule and not some unrelated defect in
    // the fixture.
    assert!(parse_artifact::<EnvelopeV1>(
        &serde_json::to_vec(&coder_envelope()).expect("fixture must serialize"),
        MAX_CONTROL_ARTIFACT_BYTES
    )
    .is_ok());
}

/// Reaching the typed schema through a different door must not weaken it.
/// `serde_json::from_str` bypasses `validate_document` by construction, so every
/// per-field rule has to live in the field's own type.
#[test]
fn raw_deserialization_cannot_bypass_the_per_field_rules() {
    // Explicit null in a genuinely optional field.
    let nulled = envelope_json_with("causation_id", serde_json::Value::Null);
    assert!(serde_json::from_slice::<EnvelopeV1>(&nulled).is_err());

    // A version the contract froze.
    let versioned = envelope_json_with("envelope_version", serde_json::json!(2));
    assert!(serde_json::from_slice::<EnvelopeV1>(&versioned).is_err());

    // A schema-specific text bound: §3.0 caps producer_adapter_version at 128
    // bytes, well under the global 65536-byte string bound.
    let long_adapter = envelope_json_with(
        "producer_adapter_version",
        serde_json::json!("x".repeat(129)),
    );
    assert!(serde_json::from_slice::<EnvelopeV1>(&long_adapter).is_err());

    // And model_identity at 256.
    let mut provider = serde_json::to_value(coder_envelope()).expect("fixture must serialize");
    if let Some(map) = provider.as_object_mut() {
        map.insert(
            "model_identity".to_owned(),
            serde_json::json!("x".repeat(257)),
        );
    }
    assert!(serde_json::from_str::<EnvelopeV1>(&provider.to_string()).is_err());

    // An unknown field, and an unrecognised enum variant.
    let unknown = envelope_json_with("shadow_authority", serde_json::json!(true));
    assert!(serde_json::from_slice::<EnvelopeV1>(&unknown).is_err());
    let future = envelope_json_with("message_kind", serde_json::json!("future_kind"));
    assert!(serde_json::from_slice::<EnvelopeV1>(&future).is_err());
}

/// A rejected artifact is untrusted input. AGENTS.md makes credential leakage a
/// P0, so no rejection may quote what it refused.
#[test]
fn a_rejected_artifact_never_appears_in_the_error() {
    const SECRET: &str = "ghp_secret_that_must_not_reach_a_log";

    let mut messages: Vec<String> = Vec::new();

    let unknown_field = envelope_json_with(SECRET, serde_json::json!(SECRET));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&unknown_field, MAX_CONTROL_ARTIFACT_BYTES) {
        messages.push(e.to_string());
    }

    let bad_variant = envelope_json_with("message_kind", serde_json::json!(SECRET));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&bad_variant, MAX_CONTROL_ARTIFACT_BYTES) {
        messages.push(e.to_string());
    }

    let bad_scalar = envelope_json_with("message_id", serde_json::json!(SECRET.repeat(20)));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&bad_scalar, MAX_CONTROL_ARTIFACT_BYTES) {
        messages.push(e.to_string());
    }

    let nulled_under_secret_key = format!(r#"{{"{SECRET}":null}}"#).into_bytes();
    if let Err(e) = validate_document(&nulled_under_secret_key, MAX_CONTROL_ARTIFACT_BYTES) {
        messages.push(e.to_string());
    }

    assert_eq!(messages.len(), 4, "every case must actually be rejected");
    for message in messages {
        assert!(!message.contains("ghp_"), "error leaked content: {message}");
    }
}

/// A typed non-envelope A1 payload is bounded as a control artifact, not as an
/// evidence blob. Previously a ref could declare a 64 MiB execution receipt and
/// pass validation.
#[test]
fn a_typed_non_envelope_ref_is_bounded_as_a_control_artifact() {
    let oversized = ArtifactRef {
        kind: ArtifactKindV1::ProviderExecutionReceipt,
        media_type: typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
        digest: digest("receipt"),
        size: MAX_CONTROL_ARTIFACT_BYTES + 1,
    };
    assert!(oversized.validate().is_err());

    let mut env = coder_envelope();
    env.provider_execution_receipt_ref = Optional::present(oversized);
    assert!(matches!(
        env.validate(),
        Err(EnvelopeError::BadReceiptRef { .. })
    ));
}

// ---------------------------------------------------------------------------
// Regressions for the second review round, on 2617370.
// ---------------------------------------------------------------------------

/// P0. `Digest256`'s own `Deserialize` forwards an error whose `Display` quotes
/// the rejected string, so a malformed digest on the wire put payload content
/// into a serde error — reachable directly, without the admission path.
#[test]
fn a_malformed_digest_never_quotes_itself_on_any_path() {
    const SECRET: &str = "ghp_not_a_digest_but_definitely_a_secret";

    let bytes = envelope_json_with("contract_digest", serde_json::json!(SECRET));

    let admitted = parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!admitted.is_empty(), "a malformed digest must be rejected");
    assert!(
        !admitted.contains("ghp_"),
        "admission path leaked: {admitted}"
    );

    // The direct door is the one that was leaking.
    let direct = serde_json::from_slice::<EnvelopeV1>(&bytes)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!direct.is_empty(), "a malformed digest must be rejected");
    assert!(
        !direct.contains("ghp_"),
        "direct deserialization leaked: {direct}"
    );

    // And a well-formed digest still parses, so the rejection above is the
    // shape check rather than the field being unusable.
    let good = envelope_json_with(
        "contract_digest",
        serde_json::json!(WireDigest::of_bytes(b"contract").as_str()),
    );
    assert!(parse_artifact::<EnvelopeV1>(&good, MAX_CONTROL_ARTIFACT_BYTES).is_ok());
}

/// FD-1.7: a typed A1 artifact's media type is fixed by its kind and version,
/// and media type is part of the envelope framing — so a ref declaring
/// `text/x-diff` for a `work_order` names a different reference than the
/// artifact it claims to point at.
#[test]
fn a_typed_ref_must_declare_its_frozen_media_type() {
    let mut env = controller_envelope();
    env.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::WorkOrder,
        media_type: "text/x-diff".to_owned(),
        digest: digest("order"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    let bytes = serde_json::to_vec(&env).expect("fixture must serialize");
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());

    // The same ref with its frozen media type is admissible.
    env.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::WorkOrder,
        media_type: typed_media_type(ArtifactKindV1::WorkOrder, 1),
        digest: digest("order"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    let bytes = serde_json::to_vec(&env).expect("fixture must serialize");
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_ok());

    // An evidence blob keeps its own concrete type: FD-1.7 constrains typed A1
    // artifacts, not blobs.
    env.artifact_refs = ArtifactRefs::new(vec![ArtifactRef {
        kind: ArtifactKindV1::Diff,
        media_type: "text/x-diff".to_owned(),
        digest: digest("blob"),
        size: 10,
    }])
    .expect("one ref is within the bound");
    let bytes = serde_json::to_vec(&env).expect("fixture must serialize");
    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_ok());
}

/// The private wire form must stay field-identical to the public one.
///
/// A field on `EnvelopeV1` but not on `EnvelopeWireV1` fails here as a missing
/// field; a field on the wire form but not the public one fails as an unknown
/// field, because both carry `deny_unknown_fields`. Drift between the two is the
/// standing risk of splitting a struct in half to get a validating
/// `Deserialize`, so it is tested rather than trusted.
#[test]
fn the_public_and_wire_envelope_forms_do_not_drift() {
    for env in [controller_envelope(), coder_envelope()] {
        let bytes = serde_json::to_vec(&env).expect("fixture must serialize");
        let back: EnvelopeV1 =
            serde_json::from_slice(&bytes).expect("a serialized envelope must deserialize");
        assert_eq!(back, env);
        assert_eq!(back.framed_digest(), env.framed_digest());
    }
}

/// Every route from bytes to an `EnvelopeV1` enforces the cross-field rules —
/// including the two serde entry points no admission function can intercept.
#[test]
fn no_deserialization_route_admits_a_provider_envelope_without_its_receipt() {
    let mut env = coder_envelope();
    env.provider_execution_receipt_ref = Optional::absent();
    let value = serde_json::to_value(&env).expect("fixture must serialize");
    let bytes = value.to_string().into_bytes();

    assert!(parse_artifact::<EnvelopeV1>(&bytes, MAX_CONTROL_ARTIFACT_BYTES).is_err());
    assert!(serde_json::from_slice::<EnvelopeV1>(&bytes).is_err());
    assert!(serde_json::from_value::<EnvelopeV1>(value).is_err());
}

/// FD-1.9 classifies `interaction_manifest` as a typed non-envelope object, so
/// FD-1.7 fixes its media type. Its 64 MiB size classification is a separate
/// question and does not make it an evidence blob.
#[test]
fn an_interaction_manifest_is_typed_for_media_type_and_large_for_size() {
    let wrong = ArtifactRef {
        kind: ArtifactKindV1::InteractionManifest,
        media_type: "application/octet-stream".to_owned(),
        digest: digest("manifest"),
        size: 1,
    };
    assert!(wrong.validate().is_err());

    let right = ArtifactRef {
        kind: ArtifactKindV1::InteractionManifest,
        media_type: typed_media_type(ArtifactKindV1::InteractionManifest, 1),
        digest: digest("manifest"),
        // Larger than a control artifact, which is the point of the separate
        // size classification.
        size: MAX_CONTROL_ARTIFACT_BYTES * 4,
    };
    assert!(right.validate().is_ok());
}

/// `ArtifactRef` had the same shape of hole `EnvelopeV1` did: its rules relate
/// `kind` to `media_type` and `size`, so a derived `Deserialize` admitted a
/// reference no validator would.
#[test]
fn a_reference_cannot_be_deserialized_past_its_own_rules() {
    let zero_sized = r#"{"kind":"diff","media_type":"text/x-diff","digest":"0000000000000000000000000000000000000000000000000000000000000000","size":0}"#;
    assert!(serde_json::from_str::<ArtifactRef>(zero_sized).is_err());

    let wrong_media = r#"{"kind":"work_order","media_type":"text/x-diff","digest":"0000000000000000000000000000000000000000000000000000000000000000","size":1}"#;
    assert!(serde_json::from_str::<ArtifactRef>(wrong_media).is_err());

    let ok = format!(
        r#"{{"kind":"work_order","media_type":"{}","digest":"{}","size":1}}"#,
        typed_media_type(ArtifactKindV1::WorkOrder, 1),
        digest("order").as_str()
    );
    assert!(serde_json::from_str::<ArtifactRef>(&ok).is_ok());
}

/// FD-1.4 wants a parse-time *rejection*, not a report filed after the memory
/// is spent: an oversized array must not be fully materialised before its cap is
/// checked.
#[test]
fn an_oversized_reference_array_is_refused_without_materialising_it() {
    // Far beyond both MAX_ARTIFACT_REFS and the global array bound. If the cap
    // were checked after collecting, this would allocate every entry first.
    let one = format!(
        r#"{{"kind":"diff","media_type":"text/x-diff","digest":"{}","size":1}}"#,
        digest("blob").as_str()
    );
    let many = vec![one.as_str(); 50_000].join(",");
    let json = format!(r#"[{many}]"#);

    assert!(serde_json::from_str::<ArtifactRefs>(&json).is_err());
    assert!(ArtifactRefs::new(Vec::new()).is_ok());
}

/// The overflow element must cost nothing to refuse.
///
/// The earlier version deserialized the 257th entry as a full `ArtifactRef`
/// before comparing against the cap, so a single oversized `media_type` on that
/// entry was allocated anyway. A bound that allocates what it is about to reject
/// is not a bound.
#[test]
fn the_element_past_the_cap_is_never_materialised() {
    let valid = format!(
        r#"{{"kind":"diff","media_type":"text/x-diff","digest":"{}","size":1}}"#,
        digest("blob").as_str()
    );
    let mut entries = vec![valid; MAX_ARTIFACT_REFS];
    // The overflowing entry carries a large string and is also individually
    // invalid; neither should be reached.
    entries.push(format!(
        r#"{{"kind":"diff","media_type":"{}","digest":"{}","size":0}}"#,
        "x".repeat(60_000),
        digest("blob").as_str()
    ));
    let json = format!("[{}]", entries.join(","));

    assert!(serde_json::from_str::<ArtifactRefs>(&json).is_err());

    // Exactly at the cap is still admissible, so the rejection above is the
    // overflow and not the batch size.
    let at_cap_entries: Vec<String> = entries.iter().take(MAX_ARTIFACT_REFS).cloned().collect();
    let at_cap = format!("[{}]", at_cap_entries.join(","));
    assert!(serde_json::from_str::<ArtifactRefs>(&at_cap).is_ok());
}
