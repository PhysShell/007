/* Registers the app-shell service worker so the spike installs as a PWA and can
 * demonstrate an offline shell. No-op outside the browser / when unsupported.
 * The production Cockpit owns real connectivity; this is a shell-only stopgap. */
export function registerAppShellSW(): void {
  if (typeof window === "undefined") return;
  if (!("serviceWorker" in navigator)) return;
  if (!import.meta.env.PROD) return; // don't cache during dev
  window.addEventListener("load", () => {
    navigator.serviceWorker
      .register(`${import.meta.env.BASE_URL}service-worker.js`)
      .catch(() => {
        /* shell caching is best-effort; ignore failures in the spike */
      });
  });
}
