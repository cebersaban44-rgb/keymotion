// Text and template cards for the keyboard screen (Screen Studio, phase B1).
//
// A card is drawn on two 240×135 layers: the background (solid or gradient)
// and the foreground (text and line art). They are composed here, not by the
// canvas, so that only background pixels get dithered: gradients stay smooth
// on the RGB565 panel while text edges stay crisp.

import { H, MAX_DELAY_MS, MAX_FRAMES, MIN_DELAY_MS, STILL_DELAY_MS, W, toRgb565, type Prepared } from "./screenImport";

export type TemplateId = "name" | "quote" | "marquee" | "mountain" | "badge";
export const TEMPLATES: TemplateId[] = ["name", "quote", "marquee", "mountain", "badge"];

export type FontId = "display" | "sans" | "mono";
export type Align = "left" | "center" | "right";
export type GradDir = "v" | "h" | "d";

export interface CardSpec {
  template: TemplateId;
  title: string;
  subtitle: string;
  /** Badge template: an emoji or one or two letters. */
  emoji: string;
  font: FontId;
  upper: boolean;
  /** Largest title size in px; titles that do not fit are drawn smaller. */
  size: number;
  align: Align;
  fg: string;
  accent: string;
  gradient: boolean;
  bg1: string;
  bg2: string;
  dir: GradDir;
  /** Scrolling text: px per second and frames per second. */
  speed: number;
  fps: number;
}

/** Title size range per template (px). */
export const SIZES: Record<TemplateId, { def: number; min: number; max: number }> = {
  name: { def: 44, min: 16, max: 96 },
  quote: { def: 24, min: 12, max: 40 },
  marquee: { def: 64, min: 20, max: 110 },
  mountain: { def: 30, min: 14, max: 64 },
  badge: { def: 28, min: 14, max: 56 },
};

/** Which controls a template uses. */
export const USES: Record<TemplateId, { align: boolean; emoji: boolean; motion: boolean }> = {
  name: { align: true, emoji: false, motion: false },
  quote: { align: true, emoji: false, motion: false },
  marquee: { align: true, emoji: false, motion: true },
  mountain: { align: true, emoji: false, motion: false },
  badge: { align: false, emoji: true, motion: false },
};

export interface Palette {
  id: string;
  gradient: boolean;
  bg1: string;
  bg2: string;
  dir: GradDir;
  fg: string;
  accent: string;
}

/** Colour themes. "night" matches the firmware's own home page. */
export const PALETTES: Palette[] = [
  { id: "night", gradient: true, bg1: "#0d2a5a", bg2: "#071a3c", dir: "v", fg: "#f4f8ff", accent: "#5fd4ff" },
  { id: "sunset", gradient: true, bg1: "#ff7a45", bg2: "#4b1d7a", dir: "d", fg: "#fff6e8", accent: "#ffd166" },
  { id: "forest", gradient: true, bg1: "#123f30", bg2: "#06170f", dir: "v", fg: "#eafff3", accent: "#7ee08a" },
  { id: "neon", gradient: true, bg1: "#1a0636", bg2: "#000000", dir: "d", fg: "#ffffff", accent: "#ff3fd8" },
  { id: "paper", gradient: false, bg1: "#f3eee3", bg2: "#e2dccd", dir: "v", fg: "#1c1b19", accent: "#d9480f" },
  { id: "mono", gradient: false, bg1: "#000000", bg2: "#1a1a1a", dir: "v", fg: "#ffffff", accent: "#9aa0a6" },
];

export const EMOJI_PICKS = ["☕", "🎧", "🎮", "💻", "🚀", "💡", "🔥", "⭐", "🌙", "❤️", "✅", "⏳"];

export function defaultSpec(template: TemplateId = "name"): CardSpec {
  const p = PALETTES[0]!;
  return {
    template,
    title: "",
    subtitle: "",
    emoji: "☕",
    font: "display",
    upper: false,
    size: SIZES[template].def,
    align: template === "badge" ? "left" : "center",
    fg: p.fg,
    accent: p.accent,
    gradient: p.gradient,
    bg1: p.bg1,
    bg2: p.bg2,
    dir: p.dir,
    speed: 60,
    // The factory animation's rate, proven on the panel; faster is optional.
    fps: 10,
  };
}

// ------------------------------------------------------------------ fonts

const FONTS: Record<FontId, { family: string; weight: number; sub: number }> = {
  display: { family: '"Bricolage Grotesque Variable"', weight: 700, sub: 500 },
  sans: { family: '"Instrument Sans Variable"', weight: 650, sub: 500 },
  mono: { family: '"JetBrains Mono Variable"', weight: 700, sub: 500 },
};
const EMOJI_FONTS = '"Segoe UI Emoji", "Apple Color Emoji", "Noto Color Emoji"';

function fontOf(spec: CardSpec, px: number, sub = false) {
  const f = FONTS[spec.font];
  return `${sub ? f.sub : f.weight} ${px}px ${f.family}, ${EMOJI_FONTS}`;
}

/** Load the card's font (with the subsets its text needs, e.g. Turkish
 *  letters) before drawing; canvas text otherwise falls back silently. */
export async function ensureFonts(spec: CardSpec) {
  const text = `${spec.title} ${spec.subtitle} ${spec.emoji} Hg“—`;
  try {
    await Promise.all([document.fonts.load(fontOf(spec, 40), text), document.fonts.load(fontOf(spec, 16, true), text)]);
  } catch {
    // Drawing still works with the fallback font.
  }
}

// ----------------------------------------------------------------- layout

export interface CardLayout {
  frames: number;
  delayMs: number;
  /** The title was drawn at this size (px) to fit; `null` when it fit. */
  shrunkTo: number | null;
  /** Scrolling text: the speed actually used (px/s) after fitting the loop
   *  into the keyboard's frame limit. */
  speed: number | null;
  draw(bg: CanvasRenderingContext2D, fg: CanvasRenderingContext2D, frame: number): void;
}

const PAD = 16;

function capHeight(ctx: CanvasRenderingContext2D) {
  return Math.ceil(ctx.measureText("H").actualBoundingBoxAscent);
}

/** Largest size ≤ `px` (≥ `min`) at which `text` is at most `maxW` wide. */
function fitPx(ctx: CanvasRenderingContext2D, spec: CardSpec, text: string, px: number, min: number, maxW: number, sub = false) {
  ctx.font = fontOf(spec, px, sub);
  const w = ctx.measureText(text).width;
  if (w <= maxW || !text) return px;
  let p = Math.max(min, Math.floor((px * maxW) / w));
  ctx.font = fontOf(spec, p, sub);
  while (p > min && ctx.measureText(text).width > maxW) {
    p--;
    ctx.font = fontOf(spec, p, sub);
  }
  return p;
}

/** Word wrap; words longer than a line are broken by letters. */
function wrap(ctx: CanvasRenderingContext2D, text: string, maxW: number): string[] {
  const out: string[] = [];
  for (const para of text.split(/\r?\n/)) {
    let line = "";
    for (const word of para.split(/\s+/).filter(Boolean)) {
      const tryLine = line ? `${line} ${word}` : word;
      if (ctx.measureText(tryLine).width <= maxW) {
        line = tryLine;
        continue;
      }
      if (line) out.push(line);
      line = "";
      if (ctx.measureText(word).width <= maxW) {
        line = word;
        continue;
      }
      for (const ch of Array.from(word)) {
        if (ctx.measureText(line + ch).width > maxW && line) {
          out.push(line);
          line = "";
        }
        line += ch;
      }
    }
    if (line || para.trim() === "") out.push(line);
  }
  while (out.length > 1 && out[out.length - 1] === "") out.pop();
  return out;
}

function anchorX(align: Align, left = PAD, right = W - PAD) {
  return align === "left" ? left : align === "right" ? right : (left + right) / 2;
}

function paintBackground(spec: CardSpec, ctx: CanvasRenderingContext2D) {
  ctx.clearRect(0, 0, W, H);
  if (!spec.gradient) {
    ctx.fillStyle = spec.bg1;
  } else {
    const g =
      spec.dir === "v"
        ? ctx.createLinearGradient(0, 0, 0, H)
        : spec.dir === "h"
          ? ctx.createLinearGradient(0, 0, W, 0)
          : ctx.createLinearGradient(0, 0, W, H);
    g.addColorStop(0, spec.bg1);
    g.addColorStop(1, spec.bg2);
    ctx.fillStyle = g;
  }
  ctx.fillRect(0, 0, W, H);
}

function luminance(hex: string) {
  const v = parseInt(hex.slice(1), 16);
  return (0.2126 * ((v >> 16) & 255) + 0.7152 * ((v >> 8) & 255) + 0.0722 * (v & 255)) / 255;
}

/** The drawn pixels' box of `text` relative to its origin (left, alphabetic
 *  baseline). Measured from pixels because colour emoji fonts report
 *  unreliable bounding boxes. */
function inkBox(text: string, font: string) {
  const size = 160;
  const ox = 32;
  const oy = 112;
  const c = document.createElement("canvas");
  c.width = size;
  c.height = size;
  const ctx = c.getContext("2d", { willReadFrequently: true })!;
  ctx.font = font;
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "#fff";
  ctx.fillText(text, ox, oy);
  const a = ctx.getImageData(0, 0, size, size).data;
  let x0 = size, y0 = size, x1 = -1, y1 = -1;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      if (a[(y * size + x) * 4 + 3]! > 24) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        if (y > y1) y1 = y;
      }
    }
  }
  if (x1 < 0) return null;
  return { cx: (x0 + x1 + 1) / 2 - ox, cy: (y0 + y1 + 1) / 2 - oy, w: x1 - x0 + 1, h: y1 - y0 + 1 };
}

/** Small deterministic generator (stars stay where they are). */
function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

/** Lay a card out once (measuring its text); `draw` then paints any frame. */
export function layoutCard(spec: CardSpec, ctx: CanvasRenderingContext2D, locale: string): CardLayout {
  const title = (spec.upper ? spec.title.toLocaleUpperCase(locale) : spec.title).trim();
  const subtitle = spec.subtitle.trim();
  const still = (shrunkTo: number | null, paint: (fg: CanvasRenderingContext2D) => void): CardLayout => ({
    frames: 1,
    delayMs: STILL_DELAY_MS,
    shrunkTo,
    speed: null,
    draw(bg, fg) {
      paintBackground(spec, bg);
      fg.clearRect(0, 0, W, H);
      fg.globalAlpha = 1;
      fg.textBaseline = "alphabetic";
      paint(fg);
    },
  });

  switch (spec.template) {
    case "name": {
      const maxW = W - 2 * PAD;
      const tPx = fitPx(ctx, spec, title, spec.size, 10, maxW);
      ctx.font = fontOf(spec, tPx);
      const capT = title ? capHeight(ctx) : 0;
      const sPx = subtitle ? fitPx(ctx, spec, subtitle, Math.min(18, Math.max(10, Math.round(tPx * 0.34))), 8, maxW, true) : 0;
      ctx.font = fontOf(spec, sPx || 10, true);
      const capS = subtitle ? capHeight(ctx) : 0;
      const gap1 = Math.max(6, Math.round(tPx * 0.3));
      const total = capT + (subtitle ? gap1 + 3 + 7 + capS : 0);
      const top = Math.round((H - total) / 2);
      const tBase = top + capT;
      const barY = tBase + gap1;
      const sBase = barY + 3 + 7 + capS;
      const x = anchorX(spec.align);
      return still(tPx < spec.size ? tPx : null, (fg) => {
        fg.textAlign = spec.align;
        fg.fillStyle = spec.fg;
        fg.font = fontOf(spec, tPx);
        if (title) fg.fillText(title, x, tBase);
        if (!subtitle) return;
        fg.fillStyle = spec.accent;
        const barX = spec.align === "left" ? PAD : spec.align === "right" ? W - PAD - 22 : W / 2 - 11;
        fg.fillRect(Math.round(barX), barY, 22, 3);
        fg.font = fontOf(spec, sPx, true);
        fg.fillText(subtitle, x, sBase);
      });
    }

    case "quote": {
      const boxL = 44;
      const boxR = W - PAD;
      const boxT = 14;
      const boxB = subtitle ? H - 30 : H - 16;
      const boxW = boxR - boxL;
      let px = spec.size;
      let lines: string[] = [];
      let lineH = 0;
      let cap = 0;
      for (; px >= 10; px--) {
        ctx.font = fontOf(spec, px);
        lines = wrap(ctx, title, boxW);
        lineH = Math.round(px * 1.2);
        cap = capHeight(ctx);
        if (cap + (lines.length - 1) * lineH <= boxB - boxT) break;
      }
      px = Math.max(10, px);
      const total = cap + (lines.length - 1) * lineH;
      const top = Math.round(boxT + (boxB - boxT - total) / 2);
      const x = anchorX(spec.align, boxL, boxR);
      return still(px < spec.size ? px : null, (fg) => {
        fg.fillStyle = spec.accent;
        fg.textAlign = "left";
        fg.font = `700 56px ${FONTS.display.family}`;
        fg.fillText("“", 10, 54);
        fg.fillStyle = spec.fg;
        fg.textAlign = spec.align;
        fg.font = fontOf(spec, px);
        lines.forEach((l, k) => fg.fillText(l, x, top + cap + k * lineH));
        if (!subtitle) return;
        fg.fillStyle = spec.accent;
        fg.textAlign = "right";
        fg.font = fontOf(spec, 12, true);
        fg.fillText(`— ${subtitle}`, W - PAD, H - 12);
      });
    }

    case "marquee": {
      const text = title.replace(/\s*\n\s*/g, " ");
      const px = Math.min(spec.size, 110);
      ctx.font = fontOf(spec, px);
      const cap = capHeight(ctx);
      const textW = Math.ceil(ctx.measureText(text).width);
      const sTop = subtitle ? 24 : 0;
      const base = Math.round(sTop + (H - sTop - cap) / 2 + cap);
      const paintSub = (fg: CanvasRenderingContext2D) => {
        if (!subtitle) return;
        fg.fillStyle = spec.accent;
        fg.textAlign = spec.align;
        fg.font = fontOf(spec, 12, true);
        fg.fillText(subtitle, anchorX(spec.align), 18);
      };
      if (!text) return still(null, paintSub);
      const fps = Math.max(1, spec.fps);
      const delayMs = Math.min(MAX_DELAY_MS, Math.max(MIN_DELAY_MS, Math.round(1000 / fps)));
      const minGap = Math.max(32, Math.round(px * 0.8));
      // Whole-pixel steps keep glyphs sharp; the gap absorbs the rounding so
      // the loop closes exactly.
      let step = Math.max(1, Math.round(spec.speed / fps));
      step = Math.max(step, Math.ceil((textW + minGap) / MAX_FRAMES));
      const frames = Math.max(2, Math.ceil((textW + minGap) / step));
      const dist = frames * step;
      const gap = dist - textW;
      return {
        frames,
        delayMs,
        shrunkTo: null,
        speed: Math.round((step * 1000) / delayMs),
        draw(bg, fg, i) {
          paintBackground(spec, bg);
          fg.clearRect(0, 0, W, H);
          fg.textBaseline = "alphabetic";
          paintSub(fg);
          fg.textAlign = "left";
          for (let x = -(i % frames) * step; x < W; x += dist) {
            fg.fillStyle = spec.fg;
            fg.font = fontOf(spec, px);
            fg.fillText(text, x, base);
            fg.fillStyle = spec.accent;
            fg.beginPath();
            fg.arc(x + textW + gap / 2, base - cap / 2, Math.max(2, px / 16), 0, Math.PI * 2);
            fg.fill();
          }
        },
      };
    }

    case "mountain": {
      // Leave the moon's corner free: both sides when centred.
      const maxW = spec.align === "center" ? W - 2 * (PAD + 26) : W - 2 * PAD - 30;
      const tPx = fitPx(ctx, spec, title, spec.size, 10, maxW);
      ctx.font = fontOf(spec, tPx);
      const capT = title ? capHeight(ctx) : 0;
      const sPx = subtitle ? fitPx(ctx, spec, subtitle, Math.min(16, Math.max(10, Math.round(tPx * 0.42))), 8, maxW, true) : 0;
      ctx.font = fontOf(spec, sPx || 10, true);
      const capS = subtitle ? capHeight(ctx) : 0;
      const tBase = 16 + capT;
      const sBase = tBase + (title ? 8 : 0) + capS;
      const x = anchorX(spec.align);
      const moonX = spec.align === "right" ? 24 : W - 24;
      const dark = luminance(spec.bg1) < 0.35;
      const r = rng(0x5eed);
      const stars = Array.from({ length: 22 }, () => ({ x: Math.floor(r() * W), y: Math.floor(4 + r() * 70), a: 0.3 + r() * 0.5 }));
      const far: [number, number][] = [
        [0, 96], [0.08, 88], [0.16, 92], [0.27, 74], [0.36, 84], [0.44, 78], [0.55, 63], [0.63, 76], [0.72, 70], [0.82, 82], [0.9, 75], [1, 86],
      ];
      const near: [number, number][] = [
        [0, 118], [0.1, 104], [0.2, 112], [0.32, 95], [0.42, 108], [0.5, 100], [0.6, 113], [0.7, 98], [0.78, 106], [0.88, 91], [0.95, 101], [1, 97],
      ];
      const ridge = (fg: CanvasRenderingContext2D, pts: [number, number][]) => {
        fg.beginPath();
        pts.forEach(([px, py], k) => (k ? fg.lineTo(px * W, py + 0.5) : fg.moveTo(px * W, py + 0.5)));
      };
      return still(tPx < spec.size ? tPx : null, (fg) => {
        if (dark) {
          for (const s of stars) {
            fg.fillStyle = spec.fg;
            fg.globalAlpha = s.a;
            fg.fillRect(s.x, s.y, 1, 1);
          }
          fg.globalAlpha = 1;
        }
        fg.strokeStyle = spec.accent;
        fg.lineJoin = "round";
        fg.lineWidth = 1.5;
        fg.beginPath();
        fg.arc(moonX, 22, 8, 0, Math.PI * 2);
        fg.stroke();
        fg.globalAlpha = 0.55;
        fg.lineWidth = 1.25;
        ridge(fg, far);
        fg.stroke();
        fg.globalAlpha = 1;
        ridge(fg, near);
        fg.lineTo(W, H);
        fg.lineTo(0, H);
        fg.closePath();
        fg.fillStyle = spec.gradient ? spec.bg2 : spec.bg1;
        fg.fill();
        fg.lineWidth = 1.5;
        ridge(fg, near);
        fg.stroke();
        fg.textAlign = spec.align;
        fg.fillStyle = spec.fg;
        fg.font = fontOf(spec, tPx);
        if (title) fg.fillText(title, x, tBase);
        if (!subtitle) return;
        fg.fillStyle = spec.accent;
        fg.font = fontOf(spec, sPx, true);
        fg.fillText(subtitle, x, sBase);
      });
    }

    case "badge": {
      const cx = 52;
      const cy = H / 2;
      const left = 100;
      const maxW = W - PAD - left;
      const tPx = fitPx(ctx, spec, title, spec.size, 10, maxW);
      ctx.font = fontOf(spec, tPx);
      const capT = title ? capHeight(ctx) : 0;
      const sPx = Math.min(14, Math.max(10, Math.round(tPx * 0.5)));
      ctx.font = fontOf(spec, sPx, true);
      const lines = subtitle ? wrap(ctx, subtitle, maxW).slice(0, 3) : [];
      const capS = lines.length ? capHeight(ctx) : 0;
      const lineH = Math.round(sPx * 1.3);
      const total = capT + (lines.length ? 9 + capS + (lines.length - 1) * lineH : 0);
      const tBase = Math.round((H - total) / 2 + capT);
      const sBase = tBase + (title ? 9 : 0) + capS;
      const mark = Array.from(spec.emoji.trim()).slice(0, 4).join("");
      // Fit the mark inside the circle, then centre it by its drawn pixels.
      let mPx = 40;
      let box = mark ? inkBox(mark, fontOf(spec, mPx)) : null;
      if (box && Math.max(box.w, box.h) > 50) {
        mPx = Math.max(10, Math.floor((mPx * 50) / Math.max(box.w, box.h)));
        box = inkBox(mark, fontOf(spec, mPx));
      }
      return still(tPx < spec.size ? tPx : null, (fg) => {
        fg.fillStyle = spec.accent;
        fg.globalAlpha = 0.16;
        fg.beginPath();
        fg.arc(cx, cy, 36, 0, Math.PI * 2);
        fg.fill();
        fg.globalAlpha = 1;
        fg.strokeStyle = spec.accent;
        fg.lineWidth = 2;
        fg.stroke();
        if (mark && box) {
          fg.textAlign = "left";
          fg.fillStyle = spec.fg;
          fg.font = fontOf(spec, mPx);
          fg.fillText(mark, Math.round(cx - box.cx), Math.round(cy - box.cy));
        }
        fg.textAlign = "left";
        fg.fillStyle = spec.fg;
        fg.font = fontOf(spec, tPx);
        if (title) fg.fillText(title, left, tBase);
        fg.fillStyle = spec.accent;
        fg.font = fontOf(spec, sPx, true);
        lines.forEach((l, k) => fg.fillText(l, left, sBase + k * lineH));
      });
    }
  }
}

// ---------------------------------------------------------------- output

export interface CardLayers {
  bg: CanvasRenderingContext2D;
  fg: CanvasRenderingContext2D;
}

export function makeLayers(): CardLayers {
  const make = () => {
    const c = document.createElement("canvas");
    c.width = W;
    c.height = H;
    const ctx = c.getContext("2d", { willReadFrequently: true })!;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    return ctx;
  };
  return { bg: make(), fg: make() };
}

/** Compose the two layers into the panel's RGB565 pixels. */
export function compose565(bg: Uint8ClampedArray, fg: Uint8ClampedArray, ditherBackground: boolean): Uint16Array {
  const rgba = new Uint8ClampedArray(W * H * 4);
  const mask = new Uint8Array(W * H);
  for (let i = 0, p = 0; i < W * H; i++, p += 4) {
    const a = fg[p + 3]!;
    if (a === 0) {
      rgba[p] = bg[p]!;
      rgba[p + 1] = bg[p + 1]!;
      rgba[p + 2] = bg[p + 2]!;
      mask[i] = ditherBackground ? 1 : 0;
    } else {
      const k = a / 255;
      rgba[p] = fg[p]! * k + bg[p]! * (1 - k);
      rgba[p + 1] = fg[p + 1]! * k + bg[p + 1]! * (1 - k);
      rgba[p + 2] = fg[p + 2]! * k + bg[p + 2]! * (1 - k);
    }
    rgba[p + 3] = 255;
  }
  return toRgb565(rgba, mask);
}

/** One frame of a laid-out card, as the panel will show it. */
export function cardFrame(spec: CardSpec, layout: CardLayout, layers: CardLayers, frame: number): Uint16Array {
  layout.draw(layers.bg, layers.fg, frame);
  return compose565(
    layers.bg.getImageData(0, 0, W, H).data,
    layers.fg.getImageData(0, 0, W, H).data,
    spec.gradient,
  );
}

/** Every frame of the card, ready for the keyboard. Cancellable. */
export async function prepareCard(
  spec: CardSpec,
  locale: string,
  name: string,
  onProgress?: (done: number, total: number) => void,
  signal?: AbortSignal,
): Promise<Prepared> {
  await ensureFonts(spec);
  const layers = makeLayers();
  const layout = layoutCard(spec, layers.fg, locale);
  const frames: Uint16Array[] = [];
  for (let i = 0; i < layout.frames; i++) {
    if (signal?.aborted) throw new DOMException("aborted", "AbortError");
    frames.push(cardFrame(spec, layout, layers, i));
    onProgress?.(i + 1, layout.frames);
    if (i % 8 === 7) await new Promise((r) => setTimeout(r));
  }
  return {
    frames,
    delaysMs: frames.map(() => layout.delayMs),
    name,
    kind: frames.length > 1 ? "animation" : "image",
  };
}
