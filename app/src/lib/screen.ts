// Screen studio state: the library (stored on disk by the backend), what the
// keyboard's GIF page shows, and a running upload.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { ScreenItem } from "../bindings/ScreenItem";
import type { ScreenProgress } from "../bindings/ScreenProgress";
import type { ScreenState } from "../bindings/ScreenState";
import { inTauri } from "./ipc";
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
};

function mockState(): ScreenState {
  return {
    items: [...mock.items].sort((a, b) => b.createdMs - a.createdMs),
    shown: mock.shown,
    uploadsToday: mock.today,
    uploadsTotal: mock.total,
    limits: MOCK_LIMITS,
  };
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

async function uploadItem(modelId: string, id: string, onProgress: (p: ScreenProgress) => void): Promise<ScreenState> {
  if (!inTauri) {
    const item = mock.items.find((i) => i.id === id);
    const total = item?.pages ?? 16;
    for (let done = 1; done <= total; done += Math.max(1, Math.floor(total / 40))) {
      await new Promise((r) => setTimeout(r, 40));
      onProgress({ id, done, total });
    }
    onProgress({ id, done: total, total });
    mock.shown = id;
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

// ----------------------------------------------------------------- store

export interface Upload {
  id: string;
  name: string;
  done: number;
  total: number;
  startedMs: number;
}

interface ScreenStore {
  modelId: string | null;
  state: ScreenState | null;
  upload: Upload | null;
  /** Resolves when the running upload ends (for the close guard). */
  running: Promise<void> | null;
  load: (modelId: string) => Promise<void>;
  add: (p: Prepared) => Promise<ScreenItem>;
  uploadToKeyboard: (item: ScreenItem) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useScreen = create<ScreenStore>((set, get) => ({
  modelId: null,
  state: null,
  upload: null,
  running: null,
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
  uploadToKeyboard: async (item) => {
    const modelId = get().modelId;
    if (!modelId || get().upload) return;
    let finish!: () => void;
    const running = new Promise<void>((r) => (finish = r));
    set({ upload: { id: item.id, name: item.name, done: 0, total: item.pages, startedMs: Date.now() }, running });
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
