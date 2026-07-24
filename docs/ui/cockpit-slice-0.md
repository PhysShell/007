# Cockpit — Slice 0 (UI spike)

> **Status: DRAFT / NON-MERGEABLE spike.** This document and the code under
> [`/cockpit-slice-0`](../../cockpit-slice-0) are a **UI-only, fixture-backed
> prototype** for the *future* 007 Cockpit (roadmap **PR 9** in
> `evidence/phase-minus-one/operator-decision.md`). It is **not** the production
> Cockpit and **not** an implementation of PR 9. It stays draft and
> non-mergeable until the **canonical event protocol is frozen in PR 4**.
>
> It touches **no** backend, ledger, worker, worktree, verifier, canonical
> protocol, or roadmap. It imports **no** Rust crates and does **not** change the
> Cargo workspace. It defines **no** canonical event names and **no**
> authoritative protocol documentation.

Branch: `spike/cockpit-slice-0`, cut from the same post-merge `main` SHA that the
next roadmap PR (PR 3 — worktree & verifier) branches from
(`aa2d29ae`, the merge of PR 1 / the ledger).

---

## What this is

A mobile-first React + Vite PWA **shell** that renders every screen and state the
Cockpit will eventually need, driven **only** by a deterministic in-memory
fixture catalog. Its job is to de-risk the presentation layer and pin down a
**narrow seam** between the UI and its data source, so that when PR 4 freezes the
canonical event protocol, a single adapter can fold canonical ledger events into
the same view model with no change to the components.

- Run it: `cd cockpit-slice-0 && npm install && npm run dev`
- Test it: `npm test` (reducer/view-model, component, and axe a11y smoke checks)
- Screens: `npm run build && npm run screenshots` (see
  [`/cockpit-slice-0/screenshots`](../../cockpit-slice-0/screenshots))

### The seam (the reason this spike exists)

The seam is **split into a read side and a command side** (`src/data/cockpit-data-source.ts`),
because the two graduate to *different* production implementations:

```
                    ┌─ CockpitReadSource ──▶ fold() ──▶ UiConversationViewModel ─▶ components
 fixture store ─────┤   (discovery/snapshot/subscribe)   (pure)   (UI-only, provisional)
                    └─ CockpitCommandPort ◀── send/stop/permission/model-lock/reconnect (mock)
```

- **`CockpitReadSource`** — conversation discovery, snapshot, and per-conversation
  subscription. **After PR 4**, a `LedgerCockpitAdapter` implements this side by
  folding canonical ledger events into the UI event shape (or the shape is
  replaced and the fold updated). Components and the fold do not change.
- **`CockpitCommandPort`** — `send` / `stop` / `setPermission` / `setModelLock` /
  `reconnect`. In production these become **daemon/o7d-owned actions over the real
  transport** — authenticated RPCs, never in-process calls. The read adapter and
  the command transport are deliberately separate objects; the presentation layer
  must not assume they are the same thing (though the fixture backs both with one
  store).
- **Dynamic conversation discovery** is provided now via
  `CockpitReadSource.subscribeConversations` (fires with the current set, then
  again whenever it changes) — so discovery is a real part of the seam, not an
  unresolved gap. The fixture set happens to be static.
- **`fold()`** (`src/data/fold.ts`) deduplicates by event `id` and orders strictly
  by `uiSeq`, so duplicate and out-of-order delivery reconcile deterministically.
  This mirrors — in UI-only terms — the ledger's idempotency-key +
  per-conversation monotonic-sequence guarantees; the real guarantees live in the
  daemon/ledger.

All fixture and view-model TypeScript types carry a file-level banner marking them
**UI-ONLY · PROVISIONAL** (`src/types/*.ts`).

---

## 1. Approved product invariants (these are NOT up for grabs)

These come from the committed design record — `ROADMAP.md` (in the `tandem`
repo), `evidence/phase-minus-one/operator-decision.md`,
`evidence/phase-minus-one/consilium-phase-minus-one.md`, and the ledger crate
docs. The UI is built to honor them and must keep honoring them:

| # | Invariant | Where the UI reflects it |
|---|-----------|--------------------------|
| I1 | **UI death ≠ agent death.** The run outlives any client, tab, or phone. | Mock run state lives in the data source, decoupled from React. Unmounting a client only unsubscribes; it never changes run state. Covered by tests in `cockpit-data-source.test.ts` and `cockpit.test.tsx`. |
| I2 | **The UI never owns run lifetime.** It observes; it does not launch/own/kill a worker. | The `CockpitEventSource` seam exposes observe + fixture-only actions; there is no run-control API. |
| I3 | **Honest state.** Never show `running` for something a reboot actually killed. | `interrupted` run status + the `INTERRUPTED_BY_HOST_RESTART` terminal-failure card (interrupted/recovered scenario). |
| I4 | **Deterministic veto.** The sandbox + explicit policy have the final say; the UI must never show a lit button that lies about the underlying process. | Controls show **requested vs effective** for both permission mode and model, with an explicit `requested ≠ effective` indicator; the permission decision buttons are labelled "owned by o7d, not the UI". |
| I5 | **Exact model — no silent fallback.** Drift trips a kill switch. | Model-lock indicator + `model drift` badge when requested ≠ effective (model-mismatch scenario). |
| I6 | **Append-only, ordered, replayable, idempotent** event history per conversation. | The fold dedups by `id` and orders by `uiSeq`; the replay scenario delivers duplicates + out-of-order events and still converges. |
| I7 | **o7d owns the verdict.** Accept/fail is not the UI's and not an LLM's. | Result cards are labelled "o7d verdict"; gate/verifier failure produces a `rejected` result regardless of what the agent said. |
| I8 | **Claude, Codex, and system/o7d are distinct actors** that can share one conversation. | Per-source color-coding + badges throughout, WITHOUT assuming their wire events are defined yet (see §3). |

## 2. Provisional UI decisions (deliberate, changeable, not load-bearing)

These are choices made **only** to make the spike concrete. None of them is a
commitment; expect them to be revisited:

- **UI event vocabulary.** The fold input is a UI-only discriminated union
  (`userMessage`, `agentMessageDelta`, `toolActivity`, `permissionRequest`,
  `artifactCard`, `gateCard`, `resultCard`, `terminalFailure`, `runStatus`,
  `delegation`, `controlState`, `connection`). These are **rendering intents**,
  deliberately **not** the roadmap's provisional wire names.
- **`uiSeq` as the ordering key** and **`id` as the dedup key.** UI-only number
  spaces; not the ledger's sequence or idempotency key.
- **View-model shape** (`UiConversationViewModel`): a timeline list, a run tree, a
  controls block, a connection block, and a composer block. Grouping rules (tool
  phases collapse by `toolCallId`, streaming chunks concatenate by `messageId`,
  gates by `gateId`, permissions by `requestId`) are UI conveniences.
- **Three tabs** (Timeline · Runs · Controls) on mobile, two-pane at ≥900px. Pure
  layout.
- **Permission modes** shown as `plan · ask · acceptEdits · auto · bypass`, and
  the mock rule that `bypass` resolves to an effective `auto` (because sandbox
  attestation is unknown in the spike). Placeholder policy.
- **Visual language**: dark theme, Claude=violet, Codex=teal, system/o7d=amber,
  user=blue. Cosmetic.
- **PWA app-shell service worker** — a *real* precache of the built shell,
  generated at build time by `vite.config.ts` (`cockpit-sw-precache`): it precaches
  `index.html` + the **content-hashed JS/CSS** + manifest + icon, serves
  navigations from the cached shell, and **never** falls back to `index.html` for a
  missing JS/CSS/asset (a missing asset fails honestly). Proven by
  `npm run offline-smoke`: install online, then a **fresh navigation while offline**
  renders the Cockpit shell entirely from the precache. Still a shell cache only —
  not a transport, and expected to be replaced by the production connectivity story.
- **Composer send/stop/attachment** are fixture-driven mock actions that append
  mock events; attachments are placeholders.

## 3. Unknown until PR 4 (wire / protocol details the UI must NOT assume)

The spike is explicitly built to avoid pre-empting these. They are open and owned
by PR 4 (`operator-decision.md` lists the *provisional* event set PR 4 will
define — `agent.init`, `agent.message.delta`, `tool.requested`, `tool.started`,
`tool.completed`, `permission.requested`, `permission.changed`, `rate_limit`,
`model.observed`, `model.drift`, `delegation.requested`, `delegation.accepted`,
`artifact.published`, `gate.completed`, `run.completed`):

- **Canonical event names, tags, and payload schemas.** The UI kinds in §2 are
  placeholders; the mapping to canonical names is a PR-4/adapter task, not
  decided here.
- **Agent identity / attribution on the wire.** How an event declares it came
  from Claude vs Codex vs o7d is undefined. The UI carries a provisional `source`
  label and must not assume a particular wire encoding of it. (This is why the
  Claude/Codex/system distinction is done **without** assuming their wire events
  exist.)
- **The authoritative run state machine** and its exact state names/transitions.
  The UI's `queued/running/waiting/cancelling/completed/failed/interrupted` is a
  presentation set, not the o7d state machine.
- **Cursor/replay semantics** (the real `?after=<seq>` contract), reconnect
  framing, and how "recovered" history is marked on the wire.
- **Streaming delta framing** (chunk boundaries, message identity, done-signal).
- **Permission request/decision payloads**, model-identity/`model.observed`
  payloads, rate-limit/quota shapes.
- **Delegation/TaskSpec payloads** and how parent/child run identity is carried.
- **Transport** (the real daemon connection): out of scope entirely. No
  WebSocket/HTTP/Tauri/SQLite here.

## 4. To be removed or replaced at integration

When the spike graduates toward the real PR 9 Cockpit, these are the pieces that
**must** be deleted or swapped — none of them should reach production as-is:

- **`src/fixtures/**` — the entire fixture catalog.** Replaced by real ledger
  data through the adapter.
- **`FixtureCockpitDataSource`** and the mock command methods. The read side is
  replaced by a `LedgerCockpitAdapter` (subscribe); the command side by real,
  daemon-owned actions (which must go through o7d over the transport, never
  in-process). The `CockpitReadSource` / `CockpitCommandPort` split is designed to
  survive; the fixture object backing both is not.
- **The UI-only event union and `uiSeq`/`id` keys** — replaced or re-derived from
  the canonical protocol once PR 4 lands; the UI-side dedup/order fold becomes a
  thin fallback behind the ledger's own guarantees.
- **The generated app-shell service worker (`vite.config.ts` `cockpit-sw-precache`)
  + `src/sw-register.ts`** — a shell-only precache. Replaced by whatever the
  production connectivity/offline story is.
- **The mock permission/model rules** (e.g. `bypass → auto`) — replaced by the
  real policy engine's `requested`/`effective` values from o7d.
- **`?c=<id>` deep-link shim** in `main.tsx` — replaced by real routing/session
  restore (roadmap S4 / workspace restore).
- **This spike's CI workflow** (`.github/workflows/cockpit-slice-0-ui.yml`) —
  a draft-branch-only, path-scoped check. Not part of the merge gate; removed or
  folded into the real UI pipeline at integration. It deliberately does not touch
  `o7-worker-gate.yml`.

---

## Fixture scenarios (deterministic catalog)

`src/fixtures/catalog.ts` — no clock, no randomness; the same catalog folds to the
same view model every run.

| Scenario id | Demonstrates |
|-------------|--------------|
| `conv-empty` | New empty conversation; empty state; enabled composer |
| `conv-claude-active` | Active Claude run; streaming message; tool activity; stop control |
| `conv-replay` | Client disconnect mid-run, then reconnect **replay with duplicate + out-of-order delivery** |
| `conv-delegation` | Claude parent + Codex child; delegation branch in the run graph; jump-to-run |
| `conv-permission` | Pending permission request + a resolved one |
| `conv-model-mismatch` | requested ≠ effective model; model-lock + drift |
| `conv-verifier-failure` | Failed gate → terminal failure → rejected o7d verdict |
| `conv-artifact-gate` | Artifact + passing gate → accepted o7d verdict → completed run |
| `conv-interrupted` | Recovered historical items + `INTERRUPTED_BY_HOST_RESTART` + fresh queued attempt |
| `conv-concurrent-a` / `conv-concurrent-b` | Two simultaneously-active conversations (unread + activity in the list) |

## Deliverables map

| Deliverable | Location |
|-------------|----------|
| Working local prototype | `cockpit-slice-0/` (`npm run dev`) |
| Component tests | `src/components/cockpit.test.tsx` |
| Reducer / view-model tests | `src/data/fold.test.ts`, `src/data/cockpit-data-source.test.ts` |
| Accessibility smoke checks | axe checks in `src/components/cockpit.test.tsx` |
| Offline PWA smoke (install online → render offline) | `scripts/offline-smoke.mjs` (`npm run offline-smoke`) |
| Deterministic fixture catalog | `src/fixtures/catalog.ts` |
| Mobile + desktop screenshots | `cockpit-slice-0/screenshots/` |
| This doc | `docs/ui/cockpit-slice-0.md` |

## Explicit non-goals (guardrails honored)

No worker launch; no run-lifetime ownership; no Tauri server lifecycle; no direct
SQLite; no production WebSocket/HTTP; no Rust crate imports; no Cargo workspace
change; no canonical event names; no authoritative protocol docs; no self-accept;
no merge. `.github/workflows/o7-worker-gate.yml` is untouched.
