// Live screen (phase C): the live mode's state, its budget, and the sources
// that hand finished cards to the backend. The backend decides when a card
// may go (budget, cable, another upload); the UI only draws cards.

import { create } from "zustand";
import type { LivePlan } from "../bindings/LivePlan";
import type { LiveState } from "../bindings/LiveState";
import type { SongStop } from "../bindings/SongStop";
import type { TrackInfo } from "../bindings/TrackInfo";
import { inTauri, invoke, listen } from "./ipc";
import { mockFactoryAtRisk, mockLiveUploaded, mockRestored, mockShown, useScreen } from "./screen";
import { toBlob, type Prepared } from "./screenImport";
import { useApp } from "./store";

// ------------------------------------------------------------------- ipc

/** Source ids, as the backend knows them. */
export const NOW_PLAYING_SOURCE = "now-playing";

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
    pendingWaitMs: null,
    uploading: null,
    last: null,
    lastError: null,
    nowPlaying: true,
    track: null,
    restore: null,
    held: false,
    holders: [],
    songStop: "back",
    own: null,
    goingBack: null,
    factoryAckNeeded: false,
  } as LiveState,
  /** The user accepted that live cards erase the factory animation. */
  factoryAck: false,
  /** What the mock GIF page showed before the first live card. */
  before: null as { id: string; name: string; frames: number; pages: number } | null,
  emit: null as ((s: LiveState) => void) | null,
  onTrack: null as ((t: TrackInfo) => void) | null,
  cover: null as Blob | null,
  timer: 0 as number,
  backTimer: 0 as number,
  /** The waiting card's data (browser preview). */
  pendingBlob: null as Uint8Array | null,
};

/** In the browser preview with `pomofast`, waits are short too. */
const FAST = import.meta.env.DEV && typeof location !== "undefined" && location.search.includes("pomofast");

function mockLiveShowing() {
  return mockShown() === "live";
}

function mockEmit() {
  const shown = mockShown();
  mock.state.own = shown === "live" ? mock.before : shown;
  mock.state.restore = shown === "live" && !mock.state.uploading ? mock.before : null;
  mock.state.factoryAckNeeded = !mock.factoryAck && mockFactoryAtRisk();
  mock.emit?.({ ...mock.state });
}

/** Put the user's own picture back (browser preview). */
function mockPutBack() {
  const back = mock.before;
  if (!back || !mockLiveShowing()) return;
  mock.state.goingBack = null;
  mock.state.uploading = { source: "restore", name: back.name };
  mockEmit();
  window.setTimeout(() => {
    mock.state.uploading = null;
    mock.before = null;
    mockRestored(back.id);
    mockEmit();
  }, 1500);
}

/** Like `LiveHandle::return_after`: only while this source's card shows. */
export function mockReturnAfter(source: string, ms: number) {
  const s = mock.state;
  if (!import.meta.env.DEV || inTauri || !s.enabled || !mockLiveShowing() || s.last?.source !== source) return;
  window.clearTimeout(mock.backTimer);
  s.goingBack = { source, atMs: Date.now() + ms };
  mock.backTimer = window.setTimeout(() => {
    const b = mock.state.goingBack;
    if (!b || b.source !== source) return;
    mock.state.goingBack = null;
    // Like `back_step`: a waiting card that can go now replaces the screen;
    // one that waits for the budget does not stop the return.
    const ready = !!mock.state.pending && mock.state.tokens >= 1;
    if (mock.state.enabled && !ready && mock.state.last?.source === source) {
      mock.state.tokens -= 1;
      mockPutBack();
    } else mockEmit();
  }, ms);
  mockEmit();
}

export function mockCancelReturn(source: string) {
  if (!import.meta.env.DEV || inTauri || mock.state.goingBack?.source !== source) return;
  window.clearTimeout(mock.backTimer);
  mock.state.goingBack = null;
  mockEmit();
}

/** Browser preview: the pretend song plays or stops (`__ksMockTrack`). */
function mockTrackPlaying(playing: boolean) {
  const t = mock.state.track;
  if (!t || t.playing === playing) return;
  mock.state.track = { ...t, playing };
  if (!playing && mock.state.songStop === "back") mockReturnAfter(NOW_PLAYING_SOURCE, FAST ? 8_000 : 300_000);
  if (playing) mockCancelReturn(NOW_PLAYING_SOURCE);
  mockEmit();
}
if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  (window as unknown as { __ksMockTrack: (p: boolean) => void }).__ksMockTrack = mockTrackPlaying;
}

/** A mock source holds the screen or lets go (browser preview only). */
export function mockLiveHold(source: string, on: boolean) {
  if (!import.meta.env.DEV || inTauri) return;
  const has = mock.state.holders.includes(source);
  if (on === has) return;
  mock.state.holders = on ? [...mock.state.holders, source] : mock.state.holders.filter((h) => h !== source);
  mock.state.held = mock.state.holders.length > 0;
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
  const tr = useApp.getState().locale === "tr";
  const track: TrackInfo = {
    key: "mock",
    title: tr ? "Gece Yarısı Treni" : "Midnight Train",
    artist: tr ? "Örnek Sanatçı" : "Sample Artist",
    album: tr ? "Deneme" : "Demo",
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
  if (import.meta.env.DEV && !inTauri) {
    mock.state.factoryAckNeeded = !mock.factoryAck && mockFactoryAtRisk();
    return Promise.resolve({ ...mock.state });
  }
  return invoke<LiveState>("live_state");
}

/** The user accepted, once, that live cards erase the factory animation
 *  (there is no backup of it). */
async function ackFactory(): Promise<LiveState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.factoryAck = true;
    mock.state.factoryAckNeeded = false;
    return { ...mock.state };
  }
  return invoke<LiveState>("live_ack_factory");
}

export interface LivePatch {
  enabled?: boolean;
  plan?: LivePlan;
  nowPlaying?: boolean;
  /** Put back what the GIF page showed before the live mode (with
   *  `enabled: false`, or on its own while the live mode stays on). */
  restore?: boolean;
  songStop?: SongStop;
}

export async function configureLive(patch: LivePatch): Promise<LiveState> {
  if (import.meta.env.DEV && !inTauri) {
    const s = mock.state;
    if (patch.restore && patch.enabled !== true && s.restore) mockPutBack();
    if (patch.enabled !== undefined) s.enabled = patch.enabled;
    if (patch.nowPlaying !== undefined) s.nowPlaying = patch.nowPlaying;
    if (patch.songStop) {
      s.songStop = patch.songStop;
      if (patch.songStop === "keep") mockCancelReturn(NOW_PLAYING_SOURCE);
    }
    if (patch.plan) {
      s.plan = patch.plan;
      s.perDay = PLAN[patch.plan].perDay;
      s.burst = PLAN[patch.plan].burst;
      s.tokens = Math.min(s.tokens, s.burst);
    }
    if (!s.enabled) {
      s.pending = null;
      s.track = null;
      s.goingBack = null;
      window.clearTimeout(mock.backTimer);
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
    songStop: patch.songStop ?? null,
  });
}

/** Browser preview: the waiting card goes up (budget taken or charged). */
async function mockUploadPending() {
  const s = mock.state;
  const item = s.pending;
  const blob = mock.pendingBlob;
  if (!item || !blob) return { ...s };
  s.tokens -= 1;
  s.nextTokenMs = s.tokens < 1 ? ((1 - s.tokens) / (s.perDay / 24)) * 3_600_000 : null;
  s.pending = null;
  s.pendingWaitMs = null;
  mock.pendingBlob = null;
  s.uploading = item;
  mockEmit();
  await new Promise((r) => setTimeout(r, 1200));
  s.uploading = null;
  s.last = { ...item, atMs: Date.now() };
  const prev = mockLiveUploaded(blob);
  if (prev !== undefined) mock.before = prev;
  mockEmit();
  return { ...s };
}

/** Hand a finished card to the backend; it waits for budget and cable. */
export async function submitLive(p: Prepared, source: string): Promise<LiveState> {
  const blob = toBlob(p);
  if (import.meta.env.DEV && !inTauri) {
    const s = mock.state;
    if (!s.enabled) throw new Error("live mode is off");
    s.pending = { source, name: p.name };
    mock.pendingBlob = blob;
    mockEmit();
    if (s.tokens < 1) {
      s.nextTokenMs = ((1 - s.tokens) / (s.perDay / 24)) * 3_600_000;
      s.pendingWaitMs = s.nextTokenMs;
      mockEmit();
      return { ...s };
    }
    return mockUploadPending();
  }
  return invoke<LiveState>("live_submit", blob, {
    headers: { "x-source": source, "x-name": encodeURIComponent(p.name) },
  });
}

/** Send the waiting card now instead of waiting for the budget (the
 *  user's choice: counted, never refused). */
export async function sendLiveNow(): Promise<LiveState> {
  if (import.meta.env.DEV && !inTauri) {
    void mockUploadPending();
    return { ...mock.state };
  }
  return invoke<LiveState>("live_send_now");
}

/** Do not send the waiting card: the keyboard keeps its current picture. */
export async function dropLiveWaiting(): Promise<LiveState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.state.pending = null;
    mock.state.pendingWaitMs = null;
    mock.pendingBlob = null;
    mockEmit();
    return { ...mock.state };
  }
  return invoke<LiveState>("live_drop_waiting");
}

/** Cover art of the announced track, if Windows gave one. */
export async function nowPlayingCover(): Promise<Blob | null> {
  if (import.meta.env.DEV && !inTauri) return mock.cover;
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
  /** Turning the live mode on waits for the user: the first card would
   *  erase the factory animation, which has no backup (asked once). */
  askingFactory: LivePatch | null;
  load: () => Promise<void>;
  configure: (patch: LivePatch) => Promise<void>;
  /** The answer to `askingFactory`: go on (and never ask again) or not. */
  answerFactory: (go: boolean) => Promise<void>;
  sendNow: () => Promise<void>;
  /** "Gönderme": forget the waiting card. */
  dropWaiting: () => Promise<void>;
}

export const useLive = create<LiveStore>((set, get) => ({
  state: null,
  drawError: null,
  askingFactory: null,
  load: async () => set({ state: await getLiveState() }),
  configure: async (patch) => {
    const s = get().state;
    if (patch.enabled === true && s && !s.enabled && s.factoryAckNeeded) {
      // Asked only if still so (an upload may have happened meanwhile).
      const fresh = await getLiveState();
      if (fresh.factoryAckNeeded) {
        set({ state: fresh, askingFactory: patch });
        return;
      }
    }
    set({ state: await configureLive(patch) });
  },
  answerFactory: async (go) => {
    const patch = get().askingFactory;
    set({ askingFactory: null });
    if (!go || !patch) return;
    // A backup taken from the official driver meanwhile makes the
    // acknowledgment unnecessary.
    const s = await getLiveState();
    if (s.factoryAckNeeded) await ackFactory();
    set({ state: await configureLive(patch) });
  },
  sendNow: async () => set({ state: await sendLiveNow() }),
  dropWaiting: async () => set({ state: await dropLiveWaiting() }),
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
  let wasUploading = false;
  const onState = (s: LiveState) => {
    useLive.setState({ state: s });
    // An upload ended: a live card, or the own picture put back (which
    // leaves `last` as it was). Either changed the GIF page, so the pages
    // read again what the keyboard shows (K8: after a put-back they kept
    // showing the last live card).
    const at = s.last?.atMs ?? null;
    const ended = wasUploading && !s.uploading;
    if ((lastShownMs !== undefined && at !== lastShownMs) || ended) {
      const modelId = useScreen.getState().modelId;
      if (modelId) useScreen.getState().load(modelId).catch(console.error);
    }
    lastShownMs = at;
    wasUploading = !!s.uploading;
  };
  const onTrack = async (t: TrackInfo) => {
    const s = useLive.getState().state;
    if (!s?.enabled || !s.nowPlaying || !t.settled) return;
    try {
      const cover = t.hasCover ? await nowPlayingCover() : null;
      const card = await draw(t, cover);
      onState(await submitLive(card, NOW_PLAYING_SOURCE));
      useLive.setState({ drawError: null });
    } catch (e) {
      console.error(e);
      useLive.setState({ drawError: String(e) });
    }
  };

  useLive.getState().load().catch(console.error);
  if (import.meta.env.DEV && !inTauri) {
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
