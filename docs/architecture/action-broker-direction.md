# Action broker direction (ABR-0)

Status: **accepted direction, implementation gated** · Track: **ABR** (this note
is ABR-0) · Scope: **authority boundaries and deferred decisions, not an
implementation contract**.

Implementation gate: **not before MG-C**. The nearest executable path is
unchanged — `SB-A0 → SB-A1 → SB-A2 → SB-B → MG-C` — and no ABR code is written
now. This note records *where* an agent-driven action broker (GitHub merge
first) is going, so later work does not re-derive the forks or smuggle in the
weaker forms. It is a frozen direction record, maintainer-ratified in an
interactive session (per rule 3 of `docs/evidence-and-decision-discipline.md`,
that makes it ratified, not pending).

The broker, when built, is a capability consumer in the same family as the model
gate (`docs/architecture/capability-fd-transport.md`) — more dangerous, because
a model can merely say wrong things, whereas a broker can merge them.

## 1. Accepted boundaries

- Raw credentials inside a Sandboy target: **prohibited**.
- Full PAT + "the agent is forbidden the rest by policy": **rejected** — a
  secret in the agent's reach is a convenience wrapper, not a security boundary.
- Permanent role PATs exposed to agents: **not the target architecture** — a
  role is too broad; it should select a policy profile, not be a credential.
- Typed action broker: **accepted direction** — roles are separated by policy
  and typed actions, not by long-lived tokens.
- Local agent→broker authority: **capability FD** (no portable token), per the
  frozen capability-FD transport.
- Merge: requires **human authorization** until an autonomous-merge policy is
  separately accepted (`docs/autonomy-controller.md` — controller not started;
  `READY_TO_MERGE` is established readiness, not a right to merge).

## 2. GitHub merge authority (fact, artifact-grounded)

Per rule 4 of `docs/evidence-and-decision-discipline.md`, the factual claims here
are bound to their primary artifacts and verification date; distinguish what the
artifact *says* from the inference drawn on it.

**Artifact says** (verified 2026-08-05 against GitHub REST API version
2022-11-28 — an external artifact is not captured by this repo's commit, so it
carries its own version anchor, and its selected-version contract is re-verified
empirically for the chosen GitHub App installation token and repository ruleset
at implementation):

- `PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge` accepts a `sha`
  parameter ("SHA that pull request head must match to allow merge") and returns
  **409 Conflict** if the head moved — GitHub REST API docs, endpoint "Merge a
  pull request" (Pulls).
- The same endpoint requires **Contents: write** — GitHub fine-grained-PAT
  permissions reference, section "Repository permissions for 'Contents'".
  (Adjacent actions mislead: create/update/review a PR use the *Pull requests*
  permission, "Update a pull request branch" needs Pull requests: write, but the
  merge endpoint is under *Contents*. Rely on the endpoint-specific entry.)

**Re-verify at implementation** — not because the fact is in doubt, but for
separate acceptance concerns: the actual GitHub App installation configuration,
interaction with rulesets and branch protection, and possible future change to
GitHub's external permission taxonomy. Confirm empirically with a scoped token.

## 3. Load-bearing argument (permission-agnostic)

- The documented upstream permission is **broader than one bounded action**:
  `Contents: write` (or whatever the minimum becomes) permits far more than
  "merge exactly this PR at this head".
- GitHub can enforce the exact-head precondition (`sha`/409) but knows nothing
  of the 007 admission receipt, the independent verdict, the campaign state,
  the frozen contract, or human authorization.
- The broker adds exactly that binding: **repository, PR, expected head,
  admission receipt, campaign state, human authorization**.
- This argument holds **regardless of any future change to GitHub's permission
  taxonomy** — the broker exists to add authorization semantics GitHub does not
  have, not to compensate for a missing `sha` (which is present).

This merge is the first concrete instance of rule 2 (atomic precondition
consumption): `merge(sha = accepted_head)` is the atomic conditional mutation,
and a typed precondition mismatch (the 409, classified by the GitHub adapter,
not read as a bare status code) is STALE → full re-adjudication, never a blind
re-fetch-and-retry.

## 4. Deferred decisions

- **Connector Kit** (manifest / OpenAPI import / native adapter / WASM ladder):
  only after **two concrete adapters inside one target-authority model** expose
  real duplication of transport, schemas, auth injection, retry, audit, and
  redaction. The harness's GitHub MCP is a harness tool, not an in-model
  adapter, and does not count toward the two.
- **Action-broker policy language: not chosen.** First broker = concrete Rust
  guards, e.g. `authorize_merge(request, current_pr, admission_receipt,
  campaign_state, human_authorization)`. Not every `if` earns a declarative
  language.
- **CUE** is chosen for *sandbox-policy authoring* (Own.NET sandboy README /
  `docs/zero-trust-framework.md`), **not** as an ambient 007 policy language; it
  is not automatically applied to the broker.
- **CEL** is **not** adopted in advance.
- **PASETO / Biscuit / Macaroons**: not chosen until a real remote or multi-hop
  delegation boundary exists. Locally, FD capability is strictly stronger than a
  portable bearer token.

## 5. Non-goals (explicit)

No WIT ABI. No OpenAPI importer. No token issuer. No universal connector schema.
No autonomous-merge policy. No implementation tasks before MG-C.

## 6. Unfreeze triggers

```text
GitHub broker            after MG-C
generic connector layer  after two real adapters in one authority model
portable grants          at the first real remote boundary
attenuable grants        at the first real multi-hop delegation chain
```

Nothing is built now. This records where to go and keeps going where the stage
chain already leads, rather than raising a handsome station on a railway that is
not yet laid.

## 7. Prior art (non-normative)

`docs/architecture/prior-art-fusio.md` (ABR-1) surveys one external system that
already implements the operation → action → connection inversion accepted in §1,
and records where it stops being a trust boundary. It is a reference record: it
adds no boundary, changes no deferred decision, and does not move the gate above.
