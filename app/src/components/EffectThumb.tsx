// Small moving picture of a built-in effect for the effect list: 8×3 mini
// keys, each effect with its own motion (an approximation, not a render of
// the firmware). Single-colour effects use the chosen colour, so "Sabit"
// is one solid colour; Renk cümbüşü and Spektrum change colour themselves.

import type { CSSProperties } from "react";

const COLS = 8;
const ROWS = 3;

type Kind = "none" | "pulse" | "flash" | "twinkle" | "dip" | "beat" | "hue" | "shuttle";

interface Cell {
  /** Animated; otherwise it stays at `still`. */
  on: boolean;
  /** Seconds after the start of a cycle when this key lights. */
  d: number;
  /** Level in a still frame (reduced motion) and for keys that do not move. */
  still?: number;
  /** Starting hue for colour-cycling effects. */
  h0?: number;
}

interface Spec {
  kind: Kind;
  /** Seconds per cycle. */
  period: number;
  /** Level between flashes. */
  lo?: number;
  cell: (c: number, r: number, i: number) => Cell;
}

/** Stable pseudo-random 0–1 per key, so the picture does not change on re-render. */
const rnd = (i: number, k = 1) => {
  const x = Math.sin(i * 12.9898 + k * 78.233) * 43758.5453;
  return x - Math.floor(x);
};
const dist = (c: number, r: number, cx: number, cy: number) => Math.hypot(c - cx, (r - cy) * 1.3);
const CX = 3.5;
const CY = 1;

const all = (d: (c: number, r: number, i: number) => number) => (c: number, r: number, i: number) => ({ on: true, d: d(c, r, i) });
const some = (lit: [number, number, number][], rest: number) => (c: number, r: number) => {
  const hit = lit.find(([x, y]) => x === c && y === r);
  return hit ? { on: true, d: hit[2] } : { on: false, d: 0, still: rest };
};

const SPECS: Record<number, Spec> = {
  0: { kind: "none", period: 1, cell: () => ({ on: false, d: 0, still: 0 }) },
  1: { kind: "none", period: 1, cell: () => ({ on: false, d: 0, still: 1 }) },
  // A pressed key lights up and fades.
  2: { kind: "flash", period: 3, lo: 0, cell: some([[1, 1, 0], [5, 0, 1], [6, 2, 2]], 0) },
  // Everything lit; a pressed key goes dark for a moment.
  3: { kind: "dip", period: 3, cell: some([[1, 1, 0], [5, 0, 1], [6, 2, 2]], 1) },
  4: { kind: "twinkle", period: 2.2, lo: 0.12, cell: all((_c, _r, i) => rnd(i) * 2.2) },
  5: { kind: "pulse", period: 3.2, lo: 0, cell: all((_c, _r, i) => rnd(i, 2) * 3.2) },
  6: { kind: "hue", period: 5, cell: (_c, _r, i) => ({ on: true, d: rnd(i, 3) * 5, h0: rnd(i, 3) * 360 }) },
  7: { kind: "pulse", period: 3, lo: 0.06, cell: all(() => 0) },
  8: { kind: "hue", period: 6, cell: all(() => 0) },
  9: { kind: "pulse", period: 2, lo: 0.08, cell: all((c, r) => dist(c, r, CX, CY) * 0.22) },
  10: { kind: "pulse", period: 1.8, lo: 0.1, cell: all((_c, r) => r * 0.35) },
  11: { kind: "pulse", period: 2.2, lo: 0.12, cell: all((c) => c * 0.16) },
  12: {
    kind: "pulse",
    period: 2.4,
    lo: 0.08,
    cell: all((c, r) => ((Math.atan2(r - CY, c - CX) + Math.PI) / (2 * Math.PI)) * 2.4),
  },
  // A burst around one key.
  13: { kind: "flash", period: 2.6, lo: 0, cell: all((c, r) => dist(c, r, 2, 1) * 0.07) },
  // Light shoots up from a key, then along the top row.
  14: {
    kind: "flash",
    period: 2.4,
    lo: 0,
    cell: (c, r) =>
      c === 4 ? { on: true, d: (2 - r) * 0.12 } : r === 0 ? { on: true, d: 0.24 + Math.abs(c - 4) * 0.1 } : { on: false, d: 0, still: 0 },
  },
  // Rings travelling out from a key.
  15: { kind: "flash", period: 1.5, lo: 0.04, cell: all((c, r) => dist(c, r, 5, 1) * 0.25) },
  // A light running along a snake path.
  16: { kind: "flash", period: 2.4, lo: 0.1, cell: all((c, r) => (r % 2 ? r * COLS + COLS - 1 - c : r * COLS + c) * 0.1) },
  17: { kind: "beat", period: 1.6, cell: all(() => 0) },
  18: { kind: "pulse", period: 2, lo: 0.08, cell: all((c, r) => (c + r) * 0.15) },
  19: { kind: "shuttle", period: 1.6, cell: () => ({ on: false, d: 0, still: 1 }) },
};

/** Level at `phase` (0–1) of the cycle, for the still frame. */
function levelAt(kind: Kind, phase: number, lo: number): number {
  switch (kind) {
    case "pulse":
      return lo + ((1 - lo) * (1 - Math.cos(2 * Math.PI * phase))) / 2;
    case "flash":
      return phase < 0.4 ? 1 - ((1 - lo) * phase) / 0.4 : lo;
    case "twinkle":
      return phase < 0.06 ? lo + ((1 - lo) * phase) / 0.06 : phase < 0.22 ? 1 - ((1 - lo) * (phase - 0.06)) / 0.16 : lo;
    case "dip":
      return phase < 0.06 ? 1 - (0.96 * phase) / 0.06 : phase < 0.45 ? 0.04 + (0.96 * (phase - 0.06)) / 0.39 : 1;
    case "beat":
      return 0.7;
    default:
      return 1;
  }
}

type Vars = CSSProperties & Record<`--${string}`, string | number>;

export function EffectThumb({ id, color, rainbow }: { id: number; color: string; rainbow: boolean }) {
  const spec = SPECS[id] ?? SPECS[11]!;
  const lo = spec.lo ?? 0.1;
  const box: Vars = { "--p": `${spec.period}s`, "--lo": lo };
  return (
    <div className={`fxp fxp-${spec.kind}`} style={box} aria-hidden>
      {Array.from({ length: COLS * ROWS }, (_, i) => {
        const c = i % COLS;
        const r = Math.floor(i / COLS);
        const cell = spec.cell(c, r, i);
        // Negative delays start every key mid-cycle, so nothing waits to begin.
        const lag = ((cell.d % spec.period) + spec.period) % spec.period;
        const phase = (((0.3 * spec.period - cell.d) / spec.period) % 1 + 1) % 1;
        const still = cell.still ?? (cell.on ? levelAt(spec.kind, phase, lo) : 1);
        const style: Vars = {
          "--d": `${(lag - spec.period).toFixed(3)}s`,
          "--still": still.toFixed(3),
          "--fx-c": rainbow ? `oklch(0.74 0.17 ${c * 45})` : color,
        };
        if (cell.h0 !== undefined) style["--h0"] = cell.h0.toFixed(0);
        return <span key={i} className={cell.on ? "on" : undefined} style={style} />;
      })}
    </div>
  );
}
