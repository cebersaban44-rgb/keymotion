// Live screen (phase C): the live mode's state, its budget, and the sources
// that hand finished cards to the backend. The backend decides when a card
// may go (budget, cable, another upload); the UI only draws cards.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { LivePlan } from "../bindings/LivePlan";
import type { LiveState } from "../bindings/LiveState";
import type { TrackInfo } from "../bindings/TrackInfo";
import { inTauri } from "./ipc";
import { mockLiveUploaded, mockRestored, useScreen } from "./screen";
import { toBlob, type Prepared } from "./screenImport";

// ------------------------------------------------------------------- ipc

const PLAN: Record<LivePlan, { perDay: number; burst: number }> = {
  saver: { perDay: 8, burst: 3 },
  balanced: { perDay: 24, burst: 6 },
  heavy: { perDay: 48, burst: 8 },
};

/** Browser preview without Tauri: the same rules, in memory. */
const mock = {
  state: {
    enabled: false,
    plan: "balanced",
    perDay: 24,
    burst: 6,
    tokens: 6,
    nextTokenMs: null,
    pending: null,
    uploading: null,
    last: null,
    lastError: null,
    nowPlaying: true,
    track: null,
    restore: null,
    held: false,
  } as LiveState,
  /** What the mock GIF page showed before the first live card. */
  before: null as { id: string; name: string; frames: number; pages: number } | null,
  emit: null as ((s: LiveState) => void) | null,
  onTrack: null as ((t: TrackInfo) => void) | null,
  cover: null as Blob | null,
  timer: 0 as number,
};

function mockEmit() {
  mock.emit?.({ ...mock.state });
}

/** The mock Pomodoro holds the screen (browser preview only). */
export function mockLiveHeld(held: boolean) {
  if (inTauri || mock.state.held === held) return;
  mock.state.held = held;
  mockEmit();
}

/** A pretend song, so the flow can be tried in the browser. */
async function mockTrackLoop() {
  if (mock.timer || !mock.state.enabled || !mock.state.nowPlaying) return;
  const c = document.createElement("canvas");
  c.width = c.height = 300;
  const x = c.getContext("2d")!;
  const g = x.createLinearGradient(0, 0, 300, 300);
  g.addColorStop(0, "#ff5e62");
  g.addColorStop(1, "#5b2a86");
  x.fillStyle = g;
  x.fillRect(0, 0, 300, 300);
  x.fillStyle = "rgba(255,255,255,.85)";
  x.beginPath();
  x.arc(150, 150, 70, 0, Math.PI * 2);
  x.fill();
  mock.cover = await new Promise((r) => c.toBlob(r, "image/png"));
  const track: TrackInfo = {
    key: "mock",
    title: "Gece Yarısı Treni",
    artist: "Örnek Sanatçı",
    album: "Deneme",
    app: "Spotify",
    playing: true,
    settled: false,
    hasCover: true,
  };
  mock.state.track = track;
  mockEmit();
  mock.timer = window.setTimeout(() => {
    const settled = { ...track, settled: true };
    mock.state.track = settled;
    mockEmit();
    // Like the backend: no song card while the Pomodoro holds the screen.
    if (!mock.state.held) mock.onTrack?.(settled);
    mock.timer = 0;
  }, 3000);
}

export function getLiveState(): Promise<LiveState> {
  if (!inTauri) return Promise.resolve({ ...mock.state });
  return invoke<LiveState>("live_state");
}

export interface LivePatch {
  enabled?: boolean;
  plan?: LivePlan;
  nowPlaying?: boolean;
  /** With `enabled: false`: put back what the GIF page showed before. */
  restore?: boolean;
}

export async function configureLive(patch: LivePatch): Promise<LiveState> {
  if (!inTauri) {
    const s = mock.state;
    if (patch.enabled === false && patch.restore && s.restore) {
      const back = s.restore;
      s.restore = null;
      s.uploading = { source: "restore", name: back.name };
      window.setTimeout(() => {
        s.uploading = null;
        mock.before = null;
        mockRestored(back.id);
        mockEmit();
      }, 1500);
    }
    if (patch.enabled !== undefined) s.enabled = patch.enabled;
    if (patch.nowPlaying !== undefined) s.nowPlaying = patch.nowPlaying;
    if (patch.plan) {
      s.plan = patch.plan;
      s.perDay = PLAN[patch.plan].perDay;
      s.burst = PLAN[patch.plan].burst;
      s.tokens = Math.min(s.tokens, s.burst);
    }
    if (!s.enabled) {
      s.pending = null;
      s.track = null;
    }
    mockEmit();
    void mockTrackLoop();
    return { ...s };
  }
  return invoke<LiveState>("live_configure", {
    enabled: patch.enabled ?? null,
    plan: patch.plan ?? null,
    nowPlaying: patch.nowPlaying ?? null,
    restore: patch.restore ?? null,
  });
}

/** Hand a finished card to the backend; it waits for budget and cable. */
export async function submitLive(p: Prepared, source: string): Promise<LiveState> {
  const blob = toBlob(p);
  if (!inTauri) {
    const s = mock.state;
    if (!s.enabled) throw new Error("live mode is off");
    const item = { source, name: p.name };
    s.pending = item;
    mockEmit();
    if (s.tokens < 1) {
      s.nextTokenMs = ((1 - s.tokens) / (s.perDay / 24)) * 3_600_000;
      mockEmit();
      return { ...s };
    }
    s.tokens -= 1;
    s.pending = null;
    s.uploading = item;
    mockEmit();
    await new Promise((r) => setTimeout(r, 1200));
    s.uploading = null;
    s.last = { ...item, atMs: Date.now() };
    const prev = mockLiveUploaded(blob);
    if (prev !== undefined) mock.before = prev;
    s.restore = mock.before;
    mockEmit();
    return { ...s };
  }
  return invoke<LiveState>("live_submit", blob, {
    headers: { "x-source": source, "x-name": encodeURIComponent(p.name) },
  });
}

/** Cover art of the announced track, if Windows gave one. */
export async function nowPlayingCover(): Promise<Blob | null> {
  if (!inTauri) return mock.cover;
  try {
    const data = await invoke<ArrayBuffer>("now_playing_cover");
    return new Blob([data]);
  } catch {
    return null;
  }
}

// ----------------------------------------------------------------- store

interface LiveStore {
  state: LiveState | null;
  /** Card drawing failed (shown on the Screen page). */
  drawError: string | null;
  load: () => Promise<void>;
  configure: (patch: LivePatch) => Promise<void>;
}

export const useLive = create<LiveStore>((set) => ({
  state: null,
  drawError: null,
  load: async () => set({ state: await getLiveState() }),
  configure: async (patch) => set({ state: await configureLive(patch) }),
}));

/**
 * Listen to the backend for the whole app session: state changes, and
 * tracks that played long enough, which become "Çalan şarkı" cards.
 * `draw` turns a track into a card (kept out of here so this module has no
 * canvas code). Returns a function that stops listening.
 */
export function startLive(draw: (t: TrackInfo, cover: Blob | null) => Promise<Prepared>): () => void {
  // `undefined` until the first state arrives (that one is not a change).
  let lastShownMs: number | null | undefined;
  const onState = (s: LiveState) => {
    useLive.setState({ state: s });
    // A live upload changed the GIF page and the upload counters.
    const at = s.last?.atMs ?? null;
    if (lastShownMs !== undefined && at !== lastShownMs) {
      const modelId = useScreen.getState().modelId;
      if (modelId) useScreen.getState().load(modelId).catch(console.error);
    }
    lastShownMs = at;
  };
  const onTrack = async (t: TrackInfo) => {
    const s = useLive.getState().state;
    if (!s?.enabled || !s.nowPlaying || !t.settled) return;
    try {
      const cover = t.hasCover ? await nowPlayingCover() : null;
      const card = await draw(t, cover);
      onState(await submitLive(card, "now-playing"));
      useLive.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      useLive.setState({ drawError: String(e) });
    }
  };

  useLive.getState().load().catch(console.error);
  if (!inTauri) {
    mock.emit = onState;
    mock.onTrack = (t) => void onTrack(t);
    return () => {
      mock.emit = null;
      mock.onTrack = null;
    };
  }
  const offs = [
    listen<LiveState>("live-screen", (e) => onState(e.payload)),
    listen<TrackInfo>("now-playing", (e) => void onTrack(e.payload)),
  ];
  return () => offs.forEach((p) => p.then((off) => off()).catch(console.error));
}

/** "38 dk", "2 sa 10 dk". */
export function fmtWait(ms: number, units: { h: string; m: string }) {
  const m = Math.max(1, Math.ceil(ms / 60_000));
  if (m < 60) return `${m} ${units.m}`;
  const h = Math.floor(m / 60);
  const r = m % 60;
  return r ? `${h} ${units.h} ${r} ${units.m}` : `${h} ${units.h}`;
}
