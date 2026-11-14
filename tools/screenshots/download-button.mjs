#!/usr/bin/env node
// The large download button at the top of the README, in English and
// Turkish. GitHub shows no real buttons in a README, so it is a picture that
// links to the latest release. It holds no version number, so it never goes
// out of date. Writes:
//   docs/images/download-windows.png      (English README)
//   docs/images/download-windows-tr.png   (Turkish README)
//
//   pnpm --dir tools/screenshots install && pnpm --dir site install
//   node tools/screenshots/download-button.mjs
//
// Drawn at twice its size so it stays sharp on high-density screens; the
// README shows it 320 px wide. The corners are transparent, so it sits on
// GitHub's light and dark themes alike. The font comes from the site's own
// files; nothing is downloaded.

import { rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

const root = fileURLToPath(new URL("../..", import.meta.url));
const url = (p) => pathToFileURL(join(root, p)).href;
const font = "site/node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-ext-wght-normal.woff2";
const fontLatin = "site/node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2";

const buttons = [
  { file: "download-windows.png", title: "Download for Windows", sub: "Free · Windows 10 and 11 · 64-bit" },
  { file: "download-windows-tr.png", title: "Windows için indir", sub: "Ücretsiz · Windows 10 ve 11 · 64 bit" },
];

// Windows 11's four panes.
const windows = `<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
<rect x="0" y="0" width="9.4" height="9.4"/><rect x="10.6" y="0" width="9.4" height="9.4"/>
<rect x="0" y="10.6" width="9.4" height="9.4"/><rect x="10.6" y="10.6" width="9.4" height="9.4"/></svg>`;

const page = (b) => `<!doctype html><meta charset="utf-8"><style>
@font-face { font-family: "Instrument"; src: url("${url(fontLatin)}") format("woff2"); font-weight: 400 700; unicode-range: U+0000-00FF, U+2000-206F; }
@font-face { font-family: "Instrument"; src: url("${url(font)}") format("woff2"); font-weight: 400 700; unicode-range: U+0100-024F; }
* { margin: 0; box-sizing: border-box }
html, body { background: transparent }
#b { width: 320px; padding: 13px 0 12px; border-radius: 10px; background: #ff8a3d; color: #1f1206;
  font-family: "Instrument"; text-align: center; line-height: 1.2 }
.t { font-size: 20px; font-weight: 650; display: flex; align-items: center; justify-content: center; gap: 11px; letter-spacing: -0.005em }
.t svg { fill: #1f1206; flex: none }
.s { margin-top: 4px; font-size: 13px; font-weight: 500; color: #5a2f0e; word-spacing: 0.15em }
</style>
<div id="b"><div class="t">${windows}${b.title}</div><div class="s">${b.sub}</div></div>`;

const browser = await chromium.launch({ channel: "msedge", headless: true });
const tab = await browser.newPage({ viewport: { width: 400, height: 200 }, deviceScaleFactor: 2 });
for (const b of buttons) {
  // A file page, so it may load the local font.
  const file = join(tmpdir(), `keymotion-${b.file}.html`);
  writeFileSync(file, page(b));
  await tab.goto(pathToFileURL(file).href, { waitUntil: "load" });
  await tab.evaluate(() => document.fonts.ready);
  const out = join(root, "docs", "images", b.file);
  await tab.locator("#b").screenshot({ path: out, omitBackground: true });
  rmSync(file);
  console.log(out);
}
await browser.close();
