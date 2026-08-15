#!/usr/bin/env node
/**
 * Headless load + screenshot of the dev server. Proves the page renders and
 * leaves a PNG to look at; it does not drive the UI.
 *
 * Exit 0 on success, 1 on navigation failure, 2 if the page logged errors.
 *
 *   npm run dev &
 *   node scripts/browser-smoke.mjs                      # → screenshots/smoke.png
 *   node scripts/browser-smoke.mjs http://127.0.0.1:8080/ out.png
 *
 * The target is restricted to loopback: this exists to look at the local dev
 * server, and pointing it at an arbitrary host is never what was meant.
 */
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { chromium } from "playwright";

const repoRoot = resolve(import.meta.dirname, "..");

/** Loopback http(s) only — see the note above. */
function checkedUrl(raw) {
  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    console.error(`not a URL: ${raw}`);
    process.exit(1);
  }
  const loopback = ["localhost", "127.0.0.1", "[::1]", "::1"];
  if (!["http:", "https:"].includes(parsed.protocol) || !loopback.includes(parsed.hostname)) {
    console.error(`refusing ${raw}: only loopback http(s) targets are allowed`);
    process.exit(1);
  }
  return parsed.toString();
}

/** Keep the screenshot inside the repository, and make it a PNG. */
function checkedOutputPath(raw) {
  const abs = resolve(repoRoot, raw);
  if (!abs.startsWith(repoRoot + "/") || !abs.endsWith(".png")) {
    console.error(`refusing ${raw}: write a .png inside the repository`);
    process.exit(1);
  }
  return abs;
}

const url = checkedUrl(process.argv[2] || "http://127.0.0.1:8080/");
const outPng = checkedOutputPath(process.argv[3] || "screenshots/smoke.png");
const timeoutMs = Number(process.env.BROWSER_SMOKE_TIMEOUT_MS || 45000);

mkdirSync(dirname(outPng), { recursive: true });

const consoleErrors = [];
const pageErrors = [];

// Launching sat outside the try below, so a missing browser binary — the
// first thing a fresh checkout hits — surfaced as an unhandled rejection
// instead of the one-line instruction that fixes it.
let browser;
try {
  browser = await chromium.launch({
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
} catch (err) {
  console.error(String(err?.message || err).split("\n")[0]);
  console.error("Run `npx playwright install chromium` first.");
  process.exit(1);
}

try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (err) => pageErrors.push(String(err?.message || err)));

  const resp = await page.goto(url, { waitUntil: "networkidle", timeout: timeoutMs });
  const status = resp?.status() ?? 0;
  await page.waitForTimeout(1000);

  const title = await page.title();
  const bodyTextLen = (await page.locator("body").innerText().catch(() => "")).trim().length;
  // The inventory is the point of the page: report whether rows actually
  // rendered, so a blank-but-successful load is visible in the output.
  const rowCount = await page.locator(".vl-row").count();

  await page.screenshot({ path: outPng, fullPage: false });

  console.log(
    JSON.stringify(
      { url, status, title, rowCount, bodyTextLen, consoleErrors, pageErrors, screenshot: outPng },
      null,
      2,
    ),
  );

  if (status >= 400 || status === 0) process.exit(1);
  if (pageErrors.length || consoleErrors.length) process.exit(2);
  process.exit(0);
} catch (err) {
  console.error(JSON.stringify({ ok: false, url, error: String(err?.message || err) }, null, 2));
  process.exit(1);
} finally {
  await browser.close();
}
