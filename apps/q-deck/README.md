# Q-Deck

A mobile-first, read-only web view of 007 agent runs — check in on a run
from a phone without SSH or tmux. This is the R0 ("Observe") vertical:
dashboard, per-run timeline, per-conversation timeline, all live via SSE.
No run creation or control actions yet — see `docs/q-deck/architecture.md`
at the repo root for the full design and R0 non-goals.

Svelte 5 + TypeScript + Vite. Node tooling is build-time only — the deployed
artifact is static assets (`dist/`), served by `o7d` in production; no Node
server runs alongside it.

## Local development

This app is a read-only client of `o7d` (`crates/o7d` at the repo root) —
nothing here talks to SQLite directly. Run both:

```sh
# from the repo root: start o7d against a ledger file
cargo run -p o7d -- serve --ledger /path/to/ledger.sqlite3

# from apps/q-deck: start the Vite dev server
npm install
npm run dev
```

`vite.config.ts` proxies `/api/*` to `o7d` on `127.0.0.1:4170` (its default),
so the app's own `fetch`/`EventSource` calls use the same relative
`/api/v1/...` paths in dev as they do in production — no dev-only branch in
the client code.

## Scripts

- `npm run dev` — Vite dev server with the `/api` proxy above.
- `npm run build` — production build to `dist/` (static shell + service
  worker + manifest, via `vite-plugin-pwa`).
- `npm run check` — `svelte-check` + `tsc`, no emit.
- `npm run test` — Vitest (`src/**/*.test.ts`): dedup, reconnect state,
  dashboard states, unknown-event/large-payload safety, router resolution.

## Production

```sh
npm run build
o7d serve --ledger /path/to/ledger.sqlite3 --static-dir apps/q-deck/dist
```

`o7d` then serves `/api/v1/*` and the built shell from one origin, with a
SPA fallback to `index.html` for client-side routes (`/runs/:id`,
`/conversations/:id`) that have no file of their own — see
`docs/q-deck/http-api-v1.md`.
