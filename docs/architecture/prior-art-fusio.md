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

Per rule 4 of `docs/evidence-and-decision-discipline.md`, each claim binds an
artifact **revision**, not merely a date. These are external artifacts, not
captured by this repo's commit, so they carry their own anchors:

```text
documentation prose   apioo/fusio-docs
                      0b369b724b9dd687630bf8ced15be1221f49dbc2  (main, 2026-06-13)
                      — the docs site is generated from this repo; the source
                        .md is the immutable anchor, the rendered page is not
distribution repo     apioo/fusio
                      884c9c3bd047a2f4b7ed81f0c08cae0ada9e5a67  (master, 2026-07-30)
core implementation   apioo/fusio-impl
                      6bffff4d24fa71e15180e631c6a40a5b1cf28fd9  (master, 2026-08-01)
release               apioo/fusio tag v7.1.0 =
                      febf15d39e1926e9a9011ef94081d19911fa1e59
                      atom <updated> 2026-07-18T16:52:24Z
```

These four full object IDs are the binding. Branch names and dates are context
only — a branch moves, and a date records when someone looked. The seven-character
forms used below (`0b369b7`, `884c9c3`, `6bffff4`, `febf15d`) abbreviate exactly
these IDs and nothing else; resolve them here, not against whatever a future
upstream `git rev-parse` makes of a prefix.

Read on the rendered site 2026-08-06; **every quotation below was then
re-derived verbatim from the source `.md` at `0b369b7` on 2026-08-07**, and two
were wrong on the first pass — the operation definition had lost its opening
clause, and the multi-protocol sentence had been split into a separate sentence
it is not. Both are corrected here. That is the argument for binding revisions
rather than dates, made at our own expense: a rendered page read through a
summarizer is not a quotable artifact.

One claim below has no immutable anchor and is marked in place. Where a fact is
about the rendered site rather than the source (the star count), it is stale by
construction and load-bears nothing.

| Claim | Artifact @ revision |
| --- | --- |
| "An operation describes a functional unit of logic that can be invoked by a remote entity. Every operation is bound to a specific HTTP path and method but they can also be triggered through an [AI Agent], [MCP], [JSON-RPC], or [GraphQL]." Defines a strict contract for query parameters, request payload, response payload. | `docs/operation/index.md` @ `0b369b7` |
| "An action contains the business logic of your API endpoint. At the core an Action is a PHP class that receives an incoming request and returns a response." | `docs/action/index.md` @ `0b369b7` |
| "A connection enables Fusio to connect to other remote services. This can be i.e. a database or message queue service." — "In general a connection should return a fully configured object which can be used at an action." The page describes no encryption at rest and no key custody. | `docs/backend/api/connection/index.md` @ `0b369b7` |
| "The Worker-Python executes the provided Python code at the remote worker", and the example handler obtains its client with `connector.get_connection('App')`. Sibling pages exist for Java, JavaScript and PHP. | `docs/backend/api/action/worker-python.md` @ `0b369b7` |
| "Define your data models once using TypeSchema. Fusio uses this metadata to enforce strict validation and automatically generate OpenAPI specifications and multi-language client SDKs." | `src/components/HomepageFeatures/index.tsx` @ `0b369b7` |
| OAuth2 with the Authorization Code, Resource Owner Password Credentials, Client Credentials and Refresh grants; the access token has "an expire time and can be revoked"; scopes are "Optional the scopes which are needed by your app". No capabilities, one-time grants, attenuation, approval gates or budgets appear on the page. | `docs/security/authorization.md` @ `0b369b7` |
| MCP: "By default, every operation is exposed, you can provide a user id as an argument to expose only specific tools." STDIO via `php bin/fusio mcp`. "The HTTP transport is by default disabled. To activate it you need to set the `fusio_mcp` configuration to `true`. Note the HTTP transport is currently experimental, use it with caution." "VSCode can currently handle only 128 tools and the Fusio MCP server provides all operations of Fusio" — tools must be deselected by hand. The worked example: "you can ask to show all available operations that should trigger the `backend-operation-getAll` tool." | `docs/protocol/mcp.md` @ `0b369b7` |
| Latest release **v7.1.0**, atom `<updated>` 2026-07-18T16:52:24Z; v7.0.0 2026-05-23. 7.0 added the agent concept, the `AgentCall` action, MCP server support and four built-in agents; 7.1 added a consumer agent, public agents, per-agent temperature and cost, and agents in deployment. | tag `v7.1.0` = `febf15d`; release notes on the releases feed |
| `composer.json` requires `php >=8.4` and `fusio/impl: ^8.0`. Apache-2.0. | `apioo/fusio` @ `884c9c3` |
| `composer.json` requires `php >=8.4`, ~20 `fusio/adapter-*` packages, and `mcp/sdk ^0.5`. | `apioo/fusio-impl` @ `6bffff4` |
| Authorship (`git log --format='%an' \| sort \| uniq -c` on `--filter=blob:none` clones): 1965 of 1969 commits by one author; 2872 of 2877 in the core. | `884c9c3` and `6bffff4` respectively |
| ~2.1k stars — **no immutable anchor**, a rendered counter read 2026-08-06; recorded for scale only and load-bearing on nothing. | github.com/apioo/fusio |

**Inference** (marked as such, not as citation): the tagged distribution is 7.1.0
while `884c9c3` already requires core `^8.0`, and the substance lives in
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

Everything below is stated as *absence in the artifacts pinned in §1, at those
revisions*, not as a defect claim about code we have not read. Each is stale the
moment its artifact moves — `0b369b7` is a June commit on a project releasing
monthly, so treat the documentation claims as the shortest-lived here.

- Authorization is OAuth2 + scopes + expiring, revocable tokens. That is
  ordinary API authorization. The surveyed pages describe no attenuation (a
  right that can only be narrowed), no short-lived or one-shot delegated
  capability, no binding of a grant to a specific payload or task identity, no
  separate policy decision point, no approval gate, no per-operation budget, and
  no provenance a third party could verify.
- An action receives an already-configured client from the connection. From that
  point the secret's confinement is a property of the action's code — including
  the `worker-*` actions, whose own documentation says they execute "the provided
  Python code at the remote worker" and whose example handler reaches the client
  through `connector.get_connection('App')` (`worker-python.md` @ `0b369b7`;
  Java, JavaScript and PHP siblings exist). Credential *injection into an
  execution platform*, which is a
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

## 7. What this hands to MG-C, and then closes

The survey is finished. Recorded per rule 3's carve-out (adjudicated
interactively with the maintainer), therefore ratified as direction:

- **Fusio enters no dependency** — not `fusio`, not `fusio-impl`, not its MCP
  layer. Reference implementation, not foundation.
- **Operation → Action → Connection is a conceptual decomposition, not an API
  contract.** We know which entities will probably surface; that is not licence
  to grow an `ActionInterface`, a `ConnectionRegistryFactoryProvider`, and the
  rest of the architectural vegetation before two real adapters exist. ABR-0 §4's
  two-adapter precondition governs the shape, not only the connector kit.
- **The next informative step is MG-C's own implementation** — not ABR-2, and not
  ten more API-management products. This note closes the Fusio question.

The point of MG-C is not to prove we can write two adapters. It is to **discover
the minimal common authority surface** — the part of the contract that is
actually shared rather than the part we would like to be shared. The shape to
test against:

```text
caller → capability/authority → operation → policy + constraints
       → provider adapter → external system
```

and not:

```text
caller → generic provider abstraction factory manager
       → forty interfaces → two adapters that differ anyway
```

The second is what growing an architecture from a photograph of Fusio produces:
an enterprise framework arriving before the product.

### Candidate probes (not requirements)

Each must be either confirmed by the implementation or deliberately rejected. A
probe quietly dropped is the failure this list exists to prevent.

```text
default deny                       a new adapter/action is never agent-reachable
                                   by virtue of existing
control/business-plane separation  administrative operations cannot land in an
                                   agent or MCP projection by accident
secret non-disclosure              the agent receives a right to act, never a
                                   credential
authority binding                  the same adapter cannot be invoked with
                                   authority the calling principal lacks
constraints before dispatch        model, budget, payload limits checked before
                                   the provider is touched
transport independence             REST/MCP/CLI do not each re-derive
                                   authorization semantics
auditability                       who → which right → which operation → which
                                   adapter → which outcome, recoverable
adapter substitutability           two providers share exactly the common part of
                                   the contract, not the wished-for part
```

### The regression anti-example

`backend-operation-getAll` (§4) is kept as a concrete anti-example rather than a
remark. If a registry is ever projected into a tool surface automatically, a test
must establish:

```text
control-plane capability  ×  agent projection  =  DENY
```

— membership in the registry does not by itself imply exportability.

The generalization it forces is the useful part: an operation descriptor needs
distinct notions —

```text
exists · callable · delegatable · agent-visible · externally-exposable
```

— rather than one dispirited `enabled: true`. That distinction is the whole
difference between a default-deny projection and an admin console.
