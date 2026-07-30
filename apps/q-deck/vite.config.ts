import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { VitePWA } from "vite-plugin-pwa";

// https://vite.dev/config/
export default defineConfig({
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
