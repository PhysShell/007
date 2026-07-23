/*
 * Deterministic screenshot capture for the Cockpit Slice 0 spike.
 *
 * Requires a prior `npm run build` (serves ./dist). Rather than run a listening
 * HTTP server, this serves the built files through Playwright request
 * interception — no socket is bound — and drives the pre-installed Chromium
 * headless shell at a mobile and a desktop viewport. Animations are disabled so
 * output is stable across runs.
 */
import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, extname } from "node:path";
import { chromium } from "playwright";

const DIST = fileURLToPath(new URL("../dist/", import.meta.url));
const OUT = new URL("../screenshots/", import.meta.url);
const ORIGIN = "http://cockpit.local";

const NO_ANIM = `*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}`;

const MOBILE = { width: 390, height: 844, dsf: 2 };
const DESKTOP = { width: 1440, height: 900, dsf: 1 };

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

async function serveRoute(route) {
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
    // Fall back to the app shell (single-page).
    await route.fulfill({
      body: await readFile(join(DIST, "index.html")),
      contentType: CT[".html"],
    });
  }
}

async function shot(browser, { width, height, dsf }, query, file, steps) {
  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: dsf,
    colorScheme: "dark",
    serviceWorkers: "block",
  });
  await context.route("**/*", serveRoute);
  const page = await context.newPage();
  await page.goto(`${ORIGIN}/index.html${query}`, { waitUntil: "networkidle" });
  await page.addStyleTag({ content: NO_ANIM });
  if (steps) await steps(page);
  await page.waitForTimeout(150);
  await page.screenshot({ path: new URL(file, OUT).pathname });
  await context.close();
  console.log("captured", file);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const executablePath =
    process.env.PW_CHROMIUM ||
    "/opt/pw-browsers/chromium_headless_shell-1194/chrome-linux/headless_shell";
  const browser = await chromium.launch({
    executablePath,
    args: ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"],
  });

  const clickTab = (name) => async (page) =>
    page.getByRole("tab", { name }).click();

  // --- mobile captures ---
  await shot(browser, MOBILE, "", "01-mobile-conversation-list.png");
  await shot(browser, MOBILE, "?c=conv-claude-active", "02-mobile-timeline-streaming.png");
  await shot(browser, MOBILE, "?c=conv-delegation", "03-mobile-run-graph.png", clickTab("Runs"));
  await shot(browser, MOBILE, "?c=conv-permission", "04-mobile-permission-request.png");
  await shot(browser, MOBILE, "?c=conv-verifier-failure", "05-mobile-verifier-failure.png");
  await shot(browser, MOBILE, "?c=conv-artifact-gate", "06-mobile-artifact-gate.png");
  await shot(browser, MOBILE, "?c=conv-model-mismatch", "07-mobile-controls-model-drift.png", clickTab("Controls"));
  await shot(browser, MOBILE, "?c=conv-replay", "08-mobile-offline-composer.png");

  // --- desktop (two-pane) captures ---
  await shot(browser, DESKTOP, "?c=conv-artifact-gate", "20-desktop-two-pane.png");
  await shot(browser, DESKTOP, "?c=conv-delegation", "21-desktop-run-graph.png", clickTab("Runs"));
  await shot(browser, DESKTOP, "?c=conv-interrupted", "22-desktop-recovered.png");

  await browser.close();
  console.log("done");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
