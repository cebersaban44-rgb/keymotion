// Screen studio state: the library (stored on disk by the backend), what the
// keyboard's GIF page shows, and a running upload.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { ScreenIncomplete } from "../bindings/ScreenIncomplete";
import type { ScreenItem } from "../bindings/ScreenItem";
import type { ScreenProgress } from "../bindings/ScreenProgress";
import type { ScreenState } from "../bindings/ScreenState";
import { flashWrite, inTauri } from "./ipc";
import { FRAME_PIXELS, HEADER_LEN, pagesFor, toBlob, type Prepared } from "./screenImport";

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
};

/** Browser preview: `?mock=usb&driver` pretends AJAZZ's official driver is
 *  installed, so its factory animation can be backed up. */
const MOCK_DRIVER = typeof location !== "undefined" && /[?&]driver\b/.test(location.search);

function mockState(): ScreenState {
  return {
    // Like the backend: the factory backup first, then newest first.
    items: [...mock.items].sort(
      (a, b) => Number(b.kind === "factory") - Number(a.kind === "factory") || b.createdMs - a.createdMs,
    ),
    shown: mock.incomplete ? null : mock.shown,
    uploadsToday: mock.today,
    uploadsTotal: mock.total,
    limits: MOCK_LIMITS,
    incomplete: mock.incomplete && {
      ...mock.incomplete,
      id: mock.items.some((i) => i.id === mock.incomplete?.id) ? mock.incomplete.id : null,
    },
  };
}

/** Browser preview: the last upload stopped partway (`__ksMockIncomplete()`
 *  in the console; an item id, or the newest item). */
function mockCutUpload(id?: string) {
  const item = id ? mock.items.find((i) => i.id === id) : [...mock.items].sort((a, b) => b.createdMs - a.createdMs)[0];
  mock.incomplete = { id: item?.id ?? null, atMs: Date.now() - 3_600_000 };
  const modelId = useScreen.getState().modelId;
  if (modelId) void useScreen.getState().load(modelId);
}
if (!inTauri && typeof window !== "undefined") {
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
  const modelId = useScreen.getState().modelId;
  if (modelId) void useScreen.getState().load(modelId);
}

export function getScreenState(modelId: string): Promise<ScreenState> {
  if (!inTauri) return Promise.resolve(mockState());
  return invoke<ScreenState>("screen_state", { modelId });
}

/** Store a prepared animation in the library (validated again in Rust). */
export async function addToLibrary(p: Prepared): Promise<ScreenItem> {
  const blob = toBlob(p);
  if (!inTauri) {
    const id = Math.random().toString(16).slice(2, 18).padEnd(16, "0");
    const loop = p.delaysMs.reduce((a, d) => a + Math.max(2, Math.round(d / 2) * 2), 0);
    const item: ScreenItem = {
      id,
      name: p.name,
      kind: p.kind,
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
    headers: { "x-name": encodeURIComponent(p.name), "x-kind": p.kind },
  });
}

/** Stored data of an item (header + frames), at most `frames` frames. */
export async function itemData(id: string, frames?: number): Promise<ArrayBuffer> {
  if (!inTauri) {
    const d = mock.data.get(id) ?? new Uint8Array(HEADER_LEN + FRAME_PIXELS * 2);
    const n = frames ? Math.min(d.length, HEADER_LEN + frames * FRAME_PIXELS * 2) : d.length;
    return d.slice(0, n).buffer;
  }
  return invoke<ArrayBuffer>("screen_data", { id, frames: frames ?? null });
}

export async function renameItem(modelId: string, id: string, name: string): Promise<ScreenState> {
  if (!inTauri) {
    const it = mock.items.find((i) => i.id === id);
    if (it) it.name = name;
    return mockState();
  }
  return invoke<ScreenState>("screen_rename", { modelId, id, name });
}

export async function removeItem(modelId: string, id: string): Promise<ScreenState> {
  if (!inTauri) {
    mock.items = mock.items.filter((i) => i.id !== id);
    return mockState();
  }
  return invoke<ScreenState>("screen_remove", { modelId, id });
}

/** Upload to the GIF page (a write to the keyboard's memory). */
function uploadItem(modelId: string, id: string, onProgress: (p: ScreenProgress) => void): Promise<ScreenState> {
  return flashWrite(() => uploadOnce(modelId, id, onProgress));
}

async function uploadOnce(modelId: string, id: string, onProgress: (p: ScreenProgress) => void): Promise<ScreenState> {
  if (!inTauri) {
    const item = mock.items.find((i) => i.id === id);
    const total = item?.pages ?? 16;
    for (let done = 1; done <= total; done += Math.max(1, Math.floor(total / 40))) {
      await new Promise((r) => setTimeout(r, 40));
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
    return await invoke<ScreenState>("screen_upload", { modelId, id });
  } finally {
    off();
  }
}

/** Whether AJAZZ's official driver left its copy of the factory animation
 *  on this PC (the backend searches once per run, off the UI thread). */
async function findDriverBackup(): Promise<boolean> {
  if (!inTauri) return MOCK_DRIVER;
  return invoke<boolean>("screen_find_driver_backup");
}

/** Keep the official driver's copy as the factory backup. */
async function backupFromDriver(modelId: string): Promise<ScreenState> {
  if (!inTauri) {
    await new Promise((r) => setTimeout(r, 600));
    if (!mock.items.some((i) => i.kind === "factory")) mock.items.push(mockFactoryItem());
    return mockState();
  }
  return invoke<ScreenState>("screen_backup_from_driver", { modelId });
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
  modelId: string | null;
  state: ScreenState | null;
  upload: Upload | null;
  /** Resolves when the running upload ends (for the close guard). */
  running: Promise<void> | null;
  driver: DriverBackup;
  /** Look for the official driver's copy (once per run). */
  findDriver: () => void;
  /** Keep the official driver's copy as the factory backup. */
  backupFromDriver: () => Promise<void>;
  load: (modelId: string) => Promise<void>;
  add: (p: Prepared) => Promise<ScreenItem>;
  uploadToKeyboard: (item: ScreenItem, origin: Upload["origin"]) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useScreen = create<ScreenStore>((set, get) => ({
  modelId: null,
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
    const modelId = get().modelId;
    if (!modelId) return;
    set({ state: await backupFromDriver(modelId) });
  },
  load: async (modelId) => {
    const state = await getScreenState(modelId);
    set({ modelId, state });
  },
  add: async (p) => {
    const item = await addToLibrary(p);
    const modelId = get().modelId;
    if (modelId) set({ state: await getScreenState(modelId) });
    return item;
  },
  uploadToKeyboard: async (item, origin) => {
    const modelId = get().modelId;
    if (!modelId || get().upload) return;
    let finish!: () => void;
    const running = new Promise<void>((r) => (finish = r));
    set({ upload: { id: item.id, origin, name: item.name, done: 0, total: item.pages, startedMs: Date.now() }, running });
    try {
      const state = await uploadItem(modelId, item.id, (p) =>
        set((s) => (s.upload ? { upload: { ...s.upload, done: p.done, total: p.total } } : {})),
      );
      set({ state });
    } catch (e) {
      // The count of uploads may have changed even when it failed.
      getScreenState(modelId).then((state) => set({ state }), console.error);
      throw e;
    } finally {
      set({ upload: null, running: null });
      finish();
    }
  },
  rename: async (id, name) => {
    const modelId = get().modelId;
    if (modelId) set({ state: await renameItem(modelId, id, name) });
  },
  remove: async (id) => {
    const modelId = get().modelId;
    if (modelId) set({ state: await removeItem(modelId, id) });
  },
}));
