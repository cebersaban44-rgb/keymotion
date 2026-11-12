#!/usr/bin/env node
// The repository's social preview card (1280×640), the picture shown when a
// link to the repository is shared. GitHub takes it only through the web page
// (Settings › General › Social preview), so this writes the file:
//   docs/images/social-preview.png
//
//   pnpm --dir tools/screenshots install && pnpm --dir site install
//   node tools/screenshots/social.mjs
//
// Fonts and the keyboard picture come from the site's own files; nothing is
// downloaded.

import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

const root = fileURLToPath(new URL("../..", import.meta.url));
const url = (p) => pathToFileURL(join(root, p)).href;
const fonts = "site/node_modules/@fontsource-variable";
const logo = readFileSync(join(root, "app/public/app-icon.svg"), "utf8").replace(/width="1024" height="1024"/, 'width="96" height="96"');

const html = `<!doctype html><meta charset="utf-8"><style>
@font-face { font-family: "Bricolage"; src: url("${url(`${fonts}/bricolage-grotesque/files/bricolage-grotesque-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 200 800; }
@font-face { font-family: "Instrument"; src: url("${url(`${fonts}/instrument-sans/files/instrument-sans-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 400 700; }
@font-face { font-family: "JBMono"; src: url("${url(`${fonts}/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2`)}") format("woff2"); font-weight: 100 800; }
* { margin: 0; box-sizing: border-box }
body { width: 1280px; height: 640px; background: #0d0e10; color: #ece9e3; font-family: "Instrument"; position: relative; overflow: hidden }
.glow { position: absolute; right: -120px; bottom: -220px; width: 900px; height: 520px; background: radial-gradient(closest-side, rgb(255 138 61 / .16), transparent); }
.text { position: absolute; left: 80px; top: 78px; width: 640px }
.brand { display: flex; align-items: center; gap: 22px; font-family: "Bricolage"; font-weight: 750; font-size: 64px; letter-spacing: -0.02em }
.tag { margin-top: 30px; font-size: 30px; line-height: 1.32; color: #cfccc6; max-width: 600px }
.meta { position: absolute; left: 80px; bottom: 64px; font: 500 20px "JBMono"; color: #8a8b8e; letter-spacing: .02em; display: flex; gap: 14px; align-items: center }
.meta i { width: 10px; height: 10px; border-radius: 50%; background: #ff8a3d; display: inline-block }
img.kb { position: absolute; right: -50px; bottom: 104px; width: 720px; filter: drop-shadow(0 30px 40px rgb(0 0 0 / .6)) }
</style>
<div class="glow"></div>
<div class="text"><div class="brand">${logo}Keymotion</div>
<p class="tag">Lighting, key mapping, macros and the little screen of the AJAZZ AK980 PRO, from Windows.</p></div>
<img class="kb" src="${url("site/src/assets/keyboards/ajazz-ak980-pro.webp")}">
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
console.log(out);
