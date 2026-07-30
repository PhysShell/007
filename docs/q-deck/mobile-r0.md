# Q-Deck mobile R0

## Design constraints

- Designed first for ~360–430px widths (a phone in portrait), not shrunk down
  from a desktop layout.
- Every tap target is at least 44px tall (Apple/WCAG's minimum), including
  full-width link/button "cards" on the dashboard.
- The page itself never scrolls horizontally. The one place wide content
  exists — an event's raw JSON payload — scrolls inside its own bounded
  container (`max-height: 40vh; overflow: auto` in `EventTimeline.svelte`),
  not the page.
- Light/dark follows the system (`prefers-color-scheme`), via CSS custom
  properties in `src/app.css` — no manual theme toggle in R0.
- No decorative chrome: no radar sweeps, spy silhouettes, or animated
  crosshairs. The one animated element is the connection-status dot's pulse,
  which carries real information (connecting/reconnecting).

## Honesty about state

A mobile client sees its network drop far more often than a desktop one.
Q-Deck's rule throughout: **never silently present stale data as fresh.**

- The dashboard keeps showing its last successfully-fetched run list on a
  failed poll, with an explicit "Offline — showing last known state" banner
  — going blank on a transient blip would be a worse failure than slightly
  stale data honestly labeled.
- The run/conversation pages' live timeline shows a `ConnectionIndicator`
  (`connecting` / `open` / `reconnecting` / `closed`) reflecting the SSE
  connection's real state — never a fake "live" indicator once the
  underlying stream has actually dropped.
- No fabricated progress percentages anywhere (the original brief's explicit
  call-out) — a run's state is exactly what the ledger says it is:
  queued/running/completed/failed/cancelled/interrupted, nothing invented in
  between.

## Offline shell (PWA)

`vite-plugin-pwa` (`generateSW` mode) precaches the built static shell
(HTML/JS/CSS — see `vite.config.ts`) so the app *shell* loads offline and is
installable. This deliberately does **not** cache `/api/*` responses — a
cached run list served while offline would be exactly the kind of
"stale-presented-as-fresh" data this milestone's honesty rule forbids. An
offline load shows the shell with its own loading/offline states, not phantom
data.

## What was deliberately cut from R0's scope

- **Per-run "latest meaningful event" on the dashboard.** `RunDto` carries no
  event summary, and there is no batch endpoint for it. Fetching each
  visible run's latest event individually would be an N+1 query pattern
  against the ledger for every dashboard poll — exactly the kind of
  unbounded-read pattern the ledger's own API design (`MAX_LIST_LIMIT`,
  `MAX_READ_LIMIT`) exists to prevent. Left out rather than done cheaply and
  wrong.
- **A real browser-driven phone-viewport smoke test.** The project brief
  itself allows this ("if the existing CI budget can support it") and
  explicitly warns against making local development depend on downloading a
  large browser bundle on this 2GB VPS. The frontend test suite
  (`apps/q-deck/src/**/*.test.ts`, Vitest + jsdom + Testing Library) proves
  component logic and state — dedup, reconnect status, dashboard states, no
  mutation controls — but jsdom has no real layout/rendering engine, so it
  cannot itself prove the CSS actually reads correctly at a 375px viewport.
  A Playwright-based phone-viewport smoke test on a hosted CI runner is the
  right follow-up, not something to fake locally.
