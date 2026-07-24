/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// UI-ONLY spike config. No proxy, no backend, no server integration — the app is
// fixture-backed and must build to a fully static bundle.

/**
 * Generates the app-shell service worker at build time with a precache list that
 * includes the CONTENT-HASHED JS/CSS assets Vite emits — not just index/manifest/
 * icon. The generated SW:
 *   • precaches the full shell (index.html + hashed js/css + manifest + icon),
 *   • serves navigations from the cached index.html (so an offline reload renders),
 *   • serves other GETs cache-first then network, and NEVER falls back to
 *     index.html for a missing JS/CSS/asset (a missing asset must fail, not be
 *     masked by an HTML document).
 * This is a spike shell-cache only; the production Cockpit owns real connectivity
 * and is expected to replace it wholesale.
 */
function generateServiceWorker(): Plugin {
  return {
    name: "cockpit-sw-precache",
    apply: "build",
    generateBundle(_options, bundle) {
      const assets = Object.keys(bundle)
        .filter((f) => /\.(js|css)$/.test(f))
        .map((f) => "./" + f)
        .sort();
      const precache = [
        "./",
        "./index.html",
        "./manifest.webmanifest",
        "./icon.svg",
        ...assets,
      ];
      // Deterministic cache version derived from the (content-hashed) asset names.
      let h = 0;
      const key = precache.join("|");
      for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
      const version = (h >>> 0).toString(36);

      const sw = `/* GENERATED at build by vite.config.ts (cockpit-sw-precache). Do not edit. */
const CACHE = "cockpit-slice-0-${version}";
const PRECACHE = ${JSON.stringify(precache)};

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE).then((c) => c.addAll(PRECACHE)).then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;

  // Navigations (address bar, reload, deep link) → the cached app shell, so an
  // offline reload still renders the Cockpit. This is the ONLY index.html fallback.
  if (request.mode === "navigate") {
    event.respondWith(
      caches.match("./index.html").then((cached) => cached || fetch(request))
    );
    return;
  }

  // Everything else: cache-first, then network. NO index.html fallback — a missing
  // JS/CSS/asset must fail honestly rather than be masked by an HTML document.
  event.respondWith(caches.match(request).then((hit) => hit || fetch(request)));
});
`;
      this.emitFile({
        type: "asset",
        fileName: "service-worker.js",
        source: sw,
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), generateServiceWorker()],
  base: "./",
  build: {
    outDir: "dist",
    sourcemap: true,
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
  },
});
