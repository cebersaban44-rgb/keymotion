// The focus timer's key light, worked out the same way as the backend
// (app/src-tauri/src/pomodoro.rs) so the keyboard drawn in the app shows
// what the real keys show. Keep the two in step.

import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { PomodoroMotion } from "../bindings/PomodoroMotion";
import type { PomodoroPhase } from "../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../bindings/PomodoroSettings";
import type { PomodoroState } from "../bindings/PomodoroState";

const FLASH_MS = 1_000;
const TRACK = 0.03;
const EDGE = 0.04;
const PAUSE_LOW = 0.15;
const PAUSE_HIGH = 0.35;
const PAUSE_PERIOD_MS = 3_000;
const PULSE_LOW = 0.45;
const PULSE_TRACK = 0.22;
const PULSE_PERIOD_MS = 1_600;
const STEPS = 32;
export const TRY_FOCUS_MS = 8_000;
export const TRY_BREAK_MS = 3_000;
/** Length of "Klavyede dene", the end blink included. */
export const TRY_MS = TRY_FOCUS_MS + TRY_BREAK_MS + FLASH_MS;

export type Rgb = [number, number, number];

export interface KeyPos {
  led: number;
  /** 0–1 across the span of the key centres. */
  x: number;
  y: number;
}

export function keyPositions(g: GeometryInfo): KeyPos[] {
  const c = g.keys.map((k) => ({ led: k.led, x: k.x + k.w / 2, y: k.y + k.h / 2 }));
  const span = (v: number[]) => {
    const lo = Math.min(...v);
    return [lo, Math.max(Math.max(...v) - lo, 1e-6)] as const;
  };
  const [x0, xs] = span(c.map((k) => k.x));
  const [y0, ys] = span(c.map((k) => k.y));
  return c.map((k) => ({ led: k.led, x: (k.x - x0) / xs, y: (k.y - y0) / ys }));
}

function drainOrder(m: PomodoroMotion, x: number, y: number) {
  switch (m) {
    case "rightToLeft":
      return x;
    case "leftToRight":
      return 1 - x;
    case "topToBottom":
      return 1 - y;
    case "toCenter":
      return Math.abs(x - 0.5) * 2;
    case "fade":
      return 0;
  }
}

const clamp = (v: number) => Math.min(1, Math.max(0, v));

function lit(m: PomodoroMotion, order: number, left: number) {
  const l = clamp(left);
  if (m === "fade") return l;
  const edge = l * (1 + 2 * EDGE) - EDGE;
  return clamp((edge - order) / EDGE + 0.5);
}

const breathe = (now: number, period: number) => 0.5 + 0.5 * Math.cos((2 * Math.PI * now) / period);

function css(c: Rgb, k: number) {
  const q = Math.round(clamp(k) * STEPS) / STEPS;
  return `rgb(${c.map((v) => Math.round(v * q)).join(",")})`;
}

export function phaseRgb(s: PomodoroSettings, p: PomodoroPhase): Rgb {
  const c = p === "focus" ? s.focusColor : p === "shortBreak" ? s.shortColor : s.longColor;
  return [c[0], c[1], c[2]];
}

export const toHex = (c: readonly number[]) => `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
export const fromHex = (h: string): Rgb => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as Rgb;

export interface LightView {
  status: "running" | "paused";
  phase: PomodoroPhase;
  phaseMs: number;
  /** Time spent in the phase. */
  elapsedMs: number;
}

/** led → CSS colour for the timer, as the keys show it. */
export function lightColors(keys: KeyPos[], s: PomodoroSettings, v: LightView, now: number): Record<number, string> {
  const color = phaseRgb(s, v.phase);
  const out: Record<number, string> = {};
  if (v.elapsedMs < FLASH_MS) {
    for (const k of keys) {
      if (s.transition === "blink") {
        const on = v.elapsedMs < 220 || (v.elapsedMs >= 420 && v.elapsedMs < 640);
        out[k.led] = on ? css(color, 1) : "rgb(0,0,0)";
      } else {
        const on = lit(s.motion, drainOrder(s.motion, k.x, k.y), v.elapsedMs / FLASH_MS);
        out[k.led] = css(color, TRACK + (1 - TRACK) * on);
      }
    }
    return out;
  }
  const left = v.phaseMs > 0 ? 1 - v.elapsedMs / v.phaseMs : 0;
  const pulsing = s.pulse && v.status === "running" && v.phaseMs - v.elapsedMs <= Math.min(v.phaseMs * 0.15, 60_000);
  for (const k of keys) {
    const on = lit(s.motion, drainOrder(s.motion, k.x, k.y), left);
    let level: number;
    if (v.status === "paused") {
      level = (TRACK + (1 - TRACK) * on) * (PAUSE_LOW + (PAUSE_HIGH - PAUSE_LOW) * breathe(now, PAUSE_PERIOD_MS));
    } else if (pulsing) {
      const b = breathe(now, PULSE_PERIOD_MS);
      const track = TRACK + (PULSE_TRACK - TRACK) * b;
      level = track + (1 - track) * on;
      if (on > 0.5) level *= PULSE_LOW + (1 - PULSE_LOW) * b;
    } else {
      level = TRACK + (1 - TRACK) * on;
    }
    out[k.led] = css(color, level);
  }
  return out;
}

/** The end of a series (and of a try): two blinks in the short break's colour. */
function doneColors(keys: KeyPos[], s: PomodoroSettings, since: number) {
  const on = since < 220 || (since >= 420 && since < 640);
  const c = on ? css(phaseRgb(s, "shortBreak"), 1) : "rgb(0,0,0)";
  return Object.fromEntries(keys.map((k) => [k.led, c]));
}

/** "Klavyede dene" at `t` ms from its start (looped by the caller). */
export function tryColors(keys: KeyPos[], s: PomodoroSettings, t: number, now: number): Record<number, string> {
  if (t < TRY_FOCUS_MS) {
    return lightColors(keys, s, { status: "running", phase: "focus", phaseMs: TRY_FOCUS_MS, elapsedMs: t }, now);
  }
  if (t < TRY_FOCUS_MS + TRY_BREAK_MS) {
    const v: LightView = { status: "running", phase: "shortBreak", phaseMs: TRY_BREAK_MS, elapsedMs: t - TRY_FOCUS_MS };
    return lightColors(keys, s, v, now);
  }
  return doneColors(keys, s, t - TRY_FOCUS_MS - TRY_BREAK_MS);
}

/** The running timer's view from its last state. */
export function viewOf(p: PomodoroState, now: number): LightView | null {
  if (p.status === "idle") return null;
  const left = p.status === "running" ? Math.max(0, p.remainingMs - (now - p.atMs)) : p.remainingMs;
  return { status: p.status, phase: p.phase, phaseMs: p.phaseMs, elapsedMs: p.phaseMs - left };
}
