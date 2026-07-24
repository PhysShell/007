/*
 * Shared helper: serve the built ./dist through Playwright request interception,
 * so no listening socket is bound (this environment kills processes that bind a
 * port). Used by both the screenshot and the offline-smoke scripts.
 */
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, extname } from "node:path";

export const DIST = fileURLToPath(new URL("../dist/", import.meta.url));
export const ORIGIN = "http://cockpit.local";

// Resolve a browser executable: prefer an explicit env override, then the
// environment's pre-installed binary, else `undefined` so Playwright uses its own
// managed browser (e.g. after `npx playwright install chromium` in CI).
function resolveExe(envVar, preinstalled) {
  if (process.env[envVar]) return process.env[envVar];
  if (existsSync(preinstalled)) return preinstalled;
  return undefined;
}

// Headless shell: light + fast, used for static screenshots. Does NOT support
// service workers.
export const CHROMIUM = resolveExe(
  "PW_CHROMIUM",
  "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell"
);
// Full Chromium: heavier but supports service workers — required by the offline
// PWA smoke.
export const CHROMIUM_FULL = resolveExe(
  "PW_CHROMIUM_FULL",
  "/opt/pw-browsers/chromium-1194/chrome-linux/chrome"
);
export const LAUNCH_ARGS = ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"];

const CT = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json",
  ".webmanifest": "application/manifest+json",
  ".map": "application/json",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

export function makeServeRoute({ spaFallback = true } = {}) {
  return async function serveRoute(route) {
    const url = new URL(route.request().url());
    let pathname = decodeURIComponent(url.pathname);
    if (pathname === "/" || pathname.endsWith("/")) pathname += "index.html";
    const filePath = join(DIST, pathname);
    try {
      const body = await readFile(filePath);
      await route.fulfill({
        body,
        contentType: CT[extname(filePath)] || "application/octet-stream",
      });
    } catch {
      if (spaFallback) {
        await route.fulfill({
          body: await readFile(join(DIST, "index.html")),
          contentType: CT[".html"],
        });
      } else {
        await route.fulfill({ status: 404, body: "not found" });
      }
    }
  };
}
