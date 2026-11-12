#!/usr/bin/env node
// The README's main picture: the app's Overview in a window with rounded
// corners and a soft shadow, and the keyboard photo in front of its bottom
// left, on a transparent background. One per theme and language.
//
//   node tools/screenshots/shoot.mjs --only hero --no-clips   # the Overview captures
//   node tools/screenshots/hero.mjs
//
// Reads <temp>/keymotion-screenshots/app-<theme>[-tr].png (written by
// shoot.mjs) and site/src/assets/keyboards/ajazz-ak980-pro.webp.
// Writes docs/images/hero-<light|dark>[-tr].webp.

import { existsSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const root = fileURLToPath(new URL("../..", import.meta.url));
const SOURCE = join(tmpdir(), "keymotion-screenshots");
const KEYBOARD = join(root, "site", "src", "assets", "keyboards", "ajazz-ak980-pro.webp");

const WIDTH = 1800;
const MARGIN = 44; // room for the window's shadow at the top and right
const WINDOW_W = Math.round(WIDTH * 0.75);
const RADIUS = 12;
const KEYBOARD_W = Math.round(WIDTH * 0.55);
/** How much of the keyboard's height sits over the window. */
const OVERLAP = 0.58;

const roundedRect = (w, h, r, fill, stroke) =>
  Buffer.from(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}">` +
      `<rect x="0.5" y="0.5" width="${w - 1}" height="${h - 1}" rx="${r}" ry="${r}" fill="${fill}"` +
      (stroke ? ` stroke="${stroke}" stroke-width="1"` : "") +
      `/></svg>`,
  );

/** A soft shadow for an image with transparency: its shape, dark, blurred. */
async function shadowOf(png, { blur, opacity, spread = 0 }) {
  const { width, height } = await sharp(png).metadata();
  const pad = Math.ceil(blur * 3) + spread;
  const alpha = await sharp(png).extractChannel("alpha").toBuffer();
  const dark = await sharp({ create: { width, height, channels: 3, background: "#000" } })
    .joinChannel(alpha)
    .png()
    .toBuffer();
  const shadow = await sharp(dark)
    .extend({ top: pad, bottom: pad, left: pad, right: pad, background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .blur(blur)
    .png()
    .toBuffer();
  // Scale the shadow's alpha down to `opacity`.
  const faded = await sharp(shadow)
    .composite([
      {
        input: Buffer.from([255, 255, 255, Math.round(255 * opacity)]),
        raw: { width: 1, height: 1, channels: 4 },
        tile: true,
        blend: "dest-in",
      },
    ])
    .png()
    .toBuffer();
  return { image: faded, pad };
}

async function build(theme, lang) {
  const suffix = lang === "tr" ? "-tr" : "";
  const appFile = join(SOURCE, `app-${theme}${suffix}.png`);
  if (!existsSync(appFile)) return false;

  // The window: the capture scaled to 75% of the width, rounded, with a hairline edge.
  const resized = await sharp(appFile).resize({ width: WINDOW_W, kernel: "lanczos3" }).png().toBuffer();
  const { height: windowH } = await sharp(resized).metadata();
  const edge = theme === "dark" ? "rgba(255,255,255,0.10)" : "rgba(0,0,0,0.14)";
  const window = await sharp(resized)
    .composite([
      { input: roundedRect(WINDOW_W, windowH, RADIUS, "#fff"), blend: "dest-in" },
      { input: roundedRect(WINDOW_W, windowH, RADIUS, "none", edge) },
    ])
    .png()
    .toBuffer();
  const windowShadow = await shadowOf(window, { blur: 22, opacity: theme === "dark" ? 0.55 : 0.28 });

  // The keyboard photo, 55% of the width, over the window's bottom left.
  const keyboard = await sharp(KEYBOARD).resize({ width: KEYBOARD_W, kernel: "lanczos3" }).png().toBuffer();
  const { height: keyboardH } = await sharp(keyboard).metadata();
  const keyboardShadow = await shadowOf(keyboard, { blur: 16, opacity: theme === "dark" ? 0.6 : 0.35 });

  const windowX = WIDTH - WINDOW_W - MARGIN;
  const windowY = MARGIN;
  const keyboardX = 8;
  const keyboardY = Math.round(windowY + windowH - keyboardH * OVERLAP);
  const height = keyboardY + keyboardH + 28;

  const out = join(root, "docs", "images", `hero-${theme}${suffix}.webp`);
  await sharp({ create: { width: WIDTH, height, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } } })
    .composite([
      { input: windowShadow.image, left: windowX - windowShadow.pad, top: windowY - windowShadow.pad + 14 },
      { input: window, left: windowX, top: windowY },
      { input: keyboardShadow.image, left: keyboardX - keyboardShadow.pad, top: keyboardY - keyboardShadow.pad + 12 },
      { input: keyboard, left: keyboardX, top: keyboardY },
    ])
    .webp({ quality: 84, alphaQuality: 90, effort: 6, smartSubsample: true })
    .toFile(out);
  const { size } = statSync(out);
  console.log(`  docs/images/hero-${theme}${suffix}.webp  ${WIDTH}×${height}  ${Math.round(size / 1024)} KB`);
  return true;
}

// Every picture whose Overview capture is there (shoot.mjs --lang/--theme
// decide which: the dark English one is the one in use).
let built = 0;
for (const lang of ["en", "tr"]) {
  for (const theme of ["light", "dark"]) if (await build(theme, lang)) built++;
}
if (!built) {
  console.error(`No Overview captures in ${SOURCE}: run "node tools/screenshots/shoot.mjs --only hero --no-clips --lang en --theme dark" first.`);
  process.exitCode = 1;
}
