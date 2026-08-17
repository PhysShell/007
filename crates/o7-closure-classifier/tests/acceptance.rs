//! Acceptance cases from issue #147 that apply to the pure classifier.
//!
//! These use CONSTRUCTED typed inputs, not the frozen Step 0B fixtures. Where a
//! case has no historical fixture witness — `OWED`, `CANNOT_CHECK` — that is
//! stated on the test itself: a logic-level unit witness is not a claim that the
//! frozen corpus covers the path.

// Justification for the restriction-lint allowance, per the precedent in
// `tests/a0_candidate_state_e2e.rs`: every panic path below is this test's own
// assertion over inputs it constructed itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_classifier::{
    classify, Acquisition, CheckConclusion, CheckEvidence, ClassifierInput, FalsificationFact,
    ObservationInput, Policy, ReviewEvidence, SourceKind, State, Subject, Verification,
};
use std::collections::BTreeMap;

const HEAD: &str = "ed7969c4636362bcfc248bcb9957e140ca899f8c";
const OTHER: &str = "c08abce9dd0356facae400db1950cd9644976114";

const CI_IDS: &[&str] = &[
    "ci/worker-gate",
    "ci/dependency-policy",
    "ci/dependency-advisories",
    "ci/worktree-verifier",
];

fn policy() -> Policy {
    let mut required: Vec<String> = CI_IDS.iter().map(|s| (*s).to_owned()).collect();
    required.push("review/codex".to_owned());
    required.push("review/coderabbit".to_owned());
    Policy {
        id: "007/closure/v1".to_owned(),
        required_observations: required,
        completeness_claimed: false,
    }
}

fn subject(expected: &str, before: &str, after: &str) -> Subject {
    Subject {
        repository: "PhysShell/007".to_owned(),
        pull_request: 145,
        expected_sha: expected.to_owned(),
        head_before: before.to_owned(),
        head_after: after.to_owned(),
    }
}

fn passing_check(sha: &str, name: &str) -> ObservationInput {
    ObservationInput::Check(Acquisition::Available(CheckEvidence {
        stable_id: format!("check:{name}"),
        head_sha: sha.to_owned(),
        conclusion: CheckConclusion::Success,
    }))
}

fn clean_review(sha: &str, author: &str) -> ObservationInput {
    ObservationInput::Review(Acquisition::Available(ReviewEvidence {
        stable_id: format!("review:{author}"),
        commit_id: sha.to_owned(),
        author: author.to_owned(),
        carries_finding: false,
    }))
}

/// Every CI check passing at `sha`, both reviewers clean at `sha`.
fn all_green(sha: &str) -> BTreeMap<String, ObservationInput> {
    let mut m = BTreeMap::new();
    for name in CI_IDS {
        m.insert((*name).to_owned(), passing_check(sha, name));
    }
    m.insert("review/codex".to_owned(), clean_review(sha, "codex"));
    m.insert(
        "review/coderabbit".to_owned(),
        clean_review(sha, "coderabbit"),
    );
    m
}

fn input(subject: Subject, observations: BTreeMap<String, ObservationInput>) -> ClassifierInput {
    ClassifierInput {
        subject,
        policy: policy(),
        observations,
        falsifications: Vec::new(),
        known_debts: Vec::new(),
    }
}

fn state_of(p: &o7_closure_classifier::Predicate, id: &str) -> State {
    p.observations
        .iter()
        .find(|o| o.id == id)
        .map(|o| o.state)
        .unwrap_or_else(|| panic!("observation {id} missing from the predicate"))
}

// ------------------------------------------------------------------ 1, 2, 6 --

#[test]
fn case_01_initial_head_differs_from_expected_is_stale() {
    let p = classify(&input(subject(HEAD, OTHER, HEAD), all_green(HEAD)));
    assert_eq!(p.headline, State::Stale);
    assert!(p.subject_stale);
}

#[test]
fn case_02_final_head_differs_from_expected_is_stale() {
    let p = classify(&input(subject(HEAD, HEAD, OTHER), all_green(HEAD)));
    assert_eq!(p.headline, State::Stale);
    assert!(p.subject_stale);
}

#[test]
fn case_02b_stale_snapshot_still_preserves_the_observation_vector() {
    // The head moving must not erase what each observation said about the frozen
    // subject; the headline is derived presentation, not a replacement.
    let p = classify(&input(subject(HEAD, HEAD, OTHER), all_green(HEAD)));
    assert_eq!(p.headline, State::Stale);
    assert_eq!(state_of(&p, "ci/worker-gate"), State::Pass);
    assert_eq!(state_of(&p, "review/codex"), State::Pass);
}

// --------------------------------------------------------------------- 3, 11 --

#[test]
fn case_03_clean_review_on_the_wrong_sha_is_not_positive_evidence() {
    let mut obs = all_green(HEAD);
    obs.insert("review/codex".to_owned(), clean_review(OTHER, "codex"));
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(
        state_of(&p, "review/codex"),
        State::Owed,
        "a review bound to another commit produces no verdict for this subject"
    );
    assert_ne!(p.headline, State::Pass);
}

#[test]
fn case_11_ci_and_reviews_from_different_shas_cannot_combine() {
    // CI genuinely at the subject, reviews genuinely elsewhere. Neither rescues
    // the other into a positive snapshot.
    let mut obs = all_green(HEAD);
    obs.insert("review/codex".to_owned(), clean_review(OTHER, "codex"));
    obs.insert(
        "review/coderabbit".to_owned(),
        clean_review(OTHER, "coderabbit"),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    for id in ["review/codex", "review/coderabbit"] {
        assert_eq!(state_of(&p, id), State::Owed);
    }
    for id in CI_IDS {
        assert_eq!(state_of(&p, id), State::Pass);
    }
    assert_eq!(p.headline, State::Owed);
}

#[test]
fn case_11b_a_check_bound_to_another_sha_is_not_evidence_here() {
    let mut obs = all_green(HEAD);
    obs.insert(
        "ci/worker-gate".to_owned(),
        passing_check(OTHER, "worker-gate"),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "ci/worker-gate"), State::Owed);
}

// ------------------------------------------------------------------------ 4 --

#[test]
fn case_04_a_mutable_comment_cannot_be_positive_verdict_evidence() {
    // BY CONSTRUCTION, and this test records that rather than pretending to
    // discover it: the ONLY channel into a `review/*` observation is
    // `ReviewEvidence`, which carries an immutable `commit_id`. There is no
    // variant through which an issue comment, reaction, walkthrough or command
    // reply could arrive as positive evidence. A reviewer that posted only a
    // clean-sounding comment therefore leaves its required observation OWED.
    let mut obs = all_green(HEAD);
    obs.insert(
        "review/coderabbit".to_owned(),
        ObservationInput::Review(Acquisition::NotProduced),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "review/coderabbit"), State::Owed);
    assert_eq!(p.headline, State::Owed);
}

// --------------------------------------------------------------------- 5, 12 --

#[test]
fn case_05_structured_falsification_from_a_non_verdict_surface_is_preserved() {
    // Everything a policy asks for is green; a falsification arrives from a
    // surface that could never carry a verdict. It must still be able to produce
    // a finding — the asymmetry #147 requires.
    let mut inp = input(subject(HEAD, HEAD, HEAD), all_green(HEAD));
    inp.falsifications.push(FalsificationFact {
        source_kind: SourceKind::IssueComment,
        stable_id: "issuecomment-5305700001".to_owned(),
        subject_sha: None,
        author: "coderabbitai[bot]".to_owned(),
        verification: Verification::Reproduced,
    });
    let p = classify(&inp);
    assert_eq!(p.headline, State::Finding);
    assert_eq!(p.falsifications.len(), 1);
    // The positive observations are NOT rewritten by it: both facts survive.
    assert_eq!(state_of(&p, "review/codex"), State::Pass);
}

#[test]
fn case_05b_falsification_without_a_commit_binding_still_counts() {
    // Positive evidence is strict about SHA binding; falsification is not. An
    // issue comment has no commit binding at all, and gating on one would
    // silently narrow the wide surface.
    let mut inp = input(subject(HEAD, HEAD, HEAD), all_green(HEAD));
    inp.falsifications.push(FalsificationFact {
        source_kind: SourceKind::IssueComment,
        stable_id: "issuecomment-1".to_owned(),
        subject_sha: None,
        author: "someone[bot]".to_owned(),
        verification: Verification::Claimed,
    });
    assert_eq!(classify(&inp).headline, State::Finding);
}

#[test]
fn case_12_same_vendor_same_sha_conflicting_surfaces_are_not_collapsed() {
    // The submitted commit-bound review controls verdict accounting; a claim on
    // another surface from the same author is preserved rather than deleted.
    let mut obs = all_green(HEAD);
    obs.insert(
        "review/codex".to_owned(),
        ObservationInput::Review(Acquisition::Available(ReviewEvidence {
            stable_id: "review:codex".to_owned(),
            commit_id: HEAD.to_owned(),
            author: "codex[bot]".to_owned(),
            carries_finding: true,
        })),
    );
    let mut inp = input(subject(HEAD, HEAD, HEAD), obs);
    inp.falsifications.push(FalsificationFact {
        source_kind: SourceKind::ReviewComment,
        stable_id: "discussion_r3791453710".to_owned(),
        subject_sha: Some(HEAD.to_owned()),
        author: "codex[bot]".to_owned(),
        verification: Verification::Reproduced,
    });
    let p = classify(&inp);
    assert_eq!(state_of(&p, "review/codex"), State::Finding);
    assert_eq!(p.falsifications.len(), 1);
    assert_eq!(p.headline, State::Finding);
}

// --------------------------------------------------------------------- 6, 7 --

#[test]
fn case_06_no_verdict_produced_is_owed_not_pass() {
    // LOGIC-LEVEL UNIT WITNESS. The frozen Step 0B corpus has no rate-limit or
    // absent-verdict specimen, and this test does not claim otherwise.
    for acq in [Acquisition::NotProduced, Acquisition::RateLimited] {
        let mut obs = all_green(HEAD);
        obs.insert("review/codex".to_owned(), ObservationInput::Review(acq));
        let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
        assert_eq!(state_of(&p, "review/codex"), State::Owed);
    }
}

#[test]
fn case_07_acquisition_failure_is_cannot_check_not_owed_and_not_empty_success() {
    // LOGIC-LEVEL UNIT WITNESS — no Step 0B specimen exists for an API error.
    let mut obs = all_green(HEAD);
    obs.insert(
        "ci/worker-gate".to_owned(),
        ObservationInput::Check(Acquisition::Failed {
            reason: "503 from the checks endpoint".to_owned(),
        }),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "ci/worker-gate"), State::CannotCheck);
    assert_ne!(state_of(&p, "ci/worker-gate"), State::Owed);
    assert_ne!(p.headline, State::Pass);
}

#[test]
fn case_07b_a_missing_required_observation_is_owed() {
    let mut obs = all_green(HEAD);
    obs.remove("review/coderabbit");
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "review/coderabbit"), State::Owed);
}

// --------------------------------------------------------------------- 8, 9 --

#[test]
fn case_08_finding_and_cannot_check_both_survive_headline_is_finding() {
    let mut obs = all_green(HEAD);
    obs.insert(
        "review/codex".to_owned(),
        ObservationInput::Review(Acquisition::Available(ReviewEvidence {
            stable_id: "review:codex".to_owned(),
            commit_id: HEAD.to_owned(),
            author: "codex[bot]".to_owned(),
            carries_finding: true,
        })),
    );
    obs.insert(
        "ci/worktree-verifier".to_owned(),
        ObservationInput::Check(Acquisition::Failed {
            reason: "runner vanished".to_owned(),
        }),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "review/codex"), State::Finding);
    assert_eq!(state_of(&p, "ci/worktree-verifier"), State::CannotCheck);
    assert_eq!(p.headline, State::Finding);
}

#[test]
fn case_09_a_later_snapshot_without_the_finding_is_cannot_check_not_pass() {
    // Each invocation is a fresh snapshot: no memory of a previous headline, and
    // equally no inheritance of one.
    let mut obs = all_green(HEAD);
    obs.insert(
        "ci/worktree-verifier".to_owned(),
        ObservationInput::Check(Acquisition::Failed {
            reason: "runner vanished".to_owned(),
        }),
    );
    let p = classify(&input(subject(HEAD, HEAD, HEAD), obs));
    assert_eq!(state_of(&p, "ci/worktree-verifier"), State::CannotCheck);
    assert_eq!(p.headline, State::CannotCheck);
    assert_ne!(p.headline, State::Pass);
}

// ------------------------------------------------------------- 10, 14, 15 --

#[test]
fn case_10_pass_enumerates_the_required_set_and_disclaims_completeness() {
    let p = classify(&input(subject(HEAD, HEAD, HEAD), all_green(HEAD)));
    assert_eq!(p.headline, State::Pass);
    assert_eq!(
        p.policy.required_observations,
        policy().required_observations
    );
    assert!(!p.policy.completeness_claimed);
    assert_eq!(p.observations.len(), policy().required_observations.len());
}

#[test]
fn case_14_known_debt_survives_clean_reviewer_evidence() {
    let mut inp = input(subject(HEAD, HEAD, HEAD), all_green(HEAD));
    inp.known_debts = vec!["W8".to_owned(), "W9 both arms".to_owned()];
    let p = classify(&inp);
    assert_eq!(p.headline, State::Pass);
    assert_eq!(
        p.known_debts,
        vec!["W8".to_owned(), "W9 both arms".to_owned()]
    );
}

#[test]
fn case_15_predicate_makes_no_merge_authorization_claim() {
    let p = classify(&input(subject(HEAD, HEAD, HEAD), all_green(HEAD)));
    let json = p.to_json().expect("serializing the predicate");
    for forbidden in ["merge", "approve", "authoriz", "mergeable", "ready_to"] {
        assert!(
            !json.to_lowercase().contains(forbidden),
            "predicate must not speak about {forbidden}: {json}"
        );
    }
}

// --------------------------------------------------------------- 13, output --

#[test]
fn case_13_classification_is_a_pure_function_of_its_input() {
    // Independence from local Git state is STRUCTURAL: this crate links no git
    // API, spawns no process and reads no path, which the PR's scope scan
    // checks. What a test can show is the observable half — the same input
    // always yields byte-identical output, so nothing ambient leaks in.
    let inp = input(subject(HEAD, HEAD, HEAD), all_green(HEAD));
    let a = classify(&inp).to_json().expect("serializing");
    let b = classify(&inp).to_json().expect("serializing");
    assert_eq!(a, b);
    assert!(!a.contains("timestamp"));
}

#[test]
fn observation_order_follows_the_authoritative_policy_order() {
    let p = classify(&input(subject(HEAD, HEAD, HEAD), all_green(HEAD)));
    let ids: Vec<&str> = p.observations.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, policy().required_observations);
}
