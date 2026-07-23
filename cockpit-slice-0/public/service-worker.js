/*
 * UI-ONLY, PROVISIONAL app-shell service worker for the Cockpit Slice 0 spike.
 *
 * This exists ONLY so the prototype installs as a PWA shell and demonstrates an
 * offline state. It caches the static bundle. It is NOT a real sync/transport
 * layer — there is no backend, no WebSocket, no HTTP data plane in this spike.
 * The production Cockpit (roadmap PR 9) will own real connectivity; this file is
 * expected to be replaced wholesale at integration.
 */
const CACHE = "cockpit-slice-0-shell-v1";
const SHELL = ["./", "./index.html", "./manifest.webmanifest", "./icon.svg"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
      )
  );
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  event.respondWith(
    caches.match(request).then((hit) => {
      if (hit) return hit;
      return fetch(request)
        .then((res) => {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put(request, copy)).catch(() => {});
          return res;
        })
        .catch(() => caches.match("./index.html"));
    })
  );
});
