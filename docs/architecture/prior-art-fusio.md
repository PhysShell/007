# Fusio as prior art for the action broker (ABR-1)

Status: **prior-art record, non-normative** · Track: **ABR** (this note is ABR-1)
· Scope: **an external system read against the boundaries ABR-0 already froze**.

This note does **not** unfreeze anything. `docs/architecture/action-broker-direction.md`
(ABR-0) keeps its implementation gate — *not before MG-C* — and its deferred
decisions stand exactly as written. What follows is a survey of one existing
system that implements a large part of the broker shape, recorded so later work
can reuse the parts that are genuinely reusable and does not re-derive, from
scratch, the part where that system stops being a trust boundary.

Fusio is a self-hosted API management platform written in PHP
(<https://github.com/apioo/fusio>). The reason it is worth a record at all: its
core model is *not* "hand the caller a credential", it is "expose a typed
operation and let the platform hold the credential" — the same inversion ABR-0
accepted in §1.

## 1. Verified facts

Per rule 4 of `docs/evidence-and-decision-discipline.md`, each claim is bound to
its artifact and a verification date. These are external artifacts, not captured
by this repo's commit, so they carry their own anchors. **All verified
2026-08-06.**

| Claim | Artifact |
| --- | --- |
| "A functional unit of logic that can be invoked by a remote entity. Every operation is bound to a specific HTTP path and method." Defines a contract for query parameters, request payload, response payload. "They can also be triggered through an AI Agent, MCP, JSON-RPC, or GraphQL." | docs.fusio-project.org/docs/operation/ |
| "An action contains the business logic of your API endpoint. At the core an Action is a PHP class that receives an incoming request and returns a response." | docs.fusio-project.org/docs/action/ |
| "A connection enables Fusio to connect to other remote services." A connection's `getConnection()` "should return a fully configured object which can be used at an action"; credentials are collected in the connection's `configure()` form. The page states nothing about encryption at rest or key custody. | docs.fusio-project.org/docs/backend/api/connection/ |
| "Define your data models once using TypeSchema. Fusio uses this metadata to enforce strict validation and automatically generate OpenAPI specifications and multi-language client SDKs." | docs.fusio-project.org/ |
| OAuth2 with the Authorization Code, Resource Owner Password Credentials, Client Credentials and Refresh grants; the access token "has always an expire time and can be revoked"; scopes are an optional parameter of the app. No mention of capabilities, one-time grants, attenuation, approval gates or budgets. | docs.fusio-project.org/docs/security/authorization |
| MCP: "By default, every operation is exposed, you can provide a user id as an argument to expose only specific tools." STDIO via `php bin/fusio mcp`. HTTP transport is **disabled by default**, enabled by setting `fusio_mcp`, and "currently experimental, use it with caution." "VSCode can currently handle only 128 tools and the Fusio MCP server provides all operations of Fusio" — tools must be deselected by hand. The worked example resolves a chat request to the `backend-operation-getAll` tool. | docs.fusio-project.org/docs/protocol/mcp |
| Latest release **v7.1.0, 2026-07-18T16:52:24Z**; v7.0.0 2026-05-23. 7.0 added the agent concept, the `AgentCall` action, MCP server support and four built-in agents; 7.1 added a consumer agent, public agents, per-agent temperature and cost, and agents in deployment. | github.com/apioo/fusio releases (atom feed) |
| Apache-2.0, ~2.1k stars as displayed. | github.com/apioo/fusio |
| `apioo/fusio` master requires `php >=8.4` and `fusio/impl: ^8.0`; `apioo/fusio-impl` master requires `php >=8.4` and pulls ~20 `fusio/adapter-*` packages plus `mcp/sdk ^0.5`. | raw `composer.json` of each repo, master |
| Authorship, measured on `--filter=blob:none` clones (`git log --format='%an' \| sort \| uniq -c`): `apioo/fusio` 1965 of 1969 commits by one author (HEAD `2026-07-30`, "update deps"); `apioo/fusio-impl` 2872 of 2877 (HEAD `2026-08-01`, "update ai adapter"). | the two repositories at those HEADs |

**Inference** (marked as such, not as citation): the tagged distribution is 7.1.0
while its master already requires core `^8.0`, and the substance lives in
`fusio-impl` plus the adapter packages — so reading `apioo/fusio` is reading a
composer manifest and a shell, not the system. Any real audit has to follow the
dependency edge. Second inference: 99.8% single-author history in both
repositories is a bus factor of one, whatever the star count says.

## 2. Where the model coincides with ABR-0

```text
Operation      typed contract: path, method, in/out schema — the callable surface
    ↓
Action         the permitted business operation (a PHP class)
    ↓
Connection     a configured client for a remote service; credentials live here
```

The shape is the one ABR-0 §1 accepted: the caller names an operation, the
platform holds the credential, and the credential's scope is a property of the
connection rather than of whoever is calling. It is the rejection of
"full PAT plus a policy telling the agent not to use the rest of it", arrived at
independently by a project with entirely different motives — which is mild
evidence that the inversion is the natural one, not a local idiosyncrasy of 007.

The second coincidence is the multi-surface projection: one operation contract
is projected to REST, MCP, JSON-RPC, GraphQL and the platform's own agents, and
the same schema drives validation, OpenAPI and SDK generation. That is the
structural answer to the failure mode where a tool definition is hand-written
JSON that drifts from the runtime it claims to describe — which is rule 1 of
`docs/evidence-and-decision-discipline.md` (projection-bound contracts) stated in
product form. Fusio does not carry rule 1's evidence obligation (it does not bind
the digests of the actually-used projections and generators), so it demonstrates
the ergonomics of the pattern, not its verifiability.

## 3. Transferable

Recorded as things a later ABR slice may borrow, not as tasks:

1. **Operation → Action → Connection** as the three-layer split, with the
   credential bound to the connection. For 007 this is the difference between
   `arliai.review_patch` (bounded model, bounded parameters, bounded cost) and a
   general `POST /proxy`, which is a remote screwdriver an agent will eventually
   put into a socket.
2. **One contract, many surfaces** — a single action registry projected to REST,
   MCP, CLI and internal-agent tool surfaces, rather than four hand-maintained
   near-copies of the same operation.
3. **Schema as the source of the tool definition**, so an agent-visible tool is
   generated from the normative contract rather than written beside it.
4. **A separate registry of connections/secrets**, so replacing a provider or
   rotating a key does not change the operation an agent is allowed to call.
5. **Versioning/freezing an operation's configuration**, which is what makes a
   receipt about "the operation as it was" meaningful.

## 4. Where it stops being a trust boundary

Everything below is stated as *absence in the surveyed artifacts as of the
verification date*, not as a defect claim about code we have not read.

- Authorization is OAuth2 + scopes + expiring, revocable tokens. That is
  ordinary API authorization. The surveyed pages describe no attenuation (a
  right that can only be narrowed), no short-lived or one-shot delegated
  capability, no binding of a grant to a specific payload or task identity, no
  separate policy decision point, no approval gate, no per-operation budget, and
  no provenance a third party could verify.
- An action receives an already-configured client from the connection. From that
  point the secret's confinement is a property of the action's code — including
  the `worker-*` actions, which run user-supplied PHP, JavaScript, Java or
  Python. This is credential *injection into an execution platform*, which is a
  strictly weaker thing than a capability machine, and it is precisely the
  property `AGENTS.md` rule 1 and `docs/security-layers.md` refuse to concede in
  007.
- The MCP projection's default is allow-all: every operation becomes a tool, and
  the documented worked example resolves to `backend-operation-getAll` — that is
  the platform's own control plane appearing in an agent's tool list. Narrowing
  happens afterwards, by passing a user id, or by unticking boxes in an editor
  that cannot hold more than 128 tools. A default of "everything, minus what you
  remembered to remove" is an admin console's default. A trust boundary's default
  is nothing, and each addition names principal, purpose, operation and
  constraints.

One correction to the session summary this note was written from: the HTTP
transport is not merely experimental, it is **off unless `fusio_mcp` is set**.
The experimental-and-enabled reading overstates the exposure; the allow-all
projection, which is the load-bearing criticism, stands as written.

## 5. Direction (maintainer-stated, interactive session)

Recorded per rule 3's carve-out in `docs/evidence-and-decision-discipline.md`:
adjudicated interactively with the maintainer, therefore **ratified as
direction** — not as an implementation contract, and **not** as a change to
ABR-0's gate.

- Fusio is **not** a candidate core for 007: a broad PHP platform, tightly
  coupled to its own model, with a bus factor of one. Adopting it would import a
  trust boundary we would then have to prove, in a language and lifecycle the
  rest of the harness does not share.
- It **is** a useful reference implementation and organ donor for the five items
  in §3.
- The layer 007 must add on top is the layer §4 says is missing: policy decision
  point, approval gates, cost budgets, payload-bound constraints, short-lived
  capabilities, audit evidence, secret-non-disclosure tests, and **default-deny
  tool projection**.

Note the ordering constraint this implies and ABR-0 already anticipated: the
generic connector/registry layer of §3 does not become work until two real
adapters inside one authority model expose real duplication (ABR-0 §4). Fusio's
twenty-odd adapters are evidence that the layer *can* be built, not evidence that
we have earned it yet.

## 6. Non-goals

No PHP. No adoption of Fusio, its packages, or its schema dialect. No connector
layer, no token issuer, no policy language chosen. No implementation task is
created by this note, and the ABR gate remains MG-C.

The bicycle exists; it has the usual brakes. The version 007 needs is the one for
descending a mountain with a rider who is simultaneously blind, curious, and
spending someone else's money.
