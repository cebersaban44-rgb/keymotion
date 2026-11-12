#!/usr/bin/env node
// Screenshots and short animations of the app for the README and the
// documentation site (roadmap stage 7). They come from the development build
// in a browser: a stand-in keyboard and sample data (app/src/lib/ipc.ts and
// the mock helpers), so no real keyboard, calendar or personal data shows,
// and they can be retaken whenever the app changes.
//
//   pnpm --dir app dev                         # in another terminal
//   pnpm --dir tools/screenshots install
//   node tools/screenshots/shoot.mjs           # everything
//   node tools/screenshots/shoot.mjs --only lighting-effects,macros --lang tr --theme dark
//   node tools/screenshots/shoot.mjs --clips   # only the animations
//   node tools/screenshots/hero.mjs            # then the README picture
//
// Writes:
//   site/src/assets/screens/<lang>/<light|dark>/<name>.webp   (SHOTS)
//   docs/images/features/<name>.webp                          (CLIPS, animated)
//   <temp>/keymotion-screenshots/app-<theme>[-tr].png         (HERO, for hero.mjs)
//
// Every shot starts from a fresh browser profile, so what one shot saves
// (lighting, profiles, macros) never shows in another.
//
// It uses the Microsoft Edge that comes with Windows (no browser download).

import { mkdirSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { chromium } from "playwright-core";
import sharp from "sharp";
import { CLIPS, HERO, SHOTS } from "./shots.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const BASE = process.env.KEYMOTION_DEV_URL ?? "http://127.0.0.1:1420/";
const VIEWPORT = { width: 1280, height: 800 };
const SCALE = 1.5;
/** Where the HERO captures go for hero.mjs (not part of the repository). */
const HERO_DIR = join(tmpdir(), "keymotion-screenshots");
// The app's diagnostics show the browser's name: a plain Edge, not "Headless".
const USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0";

const { values: opt } = parseArgs({
  options: {
    only: { type: "string" },
    lang: { type: "string" },
    theme: { type: "string" },
    clips: { type: "boolean", default: false },
    "no-clips": { type: "boolean", default: false },
  },
});
const only = opt.only ? new Set(opt.only.split(",")) : null;
// English only by default (owner's decision, 2026-11-12): Turkish pages show the
// English shots. `--lang tr` still takes Turkish ones.
const langs = opt.lang ? [opt.lang] : ["en"];
// Dark theme only by default (same decision): one shot serves both site themes.
const themes = opt.theme ? [opt.theme] : ["dark"];

const browser = await chromium.launch({ channel: "msedge", headless: true });
const write = async (file, buffer) => {
  mkdirSync(dirname(file), { recursive: true });
  await buffer.toFile(file);
  console.log(`  ${file.startsWith(root) ? file.slice(root.length).replace(/\\/g, "/") : file}`);
};

/** A page of the app with the stand-in keyboard, in `lang` and `theme`, in a
 *  browser profile of its own. Closing the page's context closes both. */
async function open(item, lang, theme) {
  const context = await browser.newContext({
    viewport: { ...VIEWPORT, ...(item.viewport ?? {}) },
    deviceScaleFactor: SCALE,
    colorScheme: theme,
    locale: lang === "tr" ? "tr-TR" : "en-US",
    // Reduced motion skips the window's opening animation and stills the
    // app's small looping previews; a clip that shows them sets `motion`.
    reducedMotion: item.motion ? "no-preference" : "reduce",
    userAgent: USER_AGENT,
  });
  await context.addInitScript(([l, t]) => {
    localStorage.setItem("ks.locale", l);
    localStorage.setItem("ks.theme", t);
  }, [lang, theme]);
  const page = await context.newPage();
  page.on("pageerror", (e) => console.warn(`    ${item.name}: page error: ${e.message}`));
  const query = new URLSearchParams({ mock: item.mock ?? "usb", ...(item.query ?? {}) });
  await page.goto(`${BASE}?${query}`.replace(/=(&|$)/g, "$1"));
  await page.waitForFunction(() => window.__keymotionStore?.getState().loaded === true, null, { timeout: 20_000 });
  // The window's opening animation is skipped with reduced motion; let it fade.
  await page.waitForTimeout(item.motion ? 1800 : 400);
  if (item.page) {
    await page.evaluate(([p, tab]) => {
      const s = window.__keymotionStore.getState();
      if (tab) s.openScreen(tab);
      else s.setPage(p);
    }, [item.page, item.screenTab ?? null]);
  }
  await page.waitForTimeout(item.settle ?? 700);
  if (item.steps) await item.steps(page, { lang, theme });
  // No hover highlight where the last click was.
  await page.mouse.move(1, 1);
  await page.waitForTimeout(200);
  return page;
}

/** A clip as separate captures: `frame(page, i)` acts before capture i. */
async function stills(page, rect, clip) {
  const frames = [];
  const times = [];
  for (let i = 0; i < clip.frames; i++) {
    if (clip.frame) await clip.frame(page, i);
    times.push(Date.now());
    frames.push(await page.screenshot({ clip: rect }));
    if (clip.interval) await page.waitForTimeout(clip.interval);
  }
  times.push(Date.now());
  return { frames, times };
}

/** A clip as the browser draws it, while `record(page)` acts: frames come
 *  from the compositor (a screenshot each time is too slow while the app's
 *  own animations run), at most `fps` a second, cropped to `rect`. */
async function screencast(page, rect, clip) {
  const cdp = await page.context().newCDPSession(page);
  const raw = [];
  cdp.on("Page.screencastFrame", (f) => {
    raw.push({ data: f.data, t: f.metadata.timestamp * 1000, cssWidth: f.metadata.deviceWidth });
    cdp.send("Page.screencastFrameAck", { sessionId: f.sessionId }).catch(() => {});
  });
  await cdp.send("Page.startScreencast", { format: "png", everyNthFrame: 1 });
  const start = Date.now();
  await page.waitForTimeout(150);
  await clip.record(page);
  const elapsed = Date.now() - start;
  await cdp.send("Page.stopScreencast");
  await cdp.detach();
  // Keep a frame at most every 1000/fps ms (the newest of each slot).
  const step = 1000 / (clip.fps ?? 12);
  const kept = [];
  for (const f of raw) {
    const last = kept[kept.length - 1];
    if (last && f.t - last.t < step) kept[kept.length - 1] = { ...f, t: last.t };
    else kept.push(f);
  }
  const frames = [];
  for (const f of kept) {
    const png = Buffer.from(f.data, "base64");
    const { width } = await sharp(png).metadata();
    const k = width / f.cssWidth;
    frames.push(
      await sharp(png)
        .extract({ left: Math.round(rect.x * k), top: Math.round(rect.y * k), width: Math.round(rect.width * k), height: Math.round(rect.height * k) })
        .png()
        .toBuffer(),
    );
  }
  const times = kept.map((f) => f.t);
  times.push(Math.max(times[times.length - 1] + step, kept[0].t + elapsed));
  return { frames, times };
}

/** The area to capture: the whole window, a fixed rectangle, or one the
 *  item works out on the page (a dialog with a margin, say). */
const area = async (item, page) => (typeof item.clip === "function" ? await item.clip(page) : item.clip);

const runShots = !opt.clips;
const runClips = opt.clips || !opt["no-clips"];
const failed = [];

for (const lang of langs) {
  for (const theme of themes) {
    if (runShots) {
      for (const shot of SHOTS) {
        if (only && !only.has(shot.name)) continue;
        try {
          const page = await open(shot, lang, theme);
          const png = await page.screenshot({ clip: await area(shot, page), animations: "disabled" });
          await write(
            join(root, "site", "src", "assets", "screens", lang, theme, `${shot.name}.webp`),
            sharp(png).webp({ quality: 82, effort: 6, smartSubsample: true }),
          );
          await page.context().close();
        } catch (e) {
          console.error(`  ${lang}/${theme}/${shot.name} FAILED: ${e.message.split("\n")[0]}`);
          failed.push(`${lang}/${theme}/${shot.name}`);
        }
      }
      if (!only || only.has("hero")) {
        const page = await open(HERO, lang, theme);
        const png = await page.screenshot();
        const suffix = lang === "tr" ? "-tr" : "";
        await write(join(HERO_DIR, `app-${theme}${suffix}.png`), sharp(png).png());
        await page.context().close();
      }
    }
    // The animations are the same in both languages: English, both themes
    // would double their size for little gain, so they use the dark theme.
    if (runClips && lang === "en" && theme === "dark") {
      for (const clip of CLIPS) {
        if (only && !only.has(clip.name)) continue;
        try {
          const page = await open(clip, lang, theme);
          const rect = await area(clip, page);
          const width = clip.width ?? 480;
          let frames, times;
          if (clip.record) ({ frames, times } = await screencast(page, rect, clip));
          else ({ frames, times } = await stills(page, rect, clip));
          const resized = await Promise.all(frames.map((f) => sharp(f).resize({ width, kernel: "lanczos3" }).png().toBuffer()));
          // How long each frame shows: as captured (`record`, `realtime`), per
          // frame (a function or a list), or one value for all. `hold` keeps
          // the last frame longer.
          const delay = frames.map((_, i) => {
            const d =
              clip.record || clip.realtime
                ? times[i + 1] - times[i]
                : typeof clip.delay === "function"
                  ? clip.delay(i)
                  : Array.isArray(clip.delay)
                    ? clip.delay[i]
                    : (clip.delay ?? clip.interval);
            return i === frames.length - 1 && clip.hold ? clip.hold : Math.max(20, Math.round(d));
          });
          await write(
            join(root, "docs", "images", "features", `${clip.name}.webp`),
            sharp(resized, { join: { animated: true } }).webp({ quality: clip.quality ?? 80, effort: 6, loop: 0, delay }),
          );
          const file = join(root, "docs", "images", "features", `${clip.name}.webp`);
          const seconds = delay.reduce((a, d) => a + d, 0) / 1000;
          console.log(`    ${frames.length} frames, ${seconds.toFixed(1)} s, ${Math.round(statSync(file).size / 1024)} KB`);
          await page.context().close();
        } catch (e) {
          console.error(`  clip ${clip.name} FAILED: ${e.message.split("\n")[0]}`);
          failed.push(`clip ${clip.name}`);
        }
      }
    }
  }
}
await browser.close();
if (failed.length) {
  console.error(`\n${failed.length} failed: ${failed.join(", ")}`);
  process.exitCode = 1;
}
