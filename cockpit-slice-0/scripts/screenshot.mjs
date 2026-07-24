/*
 * Deterministic screenshot capture for the Cockpit Slice 0 spike.
 *
 * Requires a prior `npm run build` (serves ./dist). Rather than run a listening
 * HTTP server, this serves the built files through Playwright request
 * interception — no socket is bound — and drives the pre-installed Chromium
 * headless shell at a mobile and a desktop viewport. Animations are disabled so
 * output is stable across runs.
 */
import { mkdir } from "node:fs/promises";
import { chromium } from "playwright";
import {
  ORIGIN,
  CHROMIUM,
  LAUNCH_ARGS,
  makeServeRoute,
} from "./serve-dist.mjs";

const OUT = new URL("../screenshots/", import.meta.url);
const serveRoute = makeServeRoute({ spaFallback: true });

const NO_ANIM = `*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}`;

const MOBILE = { width: 390, height: 844, dsf: 2 };
const DESKTOP = { width: 1440, height: 900, dsf: 1 };

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
  const browser = await chromium.launch({
    executablePath: CHROMIUM,
    args: LAUNCH_ARGS,
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
