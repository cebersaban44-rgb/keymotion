#!/usr/bin/env node
// The site's screenshots from the REAL app (WebView2), English, dark theme
// only: what each page shows with the real keyboard, the real fonts and the
// real window, put in the state capture-list.mjs describes. The pictures
// keep their names and sizes (1280 × 800 at 1.5×, WebP), so the guide pages
// need no change. shoot.mjs still makes the pictures only the browser
// stand-in can show (source "mock" in the list) and the README's.
//
// Before (once):
//   1. Quit every other Keymotion (only one runs at a time).
//   2. Its data: the development build uses the same folder as an installed
//      copy, %APPDATA%\dev.keymotion. Move it aside (keep it), so the shots
//      show only what this run makes and nothing of yours; put it back after.
//      Started from a packaged app's shell (an MSIX app such as a desktop
//      coding assistant), the app's data lives in that package's
//      %LOCALAPPDATA%\Packages\<package>\LocalCache\Roaming\dev.keymotion
//      instead, and the real folder's files show through wherever that copy
//      has none: move the package's copy aside and leave the real one alone
//      (an empty screen\library.json there hides a real library).
//   3. An empty test folder for the coding apps, so the AI agents shots never
//      touch your own Claude Code or Codex:
//        mkdir %TEMP%\km-shots-home\.claude %TEMP%\km-shots-home\.codex
//   4. cargo build -p keymotion -p keymotion-cli     (the debug app and CLI)
//      pnpm --dir tools/screenshots install
//
// Run (PowerShell, from the repository):
//   pnpm --dir app dev                                       # terminal 1, port 1420
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"
//   $env:KEYMOTION_CLI_TEST_HOME="$env:TEMP\km-shots-home"
//   .\target\debug\keymotion.exe                             # terminal 2
//   node tools/screenshots/capture.mjs --interactive `
//     --agents-home "$env:TEMP\km-shots-home" --cli .\target\debug\keymotion-cli.exe
//
// --interactive stops before each shot that needs the keyboard set up a
// certain way (cable, Bluetooth, unplugged, music playing) and says what to
// do. Other options:
//   --only overview,ai-agents   only these
//   --list                      print the list (name, source, pages, state) and stop
//   --out <folder>              write there instead of site/src/assets/screens/en/dark
//   --dry                       set every page up, write nothing
//   --target mock               the same steps against the browser stand-in
//                               (pnpm --dir app dev; --dev-url), to try the list
//   --target cdp-mock           the stand-in in an Edge with remote debugging,
//                               reached over CDP the way the real app is
//   --ics <url>                 the public calendar for the calendar shots
//
// In the real app a guard refuses, for the whole run, every call that would
// write to the keyboard's memory, upload to its screen, turn the live screen
// on, open a window outside the app or change another program's files
// (guard.mjs; the app's development build asks the guard before every
// command, since Tauri locks its own invoke). Connecting Claude Code and
// Codex is allowed for one step, and only when every path the connect step
// shows is inside --agents-home.
//
// The app keeps what a shot leaves (a draft, a running focus timer, the
// gauges on) for the next one: take the shots that leave something after
// those that must not show it, or one at a time with --only.
// Afterwards: close the app, delete the test folder, put the data folder back.

import { execFile } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";
import { chromium } from "playwright-core";
import sharp from "sharp";
import { CAPTURES, README_IMAGES } from "./capture-list.mjs";
import { blocked, DENY, guard } from "./guard.mjs";
import { button, go, openScreen, t } from "./shots.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const VIEWPORT = { width: 1280, height: 800 };
const SCALE = 1.5;
const LANG = "en";
const THEME = "dark";
/** A public calendar (holidays), never the owner's. */
const ICS = "https://calendar.google.com/calendar/ical/en.usa%23holiday%40group.v.calendar.google.com/public/basic.ics";
const USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0";

const { values: opt } = parseArgs({
  options: {
    target: { type: "string", default: "app" },
    cdp: { type: "string", default: "http://127.0.0.1:9222" },
    "dev-url": { type: "string", default: "http://127.0.0.1:1420/" },
    only: { type: "string" },
    out: { type: "string" },
    list: { type: "boolean", default: false },
    dry: { type: "boolean", default: false },
    interactive: { type: "boolean", default: false },
    "agents-home": { type: "string" },
    cli: { type: "string" },
    ics: { type: "string", default: ICS },
  },
});

if (opt.list) {
  const cell = (s) => String(s).replace(/\|/g, "\\|");
  console.log("| Name | Source | Site pages | State | Before |\n|---|---|---|---|---|");
  for (const c of CAPTURES) {
    console.log(`| ${c.name} | ${c.source} | ${c.pages.join(", ") || "(new)"} | ${cell(c.state)} | ${cell(c.prep ?? "")} |`);
  }
  for (const r of README_IMAGES) console.log(`| ${r.file} | ${r.source} | README | ${cell(r.state)} | |`);
  process.exit(0);
}

const target = opt.target;
if (!["app", "mock", "cdp-mock"].includes(target)) throw new Error(`--target app, mock or cdp-mock, not ${target}`);
const real = target === "app";
const out = opt.out ?? join(root, "site", "src", "assets", "screens", LANG, THEME);
const only = opt.only ? new Set(opt.only.split(",")) : null;
const run = promisify(execFile);
const ask = opt.interactive ? createInterface({ input: process.stdin, output: process.stdout }) : null;

/** What a list entry's steps get besides the page. */
const km = {
  lang: LANG,
  theme: THEME,
  target,
  ics: opt.ics,
  /** The connect step on the AI agents page: approve it when every path it
   *  shows is inside --agents-home (the real app), then OK. */
  async approveConnect(page) {
    const flow = page.locator("[data-agents-flow]");
    const details = await button(page, "ag.flow.details", { within: flow });
    if ((await details.getAttribute("aria-expanded")) !== "true") await details.click();
    await page.waitForTimeout(400);
    if (real) {
      const home = opt["agents-home"];
      if (!home) {
        console.warn("    no --agents-home: the connect step is left as it is");
        return false;
      }
      const text = await flow.innerText();
      const paths = text.match(/[A-Za-z]:\\[^\s"]+/g) ?? [];
      const inside = (p) => p.toLowerCase().startsWith(home.toLowerCase().replace(/[\\/]+$/, "") + "\\");
      // keymotion-cli's own path (the hook line) is outside, and allowed.
      const theirs = paths.filter((p) => /\\\.(claude|codex|agents)\\/i.test(p));
      if (!theirs.length || !theirs.every(inside)) {
        console.warn(`    the connect step shows files outside ${home}: not approved`);
        return false;
      }
      await page.evaluate(guard, [DENY, ["agents_connect"]]);
    }
    try {
      await (await button(page, "ag.flow.approve", { within: flow })).click();
      await (await button(page, "ag.flow.ok", { within: flow })).waitFor({ timeout: 15_000 });
      await (await button(page, "ag.flow.ok", { within: flow })).click();
      await page.waitForTimeout(800);
    } finally {
      if (real) await page.evaluate(guard, [DENY, []]);
    }
    return true;
  },
  /** Recent actions: a done signal, a color change, a request refused. */
  async agentActions() {
    if (!real) return;
    if (!opt.cli) throw new Error("--cli <keymotion-cli.exe> makes the agents' actions");
    const cli = (...args) => run(opt.cli, [...args, "--no-start", "--json"], { windowsHide: true }).catch((e) => e);
    await cli("signal", "done", "--source", "claude-code");
    await cli("settings", "set", "signals.done.color", "blue", "--source", "codex");
    // Only the user may change this: the app refuses it, and says so.
    await cli("settings", "set", "enabled", "false", "--source", "codex");
  },
};

// ------------------------------------------------------------------ pages

let browser;
let size = { ...VIEWPORT };

/** Size the page as the shot wants it, 1.5× pixels (over CDP for the app). */
async function sized(page, cdp, s) {
  size = { ...s };
  if (!cdp) return page.setViewportSize(s);
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: s.width, height: s.height, deviceScaleFactor: SCALE, mobile: false });
  await page.waitForTimeout(250);
}

/** The picture: over CDP the page's own capture, which keeps the 1.5×
 *  pixels the emulation set (Playwright's would be 1×); in the browser,
 *  Playwright's. `clip` is in CSS pixels; the app scrolls inside <main>,
 *  never the document, so the window's coordinates are the page's. */
async function capture(page, cdp, clip) {
  if (!cdp) return page.screenshot({ clip, animations: "disabled" });
  const params = { format: "png", captureBeyondViewport: false };
  if (clip) params.clip = { x: clip.x, y: clip.y, width: clip.width, height: clip.height, scale: 1 };
  const { data } = await cdp.send("Page.captureScreenshot", params);
  return Buffer.from(data, "base64");
}

/** The app's page, attached over CDP (the real app, or Edge with the stand-in). */
async function attached() {
  browser = await chromium.connectOverCDP(opt.cdp);
  const context = browser.contexts()[0];
  const pages = context.pages();
  const page = pages.find((p) => /^https?:\/\//.test(p.url())) ?? pages[0];
  if (!page) throw new Error(`no page at ${opt.cdp}`);
  const cdp = await context.newCDPSession(page);
  // The shared steps size the window with these: CDP sizes it instead.
  page.viewportSize = () => size;
  page.setViewportSize = (s) => sized(page, cdp, s);
  await cdp.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: THEME }, { name: "prefers-reduced-motion", value: "reduce" }] });
  return { page, cdp };
}

/** English, dark, a solid window background (not the translucent one, which
 *  a capture can't see through), the guard on, the top of the page. */
async function prepare(page) {
  await page.waitForFunction(() => window.__keymotionStore?.getState().loaded === true, null, { timeout: 30_000 });
  await page.evaluate(([lang, theme]) => {
    const s = window.__keymotionStore.getState();
    s.setLocale(lang);
    s.setTheme(theme);
    document.documentElement.dataset.mica = "false";
  }, [LANG, THEME]);
  const g = await page.evaluate(guard, [DENY, []]);
  if (real && g !== "on") throw new Error(`the capture guard could not be put in place (${g}): nothing is captured`);
  await page.waitForTimeout(500);
}

/** One entry's page in its state: a fresh load each time, so what the last
 *  shot left unsaved never shows. */
async function setUp(item, session) {
  let page = session?.page;
  if (session) {
    await sized(page, session.cdp, { ...VIEWPORT, ...(item.viewport ?? {}) });
    await page.reload({ waitUntil: "domcontentloaded" });
  } else {
    const context = await browser.newContext({
      viewport: { ...VIEWPORT, ...(item.viewport ?? {}) },
      deviceScaleFactor: SCALE,
      colorScheme: THEME,
      locale: "en-US",
      reducedMotion: "reduce",
      userAgent: USER_AGENT,
    });
    await context.addInitScript(([l, th]) => {
      localStorage.setItem("ks.locale", l);
      localStorage.setItem("ks.theme", th);
    }, [LANG, THEME]);
    page = await context.newPage();
    size = { ...VIEWPORT, ...(item.viewport ?? {}) };
    page.on("pageerror", (e) => console.warn(`    ${item.name}: page error: ${e.message}`));
    const query = new URLSearchParams({ mock: item.mock ?? "usb", ...(item.query ?? {}) });
    await page.goto(`${opt["dev-url"]}?${query}`.replace(/=(&|$)/g, "$1"));
  }
  await prepare(page);
  if (item.page) {
    if (item.screenTab) await openScreen(page, item.screenTab);
    else await go(page, item.page);
  }
  await page.waitForTimeout(item.settle ?? 700);
  if (item.steps) await item.steps(page, km);
  await page.mouse.move(1, 1);
  await page.waitForTimeout(300);
  return page;
}

// -------------------------------------------------------------------- run

const session = target === "mock" ? null : await attached();
if (!session) browser = await chromium.launch({ channel: "msedge", headless: true });
if (!opt.dry) mkdirSync(out, { recursive: true });
console.log(`${real ? "The real app" : target === "cdp-mock" ? "The stand-in over CDP" : "The stand-in"}, ${LANG}, ${THEME} → ${opt.dry ? "(dry run)" : out}`);

const failed = [];
const leftForMock = [];
let lastPrep = null;
for (const item of CAPTURES) {
  if (only && !only.has(item.name)) continue;
  // shoot.mjs takes these, from the stand-in, with its own steps.
  if (item.source === "mock") {
    leftForMock.push(item.name);
    continue;
  }
  if (real && item.prep && item.prep !== lastPrep) {
    console.log(`\n  ${item.name}: ${item.prep}`);
    if (ask && (await ask.question("  Ready? Enter takes it, s skips: ")).trim().toLowerCase() === "s") continue;
    lastPrep = item.prep;
  }
  try {
    const page = await setUp(item, session);
    const clip = typeof item.clip === "function" ? await item.clip(page) : item.clip;
    const png = await capture(page, session?.cdp, clip);
    const refused = real ? await page.evaluate(blocked) : [];
    if (refused.length) console.warn(`    ${item.name}: the guard refused ${refused.join(", ")}`);
    const { width, height } = await sharp(png).metadata();
    if (!opt.dry) await sharp(png).webp({ quality: 82, effort: 6, smartSubsample: true }).toFile(join(out, `${item.name}.webp`));
    console.log(`  ${item.name}.webp  ${width} × ${height}${opt.dry ? " (not written)" : ""}`);
    if (!session) await page.context().close();
  } catch (e) {
    console.error(`  ${item.name} FAILED: ${e.message.split("\n")[0]}`);
    failed.push(item.name);
  }
}

if (session) {
  await session.cdp.send("Emulation.clearDeviceMetricsOverride").catch(() => {});
  await session.cdp.send("Emulation.setEmulatedMedia", { features: [] }).catch(() => {});
  await session.page.reload().catch(() => {});
} else {
  await browser.close();
}
ask?.close();
if (leftForMock.length) console.log(`\nFor shoot.mjs (the stand-in): node tools/screenshots/shoot.mjs --only ${leftForMock.join(",")}`);
if (failed.length) {
  console.error(`\n${failed.length} failed: ${failed.join(", ")}`);
  process.exitCode = 1;
}
// A CDP connection keeps the process alive; the app stays open.
process.exit();
