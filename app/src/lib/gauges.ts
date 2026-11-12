// System gauges ("Sistem göstergeleri", phase C, the LED live layer). The
// readings, the frames and the key lights live in the backend
// (app/src-tauri/src/gauges.rs); the page changes the settings and shows
// the frame the backend works out. The browser preview keeps a small copy
// of the rules for its fake readings: keep the two in step.

import { create } from "zustand";
import type { GaugeReadings } from "../bindings/GaugeReadings";
import type { GaugesSettings } from "../bindings/GaugesSettings";
import type { GaugesState } from "../bindings/GaugesState";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { LedColor } from "../bindings/LedColor";
import { inTauri, invoke, listen } from "./ipc";

export const DEFAULT_GAUGES: GaugesSettings = {
  enabled: false,
  cpu: true,
  gpu: true,
  ram: false,
  volume: true,
  mic: true,
  micKey: "delete",
  style: "heat",
  track: true,
  base: "dark",
  tint: [124, 92, 255],
  brightness: 4,
};

/** Key rows of the gauges (key ids of the model geometry), left to right. */
export const GAUGE_ROWS = {
  cpu: ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
  gpu: ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
  ram: ["a", "s", "d", "f", "g", "h", "j", "k", "l", "semicolon"],
  volume: ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"],
} as const;

type Rgb = [number, number, number];
const HEAT_GREEN: Rgb = [52, 199, 89];
const HEAT_AMBER: Rgb = [255, 176, 32];
const HEAT_RED: Rgb = [255, 59, 48];
export const OWN_COLORS: Record<"cpu" | "gpu" | "ram", Rgb> = {
  cpu: [61, 165, 255],
  gpu: [160, 107, 255],
  ram: [46, 196, 182],
};
const VOLUME_COLOR: Rgb = [235, 235, 235];
const MUTED_COLOR: Rgb = [255, 59, 48];
const MIC_COLOR: Rgb = [255, 40, 40];
const TRACK = 0.03;
const TINT = 0.12;
const STEPS = 32;
const VOLUME_SHOW_MS = 1_500;

export const heatRgb = { green: HEAT_GREEN, amber: HEAT_AMBER, red: HEAT_RED };
export const rgbCss = (c: readonly number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;

// ------------------------------------------------------------------ mock

const scale = (c: Rgb, k: number): Rgb => {
  const q = Math.round(Math.min(1, Math.max(0, k)) * STEPS) / STEPS;
  return c.map((v) => Math.round(v * q)) as Rgb;
};
const heat = (share: number) => (share < 0.6 ? HEAT_GREEN : share < 0.8 ? HEAT_AMBER : HEAT_RED);

function bar(leds: number[], value: number, track: number, color: (share: number) => Rgb): [number, Rgb][] {
  const n = leds.length;
  const fill = (Math.min(100, Math.max(0, value)) / 100) * n;
  return leds.map((led, i) => {
    const lit = Math.min(1, Math.max(0, fill - i));
    return [led, scale(color(n > 1 ? i / (n - 1) : 0), Math.max(lit, track))];
  });
}

/** What the gauges put on the keys (the backend's `layers`, merged). */
function mockFrame(g: GeometryInfo, s: GaugesSettings, r: GaugeReadings, volumeShowing: boolean): LedColor[] {
  const byId = new Map(g.keys.map((k) => [k.id, k.led]));
  const row = (ids: readonly string[]) => ids.flatMap((id) => (byId.has(id) ? [byId.get(id)!] : []));
  const track = s.track ? TRACK : 0;
  const alerts = new Map<number, Rgb>();
  if (s.volume && volumeShowing) {
    if (r.muted) row(GAUGE_ROWS.volume).forEach((l) => alerts.set(l, MUTED_COLOR));
    else if (r.volume != null) bar(row(GAUGE_ROWS.volume), r.volume, track, () => VOLUME_COLOR).forEach(([l, c]) => alerts.set(l, c));
  }
  const micLed = byId.get(s.micKey);
  if (s.mic && r.micMuted && micLed !== undefined) alerts.set(micLed, MIC_COLOR);
  if (!(s.cpu || s.gpu || s.ram) && !alerts.size) return [];
  const out = new Map<number, Rgb>();
  const base: Rgb = s.base === "tint" ? scale(s.tint as Rgb, TINT) : [0, 0, 0];
  g.keys.forEach((k) => out.set(k.led, base));
  for (const key of ["cpu", "gpu", "ram"] as const) {
    if (!s[key]) continue;
    const color = (share: number) => (s.style === "heat" ? heat(share) : OWN_COLORS[key]);
    bar(row(GAUGE_ROWS[key]), r[key] ?? 0, track, color).forEach(([l, c]) => out.set(l, c));
  }
  alerts.forEach((c, l) => out.set(l, c));
  return [...out].map(([led, [r0, g0, b0]]) => ({ led, r: r0, g: g0, b: b0 }));
}

const mock = {
  settings: { ...DEFAULT_GAUGES },
  readings: { cpu: 32, gpu: 14, ram: 61, volume: 40, muted: false, micMuted: false } as GaugeReadings,
  volumeUntil: 0,
  geometry: null as GeometryInfo | null,
  timer: 0,
  emit: null as ((s: GaugesState) => void) | null,
};

function mockState(): GaugesState {
  const showing = Date.now() < mock.volumeUntil;
  return {
    settings: { ...mock.settings },
    readings: { ...mock.readings },
    frame: mock.geometry ? mockFrame(mock.geometry, mock.settings, mock.readings, showing) : [],
    volumeShowing: showing,
  };
}

const walk = (v: number | null, lo: number, hi: number, step: number) =>
  Math.min(hi, Math.max(lo, (v ?? lo) + (Math.random() - 0.5) * step));

function mockTick() {
  const r = mock.readings;
  r.cpu = walk(r.cpu, 3, 97, 14);
  r.gpu = walk(r.gpu, 0, 90, 8);
  r.ram = walk(r.ram, 40, 90, 1);
  mock.emit?.(mockState());
}

if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  const w = window as unknown as Record<string, unknown>;
  /** Browser preview: the knob turned (0–100), or the speakers muted. */
  w.__ksMockVolume = (level: number, muted = false) => {
    Object.assign(mock.readings, { volume: level, muted });
    mock.volumeUntil = Date.now() + VOLUME_SHOW_MS;
    mock.emit?.(mockState());
  };
  /** Browser preview: the microphone muted in Windows. */
  w.__ksMockMic = (muted: boolean) => {
    mock.readings.micMuted = muted;
    mock.emit?.(mockState());
  };
}

// ------------------------------------------------------------------- ipc

export function getGauges(): Promise<GaugesState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockState());
  return invoke<GaugesState>("gauges_state");
}

export function configureGauges(settings: GaugesSettings): Promise<GaugesState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.settings = { ...settings };
    return Promise.resolve(mockState());
  }
  return invoke<GaugesState>("gauges_configure", { settings });
}

// ----------------------------------------------------------------- store

interface GaugesStore {
  state: GaugesState | null;
  load: () => Promise<void>;
  configure: (patch: Partial<GaugesSettings>) => Promise<void>;
}

export const useGauges = create<GaugesStore>((set, get) => ({
  state: null,
  load: async () => set({ state: await getGauges() }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_GAUGES), ...patch };
    // Show the change at once; the backend answers with the same.
    if (s) set({ state: { ...s, settings } });
    set({ state: await configureGauges(settings) });
  },
}));

/**
 * The gauges page is open: the backend reads this PC and sends the state a
 * few times a second, even with the gauges off. `geometry` only feeds the
 * browser preview's own frame. Returns the clean-up for the page's effect.
 */
export function watchGauges(geometry: GeometryInfo | null): () => void {
  if (import.meta.env.DEV && !inTauri) {
    mock.geometry = geometry;
    mock.emit = (s) => useGauges.setState({ state: s });
    mock.timer = window.setInterval(mockTick, 400);
    mock.emit(mockState());
    return () => {
      window.clearInterval(mock.timer);
      mock.emit = null;
    };
  }
  invoke("gauges_watch", { on: true }).catch(console.error);
  const off = listen<GaugesState>("gauges", (e) => useGauges.setState({ state: e.payload }));
  return () => {
    invoke("gauges_watch", { on: false }).catch(console.error);
    off.then((f) => f()).catch(console.error);
  };
}
