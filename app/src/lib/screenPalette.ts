// Key lighting that matches the screen picture (Screen Studio, phase B3).
//
// A five-colour palette comes from the panel frames (median cut). LEDs
// show hue and saturation; their brightness is the keyboard's own setting,
// so every colour is lifted to full value. Near-black pixels mean "off"
// on LEDs and are left out of the palette.

import type { GeometryInfo } from "../bindings/GeometryInfo";
import { H, W, brightestFrame } from "./screenImport";
import { rgbToHex, type ColorMap } from "./lighting";

export interface Swatch {
  /** The colour as the picture has it. */
  hex: string;
  /** The same hue at full LED value. */
  led: string;
  /** Share of the sampled (non-black) pixels, 0–1. */
  share: number;
  /** Where in the picture it sits, 0 (left) – 1 (right). */
  x: number;
  /** Hue in degrees, for ordering. */
  hue: number;
}

type Rgb = [number, number, number];

function from565(v: number): Rgb {
  const r = (v >> 11) & 31;
  const g = (v >> 5) & 63;
  const b = v & 31;
  return [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)];
}

/** Lift a colour to full value; `null` when it is (nearly) black. */
export function toLed([r, g, b]: Rgb): Rgb | null {
  const m = Math.max(r, g, b);
  if (m < 10) return null;
  const k = 255 / m;
  return [Math.round(r * k), Math.round(g * k), Math.round(b * k)];
}

const hexOf = (c: Rgb) => rgbToHex(c[0], c[1], c[2]);

interface Sample {
  c: Rgb;
  x: number;
}

/** Median cut into `n` boxes. */
function medianCut(samples: Sample[], n: number): Sample[][] {
  const boxes: Sample[][] = [samples];
  while (boxes.length < n) {
    let best = -1;
    let bestScore = 0;
    let channel = 0;
    boxes.forEach((box, i) => {
      if (box.length < 2) return;
      for (let ch = 0; ch < 3; ch++) {
        let lo = 255;
        let hi = 0;
        for (const s of box) {
          const v = s.c[ch]!;
          if (v < lo) lo = v;
          if (v > hi) hi = v;
        }
        const score = (hi - lo) * Math.sqrt(box.length);
        if (score > bestScore) {
          bestScore = score;
          best = i;
          channel = ch;
        }
      }
    });
    if (best < 0 || bestScore === 0) break;
    const box = boxes[best]!.sort((a, b) => a.c[channel]! - b.c[channel]!);
    const mid = box.length >> 1;
    boxes.splice(best, 1, box.slice(0, mid), box.slice(mid));
  }
  return boxes;
}

/** The picture's five most telling colours, most important first. With
 *  `delaysMs`, short in-between frames (a slideshow's soft transitions)
 *  are left out when longer frames exist: they only blend two pictures. */
export function extractPalette(frames: Uint16Array[], delaysMs?: number[], size = 5): Swatch[] {
  if (frames.length === 0) return [];
  const longest = delaysMs ? Math.max(...delaysMs) : 0;
  const held = delaysMs && longest >= 300 ? frames.filter((_, i) => (delaysMs[i] ?? 0) >= 300) : frames;
  const pool = held.length ? held : frames;
  const picks = pool.length <= 6 ? pool : Array.from({ length: 6 }, (_, k) => pool[Math.floor((k * pool.length) / 6)]!);
  const lit: Sample[] = [];
  const all: Sample[] = [];
  for (const f of picks) {
    for (let y = 1; y < H; y += 3) {
      for (let x = 1; x < W; x += 3) {
        const c = from565(f[y * W + x]!);
        const s = { c, x: x / (W - 1) };
        all.push(s);
        if (Math.max(c[0], c[1], c[2]) >= 28) lit.push(s);
      }
    }
  }
  const samples = lit.length >= 60 ? lit : all;
  const boxes = medianCut(samples, 10).filter((b) => b.length > 0);
  const scored = boxes.map((box) => {
    let r = 0;
    let g = 0;
    let b = 0;
    let x = 0;
    for (const s of box) {
      r += s.c[0];
      g += s.c[1];
      b += s.c[2];
      x += s.x;
    }
    const n = box.length;
    const c: Rgb = [Math.round(r / n), Math.round(g / n), Math.round(b / n)];
    const max = Math.max(...c);
    const min = Math.min(...c);
    const sat = max ? (max - min) / max : 0;
    const val = max / 255;
    // Colour matters more than size: a small vivid accent beats a large
    // grey, but a large coloured background still leads.
    return { c, n, x: x / n, sat, score: n * (0.15 + sat) ** 2 * (0.3 + val) };
  });
  // Whites, greys and near-blacks (text, outlines) say little about the
  // picture's colour; leave them out when it has real colours.
  const chromatic = scored.filter((s) => s.sat >= 0.2 && s.n / samples.length >= 0.01);
  const pool2 = chromatic.length ? chromatic : scored;
  pool2.sort((a, b) => b.score - a.score);
  const out: Swatch[] = [];
  for (const s of pool2) {
    if (out.length >= size) break;
    const led = toLed(s.c) ?? [255, 255, 255];
    // Skip colours the LEDs would show the same as one already picked.
    if (out.some((o) => distance(hexRgb(o.led), led) < 60)) continue;
    out.push({ hex: hexOf(s.c), led: hexOf(led), share: s.n / samples.length, x: s.x, hue: hueOf(s.c) });
  }
  return out;
}

function hueOf([r, g, b]: Rgb) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  if (max === min) return 0;
  const d = max - min;
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return (h * 60 + 360) % 360;
}

/** Order for a gradient: left to right as in the picture when its colours
 *  sit apart; otherwise (centred text, a single background) by hue, starting
 *  after the widest hue gap so neighbours blend smoothly. */
function gradientOrder(palette: Swatch[]): Swatch[] {
  const xs = palette.map((s) => s.x);
  if (Math.max(...xs) - Math.min(...xs) >= 0.25) return [...palette].sort((a, b) => a.x - b.x);
  const byHue = [...palette].sort((a, b) => a.hue - b.hue);
  let start = 0;
  let widest = -1;
  byHue.forEach((s, i) => {
    const prev = byHue[(i - 1 + byHue.length) % byHue.length]!;
    const gap = (s.hue - prev.hue + 360) % 360 || 360;
    if (gap > widest) {
      widest = gap;
      start = i;
    }
  });
  return [...byHue.slice(start), ...byHue.slice(0, start)];
}

function hexRgb(hex: string): Rgb {
  const v = parseInt(hex.slice(1), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

function distance(a: Rgb, b: Rgb) {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

export type LightMatch = "single" | "gradient" | "image";

/** Every key in the palette's first colour. */
function single(g: GeometryInfo, palette: Swatch[]): ColorMap {
  const map: ColorMap = {};
  const c = palette[0]?.led ?? "#ffffff";
  for (const k of g.keys) map[k.led] = c;
  return map;
}

/** The palette from left to right, in the order the picture has it. */
function gradient(g: GeometryInfo, palette: Swatch[]): ColorMap {
  const stops = gradientOrder(palette).map((s) => hexRgb(s.led));
  const map: ColorMap = {};
  if (stops.length === 0) return map;
  for (const k of g.keys) {
    const u = Math.min(1, Math.max(0, (k.x + k.w / 2) / g.width));
    if (stops.length === 1) {
      map[k.led] = hexOf(stops[0]!);
      continue;
    }
    const pos = u * (stops.length - 1);
    const i = Math.min(stops.length - 2, Math.floor(pos));
    const t = pos - i;
    const a = stops[i]!;
    const b = stops[i + 1]!;
    map[k.led] = hexOf([
      Math.round(a[0] + (b[0] - a[0]) * t),
      Math.round(a[1] + (b[1] - a[1]) * t),
      Math.round(a[2] + (b[2] - a[2]) * t),
    ]);
  }
  return map;
}

/** The picture itself spread over the keys: each key takes the average of
 *  the area it covers. Brightness is lifted on a curve against the 90th
 *  percentile, so a dark coloured background still shows its colour next to
 *  bright text; keys over black stay off. */
function image(g: GeometryInfo, frames: Uint16Array[]): ColorMap {
  const f = brightestFrame(frames);
  const map: ColorMap = {};
  if (!f) return map;
  const avg = g.keys.map((k) => {
    const x0 = Math.max(0, Math.floor((k.x / g.width) * W));
    const x1 = Math.min(W, Math.max(x0 + 1, Math.ceil(((k.x + k.w) / g.width) * W)));
    const y0 = Math.max(0, Math.floor((k.y / g.height) * H));
    const y1 = Math.min(H, Math.max(y0 + 1, Math.ceil(((k.y + k.h) / g.height) * H)));
    let r = 0;
    let gg = 0;
    let b = 0;
    let n = 0;
    for (let y = y0; y < y1; y++) {
      for (let x = x0; x < x1; x++) {
        const c = from565(f[y * W + x]!);
        r += c[0];
        gg += c[1];
        b += c[2];
        n++;
      }
    }
    return { led: k.led, c: [r / n, gg / n, b / n] as Rgb };
  });
  const values = avg.map((a) => Math.max(...a.c)).sort((x, y) => x - y);
  const peak = Math.max(8, values[Math.floor(values.length * 0.9)] ?? 0);
  for (const a of avg) {
    const v = Math.max(...a.c);
    const rel = Math.min(1, v / peak);
    if (rel < 0.06 || v < 4) continue;
    const k = (Math.sqrt(rel) * 255) / v;
    map[a.led] = hexOf(a.c.map((x) => Math.min(255, Math.round(x * k))) as Rgb);
  }
  return map;
}

export function lightDesign(kind: LightMatch, g: GeometryInfo, palette: Swatch[], frames: Uint16Array[]): ColorMap {
  if (kind === "single") return single(g, palette);
  if (kind === "gradient") return gradient(g, palette);
  return image(g, frames);
}
