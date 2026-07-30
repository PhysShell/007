/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { VitePWA } from "vite-plugin-pwa";

// https://vite.dev/config/
export default defineConfig({
  // Without this, Vite/Vitest resolves Svelte's package `exports` via the
  // default (non-browser) condition even under `environment: "jsdom"` —
  // Vitest doesn't run in a real browser, so Vite has no other signal to
  // prefer the client build, and picks Svelte's server-rendering build
  // instead (whose `mount()` doesn't exist — SSR renders to a string, it
  // doesn't mount to a DOM). `process.env.VITEST` is only set during a
  // Vitest run, so `vite build`/`vite dev` are unaffected.
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "jsdom",
    setupFiles: ["./src/setupTests.ts"],
    // Rules out an in-source `.svelte.ts` test accidentally shipping to prod
    // via the app's own dependency graph — Vitest sees `*.svelte.test.ts`/
    // `*.test.ts` only, never bundled by the `build` script above.
    include: ["src/**/*.test.ts"],
  },
  plugins: [
    svelte(),
    VitePWA({
      // R0 caches the static shell for offline load — it does not cache API
      // responses. `generateSW` with a narrow `globPatterns` (default: the
      // built JS/CSS/HTML) is exactly that scope; nothing here intercepts
      // `/api/*`, so a cached shell never presents stale run/event data as
      // fresh (see docs/q-deck/mobile-r0.md).
      registerType: "autoUpdate",
      manifest: {
        name: "Q-Deck",
        short_name: "Q-Deck",
        description: "007 control surface",
        start_url: "/",
        display: "standalone",
        background_color: "#101215",
        theme_color: "#101215",
        icons: [
          {
            src: "/favicon.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any",
          },
        ],
      },
    }),
  ],
  server: {
    proxy: {
      // In dev, Vite serves the SPA on its own port; o7d serves the API on
      // its own. Proxying here means the app's own fetch/EventSource calls
      // use the same relative `/api/v1/...` paths in dev and in production
      // (where o7d serves the built shell and the API from one origin) —
      // one code path, not a dev-only special case.
      "/api": {
        target: "http://127.0.0.1:4170",
        changeOrigin: true,
      },
    },
  },
});
