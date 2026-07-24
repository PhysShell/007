/*
 * Offline PWA smoke test.
 *
 * Proves the reviewer's requirement: after ONE online load/install, a FRESH
 * navigation while offline still renders the Cockpit shell — served entirely by
 * the service worker's precache (including the content-hashed JS/CSS).
 *
 * Method (no listening socket — this environment kills processes that bind a
 * port): serve ./dist via Playwright request interception. Phase 1 loads online,
 * installs the SW, and waits for the shell (incl. a hashed JS and CSS asset) to be
 * precached. Phase 2 goes OFFLINE, REMOVES the interception "server" entirely, and
 * opens a brand-new page — so the only thing that can answer is the SW cache.
 *
 * Requires a prior `npm run build`. Prints PASS/FAIL and sets the exit code.
 */
import { chromium } from "playwright";
import {
  ORIGIN,
  CHROMIUM_FULL,
  LAUNCH_ARGS,
  makeServeRoute,
} from "./serve-dist.mjs";

function fail(msg) {
  console.error("OFFLINE-SMOKE: FAIL —", msg);
  process.exitCode = 1;
}

async function main() {
  // Service workers require the FULL Chromium, not the headless shell (which is
  // Playwright's default for headless chromium and lacks SW support). Prefer an
  // explicit full-Chromium binary; otherwise force the full build via
  // `channel: "chromium"` (what `npx playwright install chromium` provides in CI).
  const launchOptions = {
    headless: true,
    // Treat the virtual origin as secure so service workers + CacheStorage are
    // available exactly as they would be over https / on the phone.
    args: [...LAUNCH_ARGS, `--unsafely-treat-insecure-origin-as-secure=${ORIGIN}`],
  };
  if (CHROMIUM_FULL) launchOptions.executablePath = CHROMIUM_FULL;
  else launchOptions.channel = "chromium";
  const browser = await chromium.launch(launchOptions);
  const context = await browser.newContext({ serviceWorkers: "allow" });

  // The "server": SPA-fallback ON here so the first online load behaves normally.
  await context.route("**/*", makeServeRoute({ spaFallback: true }));

  // --- Phase 1: online load + install + precache ---
  const page = await context.newPage();
  await page.goto(`${ORIGIN}/index.html`, { waitUntil: "networkidle" });

  // Service workers must be supported in this (secured) origin, or the whole PWA
  // claim is void — fail loudly rather than silently "passing".
  const swSupported = await page.evaluate(() => "serviceWorker" in navigator);
  if (!swSupported) {
    fail("navigator.serviceWorker is unavailable in this context.");
    await browser.close();
    return;
  }
  // Wait for the SW to reach 'activated'.
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
  });

  // Wait until the shell (index + a hashed JS + CSS) is actually precached.
  await page.waitForFunction(
    async () => {
      const names = await caches.keys();
      if (names.length === 0) return false;
      const urls = [];
      for (const n of names) {
        const c = await caches.open(n);
        for (const req of await c.keys()) urls.push(req.url);
      }
      const hasIndex = !!(await caches.match("./index.html"));
      const hasJs = urls.some((u) => u.endsWith(".js"));
      const hasCss = urls.some((u) => u.endsWith(".css"));
      return hasIndex && hasJs && hasCss;
    },
    null,
    { timeout: 20000 }
  );

  const cachedUrls = await page.evaluate(async () => {
    const names = await caches.keys();
    const urls = [];
    for (const n of names) {
      const c = await caches.open(n);
      for (const req of await c.keys()) urls.push(new URL(req.url).pathname);
    }
    return urls.sort();
  });
  console.log("OFFLINE-SMOKE: precached", cachedUrls.length, "entries:", cachedUrls.join(", "));

  // --- Phase 2: go offline, remove the server, navigate fresh ---
  await context.setOffline(true);
  await context.unroute("**/*"); // there is no server anymore; only the SW cache

  const offlinePage = await context.newPage();
  let rendered = false;
  try {
    await offlinePage.goto(`${ORIGIN}/index.html`, { waitUntil: "load", timeout: 15000 });
    await offlinePage.getByText("Cockpit", { exact: true }).first().waitFor({ timeout: 10000 });
    const convCount = await offlinePage.getByText("Refactor the auth module").count();
    rendered = convCount > 0;
  } catch (err) {
    fail(`offline navigation did not render the shell: ${err.message}`);
  }

  if (rendered) {
    console.log(
      "OFFLINE-SMOKE: PASS — fresh offline navigation rendered the Cockpit shell from the SW precache."
    );
  } else if (process.exitCode !== 1) {
    fail("offline navigation loaded but the Cockpit shell was not found.");
  }

  await browser.close();
}

main().catch((err) => {
  fail(err.stack || String(err));
});
