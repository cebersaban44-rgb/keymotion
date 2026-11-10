// Slideshows for the keyboard screen (Screen Studio, phase B2).
//
// The panel plays frames, each shown for at most 508 ms (delay byte 254).
// A slide that stays longer is the same frame repeated, so every slide
// costs ⌈seconds / 0.508⌉ of the 140 frames; a soft transition adds three
// blended frames of 100 ms.

import {
  H,
  MAX_DELAY_MS,
  MAX_FRAMES,
  MIN_DELAY_MS,
  brightestFrame,
  outputCanvas,
  renderAt,
  toRgb565,
  toRgba,
  W,
  type Source,
} from "./screenImport";

export const MAX_SLIDES = 24;
export const SLIDE_MIN_S = 1;
export const SLIDE_MAX_S = 20;
export const SLIDE_DEFAULT_S = 3;
export const FADE_FRAMES = 3;
export const FADE_MS = 100;

export type Transition = "cut" | "fade";
export type SlideFit = "fill" | "fit";

/** Frames a still slide of `seconds` needs. */
export function framesFor(seconds: number) {
  return Math.max(1, Math.ceil((seconds * 1000) / MAX_DELAY_MS));
}

export function budget(seconds: number[], transition: Transition) {
  const fades = transition === "fade" && seconds.length > 1 ? FADE_FRAMES * seconds.length : 0;
  const frames = seconds.reduce((a, s) => a + framesFor(s), 0) + fades;
  const loopMs = seconds.reduce((a, s) => a + s * 1000, 0) + fades * FADE_MS;
  return { frames, loopMs, fits: frames <= MAX_FRAMES };
}

function evenDelay(ms: number) {
  return Math.min(MAX_DELAY_MS, Math.max(MIN_DELAY_MS, Math.round(ms / 2) * 2));
}

/** Mix two panel frames (k = share of `b`), in 8-bit and back to RGB565. */
function blend(a: Uint16Array, b: Uint16Array, k: number): Uint16Array {
  const x = toRgba(a);
  const y = toRgba(b);
  for (let i = 0; i < x.length; i++) x[i] = x[i]! + (y[i]! - x[i]!) * k;
  return toRgb565(x, false);
}

/** The frames the keyboard gets: each still held for its time, with soft
 *  transitions (also from the last slide back to the first, so the loop
 *  is seamless). */
export function assemble(stills: Uint16Array[], seconds: number[], transition: Transition) {
  const frames: Uint16Array[] = [];
  const delaysMs: number[] = [];
  const n = stills.length;
  stills.forEach((still, i) => {
    const count = framesFor(seconds[i]!);
    const each = evenDelay((seconds[i]! * 1000) / count);
    for (let k = 0; k < count; k++) {
      frames.push(still);
      delaysMs.push(each);
    }
    if (transition === "fade" && n > 1) {
      const next = stills[(i + 1) % n]!;
      for (let k = 1; k <= FADE_FRAMES; k++) {
        frames.push(blend(still, next, k / (FADE_FRAMES + 1)));
        delaysMs.push(FADE_MS);
      }
    }
  });
  return { frames, delaysMs };
}

/** One moment of a file as a 240×135 panel frame. */
export async function renderStill(source: Source, fit: SlideFit, atMs: number, dither: boolean): Promise<Uint16Array> {
  const { ctx } = outputCanvas();
  await renderAt(source, ctx, atMs, { fit, zoom: 1, panX: 0, panY: 0 });
  return toRgb565(ctx.getImageData(0, 0, W, H).data, dither);
}

/** The moment of an animation or video that shows the most (videos and
 *  GIFs often start on black); 0 for a still image. */
export async function bestMoment(source: Source, fit: SlideFit, dither: boolean): Promise<number> {
  if (source.kind === "image" || source.durationMs <= 0) return 0;
  const moments = Array.from({ length: 8 }, (_, k) => Math.floor((source.durationMs * (k + 0.5)) / 8));
  const frames: Uint16Array[] = [];
  for (const at of moments) frames.push(await renderStill(source, fit, at, dither));
  const best = brightestFrame(frames);
  return moments[Math.max(0, frames.indexOf(best!))] ?? 0;
}

