# cockpit-slice-0 — UI spike (draft / non-mergeable)

A **UI-only, fixture-backed** mobile-first React + Vite PWA shell for the *future*
007 Cockpit (roadmap **PR 9**). It is **not** the production Cockpit and **not**
an implementation of PR 9, and it stays draft until the canonical event protocol
is frozen in **PR 4**.

Full rationale, invariants, and the integration plan live in
[`../docs/ui/cockpit-slice-0.md`](../docs/ui/cockpit-slice-0.md).

## Quick start

```bash
npm install
npm run dev        # local prototype
npm test           # reducer/view-model + component + a11y (axe) checks
npm run build      # type-check + static bundle
npm run screenshots  # after build: mobile + desktop PNGs → ./screenshots
```

## What it does and does not do

- **Does:** render every conversation/timeline/run-graph/composer/controls state
  from a deterministic fixture catalog; fold duplicate + out-of-order deliveries
  into a stable view model; demonstrate offline/reconnect/replay.
- **Does not:** launch or own any run; talk to a backend, ledger, worker, or
  daemon; use Tauri, SQLite, WebSocket, or HTTP; import Rust crates or change the
  Cargo workspace; define any canonical event names.

## Layout

```
src/types/       UI-ONLY · PROVISIONAL fixture-event and view-model types
src/data/        the CockpitEventSource seam + the pure fold (dedup/order)
src/fixtures/    deterministic scenario catalog (no clock, no randomness)
src/components/  presentational React components (pure)
src/app/         the one impure hook layer + top-level shell
scripts/         screenshot capture (Playwright, offline via request interception)
```

The **seam** (`src/data/cockpit-data-source.ts`) is the point where, after PR 4, a
`LedgerCockpitAdapter` will fold canonical ledger events into the same view model
with no change to the components.
