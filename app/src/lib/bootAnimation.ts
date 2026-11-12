// Keymotion's own animation for the keyboard's screen (docs/yol-haritasi.md,
// stage 3; the owner chose design A with the wordmark, 2026-11-12): the "K"
// of the logo drawn stroke by stroke, the orange stroke glows, "keymotion"
// opens beside it, a hold, a fade, and again. It is for keyboards whose
// factory animation has no backup (AJAZZ's animation is not shipped).
//
// Drawn here because the wordmark needs the app's font; stored once in the
// library as the built-in "keymotion" item (screen.ts). Few frames, long
// holds: every frame costs about 2.3 s of upload and some flash wear.

import { H, W, outputCanvas, toRgb565, type Prepared } from "./screenImport";

const BG = "#16181C";
const CREAM = "#ECE9E3";
const ORANGE = "#FF8A3D";
const FONT = '"Bricolage Grotesque Variable", "Segoe UI", sans-serif';

/** The logo's strokes in its 100×100 box (app-icon.svg). */
const STROKES = {
  stem: [34, 25, 34, 75],
  arm: [46, 51, 68, 27],
  leg: [51, 57, 69, 75],
} as const;
type Stroke = keyof typeof STROKES;
const STROKE_WIDTH = 13;

/** The K on the left third; the wordmark to its right. */
const LOGO = { cx: 60, cy: 67.5, scale: 1.22 };
const WORD_X = 110;
const WORD_BASELINE = 77;
const WORD_MAX_W = 122;

const ease = (t: number) => (t <= 0 ? 0 : t >= 1 ? 1 : 1 - (1 - t) ** 3);

function point(x: number, y: number): [number, number] {
  return [LOGO.cx + (x - 51.5) * LOGO.scale, LOGO.cy + (y - 50) * LOGO.scale];
}

/** `part` of a stroke (0–1, from its start), a little wider by `extra`. */
function stroke(c: CanvasRenderingContext2D, s: Stroke, part: number, color: string, extra = 0) {
  if (part <= 0) return;
  const [a, b, d, e] = STROKES[s];
  const [x1, y1] = point(a, b);
  const [x2, y2] = point(d, e);
  c.strokeStyle = color;
  c.lineWidth = STROKE_WIDTH * LOGO.scale + extra;
  c.lineCap = "round";
  c.beginPath();
  c.moveTo(x1, y1);
  c.lineTo(x1 + (x2 - x1) * part, y1 + (y2 - y1) * part);
  c.stroke();
}

/** A soft halo around the orange stroke, `g` 0–1. */
function glow(c: CanvasRenderingContext2D, g: number, alpha: number) {
  if (g <= 0) return;
  for (const k of [3, 2, 1]) {
    c.globalAlpha = 0.14 * g * alpha;
    stroke(c, "leg", 1, ORANGE, k * 5 * g);
  }
  c.globalAlpha = 1;
}

function wordFont(c: CanvasRenderingContext2D): void {
  let size = 27;
  c.font = `600 ${size}px ${FONT}`;
  const w = c.measureText("keymotion").width;
  if (w > WORD_MAX_W) {
    size = Math.floor((size * WORD_MAX_W) / w);
    c.font = `600 ${size}px ${FONT}`;
  }
}

/** "key" in cream, "motion" in orange, opened from the left by `part`. */
function wordmark(c: CanvasRenderingContext2D, part: number, alpha: number) {
  if (part <= 0) return;
  wordFont(c);
  const w = c.measureText("keymotion").width;
  const keyW = c.measureText("key").width;
  c.save();
  c.beginPath();
  c.rect(WORD_X - 2, 0, (w + 6) * part, H);
  c.clip();
  c.globalAlpha = alpha;
  c.textBaseline = "alphabetic";
  c.fillStyle = CREAM;
  c.fillText("key", WORD_X, WORD_BASELINE);
  c.fillStyle = ORANGE;
  c.fillText("motion", WORD_X + keyW, WORD_BASELINE);
  c.restore();
}

function background(c: CanvasRenderingContext2D) {
  c.globalAlpha = 1;
  c.fillStyle = BG;
  c.fillRect(0, 0, W, H);
}

/** The whole logo: halo `g`, wordmark opened by `word`, faded to `alpha`. */
function full(c: CanvasRenderingContext2D, g: number, word: number, alpha: number) {
  c.globalAlpha = alpha;
  stroke(c, "stem", 1, CREAM);
  stroke(c, "arm", 1, CREAM);
  glow(c, g, alpha);
  c.globalAlpha = alpha;
  stroke(c, "leg", 1, ORANGE);
  c.globalAlpha = 1;
  wordmark(c, word, alpha);
}

/** Each frame: how long it shows (ms) and how it is drawn. */
export function bootTimeline(): { delayMs: number; draw: (c: CanvasRenderingContext2D) => void }[] {
  const out: { delayMs: number; draw: (c: CanvasRenderingContext2D) => void }[] = [];
  const add = (delayMs: number, draw: (c: CanvasRenderingContext2D) => void) =>
    out.push({
      delayMs,
      draw: (c) => {
        background(c);
        draw(c);
      },
    });
  add(160, () => {});
  for (let k = 1; k <= 6; k++) add(40, (c) => stroke(c, "stem", ease(k / 6), CREAM));
  for (let k = 1; k <= 5; k++)
    add(40, (c) => {
      stroke(c, "stem", 1, CREAM);
      stroke(c, "arm", ease(k / 5), CREAM);
    });
  for (let k = 1; k <= 5; k++)
    add(40, (c) => {
      stroke(c, "stem", 1, CREAM);
      stroke(c, "arm", 1, CREAM);
      stroke(c, "leg", ease(k / 5), ORANGE);
    });
  for (const g of [0.35, 0.7, 1, 0.8, 0.55, 0.35]) add(60, (c) => full(c, g, 0, 1));
  for (let k = 1; k <= 6; k++) add(50, (c) => full(c, 0.35, ease(k / 6), 1));
  for (let k = 0; k < 4; k++) add(500, (c) => full(c, 0.35, 1, 1));
  for (const a of [0.75, 0.5, 0.25, 0.05]) add(60, (c) => full(c, 0.35 * a, 1, a));
  return out;
}

/** The animation as the keyboard takes it (37 frames, about 3.9 s a loop). */
export async function keymotionAnimation(): Promise<Prepared> {
  try {
    await document.fonts.load(`600 27px ${FONT}`, "keymotion");
  } catch {
    // The fallback font still draws.
  }
  const { ctx } = outputCanvas();
  const frames: Uint16Array[] = [];
  const delaysMs: number[] = [];
  for (const f of bootTimeline()) {
    f.draw(ctx);
    // Flat colours: no dithering.
    frames.push(toRgb565(ctx.getImageData(0, 0, W, H).data, false));
    delaysMs.push(f.delayMs);
  }
  return { frames, delaysMs, name: "Keymotion", kind: "animation" };
}
