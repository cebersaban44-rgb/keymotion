import { create } from "zustand";
import type { KeyActionInfo } from "../bindings/KeyActionInfo";
import type { KeyAssignment } from "../bindings/KeyAssignment";
import type { KeymapLayer } from "../bindings/KeymapLayer";
import type { KeymapState } from "../bindings/KeymapState";
import { applyKeymap, getKeymapState } from "./ipc";

/** led → action; keys not listed use the firmware default. */
export type LayerMap = Record<number, KeyActionInfo>;
export type Layers = Record<KeymapLayer, LayerMap>;

export const LAYERS: readonly KeymapLayer[] = ["base", "fn"];
/** HID usage of the Fn key; it is never reassigned. */
export const FN_USAGE = 0xaf;

const EMPTY: Layers = { base: {}, fn: {} };

export interface KeymapStore {
  /** Model the state below belongs to. */
  modelId: string | null;
  /**
   * What the app last wrote to the keyboard (its own record: the keyboard
   * cannot report its keymap). `null` until loaded.
   */
  saved: Layers | null;
  writtenMs: Record<KeymapLayer, number | null>;
  /** Edits being made; equals `saved` when nothing is pending. */
  draft: Layers;
  layer: KeymapLayer;
  selected: number | null;
  /** Load the record for `modelId`, keeping an unsaved draft for the same model. */
  load: (modelId: string) => Promise<void>;
  setLayer: (layer: KeymapLayer) => void;
  select: (led: number | null) => void;
  /** Assign `action` in `layer` (default: the current one); `default` removes the entry. */
  assign: (led: number, action: KeyActionInfo, layer?: KeymapLayer) => void;
  /** Every key of `layer` back to the firmware default (in the draft). */
  resetLayer: (layer: KeymapLayer) => void;
  /** Write every layer that differs from the record (flash writes). */
  save: () => Promise<void>;
  /** Drop the draft: back to what the keyboard has. */
  discard: () => void;
}

const KEY = "ks.keymap.v1";

interface Persisted {
  modelId: string;
  draft: Layers;
  layer: KeymapLayer;
}

function loadPersisted(): Partial<Persisted> {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? (JSON.parse(raw) as Partial<Persisted>) : {};
  } catch {
    return {};
  }
}

function persist(p: Persisted) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    /* per-machine convenience only */
  }
}

function fromAssignments(list: KeyAssignment[]): LayerMap {
  const map: LayerMap = {};
  for (const a of list) if (a.action.kind !== "default") map[a.led] = a.action;
  return map;
}

export function toAssignments(map: LayerMap): KeyAssignment[] {
  return Object.entries(map).map(([led, action]) => ({ led: Number(led), action }));
}

function layersOf(state: KeymapState): Layers {
  return { base: fromAssignments(state.base.assignments), fn: fromAssignments(state.fn.assignments) };
}

export const useKeymap = create<KeymapStore>((set, get) => {
  const stored = loadPersisted();
  const save = () => {
    const { modelId, draft, layer } = get();
    if (modelId) persist({ modelId, draft, layer });
  };
  const update = (patch: Partial<KeymapStore>) => {
    set(patch);
    save();
  };
  const record = (state: KeymapState) => ({
    saved: layersOf(state),
    writtenMs: { base: state.base.writtenMs, fn: state.fn.writtenMs },
  });
  return {
    modelId: null,
    saved: null,
    writtenMs: { base: null, fn: null },
    draft: EMPTY,
    layer: stored.layer === "fn" ? "fn" : "base",
    selected: null,
    load: async (modelId) => {
      const state = await getKeymapState(modelId);
      const { saved, writtenMs } = record(state);
      const kept = loadPersisted();
      const draft = kept.modelId === modelId && kept.draft ? sanitize(kept.draft) : clone(saved);
      update({ modelId, saved, writtenMs, draft });
    },
    setLayer: (layer) => update({ layer }),
    select: (selected) => set({ selected }),
    assign: (led, action, target) => {
      const { draft } = get();
      const layer = target ?? get().layer;
      const next = { ...draft[layer] };
      if (action.kind === "default") delete next[led];
      else next[led] = action;
      update({ draft: { ...draft, [layer]: next } });
    },
    resetLayer: (layer) => update({ draft: { ...get().draft, [layer]: {} } }),
    save: async () => {
      for (const layer of dirtyLayers(get())) {
        const state = await applyKeymap(layer, toAssignments(get().draft[layer]));
        const { saved, writtenMs } = record(state);
        // The backend stores keys in canonical form; keep the draft in step.
        const draft = { ...get().draft, [layer]: clone(saved)[layer] };
        update({ saved, writtenMs, draft });
      }
    },
    discard: () => {
      const saved = get().saved;
      if (saved) update({ draft: clone(saved) });
    },
  };
});

function clone(l: Layers): Layers {
  return { base: { ...l.base }, fn: { ...l.fn } };
}

function sanitize(l: Partial<Layers>): Layers {
  return { base: { ...(l.base ?? {}) }, fn: { ...(l.fn ?? {}) } };
}

export function sameAction(a: KeyActionInfo | undefined, b: KeyActionInfo | undefined): boolean {
  const x = a ?? { kind: "default" };
  const y = b ?? { kind: "default" };
  if (x.kind !== y.kind) return false;
  switch (x.kind) {
    case "key":
      return y.kind === "key" && x.modifiers === y.modifiers && x.code === y.code;
    case "media":
      return y.kind === "media" && x.usage === y.usage;
    case "mouse":
      return y.kind === "mouse" && x.button === y.button;
    case "wheel":
      return y.kind === "wheel" && x.up === y.up;
    case "macro":
      return y.kind === "macro" && x.slot === y.slot && x.mode === y.mode && x.count === y.count;
    default:
      return true;
  }
}

export function sameLayer(a: LayerMap, b: LayerMap): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)].map(Number));
  return [...keys].every((led) => sameAction(a[led], b[led]));
}

/** Layers whose draft differs from what the keyboard has. */
export function dirtyLayers(s: Pick<KeymapStore, "saved" | "draft">): KeymapLayer[] {
  const saved = s.saved;
  if (!saved) return [];
  return LAYERS.filter((l) => !sameLayer(s.draft[l], saved[l]));
}

/** The action a key sends when assigned keyboard usage `code`. */
export function usageAction(code: number, modifiers = 0): KeyActionInfo {
  if (code >= 0xe0 && code <= 0xe7) return { kind: "key", modifiers: modifiers | (1 << (code - 0xe0)), code: 0 };
  return { kind: "key", modifiers, code };
}

/**
 * Canonical form, as the backend stores it: on the base layer a key
 * assigned its own usage is the default (it sends the same either way).
 */
export function normalize(layer: KeymapLayer, keyHid: number, action: KeyActionInfo): KeyActionInfo {
  if (layer === "base" && sameAction(action, usageAction(keyHid))) return { kind: "default" };
  return action;
}
