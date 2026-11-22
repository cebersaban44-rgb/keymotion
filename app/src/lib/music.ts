// Music mode ("Müzik modu"): the key lights move with the sound the PC
// plays. The sound, the frames and the key lights live in the backend
// (app/src-tauri/src/music.rs); the page changes the settings and shows the
// frame the backend works out. The browser preview has no sound to read: it
// makes up a 120 BPM beat and draws the four modes with a small copy of the
// backend's rules (music.rs `frame`): keep the two in step. The made-up
// sound has no randomness, so the same moment always looks the same
// (screenshots step to a chosen frame with `__ksMockMusicStep`).

import { create } from "zustand";
import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { LedColor } from "../bindings/LedColor";
import type { MusicSettings } from "../bindings/MusicSettings";
import type { MusicState } from "../bindings/MusicState";
import { inTauri, invoke, listen } from "./ipc";

export const DEFAULT_MUSIC: MusicSettings = {
  enabled: false,
  mode: "bars",
  palette: "heat",
  color: [0, 200, 255],
  color2: [255, 45, 149],
  center: false,
  sensitivity: 100,
  rest: "dark",
  silence: "saved",
  pauseInGames: true,
  brightness: 4,
};

export const SENSITIVITY_MIN = 50;
export const SENSITIVITY_MAX = 200;

type Rgb = [number, number, number];

// ------------------------------------------------------------------ mock

const COLUMNS = 20;
const REF_WIDTH = 20.5;
const QUIET_AFTER_MS = 3_000;

function hsv(h: number, s: number, v: number): Rgb {
  const x = (((h % 360) + 360) % 360) / 60;
  const i = Math.floor(x);
  const f = x - i;
  const p = v * (1 - s);
  const q = v * (1 - s * f);
  const u = v * (1 - s * (1 - f));
  const c = [
    [v, u, p],
    [q, v, p],
    [p, v, u],
    [p, q, v],
    [u, p, v],
    [v, p, q],
  ][i % 6]!;
  return c.map((n) => Math.round(Math.min(1, Math.max(0, n)) * 255)) as Rgb;
}

const lerp = (a: readonly number[], b: readonly number[], f: number): Rgb =>
  a.map((c, i) => Math.round(c + (b[i]! - c) * Math.min(1, Math.max(0, f)))) as Rgb;

function palette(s: MusicSettings, f: number): Rgb {
  const v = Math.min(1, Math.max(0, f));
  if (s.palette === "heat") return hsv(120 - v * 120, 1, 1);
  if (s.palette === "rainbow") return hsv(v * 300, 1, 1);
  if (s.palette === "one") return [...s.color] as Rgb;
  return lerp(s.color, s.color2, v);
}

const paint = (c: Rgb, a: number): Rgb => {
  const k = Math.pow(Math.min(1, Math.max(0, a)), 0.6);
  return c.map((v) => Math.round(v * k)) as Rgb;
};

interface MockKey {
  led: number;
  cx: number;
  cy: number;
  row: number;
  col: number;
}

/** The model's keys as the modes see them (music.rs `Keys::of`). */
function mockKeys(g: GeometryInfo) {
  const starts: number[] = [];
  for (const t of g.keys.map((k) => k.y).sort((a, b) => a - b)) {
    if (!starts.length || t - starts[starts.length - 1]! > 0.5) starts.push(t);
  }
  const rows = Math.max(1, starts.length);
  const keys: MockKey[] = g.keys.map((k) => {
    let top = 0;
    starts.forEach((s, i) => {
      if (k.y >= s - 1e-3) top = i;
    });
    const cx = k.x + k.w / 2;
    return { led: k.led, cx, cy: k.y + k.h / 2, row: rows - 1 - top, col: Math.min(COLUMNS - 1, Math.floor((cx / g.width) * COLUMNS)) };
  });
  return { keys, width: g.width, height: g.height, rows };
}

const at = (values: number[], p: number) => {
  const f = Math.min(1, Math.max(0, p)) * (values.length - 1);
  const i = Math.floor(f);
  return values[i]! * (1 - (f - i)) + (values[Math.min(values.length - 1, i + 1)] ?? 0) * (f - i);
};

const mock = {
  settings: { ...DEFAULT_MUSIC },
  geometry: null as GeometryInfo | null,
  timer: 0,
  emit: null as ((s: MusicState) => void) | null,
  /** Stepped by hand (`__ksMockMusicStep`): the clock stands still. */
  held: false,
  playing: true,
  quietSince: 0,
  showing: true,
  t: 0,
  bands: new Array(COLUMNS).fill(0) as number[],
  peaks: new Array(COLUMNS).fill(0) as number[],
  peakAt: new Array(COLUMNS).fill(0) as number[],
  loud: 0,
  loudPeak: 0,
  loudPeakAt: 0,
  beats: 0,
  beat: -9,
  strong: -9,
  waves: [] as { t0: number; hue: number }[],
  lastBeat: -1,
};

/** A made-up song: kick, snare, hi-hat and a moving melody, 120 BPM. */
function mockSynth() {
  const m = mock;
  const sens = m.settings.sensitivity / 100;
  const bi = Math.floor(m.t / 0.5);
  const ph = m.t - bi * 0.5;
  if (m.playing && bi !== m.lastBeat) {
    m.lastBeat = bi;
    m.beats++;
    m.beat = m.t;
    if (m.beats % 4 === 1) m.strong = m.t;
    m.waves.push({ t0: m.t, hue: (m.beats * 47) % 360 });
    if (m.waves.length > 6) m.waves.shift();
  }
  const kick = Math.exp(-ph / 0.09);
  const snare = bi % 2 ? Math.exp(-ph / 0.13) : 0;
  const hat = Math.exp(-(m.t % 0.25) / 0.035);
  const sec = 0.75 + 0.25 * Math.sin((m.t * Math.PI) / 8);
  let sum = 0;
  for (let b = 0; b < COLUMNS; b++) {
    let raw = 0;
    if (m.playing) {
      const kw = Math.max(0, 1 - b / 4);
      const sw = Math.exp(-((b - 8) ** 2) / 12.5);
      const hw = b >= 12 ? (b - 12) / 7 : 0;
      const mel = b >= 3 && b <= 14 ? 0.28 + 0.22 * Math.sin(m.t * 2.1 + b * 0.9) * Math.sin(m.t * 0.6 + b * 0.33) : 0.05;
      // A little slow shimmer instead of noise: no flicker between frames.
      const shimmer = 0.02 * (1 + Math.sin(m.t * 7.3 + b * 1.7));
      raw = (kick * kw + snare * sw * 0.75 + hat * hw * 0.55 + mel * 0.6 + shimmer) * sec * sens;
    }
    const v = m.bands[b]!;
    m.bands[b] = Math.max(0, Math.min(1, v + (raw > v ? (raw - v) * 0.7 : (raw - v) * 0.25)));
    sum += m.bands[b]!;
    if (m.bands[b]! >= m.peaks[b]!) {
      m.peaks[b] = m.bands[b]!;
      m.peakAt[b] = m.t;
    } else if (m.t - m.peakAt[b]! > 0.4) m.peaks[b] = Math.max(m.bands[b]!, m.peaks[b]! - 0.0625);
  }
  const l = Math.min(1, (sum / COLUMNS) * 1.9);
  m.loud += (l - m.loud) * (l > m.loud ? 0.6 : 0.2);
  if (m.loud >= m.loudPeak) {
    m.loudPeak = m.loud;
    m.loudPeakAt = m.t;
  } else if (m.t - m.loudPeakAt > 0.5) m.loudPeak = Math.max(m.loud, m.loudPeak - 0.0235);
}

/** What the keys show (music.rs `frame`), on the preview's made-up sound. */
function mockFrame(g: GeometryInfo, s: MusicSettings): LedColor[] {
  const m = mock;
  const k = mockKeys(g);
  const u = k.width / REF_WIDTH;
  const midX = k.width / 2;
  const midY = k.height / 2;
  const rest = s.rest === "dim" ? 0.06 : 0;
  const top = Math.max(1, k.rows - 1);
  const out = (led: number, c: Rgb): LedColor => ({ led, r: c[0], g: c[1], b: c[2] });
  if (!m.showing) {
    if (s.silence === "saved") return [];
    const c = s.silence === "dim" ? paint(palette(s, 0.2), 0.07) : ([0, 0, 0] as Rgb);
    return k.keys.map((x) => out(x.led, c));
  }
  if (s.mode === "bars") {
    const cols = Array.from({ length: COLUMNS }, (_, c) => {
      const p = s.center ? Math.abs(2 * c + 1 - COLUMNS) / COLUMNS : (2 * c + 1) / (2 * COLUMNS);
      return [Math.min(1, at(m.bands, p) * 1.15), Math.min(1, at(m.peaks, p) * 1.15)] as const;
    });
    return k.keys.map((x) => {
      const [v, peak] = cols[x.col]!;
      const fill = Math.min(1, Math.max(0, v * k.rows - x.row));
      const c = palette(s, s.palette === "rainbow" ? x.col / (COLUMNS - 1) : x.row / top);
      const mark = Math.min(k.rows - 1, Math.floor(peak * k.rows));
      return out(x.led, paint(c, fill > 0 ? Math.max(fill, rest) : x.row === mark && peak > 0.08 ? 0.8 : rest));
    });
  }
  if (s.mode === "beat") {
    const pulse = Math.exp(-(m.t - m.beat) / 0.25);
    const base = 0.12 + 0.3 * m.loud;
    const age = m.t - m.strong;
    return k.keys.map((x) => {
      const d = Math.hypot(x.cx - midX, (x.cy - midY) * 1.3);
      const ring = Math.exp(-(((d - age * 22 * u) / u) ** 2) / 1.6) * Math.exp(-age / 0.6);
      const c =
        s.palette === "rainbow"
          ? hsv(m.beats * 40, 1, 1)
          : s.palette === "heat"
            ? hsv(120 - m.loud * 120, 1, 1)
            : s.palette === "two"
              ? lerp(s.color, s.color2, Math.min(1, ring))
              : ([...s.color] as Rgb);
      return out(x.led, paint(c, base + 0.85 * pulse + ring * 0.8));
    });
  }
  if (s.mode === "level") {
    const reach = midX + 0.35 * u;
    const half = m.loud * reach;
    const mark = m.loudPeak * reach;
    return k.keys.map((x) => {
      const d = Math.abs(x.cx - midX);
      const fill = Math.min(1, Math.max(0, (half - d) / u + 0.5));
      const c = palette(s, d / midX);
      return out(x.led, paint(c, fill > 0 ? Math.max(fill, rest) : Math.abs(d - mark) < 0.55 * u && mark > 0.6 * u ? 0.85 : rest));
    });
  }
  return k.keys.map((x) => {
    const pos = s.center ? Math.abs(x.cx - midX) : x.cx;
    let light = 0;
    let hue = 0;
    for (const w of m.waves) {
      const age = m.t - w.t0;
      const front = age * (s.center ? 13 : 24) * u;
      const i = Math.exp(-(((pos - front) / u) ** 2) / 2.9) * Math.exp(-age / 1.2);
      if (i > light) [light, hue] = [i, w.hue];
    }
    const c = s.palette === "rainbow" ? hsv(hue, 1, 1) : palette(s, s.center ? Math.abs(x.cx - midX) / midX : x.cx / k.width);
    return out(x.led, paint(c, Math.max(Math.min(1, light * (0.5 + m.loud)), rest)));
  });
}

function mockState(): MusicState {
  const s = mock.settings;
  const frame = mock.geometry ? mockFrame(mock.geometry, s) : [];
  return {
    settings: { ...s },
    status: !s.enabled ? "off" : mock.showing ? "playing" : "waiting",
    covers: s.enabled && frame.length > 0,
    frame,
  };
}

/** One frame (50 ms) of the made-up sound. */
function mockAdvance() {
  mock.t += 0.05;
  mockSynth();
  if (mock.playing) mock.showing = true;
  else if (Date.now() - mock.quietSince >= QUIET_AFTER_MS) mock.showing = false;
}

function mockTick() {
  if (mock.held) return;
  mockAdvance();
  mock.emit?.(mockState());
}

/** Back to the first moment of the made-up song. */
function mockRestart() {
  Object.assign(mock, {
    t: 0,
    loud: 0,
    loudPeak: 0,
    loudPeakAt: 0,
    beats: 0,
    beat: -9,
    strong: -9,
    waves: [],
    lastBeat: -1,
    playing: true,
    showing: true,
  });
  mock.bands.fill(0);
  mock.peaks.fill(0);
  mock.peakAt.fill(0);
}

if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  const w = window as unknown as Record<string, unknown>;
  /** Browser preview: the made-up sound plays (`true`) or stops. */
  w.__ksMockMusic = (playing: boolean) => {
    if (mock.playing && !playing) mock.quietSince = Date.now();
    mock.playing = playing;
  };
  /** Browser preview: stop the clock and move the made-up song on `frames`
   *  frames of 50 ms (from its start with `fromStart`), so the same moment
   *  shows every time. `__ksMockMusicStep(-1)` lets the clock run again. */
  w.__ksMockMusicStep = (frames: number, fromStart = false) => {
    mock.held = frames >= 0;
    if (fromStart) mockRestart();
    for (let i = 0; i < frames; i++) mockAdvance();
    mock.emit?.(mockState());
  };
}

// ------------------------------------------------------------------- ipc

export function getMusic(): Promise<MusicState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockState());
  return invoke<MusicState>("music_state");
}

export function configureMusic(settings: MusicSettings): Promise<MusicState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.settings = { ...settings };
    return Promise.resolve(mockState());
  }
  return invoke<MusicState>("music_configure", { settings });
}

// ----------------------------------------------------------------- store

interface MusicStore {
  state: MusicState | null;
  /** Changes sent and not answered yet. */
  pending: number;
  load: () => Promise<void>;
  configure: (patch: Partial<MusicSettings>) => Promise<void>;
}

export const useMusic = create<MusicStore>((set, get) => ({
  state: null,
  pending: 0,
  load: async () => set({ state: await getMusic() }),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_MUSIC), ...patch };
    // Show the change at once; the backend answers with the same.
    set((x) => ({ pending: x.pending + 1, state: s ? { ...s, settings } : x.state }));
    try {
      const state = await configureMusic(settings);
      set({ state });
    } finally {
      set((x) => ({ pending: x.pending - 1 }));
    }
  },
}));

/** A state the backend sent on its own: while a change of ours is on its
 *  way, its settings may be older than what the page shows, so only the
 *  status and the frame are taken. */
function received(next: MusicState) {
  useMusic.setState((x) => ({ state: x.pending > 0 && x.state ? { ...next, settings: x.state.settings } : next }));
}

/** Follow changes made elsewhere (the tray menu's switch, a status change). */
export function followMusic(): () => void {
  if (import.meta.env.DEV && !inTauri) return () => {};
  const off = listen<MusicState>("music", (e) => received(e.payload));
  return () => {
    off.then((f) => f()).catch(console.error);
  };
}

/**
 * The music page is open: the backend listens and sends the state about 12
 * times a second, even with music mode off. `geometry` only feeds the
 * browser preview's own frame. Returns the clean-up for the page's effect.
 */
export function watchMusic(geometry: GeometryInfo | null): () => void {
  if (import.meta.env.DEV && !inTauri) {
    mock.geometry = geometry;
    mock.emit = (s) => useMusic.setState({ state: s });
    mock.timer = window.setInterval(mockTick, 50);
    mock.emit(mockState());
    return () => {
      window.clearInterval(mock.timer);
      mock.emit = null;
    };
  }
  invoke("music_watch", { on: true }).catch(console.error);
  const off = listen<MusicState>("music", (e) => received(e.payload));
  return () => {
    invoke("music_watch", { on: false }).catch(console.error);
    off.then((f) => f()).catch(console.error);
  };
}
