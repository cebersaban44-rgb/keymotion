import { create } from "zustand";
import type { KeyActionInfo } from "../bindings/KeyActionInfo";
import type { KeyAssignment } from "../bindings/KeyAssignment";
import type { KeymapLayer } from "../bindings/KeymapLayer";
import type { KeymapState } from "../bindings/KeymapState";
import { withoutFixed } from "./aaText";
import { applyKeymap, getKeymapState } from "./ipc";

/** led → action; keys not listed use the firmware default. */
export type LayerMap = Record<number, KeyActionInfo>;
export type Layers = Record<KeymapLayer, LayerMap>;

export const LAYERS: readonly KeymapLayer[] = ["base", "fn"];
/** HID usage of the Fn key; it is never reassigned. */
export const FN_USAGE = 0xaf;

const EMPTY: Layers = { base: {}, fn: {} };

export interface KeymapStore {
  /** Record id (`KeyboardInfo.recordId`) of the keyboards the state below
   *  belongs to. */
  recordId: string | null;
  /**
   * What the keyboard has: what the app last wrote ("04 xx", which cannot
   * report its keymap), or what the keyboard itself holds ("AA", `readMs`).
   * `null` until loaded.
   */
  saved: Layers | null;
  writtenMs: Record<KeymapLayer, number | null>;
  /** LEDs of keys set in another program to something Keymotion can
   *  neither show nor change ("AA" family): not editable, kept by a save. */
  fixed: Record<KeymapLayer, number[]>;
  /** When `saved` was read from the keyboard itself; `null`: the app's own
   *  record. */
  readMs: number | null;
  /** Edits being made; equals `saved` when nothing is pending. */
  draft: Layers;
  layer: KeymapLayer;
  selected: number | null;
  /** Load the record for `recordId`, keeping an unsaved draft for the same keyboards. */
  load: (recordId: string) => Promise<void>;
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
  /** The record id. Named after the model id it was until 1.2.x: the same
   *  value for a keyboard of the "04 xx" family, so drafts saved then still
   *  match. */
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
    const { recordId, draft, layer } = get();
    if (recordId) persist({ modelId: recordId, draft, layer });
  };
  const update = (patch: Partial<KeymapStore>) => {
    set(patch);
    save();
  };
  const record = (state: KeymapState) => ({
    saved: layersOf(state),
    writtenMs: { base: state.base.writtenMs, fn: state.fn.writtenMs },
    fixed: fixedOf(state),
    readMs: state.readMs,
  });
  return {
    recordId: null,
    saved: null,
    writtenMs: { base: null, fn: null },
    fixed: { base: [], fn: [] },
    readMs: null,
    draft: EMPTY,
    layer: stored.layer === "fn" ? "fn" : "base",
    selected: null,
    load: async (recordId) => {
      const state = await getKeymapState(recordId);
      const r = record(state);
      const kept = loadPersisted();
      const draft = kept.modelId === recordId && kept.draft ? sanitize(kept.draft, r.fixed) : clone(r.saved);
      update({ recordId, ...r, draft });
    },
    setLayer: (layer) => update({ layer }),
    select: (selected) => set({ selected }),
    assign: (led, action, target) => {
      const { draft } = get();
      const layer = target ?? get().layer;
      // Set in another program: kept as the keyboard has it.
      if (get().fixed[layer].includes(led)) return;
      const next = { ...draft[layer] };
      if (action.kind === "default") delete next[led];
      else next[led] = action;
      update({ draft: { ...draft, [layer]: next } });
    },
    resetLayer: (layer) => update({ draft: { ...get().draft, [layer]: {} } }),
    save: async () => {
      for (const layer of dirtyLayers(get())) {
        const state = await applyKeymap(layer, toAssignments(get().draft[layer]));
        const r = record(state);
        // The backend stores keys in canonical form; keep the draft in step.
        const draft = { ...get().draft, [layer]: clone(r.saved)[layer] };
        update({ ...r, draft });
      }
    },
    discard: () => {
      const saved = get().saved;
      if (saved) update({ draft: clone(saved) });
    },
  };
});

/**
 * A keymap from a backup, opened as unsaved edits for the keyboards of
 * `recordId`: nothing is written to the keyboard until the user saves it on
 * the Key mapping page.
 */
export function stageKeymapImport(recordId: string, state: KeymapState) {
  const store = useKeymap.getState();
  const draft = sanitize(layersOf(state), store.recordId === recordId ? store.fixed : { base: [], fn: [] });
  if (store.recordId === recordId) useKeymap.setState({ draft });
  persist({ modelId: recordId, draft, layer: store.layer });
}

/**
 * The record may have changed outside the editor (a profile was loaded):
 * read it again. When it did, the draft (made against the old one) goes.
 */
export async function adoptKeymapRecord(recordId: string) {
  const state = await getKeymapState(recordId);
  const saved = layersOf(state);
  const fixed = fixedOf(state);
  const store = useKeymap.getState();
  const old = store.recordId === recordId ? store.saved : null;
  const sameFixed = LAYERS.every((l) => store.fixed[l].join(",") === fixed[l].join(","));
  if (old && sameFixed && LAYERS.every((l) => sameLayer(old[l], saved[l]))) {
    // Read again, the same keys: only the time changes.
    if (store.recordId === recordId) useKeymap.setState({ readMs: state.readMs });
    return;
  }
  if (store.recordId === recordId) {
    useKeymap.setState({
      saved,
      writtenMs: { base: state.base.writtenMs, fn: state.fn.writtenMs },
      fixed,
      readMs: state.readMs,
      draft: clone(saved),
    });
  }
  persist({ modelId: recordId, draft: clone(saved), layer: store.layer });
}

function clone(l: Layers): Layers {
  return { base: { ...l.base }, fn: { ...l.fn } };
}

/** Keys set in another program, per layer. */
function fixedOf(state: KeymapState): Record<KeymapLayer, number[]> {
  return { base: [...state.base.fixed], fn: [...state.fn.fixed] };
}

/** A draft as kept or imported: whole layers, without the keys set in
 *  another program (never changed from here). */
function sanitize(l: Partial<Layers>, fixed: Record<KeymapLayer, readonly number[]>): Layers {
  return { base: withoutFixed(l.base ?? {}, fixed.base), fn: withoutFixed(l.fn ?? {}, fixed.fn) };
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
