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

use o7_a1_contracts::{
    parse_artifact, typed_media_type, AdapterVersion, ArtifactKindV1, ArtifactRef, ArtifactRefs,
    BudgetPolicy, BudgetPolicyError, CommitId, EnvelopeError, EnvelopeFieldsV1, EnvelopeV1,
    EnvelopeVersion, Id, MessageKindV1, MessageKindVersion, ModelIdentity, Optional, ParseError,
    ProducerRole, RefError, Timestamp, WireDigest,
};
use o7_a1_contracts::{
    MAX_ARRAY_LEN, MAX_ARTIFACT_REFS, MAX_CONTROL_ARTIFACT_BYTES, MAX_EVIDENCE_BLOB_BYTES,
    MAX_REACHABLE_CLOSURE_BYTES,
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
    ArtifactRef::new(
        ArtifactKindV1::ProviderExecutionReceipt,
        typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
        digest("receipt"),
        512,
    )
    .expect("the receipt-ref fixture must be admissible")
}

/// The controller shape as editable, unauthorised fields.
fn controller_fields() -> EnvelopeFieldsV1 {
    EnvelopeFieldsV1 {
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

/// A controller-produced envelope: no provider fields, no receipt ref.
fn controller_envelope() -> EnvelopeV1 {
    EnvelopeV1::new(controller_fields()).expect("the controller fixture must be admissible")
}

/// The coder shape as editable, unauthorised fields.
fn coder_fields() -> EnvelopeFieldsV1 {
    EnvelopeFieldsV1 {
        message_kind: MessageKindV1::CoderReport,
        producer_role: ProducerRole::Coder,
        producer_execution_id: id("exec-coder-1"),
        model_identity: Optional::present(model("claude-opus-5")),
        prompt_digest: Optional::present(digest("prompt")),
        tool_policy_digest: Optional::present(digest("tool-policy")),
        provider_execution_receipt_ref: Optional::present(receipt_ref()),
        ..controller_fields()
    }
}

/// Edit an admitted envelope the only way there is: take its fields, change
/// them, and construct again. The result is admitted or it does not exist.
fn rebuilt(base: &EnvelopeV1, edit: impl FnOnce(&mut EnvelopeFieldsV1)) -> EnvelopeV1 {
    let mut fields = base.to_fields();
    edit(&mut fields);
    EnvelopeV1::new(fields).expect("this edit must keep the envelope admissible")
}

/// A coder-produced envelope: provider fields and a receipt ref, per §3.0.
fn coder_envelope() -> EnvelopeV1 {
    EnvelopeV1::new(coder_fields()).expect("the coder fixture must be admissible")
}

/// The checked producer encoding — the only route from an admitted envelope to
/// bytes, and therefore the only honest place for a wire test to start.
fn wire_bytes(env: &EnvelopeV1) -> Vec<u8> {
    env.to_wire_bytes()
        .expect("an admitted envelope encodes under its own ceiling")
}

/// Those bytes as a document, so a negative test can vary **the input** — one
/// member of an otherwise conforming envelope — rather than relax a bound or
/// reach the wire by a route no production producer has.
///
/// Six times on this PR a test reached the layer under test through a weaker
/// route and recorded the bypass instead of closing it. The rule this helper
/// exists to keep: however a regression test arrives at the state before the
/// check, it arrives the way a producer would.
fn wire_document(env: &EnvelopeV1) -> serde_json::Value {
    serde_json::from_slice(&wire_bytes(env)).expect("the producer encoding is a JSON document")
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
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
    // ...and through every other door as well.
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
}

#[test]
fn an_unsupported_message_kind_version_is_refused() {
    let bytes = envelope_json_with("message_kind_version", serde_json::json!(7));
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
}

/// A serialized controller envelope with one member replaced.
fn envelope_json_with(key: &str, value: serde_json::Value) -> Vec<u8> {
    let mut v = wire_document(&controller_envelope());
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
    let mut value = wire_document(&controller_envelope());
    if let Some(map) = value.as_object_mut() {
        map.insert("shadow_authority".to_owned(), serde_json::json!(true));
    }
    let bytes = value.to_string().into_bytes();
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&bytes),
        Err(ParseError::SchemaMismatch { .. })
    ));
}

#[test]
fn an_unrecognised_enum_variant_fails_closed() {
    // FD-1.6: no `#[serde(other)]` catch-all anywhere, so a future kind is
    // refused rather than absorbed into a "we did not understand this" variant.
    let mut value = wire_document(&controller_envelope());
    if let Some(map) = value.as_object_mut() {
        map.insert("message_kind".to_owned(), serde_json::json!("future_kind"));
    }
    let bytes = value.to_string().into_bytes();
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
}

// ---------------------------------------------------------------------------
// Row: explicit JSON null in an optional field — "rejected (FD-1.3)"
// ---------------------------------------------------------------------------

#[test]
fn an_explicit_null_in_an_optional_field_is_rejected() {
    // The field is genuinely optional — absent parses fine — so this is the
    // null itself being refused, not a missing required value.
    let mut value = wire_document(&controller_envelope());
    if let Some(map) = value.as_object_mut() {
        map.insert("causation_id".to_owned(), serde_json::Value::Null);
    }
    let bytes = value.to_string().into_bytes();
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&bytes),
        Err(ParseError::ExplicitNull { .. })
    ));
}

#[test]
fn the_same_optional_field_absent_is_accepted() {
    let env = controller_envelope();
    let bytes = wire_bytes(&env);
    assert!(!String::from_utf8_lossy(&bytes).contains("causation_id"));
    let parsed: EnvelopeV1 = parse_artifact(&bytes).expect("absent optional must parse");
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
            parse_artifact::<EnvelopeV1>(bad).is_err(),
            "expected rejection for {bad:?}"
        );
    }
}

#[test]
fn a_leading_bom_is_refused_rather_than_stripped() {
    // Stripping would mean the payload digest names bytes nobody parsed.
    let mut bytes = "\u{feff}".as_bytes().to_vec();
    bytes.extend(wire_bytes(&controller_envelope()));
    assert_eq!(
        parse_artifact::<EnvelopeV1>(&bytes),
        Err(ParseError::LeadingBom)
    );
}

// ---------------------------------------------------------------------------
// Row: payload exceeding a per-object bound
//      "rejected, never truncated (FD-1.4)"
// ---------------------------------------------------------------------------

#[test]
fn a_payload_over_its_bound_is_rejected_not_truncated() {
    // The ceiling is `EnvelopeV1::MAX_BYTES`, not a caller's argument, so the
    // document has to actually exceed the protocol maximum.
    let oversized = format!(
        r#"{{"a":"{}"}}"#,
        "x".repeat(MAX_CONTROL_ARTIFACT_BYTES as usize + 1)
    )
    .into_bytes();
    let err = parse_artifact::<EnvelopeV1>(&oversized);
    assert!(matches!(
        err,
        Err(ParseError::PayloadTooLarge {
            max: MAX_CONTROL_ARTIFACT_BYTES,
            ..
        })
    ));
    // Nothing partial came back: the failure carries no value at all.
    assert!(err.is_err());
}

#[test]
fn too_many_artifact_refs_are_rejected() {
    let env = controller_envelope();
    let too_many: Vec<ArtifactRef> = (0..=MAX_ARTIFACT_REFS)
        .map(|i| {
            ArtifactRef::new(
                ArtifactKindV1::Diff,
                "text/x-diff",
                digest(&format!("blob-{i}")),
                1,
            )
            .expect("each ref is individually admissible")
        })
        .collect();
    // The bound is on the type, so an over-long list cannot even be built.
    assert!(ArtifactRefs::new(too_many.clone()).is_err());
    let within: Vec<ArtifactRef> = too_many.iter().take(MAX_ARTIFACT_REFS).cloned().collect();
    assert!(ArtifactRefs::new(within).is_ok());

    // ...and it is refused on the wire, including through the direct door.
    let mut value = wire_document(&env);
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "artifact_refs".to_owned(),
            serde_json::to_value(&too_many).expect("refs must serialize"),
        );
    }
    let bytes = value.to_string().into_bytes();
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
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
    let fields = EnvelopeFieldsV1 {
        provider_execution_receipt_ref: Optional::absent(),
        ..coder_fields()
    };
    assert!(matches!(
        EnvelopeV1::new(fields),
        Err(EnvelopeError::MissingProviderField {
            field: "provider_execution_receipt_ref",
            ..
        })
    ));
}

#[test]
fn a_controller_artifact_carrying_a_receipt_ref_is_rejected() {
    let fields = EnvelopeFieldsV1 {
        provider_execution_receipt_ref: Optional::present(receipt_ref()),
        ..controller_fields()
    };
    assert!(matches!(
        EnvelopeV1::new(fields),
        Err(EnvelopeError::ForbiddenProviderField {
            field: "provider_execution_receipt_ref",
            ..
        })
    ));
}

#[test]
fn a_controller_artifact_carrying_provider_evidence_is_rejected() {
    let fields = EnvelopeFieldsV1 {
        model_identity: Optional::present(model("claude-opus-5")),
        ..controller_fields()
    };
    assert!(matches!(
        EnvelopeV1::new(fields),
        Err(EnvelopeError::ForbiddenProviderField {
            field: "model_identity",
            ..
        })
    ));
}

#[test]
fn well_formed_envelopes_of_both_shapes_validate() {
    assert!(EnvelopeV1::new(controller_fields()).is_ok());
    assert!(EnvelopeV1::new(coder_fields()).is_ok());
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
    let redelivered = EnvelopeV1::new(EnvelopeFieldsV1 {
        created_at: Timestamp::parse("2099-01-01T00:00:00Z").expect("fixture timestamp"),
        ..env.to_fields()
    })
    .expect("a different created_at is still admissible");
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
    let moved = rebuilt(&env, |f| {
        f.expected_input_head = CommitId::parse(&"b".repeat(40)).ok().into();
    });
    assert_eq!(env.payload_digest(), moved.payload_digest());
    assert_ne!(env.framed_digest(), moved.framed_digest());
}

#[test]
fn every_framed_field_changes_the_identity() {
    let base = controller_envelope();
    let d = base.framed_digest();

    // Another kind the controller also authors (§3.4), so this varies the kind
    // and nothing else.
    let kind = rebuilt(&base, |f| f.message_kind = MessageKindV1::ReviewRequest);
    assert_ne!(d, kind.framed_digest(), "message_kind");

    let msg = rebuilt(&base, |f| f.message_id = id("msg-2"));
    assert_ne!(d, msg.framed_digest(), "message_id");

    // `producer_role` is framed too, and it is no longer independently variable
    // on an admitted envelope: §3.1–§3.11 give each kind exactly one author, so
    // the pair moves together or the envelope does not exist. A role-only edit
    // is therefore asserted as *inadmissible* rather than as a different
    // identity — the direction rule's own test covers the whole class.
    assert!(matches!(
        EnvelopeV1::new(EnvelopeFieldsV1 {
            producer_role: ProducerRole::Human,
            ..base.to_fields()
        }),
        Err(EnvelopeError::WrongProducerRole { .. })
    ));
    let authored = rebuilt(&base, |f| {
        f.message_kind = MessageKindV1::CoderReport;
        f.producer_role = ProducerRole::Coder;
        f.model_identity = Optional::present(model("claude-opus-5"));
        f.prompt_digest = Optional::present(digest("prompt"));
        f.tool_policy_digest = Optional::present(digest("tool-policy"));
        f.provider_execution_receipt_ref = Optional::present(receipt_ref());
    });
    assert_ne!(d, authored.framed_digest(), "message_kind + producer_role");

    let payload = rebuilt(&base, |f| f.payload_digest = WireDigest::of_bytes(b"other"));
    assert_ne!(d, payload.framed_digest(), "payload_digest");

    let contract = rebuilt(&base, |f| f.contract_digest = digest("other-contract"));
    assert_ne!(d, contract.framed_digest(), "contract_digest");

    let round = rebuilt(&base, |f| f.round_id = Optional::absent());
    assert_ne!(d, round.framed_digest(), "round_id absent vs present");

    let refs = rebuilt(&base, |f| {
        f.artifact_refs = ArtifactRefs::new(vec![ArtifactRef::new(
            ArtifactKindV1::Diff,
            "text/x-diff",
            digest("blob"),
            10,
        )
        .expect("admissible")])
        .expect("one ref is within the bound");
    });
    assert_ne!(d, refs.framed_digest(), "artifact_refs");
}

#[test]
fn a_ref_under_a_different_declared_media_type_is_a_different_reference() {
    // FD-1.7 / FD-2.5: "the same bytes under a different declared type are a
    // different reference".
    let with_media = |m: &str| {
        rebuilt(&controller_envelope(), |f| {
            f.artifact_refs = ArtifactRefs::new(vec![ArtifactRef::new(
                ArtifactKindV1::Diff,
                m,
                digest("blob"),
                10,
            )
            .expect("a well-formed evidence ref")])
            .expect("one ref is within the bound");
        })
    };
    let a = with_media("text/x-diff");
    let b = with_media("application/octet-stream");
    assert_ne!(a.framed_digest(), b.framed_digest());
}

#[test]
fn the_envelope_binds_the_exact_stored_payload_bytes() {
    let env = rebuilt(&controller_envelope(), |f| {
        f.payload_digest = WireDigest::of_bytes(br#"{"status":"candidate_produced"}"#);
    });
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
    // The invalid envelope cannot be built as a value any more — that is the
    // point of `EnvelopeV1::new`. So the *document* is built instead, by
    // removing the receipt ref from an admissible one, which is what a
    // non-conforming peer would actually send.
    let mut value = wire_document(&coder_envelope());
    if let Some(map) = value.as_object_mut() {
        map.remove("provider_execution_receipt_ref");
    }
    let bytes = value.to_string().into_bytes();

    // The cross-field rule now runs *inside* deserialization, so it surfaces as
    // a schema mismatch rather than a separate post-parse verdict — earlier,
    // and on every route rather than only this one.
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());

    // The same envelope with its receipt ref restored is admissible, so the
    // rejection above is the cross-field rule and not some unrelated defect in
    // the fixture.
    assert!(parse_artifact::<EnvelopeV1>(&wire_bytes(&coder_envelope())).is_ok());
}

/// Reaching the typed schema through a different door must not weaken it.
/// `serde_json::from_str` bypasses the admission path by construction, so every
/// per-field rule has to live in the field's own type.
#[test]
fn raw_deserialization_cannot_bypass_the_per_field_rules() {
    // Explicit null in a genuinely optional field.
    let nulled = envelope_json_with("causation_id", serde_json::Value::Null);
    assert!(parse_artifact::<EnvelopeV1>(&nulled).is_err());

    // A version the contract froze.
    let versioned = envelope_json_with("envelope_version", serde_json::json!(2));
    assert!(parse_artifact::<EnvelopeV1>(&versioned).is_err());

    // A schema-specific text bound: §3.0 caps producer_adapter_version at 128
    // bytes, well under the global 65536-byte string bound.
    let long_adapter = envelope_json_with(
        "producer_adapter_version",
        serde_json::json!("x".repeat(129)),
    );
    assert!(parse_artifact::<EnvelopeV1>(&long_adapter).is_err());

    // And model_identity at 256.
    let mut provider = wire_document(&coder_envelope());
    if let Some(map) = provider.as_object_mut() {
        map.insert(
            "model_identity".to_owned(),
            serde_json::json!("x".repeat(257)),
        );
    }
    assert!(parse_artifact::<EnvelopeV1>(provider.to_string().as_bytes()).is_err());

    // An unknown field, and an unrecognised enum variant.
    let unknown = envelope_json_with("shadow_authority", serde_json::json!(true));
    assert!(parse_artifact::<EnvelopeV1>(&unknown).is_err());
    let future = envelope_json_with("message_kind", serde_json::json!("future_kind"));
    assert!(parse_artifact::<EnvelopeV1>(&future).is_err());
}

/// A rejected artifact is untrusted input. AGENTS.md makes credential leakage a
/// P0, so no rejection may quote what it refused.
#[test]
fn a_rejected_artifact_never_appears_in_the_error() {
    const SECRET: &str = "ghp_secret_that_must_not_reach_a_log";

    let mut messages: Vec<String> = Vec::new();

    let unknown_field = envelope_json_with(SECRET, serde_json::json!(SECRET));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&unknown_field) {
        messages.push(e.to_string());
    }

    let bad_variant = envelope_json_with("message_kind", serde_json::json!(SECRET));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&bad_variant) {
        messages.push(e.to_string());
    }

    let bad_scalar = envelope_json_with("message_id", serde_json::json!(SECRET.repeat(20)));
    if let Err(e) = parse_artifact::<EnvelopeV1>(&bad_scalar) {
        messages.push(e.to_string());
    }

    let nulled_under_secret_key = format!(r#"{{"{SECRET}":null}}"#).into_bytes();
    if let Err(e) = parse_artifact::<EnvelopeV1>(&nulled_under_secret_key) {
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
    // The ref itself is now unconstructible above its bound, which is a
    // stronger statement than "a validator rejects it".
    assert!(matches!(
        ArtifactRef::new(
            ArtifactKindV1::ProviderExecutionReceipt,
            typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
            digest("receipt"),
            MAX_CONTROL_ARTIFACT_BYTES + 1,
        ),
        Err(RefError::SizeAboveBound { .. })
    ));

    // And the same over-bound receipt on the wire is refused by the envelope's
    // own cross-field pass, which is the route a peer actually takes.
    let mut value = wire_document(&coder_envelope());
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "provider_execution_receipt_ref".to_owned(),
            serde_json::json!({
                "kind": "provider_execution_receipt",
                "media_type": typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
                "digest": digest("receipt").as_str(),
                "size": MAX_CONTROL_ARTIFACT_BYTES + 1,
            }),
        );
    }
    assert!(parse_artifact::<EnvelopeV1>(&value.to_string().into_bytes()).is_err());
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

    let admitted = parse_artifact::<EnvelopeV1>(&bytes)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(!admitted.is_empty(), "a malformed digest must be rejected");
    assert!(
        !admitted.contains("ghp_"),
        "admission path leaked: {admitted}"
    );

    // The direct door is the one that was leaking.
    let direct = parse_artifact::<EnvelopeV1>(&bytes)
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
    assert!(parse_artifact::<EnvelopeV1>(&good).is_ok());
}

/// FD-1.7: a typed A1 artifact's media type is fixed by its kind and version,
/// and media type is part of the envelope framing — so a ref declaring
/// `text/x-diff` for a `work_order` names a different reference than the
/// artifact it claims to point at.
#[test]
fn a_typed_ref_must_declare_its_frozen_media_type() {
    // A typed ref under the wrong media type is unconstructible, so the
    // document is built directly — the shape a non-conforming peer sends.
    assert!(ArtifactRef::new(
        ArtifactKindV1::WorkOrder,
        "text/x-diff",
        digest("order"),
        10
    )
    .is_err());
    let with_ref = |media: String| {
        let mut v = wire_document(&controller_envelope());
        if let Some(map) = v.as_object_mut() {
            map.insert(
                "artifact_refs".to_owned(),
                serde_json::json!([{
                    "kind": "work_order",
                    "media_type": media,
                    "digest": digest("order").as_str(),
                    "size": 10,
                }]),
            );
        }
        v.to_string().into_bytes()
    };
    let bytes = with_ref("text/x-diff".to_owned());
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());

    // The same ref with its frozen media type is admissible.
    let bytes = with_ref(typed_media_type(ArtifactKindV1::WorkOrder, 1));
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_ok());

    // An evidence blob keeps its own concrete type: FD-1.7 constrains typed A1
    // artifacts, not blobs.
    let blob = rebuilt(&controller_envelope(), |f| {
        f.artifact_refs = ArtifactRefs::new(vec![ArtifactRef::new(
            ArtifactKindV1::Diff,
            "text/x-diff",
            digest("blob"),
            10,
        )
        .expect("an evidence blob keeps its own concrete type")])
        .expect("one ref is within the bound");
    });
    let bytes = wire_bytes(&blob);
    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_ok());
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
        let bytes = wire_bytes(&env);
        let back: EnvelopeV1 =
            parse_artifact(&bytes).expect("a serialized envelope must be admissible");
        assert_eq!(back, env);
        assert_eq!(back.framed_digest(), env.framed_digest());
    }
}

/// There is exactly one route from bytes to an `EnvelopeV1`, and it enforces
/// the cross-field rules.
///
/// The earlier form of this test asserted the same thing about *three* serde
/// entry points, because `EnvelopeV1` implemented `Deserialize` and each door
/// had to be checked. Those doors are gone: `serde_json::from_slice::<EnvelopeV1>`
/// no longer compiles. A rule enforced by the absence of an API needs no test
/// to keep it true, which is why this one now only has to check the rule.
#[test]
fn the_only_admission_route_rejects_a_provider_envelope_without_its_receipt() {
    // The invalid envelope cannot be built as a value any more — that is the
    // point of `EnvelopeV1::new`. So the *document* is built instead, by
    // removing the receipt ref from an admissible one, which is what a
    // non-conforming peer would actually send.
    let mut value = wire_document(&coder_envelope());
    if let Some(map) = value.as_object_mut() {
        map.remove("provider_execution_receipt_ref");
    }
    let bytes = value.to_string().into_bytes();

    assert!(parse_artifact::<EnvelopeV1>(&bytes).is_err());

    // The same envelope with its receipt restored is admissible, so the
    // rejection is the cross-field rule and not a broken fixture.
    let good = wire_bytes(&coder_envelope());
    assert!(parse_artifact::<EnvelopeV1>(&good).is_ok());
}

/// FD-1.9 classifies `interaction_manifest` as a typed non-envelope object, so
/// FD-1.7 fixes its media type. Its 64 MiB size classification is a separate
/// question and does not make it an evidence blob.
#[test]
fn an_interaction_manifest_is_typed_for_media_type_and_large_for_size() {
    assert!(ArtifactRef::new(
        ArtifactKindV1::InteractionManifest,
        "application/octet-stream",
        digest("manifest"),
        1,
    )
    .is_err());

    let manifest_of = |size| {
        ArtifactRef::new(
            ArtifactKindV1::InteractionManifest,
            typed_media_type(ArtifactKindV1::InteractionManifest, 1),
            digest("manifest"),
            size,
        )
    };

    // Larger than a control artifact, which is the point of the separate size
    // classification.
    assert!(manifest_of(MAX_CONTROL_ARTIFACT_BYTES * 4).is_ok());

    // S1 fixed the exact edge, so the edge is what gets asserted. Before the
    // supersede this pair would have pinned a reading rather than the contract.
    assert!(manifest_of(MAX_EVIDENCE_BLOB_BYTES).is_ok());
    assert!(manifest_of(MAX_EVIDENCE_BLOB_BYTES + 1).is_err());
}

/// `ArtifactRef` had the same shape of hole `EnvelopeV1` did, and it is closed
/// the same way: the type does not deserialize at all, so a ref reaches memory
/// only inside an artifact admitted through `parse_artifact`.
#[test]
fn a_reference_cannot_reach_an_envelope_past_its_own_rules() {
    let with_ref = |r: serde_json::Value| {
        let mut v = wire_document(&controller_envelope());
        if let Some(map) = v.as_object_mut() {
            map.insert("artifact_refs".to_owned(), serde_json::json!([r]));
        }
        v.to_string().into_bytes()
    };

    // A typed kind declaring an evidence-blob media type (FD-1.7).
    let wrong_media = with_ref(serde_json::json!({
        "kind": "work_order",
        "media_type": "text/x-diff",
        "digest": digest("order").as_str(),
        "size": 1
    }));
    assert!(parse_artifact::<EnvelopeV1>(&wrong_media).is_err());

    // An unknown member on the ref itself.
    let unknown = with_ref(serde_json::json!({
        "kind": "diff",
        "media_type": "text/x-diff",
        "digest": digest("d").as_str(),
        "size": 1,
        "path": "/etc/passwd"
    }));
    assert!(parse_artifact::<EnvelopeV1>(&unknown).is_err());

    // And the well-formed one is admitted, including at size 0 — FD-1.8 states
    // no lower bound, and an empty diff is a real object with a real digest.
    let ok = with_ref(serde_json::json!({
        "kind": "diff",
        "media_type": "text/x-diff",
        "digest": digest("empty").as_str(),
        "size": 0
    }));
    assert!(parse_artifact::<EnvelopeV1>(&ok).is_ok());
}

/// An envelope carrying a raw `artifact_refs` array, so the array rules can be
/// exercised on the only route that exists.
fn envelope_with_raw_refs(refs_json: &str) -> Vec<u8> {
    let v = wire_document(&controller_envelope());
    let mut text = v.to_string();
    let marker = r#""artifact_refs":[]"#;
    let at = text
        .find(marker)
        .expect("fixture carries an empty refs array");
    text.replace_range(
        at..at + marker.len(),
        &format!(r#""artifact_refs":{refs_json}"#),
    );
    text.into_bytes()
}

fn one_ref_json() -> String {
    format!(
        r#"{{"kind":"diff","media_type":"text/x-diff","digest":"{}","size":1}}"#,
        digest("blob").as_str()
    )
}

/// FD-1.4 wants a parse-time *rejection*, not a report filed after the memory
/// is spent: an oversized array must not be fully materialised before its cap is
/// checked.
///
/// Two layers can refuse an over-long array, and each is exercised where it is
/// the one that fires.
#[test]
fn an_oversized_reference_array_is_refused_without_materialising_it() {
    // The global FD-1.4 array bound applies to *any* array. 4097 tiny values
    // is ~20 KB, well inside the 1 MiB ceiling, so the document walk is what
    // refuses it — before the typed schema sees a single field.
    let tiny = (0..=MAX_ARRAY_LEN)
        .map(|_| "0")
        .collect::<Vec<_>>()
        .join(",");
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&envelope_with_raw_refs(&format!("[{tiny}]"))),
        Err(ParseError::ArrayTooLong { .. })
    ));

    // 50,000 real refs is past the byte ceiling as well, and *that* fires
    // first: the cheapest layer that can refuse a document does.
    let many = vec![one_ref_json(); 50_000].join(",");
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&envelope_with_raw_refs(&format!("[{many}]"))),
        Err(ParseError::PayloadTooLarge { .. })
    ));
}

/// The overflow element must cost nothing to refuse.
///
/// The earlier version deserialized the 257th entry as a full `ArtifactRef`
/// before comparing against the cap, so a single oversized `media_type` on that
/// entry was allocated anyway. A bound that allocates what it is about to reject
/// is not a bound.
///
/// 257 entries sits under the global 4096 array bound, so the document walk
/// passes and `BoundedVec`'s own cap is what fires — which is the layer under
/// test.
#[test]
fn the_element_past_the_cap_is_never_materialised() {
    let mut entries = vec![one_ref_json(); MAX_ARTIFACT_REFS];
    // The overflowing entry carries a large string and is also individually
    // invalid; neither should be reached.
    entries.push(format!(
        r#"{{"kind":"diff","media_type":"{}","digest":"{}","size":1,"extra":true}}"#,
        "x".repeat(60_000),
        digest("blob").as_str()
    ));
    let over = envelope_with_raw_refs(&format!("[{}]", entries.join(",")));
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&over),
        Err(ParseError::SchemaMismatch { .. })
    ));

    // Exactly at the cap is still admissible, so the rejection above is the
    // overflow and not the batch size.
    let at_cap = envelope_with_raw_refs(&format!(
        "[{}]",
        entries
            .iter()
            .take(MAX_ARTIFACT_REFS)
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    ));
    assert!(parse_artifact::<EnvelopeV1>(&at_cap).is_ok());
}

/// Regression for the two findings that shared a door.
///
/// This envelope is individually flawless: 256 refs is exactly the FD-1.4 limit,
/// every ref is valid, every media type is under the string bound, and the
/// cross-field rules pass. It is 15 MB. The only rule it breaks is the byte
/// bound — a property of the byte string that **no field of the value can
/// know** — so the earlier `serde_json::from_slice::<EnvelopeV1>` returned `Ok`
/// on it, 15× over a 1 MiB ceiling.
///
/// That door also returned `serde_json`'s own error, whose `Display` quotes the
/// unknown field name and the unrecognised enum value verbatim (AGENTS.md P0).
/// One reviewer found each half; both were the same public `Deserialize`.
///
/// The door is gone: `serde_json::from_slice::<EnvelopeV1>(&bytes)` does not
/// compile, and neither does it for `ArtifactRef` or `ArtifactRefs`. That is not
/// assertable at runtime — the compiler is the assertion — so what this test
/// pins is the rule the door was bypassing.
#[test]
fn an_oversized_but_otherwise_valid_envelope_is_refused_by_the_byte_bound() {
    let refs: Vec<ArtifactRef> = (0..MAX_ARTIFACT_REFS)
        .map(|i| {
            ArtifactRef::new(
                ArtifactKindV1::Diff,
                format!("text/x-diff;p={}", "a".repeat(60_000)),
                digest(&format!("blob-{i}")),
                1,
            )
            .expect("each ref is individually admissible")
        })
        .collect();
    let fields = EnvelopeFieldsV1 {
        artifact_refs: ArtifactRefs::new(refs).expect("exactly at the ref-count bound"),
        ..controller_fields()
    };

    // Every field is admissible and every cross-field rule passes; the only
    // rule this breaks is the FD-1.4 byte ceiling, which no field can see.
    // The producer-side constructor enforces it too, so `new` and
    // `parse_artifact` agree on what "admitted" means — before this, a
    // producer could mint a 15 MB envelope that every conforming peer would
    // refuse, and hold it as valid until someone else said no.
    assert!(matches!(
        EnvelopeV1::new(fields),
        Err(EnvelopeError::AboveByteCeiling { .. })
    ));

    // The same document on the wire is refused by the admission path.
    let mut value = wire_document(&controller_envelope());
    let one = serde_json::json!({
        "kind": "diff",
        "media_type": format!("text/x-diff;p={}", "a".repeat(60_000)),
        "digest": digest("blob").as_str(),
        "size": 1,
    });
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "artifact_refs".to_owned(),
            serde_json::Value::Array(vec![one.clone(); MAX_ARTIFACT_REFS]),
        );
    }
    let bytes = value.to_string().into_bytes();
    assert!(bytes.len() as u64 > MAX_CONTROL_ARTIFACT_BYTES * 14);
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(&bytes),
        Err(ParseError::PayloadTooLarge { .. })
    ));

    // The rejection is the byte bound and not some other rule: the same
    // construction with few enough refs to fit under the ceiling is admitted
    // on both routes.
    let small = rebuilt(&controller_envelope(), |f| {
        f.artifact_refs = ArtifactRefs::new(vec![ArtifactRef::new(
            ArtifactKindV1::Diff,
            format!("text/x-diff;p={}", "a".repeat(60_000)),
            digest("blob-0"),
            1,
        )
        .expect("admissible")])
        .expect("one ref");
    });
    let small = wire_bytes(&small);
    assert!(parse_artifact::<EnvelopeV1>(&small).is_ok());
}

/// Admission is a **postcondition**, and it survives editing.
///
/// The defect this closes: `EnvelopeV1` had public fields, so
/// `env.producer_role = Coder` on an envelope that had just been admitted
/// produced an admitted-typed value violating its own cross-field contract.
/// Admission was closed at the front and open at the back.
///
/// The role flip is the reported case, and it is deliberately not the only one
/// here. Every field the cross-field rule reads is exercised in both
/// directions, because a fix aimed at the one witness in hand is how the same
/// defect came back three times on this PR.
#[test]
fn no_edit_can_turn_an_admitted_envelope_into_an_invalid_one() {
    let controller = controller_envelope();
    let coder = coder_envelope();

    // The reported witness: an admitted controller envelope re-roled to a
    // provider without acquiring any provider evidence.
    assert!(matches!(
        EnvelopeV1::new(EnvelopeFieldsV1 {
            producer_role: ProducerRole::Coder,
            ..controller.to_fields()
        }),
        Err(EnvelopeError::MissingProviderField { .. })
    ));

    // ...and the mirror: a provider envelope re-roled to controller while
    // keeping the evidence that only a provider may carry.
    assert!(matches!(
        EnvelopeV1::new(EnvelopeFieldsV1 {
            producer_role: ProducerRole::Controller,
            ..coder.to_fields()
        }),
        Err(EnvelopeError::ForbiddenProviderField { .. })
    ));

    // Each provider-only field, added to a controller envelope one at a time.
    type Edit = (&'static str, fn(&mut EnvelopeFieldsV1));
    let additions: [Edit; 4] = [
        ("model_identity", |f| {
            f.model_identity = Optional::present(model("claude-opus-5"));
        }),
        ("prompt_digest", |f| {
            f.prompt_digest = Optional::present(digest("prompt"));
        }),
        ("tool_policy_digest", |f| {
            f.tool_policy_digest = Optional::present(digest("tool-policy"));
        }),
        ("provider_execution_receipt_ref", |f| {
            f.provider_execution_receipt_ref = Optional::present(receipt_ref());
        }),
    ];
    for (name, add) in additions {
        let mut fields = controller.to_fields();
        add(&mut fields);
        assert!(
            matches!(
                EnvelopeV1::new(fields),
                Err(EnvelopeError::ForbiddenProviderField { .. })
            ),
            "a controller envelope must not acquire {name}"
        );
    }

    // Each provider-only field, removed from a provider envelope one at a time.
    let removals: [Edit; 4] = [
        ("model_identity", |f| f.model_identity = Optional::absent()),
        ("prompt_digest", |f| f.prompt_digest = Optional::absent()),
        ("tool_policy_digest", |f| {
            f.tool_policy_digest = Optional::absent();
        }),
        ("provider_execution_receipt_ref", |f| {
            f.provider_execution_receipt_ref = Optional::absent();
        }),
    ];
    for (name, remove) in removals {
        let mut fields = coder.to_fields();
        remove(&mut fields);
        assert!(
            matches!(
                EnvelopeV1::new(fields),
                Err(EnvelopeError::MissingProviderField { .. })
            ),
            "a provider envelope must not lose {name}"
        );
    }

    // An edit that keeps the contract satisfied still succeeds, so the rule
    // above is the cross-field check and not a constructor that refuses
    // everything.
    assert!(EnvelopeV1::new(EnvelopeFieldsV1 {
        message_id: id("msg-edited"),
        ..controller.to_fields()
    })
    .is_ok());
}

// ---------------------------------------------------------------------------
// Regressions for the seventh review round, on b339d54. Two P1s, both of the
// same family: a rule held on one route and not on another.
// ---------------------------------------------------------------------------

/// §3.1–§3.11, transcribed from the section headings rather than from the
/// implementation.
///
/// Each heading fixes a direction — `### 3.1 WorkOrderV1 (Controller → Coder,
/// rank 5)`, `### 3.5 ReviewerReportV1 (Reviewer → Controller, untrusted,
/// rank 3)`, `### 3.3 CandidateReceiptV1 (controller-derived, ...)` — and the
/// author is the left-hand side. This is the crate's expectation of the
/// contract, held at the boundary where the rule is enforced; the crate's own
/// mapping is private and is never consulted to build it.
const SECTION_3_AUTHORS: [(MessageKindV1, ProducerRole); 11] = [
    (MessageKindV1::WorkOrder, ProducerRole::Controller), // §3.1 Controller → Coder
    (MessageKindV1::CoderReport, ProducerRole::Coder),    // §3.2 Coder → Controller
    (MessageKindV1::CandidateReceipt, ProducerRole::Controller), // §3.3 controller-derived
    (MessageKindV1::ReviewRequest, ProducerRole::Controller), // §3.4 Controller → Reviewer
    (MessageKindV1::ReviewerReport, ProducerRole::Reviewer), // §3.5 Reviewer → Controller
    (MessageKindV1::ReviewVerdict, ProducerRole::Controller), // §3.6 controller-accepted
    (MessageKindV1::CorrectiveDirective, ProducerRole::Controller), // §3.7 Controller → Coder
    (MessageKindV1::CampaignFeedItem, ProducerRole::Controller), // §3.8 Controller → Human
    (
        MessageKindV1::HumanAttentionRequest,
        ProducerRole::Controller,
    ), // §3.9 Controller → Human
    (MessageKindV1::HumanCommandRequest, ProducerRole::Human), // §3.10 Human → Controller
    (MessageKindV1::HumanDecision, ProducerRole::Controller), // §3.11 controller-accepted
];

/// Fields claiming `kind`, authored by `role`, **correct in every other
/// respect**.
///
/// The provider evidence follows the stated `role`, not the fixture it came
/// from: present for a provider role (§3.0 requires it), absent otherwise
/// (§3.0 forbids it). Without that, a wrong-role case would be refused by the
/// neighbouring provider-evidence rule and would prove nothing about the
/// direction — the exact way a test on this PR has already recorded a bypass
/// instead of closing one.
fn fields_claiming(kind: MessageKindV1, role: ProducerRole) -> EnvelopeFieldsV1 {
    let provider = role.is_provider_role();
    EnvelopeFieldsV1 {
        message_kind: kind,
        producer_role: role,
        producer_execution_id: id("exec-1"),
        model_identity: provider
            .then(|| model("claude-opus-5"))
            .map_or_else(Optional::absent, Optional::present),
        prompt_digest: provider
            .then(|| digest("prompt"))
            .map_or_else(Optional::absent, Optional::present),
        tool_policy_digest: provider
            .then(|| digest("tool-policy"))
            .map_or_else(Optional::absent, Optional::present),
        provider_execution_receipt_ref: provider
            .then(receipt_ref)
            .map_or_else(Optional::absent, Optional::present),
        ..controller_fields()
    }
}

/// The same claim as a document, reached the way a peer would reach it.
///
/// The base is the envelope §3 *does* authorise for `kind`, encoded through
/// `to_wire_bytes` — so the bytes start conforming and one thing varies: the
/// author, with the evidence that author is required to carry. No bound is
/// relaxed and no member is written that a producer could not write.
fn document_claiming(kind: MessageKindV1, role: ProducerRole, authorised: ProducerRole) -> Vec<u8> {
    let base = EnvelopeV1::new(fields_claiming(kind, authorised))
        .expect("the authorised shape of every kind is admissible");
    let mut value = wire_document(&base);
    let provider = role.is_provider_role();
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "producer_role".to_owned(),
            serde_json::json!(match role {
                ProducerRole::Controller => "controller",
                ProducerRole::Coder => "coder",
                ProducerRole::Reviewer => "reviewer",
                ProducerRole::Human => "human",
            }),
        );
        for (field, value_when_present) in [
            ("model_identity", serde_json::json!("claude-opus-5")),
            (
                "prompt_digest",
                serde_json::json!(digest("prompt").as_str()),
            ),
            (
                "tool_policy_digest",
                serde_json::json!(digest("tool-policy").as_str()),
            ),
            (
                "provider_execution_receipt_ref",
                serde_json::json!({
                    "kind": "provider_execution_receipt",
                    "media_type": typed_media_type(ArtifactKindV1::ProviderExecutionReceipt, 1),
                    "digest": digest("receipt").as_str(),
                    "size": 512,
                }),
            ),
        ] {
            if provider {
                map.insert(field.to_owned(), value_when_present);
            } else {
                map.remove(field);
            }
        }
    }
    value.to_string().into_bytes()
}

/// P1 №1, as a class rather than as one witness.
///
/// For every one of the eleven kinds: the role §3 names admits, and each of the
/// other three is refused — on the producer route and on the inbound route
/// alike, because both run one invariant. Every wrong-role case carries the
/// provider evidence its *claimed* role requires, so the only rule left to
/// refuse it is the direction itself, and the assertions pin that specific
/// refusal rather than "an error happened".
#[test]
fn every_kind_admits_exactly_the_producer_role_section_3_authorises() {
    assert_eq!(
        SECTION_3_AUTHORS.len(),
        MessageKindV1::ALL.len(),
        "every kind needs an author"
    );

    for kind in MessageKindV1::ALL {
        let (_, authorised) = SECTION_3_AUTHORS
            .into_iter()
            .find(|(k, _)| *k == kind)
            .expect("§3 names an author for every kind");

        let mut admitted = Vec::new();
        for role in ProducerRole::ALL {
            let by_new = EnvelopeV1::new(fields_claiming(kind, role));
            let bytes = document_claiming(kind, role, authorised);
            let inbound = parse_artifact::<EnvelopeV1>(&bytes);

            if role == authorised {
                let env = by_new.expect("the authorised author must be admitted");
                assert_eq!(env.message_kind(), kind);
                assert_eq!(env.producer_role(), role);
                let inbound = inbound.expect("and admitted from bytes too");
                assert_eq!(inbound, env, "both routes admit the same envelope");
                admitted.push(role.name());
            } else {
                assert!(
                    matches!(
                        by_new,
                        Err(EnvelopeError::WrongProducerRole {
                            expected,
                            actual,
                            ..
                        }) if expected == authorised.name() && actual == role.name()
                    ),
                    "{} authored by {} must be refused as a direction violation, got {by_new:?}",
                    kind.name(),
                    role.name()
                );
                let reason = inbound.expect_err("the same document must be refused inbound");
                assert!(
                    matches!(&reason, ParseError::Invalid { reason } if reason.contains("is authored by")),
                    "{} authored by {} must be refused inbound by the direction rule, got {reason:?}",
                    kind.name(),
                    role.name()
                );
            }
        }
        assert_eq!(
            admitted,
            [authorised.name()],
            "exactly one role may author {}",
            kind.name()
        );
    }
}

/// Falsification 2. `EnvelopeV1` does not implement `Serialize`, so
/// `to_wire_bytes` is the only route from an admitted envelope to bytes.
///
/// Autoref specialisation: the inherent method applies only when the bound
/// holds, and an inherent method wins over a trait method when it applies. So
/// what this reports is whether the impl *exists*, which is not otherwise
/// observable at runtime. The two neighbours are probed as well — a `Serialize`
/// on the fields or on the private wire form would restore the second route by
/// another name.
#[test]
fn an_admitted_envelope_has_no_second_route_to_bytes() {
    use std::marker::PhantomData;

    struct Probe<T>(PhantomData<T>);
    trait NotSerialize {
        fn is_serialize(&self) -> bool {
            false
        }
    }
    impl<T> NotSerialize for Probe<T> {}
    impl<T: serde::Serialize> Probe<T> {
        fn is_serialize(&self) -> bool {
            true
        }
    }

    assert!(
        !Probe::<EnvelopeV1>(PhantomData).is_serialize(),
        "EnvelopeV1 must not be publicly Serialize: that was P1 №2"
    );
    assert!(
        !Probe::<EnvelopeFieldsV1>(PhantomData).is_serialize(),
        "the producer-side fields must not carry an emission route either"
    );
    // The probe reports the bound honestly rather than always saying `false`:
    // a type that *is* Serialize must come back true.
    assert!(Probe::<ArtifactRef>(PhantomData).is_serialize());
}

/// Falsification 3. An admitted envelope's own encoder can always emit it
/// under the FD-1.4 ceiling.
///
/// The sweep walks the document across the 1 MiB boundary — each extra
/// `media_type` byte costs `MAX_ARTIFACT_REFS` bytes — and asserts the
/// implication on both sides of it: `new` returns `Ok` only for fields whose
/// encoding fits, and those bytes are then admitted by `parse_artifact`. This
/// is the inverse of the defect: previously a length existed for which `new`
/// succeeded and an emission route produced bytes a peer had to refuse.
#[test]
fn no_admitted_envelope_can_emit_bytes_its_own_ceiling_refuses() {
    let with_media_len = |len: usize| {
        let media = "x".repeat(len);
        let refs: Vec<ArtifactRef> = (0..MAX_ARTIFACT_REFS)
            .map(|i| {
                ArtifactRef::new(
                    ArtifactKindV1::Diff,
                    media.clone(),
                    digest(&format!("blob-{i}")),
                    1,
                )
                .expect("each ref is individually admissible")
            })
            .collect();
        EnvelopeFieldsV1 {
            artifact_refs: ArtifactRefs::new(refs).expect("exactly at the ref-count bound"),
            ..controller_fields()
        }
    };

    let crossing = (1..=4096)
        .rev()
        .find(|len| EnvelopeV1::new(with_media_len(*len)).is_ok())
        .expect("some filler length is admissible");
    assert!(
        matches!(
            EnvelopeV1::new(with_media_len(crossing + 1)),
            Err(EnvelopeError::AboveByteCeiling { .. })
        ),
        "one byte past the crossing must be refused by the byte bound"
    );

    let mut admitted = 0_usize;
    let mut refused = 0_usize;
    for len in crossing.saturating_sub(2)..=crossing + 3 {
        match EnvelopeV1::new(with_media_len(len)) {
            Ok(env) => {
                admitted += 1;
                let bytes = env
                    .to_wire_bytes()
                    .expect("an admitted envelope must encode");
                assert!(
                    bytes.len() as u64 <= MAX_CONTROL_ARTIFACT_BYTES,
                    "emitted {} bytes over the ceiling",
                    bytes.len()
                );
                assert!(
                    parse_artifact::<EnvelopeV1>(&bytes).is_ok(),
                    "a conforming peer must admit what this producer emits"
                );
            }
            Err(EnvelopeError::AboveByteCeiling { .. }) => refused += 1,
            Err(other) => panic!("unexpected refusal at media_type length {len}: {other}"),
        }
    }
    assert!(
        admitted > 0 && refused > 0,
        "the sweep must straddle the ceiling ({admitted} admitted, {refused} refused)"
    );
}

/// Falsification 4. `to_wire_bytes` → `parse_artifact` is the identity on
/// admitted envelopes.
///
/// This is also what keeps the private encoder from drifting from the private
/// wire form: a field the encoder omits fails here as a missing field, and one
/// it spells differently fails as an unknown field against
/// `deny_unknown_fields`.
#[test]
fn the_producer_encoding_reads_back_as_the_same_envelope() {
    for env in [controller_envelope(), coder_envelope()] {
        let bytes = wire_bytes(&env);
        let back: EnvelopeV1 =
            parse_artifact(&bytes).expect("the producer encoding must be admissible");
        assert_eq!(back, env);
        assert_eq!(back.framed_digest(), env.framed_digest());
        // And emission is deterministic, so "the bytes" is a well-defined thing
        // to say about an admitted envelope.
        assert_eq!(wire_bytes(&back), bytes);
    }
}

/// Falsification 5. No canonical JSON was introduced.
///
/// FD-1.2: "No canonical-JSON scheme is introduced." So a conforming peer that
/// spells the same envelope with different whitespace, or emits its members in
/// a different order, must still be admitted — and must yield the same
/// `EnvelopeV1` and the same framed identity, because identity is computed
/// from fields and not from bytes. If admission had started comparing against
/// a re-serialization, these would be refused.
#[test]
fn a_differently_spelled_but_conforming_document_is_still_admitted() {
    let env = coder_envelope();
    let document = wire_document(&env);
    let members = document
        .as_object()
        .expect("the encoding is a JSON object")
        .clone();

    // Whitespace: the same document, indented.
    let pretty = serde_json::to_vec_pretty(&document).expect("a Value re-serializes");
    assert_ne!(
        pretty,
        wire_bytes(&env),
        "the spelling must actually differ"
    );

    // Member order: reversed relative to whatever order the encoder used.
    let reversed = {
        let mut parts: Vec<String> = members
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}:{}",
                    serde_json::to_string(k).expect("a key is a string"),
                    v
                )
            })
            .collect();
        parts.reverse();
        format!("{{{}}}", parts.join(",")).into_bytes()
    };

    for (label, bytes) in [("indented", pretty), ("reordered", reversed)] {
        let back: EnvelopeV1 = parse_artifact(&bytes)
            .unwrap_or_else(|e| panic!("a conforming {label} document must be admitted: {e}"));
        assert_eq!(back, env, "{label}");
        assert_eq!(back.framed_digest(), env.framed_digest(), "{label}");
    }

    // The ceiling still applies to whatever spelling arrives: a document that
    // conforms in shape but not in size is refused, so accepting other
    // spellings is not accepting other sizes.
    let padded = format!(
        "{{{}, \"pad\":\"{}\"}}",
        String::from_utf8(wire_bytes(&env))
            .expect("utf-8")
            .trim_matches(|c| c == '{' || c == '}')
            .to_owned(),
        "x".repeat(MAX_CONTROL_ARTIFACT_BYTES as usize)
    );
    assert!(matches!(
        parse_artifact::<EnvelopeV1>(padded.as_bytes()),
        Err(ParseError::PayloadTooLarge { .. })
    ));
}
