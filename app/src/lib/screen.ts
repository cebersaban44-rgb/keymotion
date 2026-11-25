// Screen studio state: the library (stored on disk by the backend), what the
// keyboard's GIF page shows, and a running upload.

import { create } from "zustand";
import type { ScreenIncomplete } from "../bindings/ScreenIncomplete";
import type { ScreenItem } from "../bindings/ScreenItem";
import type { ScreenItemKind } from "../bindings/ScreenItemKind";
import type { ScreenProgress } from "../bindings/ScreenProgress";
import type { ScreenState } from "../bindings/ScreenState";
import type { ScreenFrameLimits } from "../bindings/ScreenFrameLimits";
import { keymotionAnimation } from "./bootAnimation";
import { flashWrite, inTauri, invoke, listen } from "./ipc";
import { FRAME_PIXELS, HEADER_LEN, MAX_FRAMES, PAGE_MS, PROVEN_FRAMES, pagesFor, toBlob, type Prepared } from "./screenImport";
import { useActiveKeyboard } from "./store";

// ------------------------------------------------------------------- ipc

const MOCK_LIMITS: ScreenState["limits"] = {
  width: 240,
  height: 135,
  maxFrames: 140,
  provenFrames: 135,
  minDelayMs: 2,
  maxDelayMs: 508,
  stillDelayMs: 100,
  pageMs: 145,
  headerLen: 256,
  frameBytes: 64800,
  maxPages: 2215,
  nameMax: 60,
};

const mock = {
  items: [] as ScreenItem[],
  data: new Map<string, Uint8Array>(),
  shown: null as string | null,
  today: 0,
  total: 0,
  incomplete: null as ScreenIncomplete | null,
  /** `?mock=aa769&aa=clock`: the stand-in "AA" keyboard's clock page is at
   *  risk until the user says yes. */
  clockAck: false,
  /** The upload being cancelled (`screen_upload_cancel`). */
  cancel: null as string | null,
  /** `&aa=cut`: the next upload meets the quiet time after a stopped one. */
  cut: import.meta.env.DEV && typeof location !== "undefined" && /[?&]aa=[^&]*\bcut\b/.test(location.search),
};

/** Browser preview: the stand-in keyboard is an "AA" one whose clock page
 *  an upload may replace (`?mock=aa769&aa=clock`). */
export function mockClockAtRisk(): boolean {
  if (!import.meta.env.DEV || typeof location === "undefined") return false;
  const q = new URLSearchParams(location.search);
  const aa = (q.get("aa") ?? "").split(",");
  return q.get("mock") === "aa769" && aa.includes("clock") && !mock.clockAck && mock.shown === null;
}

/** Browser preview: the user said yes to losing the clock page. */
export function mockAckClock() {
  mock.clockAck = true;
}

/** Browser preview: `?mock=usb&driver` pretends AJAZZ's official driver is
 *  installed, so its factory animation can be backed up. */
const MOCK_DRIVER = import.meta.env.DEV && typeof location !== "undefined" && /[?&]driver\b/.test(location.search);

function kindRank(k: ScreenItemKind): number {
  return k === "factory" ? 2 : k === "keymotion" ? 1 : 0;
}

/** Comes with the app or the keyboard: not renamed or removed. */
export function builtIn(k: ScreenItemKind): boolean {
  return k === "factory" || k === "keymotion";
}

function mockState(): ScreenState {
  return {
    // Like the backend: the factory backup, Keymotion's own, then newest first.
    items: [...mock.items].sort((a, b) => kindRank(b.kind) - kindRank(a.kind) || b.createdMs - a.createdMs),
    shown: mock.incomplete ? null : mock.shown,
    uploadsToday: mock.today,
    uploadsTotal: mock.total,
    limits: MOCK_LIMITS,
    incomplete: mock.incomplete && {
      ...mock.incomplete,
      id: mock.items.some((i) => i.id === mock.incomplete?.id) ? mock.incomplete.id : null,
    },
    // A "04 xx" stand-in keyboard's clock page is never at risk.
    clockPageAtRisk: mockClockAtRisk(),
  };
}

/** Browser preview: the last upload stopped partway (`__ksMockIncomplete()`
 *  in the console; an item id, or the newest item). */
function mockCutUpload(id?: string) {
  const item = id ? mock.items.find((i) => i.id === id) : [...mock.items].sort((a, b) => b.createdMs - a.createdMs)[0];
  mock.incomplete = { id: item?.id ?? null, atMs: Date.now() - 3_600_000 };
  const recordId = useScreen.getState().recordId;
  if (recordId) void useScreen.getState().load(recordId);
}
if (import.meta.env.DEV && !inTauri && typeof window !== "undefined") {
  (window as unknown as { __ksMockIncomplete: (id?: string) => void }).__ksMockIncomplete = mockCutUpload;
}

/** Browser preview: a stand-in for the factory animation (a dark blue
 *  screen with a light band), not AJAZZ's. */
function mockFactoryItem(): ScreenItem {
  const id = "fac70e1a0a0a0a0a";
  const frame = new Uint16Array(FRAME_PIXELS).fill(0x10a6);
  frame.fill(0xffff, FRAME_PIXELS / 2 - 240 * 4, FRAME_PIXELS / 2 + 240 * 4);
  mock.data.set(id, toBlob({ frames: [frame], delaysMs: [100], name: "", kind: "image" }));
  return { id, name: "AJAZZ factory animation", kind: "factory", frames: 135, loopMs: 13_500, pages: 2136, createdMs: 0, uploadedMs: null };
}

/** Browser preview: no factory backup and nothing uploaded yet (the live
 *  mode asks before its first card, like `factory_at_risk`). */
export function mockFactoryAtRisk() {
  return !mock.items.some((i) => i.kind === "factory") && mock.shown === null && mock.incomplete === null;
}

/** Browser preview: a live card reached the GIF page. Returns what it
 *  replaced the first time (`undefined` when a live card was there). */
export function mockLiveUploaded(blob: Uint8Array) {
  const prev = mock.shown;
  mock.data.set("live", blob);
  mock.shown = "live";
  mock.incomplete = null;
  mock.today++;
  mock.total++;
  if (prev === "live") return undefined;
  const item = mock.items.find((i) => i.id === prev);
  return item ? { id: item.id, name: item.name, frames: item.frames, pages: item.pages } : null;
}

/** Browser preview: what the mock GIF page shows ("live" for a live card). */
export function mockShown(): "live" | { id: string; name: string; frames: number; pages: number } | null {
  if (mock.shown === "live") return "live";
  const item = mock.items.find((i) => i.id === mock.shown);
  return item ? { id: item.id, name: item.name, frames: item.frames, pages: item.pages } : null;
}

/** Browser preview: a library item was put back on the GIF page. */
export function mockRestored(id: string) {
  mock.shown = id;
  mock.incomplete = null;
  mock.today++;
  mock.total++;
  const recordId = useScreen.getState().recordId;
  if (recordId) void useScreen.getState().load(recordId);
}

export function getScreenState(recordId: string): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(mockState());
  return invoke<ScreenState>("screen_state", { recordId });
}

/** Store a prepared animation in the library (validated again in Rust).
 *  `kind` "keymotion" stores the built-in Keymotion animation. */
export async function addToLibrary(p: Prepared, kind: ScreenItemKind = p.kind): Promise<ScreenItem> {
  const blob = toBlob(p);
  if (import.meta.env.DEV && !inTauri) {
    const id = Math.random().toString(16).slice(2, 18).padEnd(16, "0");
    const loop = p.delaysMs.reduce((a, d) => a + Math.max(2, Math.round(d / 2) * 2), 0);
    const item: ScreenItem = {
      id,
      name: p.name,
      kind,
      frames: p.frames.length,
      loopMs: loop,
      pages: pagesFor(p.frames.length),
      createdMs: Date.now(),
      uploadedMs: null,
    };
    mock.items.push(item);
    mock.data.set(id, blob);
    return item;
  }
  return invoke<ScreenItem>("screen_add", blob, {
    headers: { "x-name": encodeURIComponent(p.name), "x-kind": kind },
  });
}

/** Stored data of an item (header + frames), at most `frames` frames. */
export async function itemData(id: string, frames?: number): Promise<ArrayBuffer> {
  if (import.meta.env.DEV && !inTauri) {
    const d = mock.data.get(id) ?? new Uint8Array(HEADER_LEN + FRAME_PIXELS * 2);
    const n = frames ? Math.min(d.length, HEADER_LEN + frames * FRAME_PIXELS * 2) : d.length;
    return d.slice(0, n).buffer;
  }
  return invoke<ArrayBuffer>("screen_data", { id, frames: frames ?? null });
}

export async function renameItem(recordId: string, id: string, name: string): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) {
    const it = mock.items.find((i) => i.id === id);
    if (it) it.name = name;
    return mockState();
  }
  return invoke<ScreenState>("screen_rename", { recordId, id, name });
}

export async function removeItem(recordId: string, id: string): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.items = mock.items.filter((i) => i.id !== id);
    return mockState();
  }
  return invoke<ScreenState>("screen_remove", { recordId, id });
}

/** Upload to the GIF page (a write to the keyboard's memory). */
function uploadItem(recordId: string, id: string, onProgress: (p: ScreenProgress) => void): Promise<ScreenState> {
  return flashWrite(() => uploadOnce(recordId, id, onProgress));
}

async function uploadOnce(recordId: string, id: string, onProgress: (p: ScreenProgress) => void): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) {
    if (mockClockAtRisk()) throw new Error("screen upload would replace the keyboard's clock page; not acknowledged");
    if (mock.cut) {
      mock.cut = false;
      throw new Error("a screen upload stopped part-way; no commands for another 3400 ms");
    }
    const item = mock.items.find((i) => i.id === id);
    const total = item?.pages ?? 16;
    // Slower for an "AA" stand-in, so its Cancel can be tried.
    const step = /[?&]mock=aa769/.test(location.search) ? 120 : 40;
    for (let done = 1; done <= total; done += Math.max(1, Math.floor(total / 40))) {
      await new Promise((r) => setTimeout(r, step));
      if (mock.cancel === id) {
        mock.cancel = null;
        mock.incomplete = { id, atMs: Date.now() };
        throw new Error(`screen upload cancelled after ${done} of ${total} parts`);
      }
      onProgress({ id, done, total });
    }
    onProgress({ id, done: total, total });
    mock.shown = id;
    mock.incomplete = null;
    mock.today++;
    mock.total++;
    if (item) item.uploadedMs = Date.now();
    return mockState();
  }
  const off = await listen<ScreenProgress>("screen-upload-progress", (e) => {
    if (e.payload.id === id) onProgress(e.payload);
  });
  try {
    return await invoke<ScreenState>("screen_upload", { recordId, id });
  } finally {
    off();
  }
}

/** The user said yes to losing the keyboard's clock page with its next
 *  upload (`ScreenState.clockPageAtRisk`: the first upload to a keyboard of
 *  AJAZZ's "AA" family; never a "04 xx" one). It also lets a live card that
 *  waited for the answer go (`LiveState.clockAckNeeded`). */
export async function ackClockPage(recordId: string): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) {
    mockAckClock();
    return mockState();
  }
  return invoke<ScreenState>("screen_ack_clock_page", { recordId });
}

/** Stop the upload of library item `id` (an "AA" keyboard's; a "04 xx"
 *  upload can't be stopped). Its `uploadToKeyboard` then fails with "screen
 *  upload cancelled after N of M parts" ("… before it started" when it was
 *  still waiting its turn). False when no such upload runs. */
export async function cancelUpload(id: string): Promise<boolean> {
  if (import.meta.env.DEV && !inTauri) {
    mock.cancel = id;
    return true;
  }
  return invoke<boolean>("screen_upload_cancel", { id });
}

/** The "04 xx" family's screen limits, while no keyboard says otherwise. */
const FF13_SCREEN: ScreenFrameLimits = { maxFrames: MAX_FRAMES, provenFrames: PROVEN_FRAMES, pageMs: PAGE_MS };

/** What an upload may hold on the keyboard the UI shows
 *  (`KeyboardInfo.limits.screen`): frames it takes, frames proven, time
 *  per page. */
export function useScreenLimits(): ScreenFrameLimits {
  return useActiveKeyboard()?.limits.screen ?? FF13_SCREEN;
}

/** Whether AJAZZ's official driver left its copy of the factory animation
 *  on this PC (the backend searches once per run, off the UI thread). */
async function findDriverBackup(): Promise<boolean> {
  if (import.meta.env.DEV && !inTauri) return MOCK_DRIVER;
  return invoke<boolean>("screen_find_driver_backup");
}

/** Keep the official driver's copy as the factory backup. */
async function backupFromDriver(recordId: string): Promise<ScreenState> {
  if (import.meta.env.DEV && !inTauri) {
    await new Promise((r) => setTimeout(r, 600));
    if (!mock.items.some((i) => i.kind === "factory")) mock.items.push(mockFactoryItem());
    return mockState();
  }
  return invoke<ScreenState>("screen_backup_from_driver", { recordId });
}

// ----------------------------------------------------------------- store

export interface Upload {
  id: string;
  /** Where it was started: the studio shows its own progress. */
  origin: "editor" | "library";
  name: string;
  done: number;
  total: number;
  startedMs: number;
}

/** The official driver's copy of the factory animation on this PC. */
export type DriverBackup = "unknown" | "searching" | "found" | "none";

interface ScreenStore {
  recordId: string | null;
  state: ScreenState | null;
  upload: Upload | null;
  /** Resolves when the running upload ends (for the close guard). */
  running: Promise<void> | null;
  driver: DriverBackup;
  /** Look for the official driver's copy (once per run). */
  findDriver: () => void;
  /** Keep the official driver's copy as the factory backup. */
  backupFromDriver: () => Promise<void>;
  load: (recordId: string) => Promise<void>;
  /** The user said yes to losing the clock page (`clockPageAtRisk`). */
  ackClockPage: () => Promise<void>;
  add: (p: Prepared) => Promise<ScreenItem>;
  uploadToKeyboard: (item: ScreenItem, origin: Upload["origin"]) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

let keymotionAdding = false;

/** The built-in Keymotion animation is drawn and stored once: a new
 *  library, or one from before it existed. Nothing goes to the keyboard. */
async function addKeymotion() {
  if (keymotionAdding) return;
  keymotionAdding = true;
  try {
    await addToLibrary(await keymotionAnimation(), "keymotion");
    const recordId = useScreen.getState().recordId;
    if (recordId) useScreen.setState({ state: await getScreenState(recordId) });
  } catch (e) {
    console.error(e);
    // Tried again on the next load.
    keymotionAdding = false;
  }
}

export const useScreen = create<ScreenStore>((set, get) => ({
  recordId: null,
  state: null,
  upload: null,
  running: null,
  driver: "unknown",
  findDriver: () => {
    if (get().driver !== "unknown") return;
    set({ driver: "searching" });
    findDriverBackup().then(
      (found) => set({ driver: found ? "found" : "none" }),
      (e) => {
        console.error(e);
        set({ driver: "none" });
      },
    );
  },
  backupFromDriver: async () => {
    const recordId = get().recordId;
    if (!recordId) return;
    set({ state: await backupFromDriver(recordId) });
  },
  load: async (recordId) => {
    const state = await getScreenState(recordId);
    set({ recordId, state });
    if (!state.items.some((i) => i.kind === "keymotion")) void addKeymotion();
  },
  ackClockPage: async () => {
    const recordId = get().recordId;
    if (recordId) set({ state: await ackClockPage(recordId) });
  },
  add: async (p) => {
    const item = await addToLibrary(p);
    const recordId = get().recordId;
    if (recordId) set({ state: await getScreenState(recordId) });
    return item;
  },
  uploadToKeyboard: async (item, origin) => {
    const recordId = get().recordId;
    if (!recordId || get().upload) return;
    let finish!: () => void;
    const running = new Promise<void>((r) => (finish = r));
    set({ upload: { id: item.id, origin, name: item.name, done: 0, total: item.pages, startedMs: Date.now() }, running });
    try {
      const state = await uploadItem(recordId, item.id, (p) =>
        set((s) => (s.upload ? { upload: { ...s.upload, done: p.done, total: p.total } } : {})),
      );
      set({ state });
    } catch (e) {
      // The count of uploads may have changed even when it failed.
      getScreenState(recordId).then((state) => set({ state }), console.error);
      throw e;
    } finally {
      set({ upload: null, running: null });
      finish();
    }
  },
  rename: async (id, name) => {
    const recordId = get().recordId;
    if (recordId) set({ state: await renameItem(recordId, id, name) });
  },
  remove: async (id) => {
    const recordId = get().recordId;
    if (recordId) set({ state: await removeItem(recordId, id) });
  },
}));
