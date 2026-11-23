#!/usr/bin/env node
// The repository's social preview card (1280×640), the picture shown when a
// link to the repository or the site is shared: the name, what Keymotion
// does, and the README's main picture (the Overview page with the keyboard in
// front of it). GitHub takes the card only through the web page (Settings ›
// General › Social preview), so this writes the files:
//   docs/images/social-preview.png          (for GitHub's settings page)
//   site/public/images/social-preview.png   (the site's share picture)
//
//   pnpm --dir tools/screenshots install && pnpm --dir site install
//   node tools/screenshots/hero.mjs          (when the app's look changes)
//   node tools/screenshots/social.mjs
//
// Fonts come from the site's own files and the picture from
// docs/images/hero-dark.webp (hero.mjs); nothing is downloaded.

import { copyFileSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

const root = fileURLToPath(new URL("../..", import.meta.url));
const url = (p) => pathToFileURL(join(root, p)).href;
const fonts = "site/node_modules/@fontsource-variable";
const logo = readFileSync(join(root, "app/public/app-icon.svg"), "utf8").replace(/width="1024" height="1024"/, 'width="84" height="84"');

const html = `<!doctype html><meta charset="utf-8"><style>
@font-face { font-family: "Bricolage"; src: url("${url(`${fonts}/bricolage-grotesque/files/bricolage-grotesque-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 200 800; }
@font-face { font-family: "Instrument"; src: url("${url(`${fonts}/instrument-sans/files/instrument-sans-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 400 700; }
@font-face { font-family: "JBMono"; src: url("${url(`${fonts}/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 100 800; }
* { margin: 0; box-sizing: border-box }
body { width: 1280px; height: 640px; background: #0d0e10; color: #ece9e3; font-family: "Instrument"; position: relative; overflow: hidden }
.glow { position: absolute; right: -160px; bottom: -240px; width: 1000px; height: 560px; background: radial-gradient(closest-side, rgb(255 138 61 / .15), transparent); }
.text { position: absolute; left: 72px; top: 64px; width: 500px }
.brand { display: flex; align-items: center; gap: 20px; font-family: "Bricolage"; font-weight: 750; font-size: 58px; letter-spacing: -0.02em }
.tag { margin-top: 28px; font-size: 27px; line-height: 1.32; color: #cfccc6 }
ul { margin-top: 26px; padding: 0; list-style: none; display: grid; gap: 9px; font-size: 21px; color: #b4b1ab }
li { display: flex; align-items: center; gap: 12px }
li::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: #ff8a3d; flex: none }
.meta { position: absolute; left: 72px; bottom: 52px; font: 500 19px "JBMono"; color: #8a8b8e; letter-spacing: .02em; display: flex; gap: 14px; align-items: center }
.meta i { width: 10px; height: 10px; border-radius: 50%; background: #ff8a3d; display: inline-block }
img.hero { position: absolute; right: -64px; top: 92px; width: 790px }
</style>
<div class="glow"></div>
<div class="text"><div class="brand">${logo}Keymotion</div>
<p class="tag">Lighting, key mapping, macros and the little screen of the AJAZZ AK980 PRO, from Windows.</p>
<ul><li>Music mode</li><li>Game session and League of Legends lights</li><li>Profiles by app</li><li>Lights for AI coding agents</li></ul></div>
<img class="hero" src="${url("docs/images/hero-dark.webp")}">
<div class="meta"><i></i>Free and open source · Windows 10 and 11</div>`;

const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 640 }, deviceScaleFactor: 1 });
// A file page, so it may load the local fonts and picture.
const file = join(tmpdir(), "keymotion-social-preview.html");
writeFileSync(file, html);
await page.goto(pathToFileURL(file).href, { waitUntil: "load" });
await page.evaluate(() => document.fonts.ready);
const out = join(root, "docs", "images", "social-preview.png");
await page.screenshot({ path: out });
await browser.close();
rmSync(file);
const site = join(root, "site", "public", "images", "social-preview.png");
copyFileSync(out, site);
console.log(out);
console.log(site);
