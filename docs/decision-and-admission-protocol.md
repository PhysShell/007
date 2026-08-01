# Decision under uncertainty and exact-head admission protocol

Status: design decision · Scope: autonomous planning, review, correction, and merge admission

## Purpose

Software work is performed under partial observability. The controller rarely knows the complete world state:

- a provider call may have crossed an irreversible boundary before the process died;
- CI may be delayed, stale, unavailable, or attached to a different commit;
- a reviewer may have inspected an earlier head;
- a mutable worktree may no longer match the artifact that was reviewed;
- a model may present an assumption as if it were an established fact;
- a new blocker may be discovered only after an attempted implementation.

The correct response is not to pretend uncertainty has disappeared. It is to represent uncertainty explicitly, buy information when useful, prefer reversible actions, and refuse irreversible transitions whose preconditions are not established.

This document records those decision rules and the exact-head workflow developed through the R0/R1 corrective rounds.

## Decision-theory concepts worth adopting

`007` does not need a full Bayesian planner or POMDP implementation for the first autonomous controller. It should adopt several concepts from decision-making under incomplete information.

### Partial observability

The real world state and the controller's known state are different objects.

```text
world state       what actually happened
observations      events, artifacts, CI, GitHub, process status
belief state      what the controller can currently justify
decision state    what actions are allowed from that justified knowledge
```

A model-generated explanation is an observation or hypothesis. It is never direct access to world state.

### Explicit epistemic status

Claims used by the controller should carry one of these meanings rather than a vague confidence score:

```text
ESTABLISHED    supported by current, identity-bound evidence
REFUTED        contradicted by current evidence
UNKNOWN        not observed or not yet checked
CONFLICTING    credible evidence disagrees
STALE          once established, but no longer bound to the current candidate
UNAVAILABLE    the required observer or verifier could not run
OUT_OF_SCOPE   deliberately not claimed by this task
```

For safety-sensitive guards, `UNKNOWN`, `CONFLICTING`, `STALE`, and `UNAVAILABLE` do not count as success.

A scalar probability may later help scheduling or prioritization. It must not silently turn an unproved merge precondition into a green check.

### Value of information

Before acting, the controller should ask whether one more observation can materially change the decision.

Examples:

- rerunning a flaky test may distinguish a timing failure from a deterministic regression;
- fetching the PR head again can determine whether a review is stale;
- inspecting a real process-level trace can close an ambiguity that synthetic fixtures cannot;
- an additional reviewer is useful only if the decision remains genuinely underdetermined.

Information gathering should also be bounded. Repeating the same check without changing the evidence model is not investigation; it is ritualized CPU use.

### Reversible-first policy

When uncertainty remains, prefer actions that preserve future options:

```text
draft PR before ready-for-review
fresh branch/worktree before mutating main
new additive correction commit before rewriting history
read-only reconciliation before recovery mutation
local gate before remote merge
human escalation before irreversible admission
```

The more irreversible an action is, the stronger and more identity-specific its evidence must be.

### Robust decisions

When several plausible world states remain, choose an action safe across all of them.

R1's post-dispatch ambiguity rule is the reference example:

```text
provider definitely not invoked     -> safe pre-dispatch redrive
provider may have been invoked       -> fail closed; never auto-redrive
```

This sacrifices liveness in the ambiguous branch to preserve at-most-once behavior. That is a robust decision under partial observability, not a failure to guess confidently enough.

### Sequential decisions and stopping rules

Autonomous work is a sequence of observations and actions, not one giant prediction. Every loop needs explicit stop conditions:

- acceptance established;
- a human decision is required;
- evidence conflicts;
- the same blocker repeats without progress;
- budget is exhausted;
- the requested correction changes frozen scope;
- the next useful observation is unavailable.

A controller that cannot stop is not autonomous. It is merely unattended.

## Exact-head candidate admission protocol

The candidate admission protocol protects consistency between implementation, evidence, review, and merge.

### 1. Freeze the predecessor

Every task starts from an exact accepted predecessor identity:

```text
base commit SHA
base repository identity
frozen task/acceptance contract
```

A branch name such as `main` is convenient input for resolving a commit. It is not the frozen base identity.

### 2. Use a fresh branch and worktree

Implementation starts in a fresh child branch/worktree from the exact base.

Do not reuse:

- the predecessor implementation worktree;
- an independent review worktree;
- a mutable directory whose relationship to the frozen base is only assumed.

Candidate state must be reconstructed from verified identities and artifacts, not inherited from an old shell session.

### 3. Open a draft PR as quarantine

A draft PR is the integration boundary for a candidate that is observable but not admitted.

While draft, the PR may accumulate:

- additive implementation commits;
- CI evidence;
- independent review findings;
- corrective rounds;
- documentation of limits and residuals.

Draft status is not cosmetic. It means the candidate remains explicitly non-admissible while evidence is incomplete.

### 4. Record the exact candidate head

Every gate, CI run, review request, verdict, and merge decision must name the full candidate commit SHA.

```text
candidate_head = <40-hex commit SHA>
```

Short SHAs may be displayed for humans but must not be the identity stored in machine-readable evidence.

### 5. Bind evidence to that head

Evidence is valid only for the candidate identity it actually examined.

At minimum, record:

```text
candidate head SHA
base SHA
verifier identity/version
workflow or command identity
result
artifact identities/digests
observation time
```

A green workflow attached to another head is not supporting evidence. A locally green command whose binary or configuration identity is unknown may be diagnostic evidence, but not necessarily admission evidence.

### 6. Review independently at the exact head

The independent reviewer receives a fresh, detached view of the exact candidate head and the frozen contract.

The review verdict must bind:

```text
reviewed_head
verdict
findings
properties checked
required corrections or residuals
reviewer identity/version
```

The reviewer does not implement fixes in the review worktree and does not mutate PR state, merge, or begin the next phase.

### 7. Head drift invalidates acceptance

Any new commit after review changes the candidate identity.

```text
reviewed_head != current_pr_head
-> prior verdict is STALE
-> rerun required gates and independent exact-head review
```

The semantic size of the delta does not waive this rule. A one-line test stabilization is still a new candidate. Software has repeatedly demonstrated that one line is sufficient for both salvation and catastrophe.

### 8. Correct forward only

During an active review series, corrections are additive:

```text
no amend
no rebase
no squash
no force-push
```

Forward-only history preserves:

- which defect each round addressed;
- whether a fix regressed earlier properties;
- the exact delta reviewed at each stage;
- reproducibility of old review findings;
- forensic value when the same class of defect returns.

History cleanup, if ever permitted, is a separate post-acceptance policy and must not destroy the identity chain used for admission.

### 9. Freeze accepted identity

Acceptance is expressed as:

```text
ACCEPTED / CLOSED / FROZEN
accepted_head = <exact SHA>
```

`FROZEN` means no further mutation is allowed under that verdict. A later change is a new candidate and requires new evidence.

The accepted implementation SHA and the eventual merge commit SHA have different roles and should both be retained.

### 10. Reverify immediately before merge

Immediately before merge, re-establish:

```text
PR is open and unmerged
current head == accepted_head
required CI is green for accepted_head
independent verdict accepts accepted_head
PR is mergeable
base policy still permits admission
```

Then mark ready and merge using an expected-head precondition when the hosting API supports it.

```text
merge(expected_head_sha = accepted_head)
```

This closes the time-of-check/time-of-use window between final verification and merge.

### 11. Record post-merge provenance

After merge, retain:

```text
accepted implementation head
merge commit
PR number
review verdict identity
required CI run identities
frozen contract identity
known residuals and explicit non-goals
```

The merge commit may have an identical tree to the accepted implementation head while still being a distinct provenance event.

## Decision table

| Evidence state | Reversible action | Irreversible or admission action |
|---|---|---|
| `ESTABLISHED` and identity-bound | allowed by normal policy | allowed if all other guards hold |
| `UNKNOWN` | investigate or proceed only inside a bounded reversible sandbox | blocked |
| `STALE` | refresh evidence | blocked |
| `CONFLICTING` | gather discriminating evidence or escalate | blocked |
| `UNAVAILABLE` | retry within budget or escalate | blocked |
| `REFUTED` | correct, abandon, or revise the explicit contract through human authority | blocked |
| `OUT_OF_SCOPE` | allowed only when the task contract explicitly excludes the claim | cannot be presented as proved |

## Assumptions and expiry

An assumption must be explicit and must identify what would invalidate it.

Example:

```text
claim: hosted CI failure is timing-sensitive, not a product regression
status: assumption
support: local repeated pass; failure occurred after the asserted product state
invalidated_by: deterministic reproduction or evidence that the product state never converges
required_action: add bounded polling with unchanged terminal assertion; rerun exact-head CI
```

Assumptions may authorize experiments. They do not authorize final acceptance unless converted into established evidence or explicitly accepted as a residual risk by the appropriate human role.

## Relationship to the autonomy controller

`docs/autonomy-controller.md` defines who chooses tasks and who controls lifecycle transitions.

This document defines how those components reason when observations are incomplete:

```text
planner
  may rank hypotheses and request information

campaign state machine
  applies deterministic guards over epistemic status

bounded workflows
  gather evidence or perform reversible corrections

canonical reducer
  records what happened, not what the planner expected

human authority
  resolves legitimate ambiguity, scope changes, and residual-risk acceptance
```

A future goal planner may use Bayesian, POMDP, robust-planning, or value-of-information techniques internally. Its output remains advisory until campaign guards are established by durable evidence.

## What is mandatory now

- Explicit unknown/stale/conflicting states rather than implicit optimism.
- Reversible-first actions under uncertainty.
- Fail-closed irreversible transitions.
- Exact-SHA binding for candidate evidence and review.
- Draft PR quarantine.
- Independent exact-head review.
- Forward-only corrective rounds.
- Immediate pre-merge head revalidation.
- Expected-head merge protection where available.
- Durable provenance after merge.

## What is deferred

- Numeric Bayesian belief distributions for all claims.
- A full POMDP model of software engineering.
- Automated utility functions for every product decision.
- General multi-agent consensus.
- Automatic acceptance of residual risk.
- Automatic merge as the default policy.

## Non-goals

This protocol does not claim that exact-head review proves complete software correctness. It establishes a narrower and necessary property:

> The code admitted is exactly the code for which the recorded gates, CI, and independent verdict were produced, and uncertainty that could invalidate admission was not silently converted into success.
