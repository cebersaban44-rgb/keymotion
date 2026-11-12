import { create } from "zustand";
import type { MacroInfo } from "../bindings/MacroInfo";
import type { MacroStepInfo } from "../bindings/MacroStepInfo";
import type { MacrosState } from "../bindings/MacrosState";
import { applyMacros, getMacrosState } from "./ipc";

/** Slot `i` is macro `i`; `null` is an unused slot. */
export type Slots = (MacroInfo | null)[];

export interface MacroLimits {
  budget: number;
  maxSlots: number;
  minDelayMs: number;
  maxName: number;
}

export interface MacroStore {
  modelId: string | null;
  /** What the app last wrote to the keyboard (its own record); `null` until loaded. */
  saved: Slots | null;
  writtenMs: number | null;
  limits: MacroLimits;
  /** Edits being made; equals `saved` when nothing is pending. */
  draft: Slots;
  selected: number | null;
  load: (modelId: string) => Promise<void>;
  select: (slot: number | null) => void;
  /** New empty macro in the first free slot; returns the slot, or `null` when full. */
  create: (name: string) => number | null;
  rename: (slot: number, name: string) => void;
  setSteps: (slot: number, steps: MacroStepInfo[]) => void;
  /** Free the slot; later slots keep their numbers. */
  remove: (slot: number) => void;
  /** Write every macro (one flash write). */
  save: () => Promise<void>;
  discard: () => void;
}

const KEY = "ks.macros.v1";

const DEFAULT_LIMITS: MacroLimits = { budget: 0xa8c, maxSlots: 100, minDelayMs: 10, maxName: 40 };

interface Persisted {
  modelId: string;
  draft: Slots;
  selected: number | null;
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

/** Unused slots at the end carry no information. */
export function trim(slots: Slots): Slots {
  const out = slots.slice();
  while (out.length && out[out.length - 1] === null) out.pop();
  return out;
}

function clone(slots: Slots): Slots {
  return slots.map((m) => (m ? { name: m.name, steps: m.steps.map((s) => ({ ...s })) } : null));
}

export const useMacros = create<MacroStore>((set, get) => {
  const save = () => {
    const { modelId, draft, selected } = get();
    if (modelId) persist({ modelId, draft, selected });
  };
  const update = (patch: Partial<MacroStore>) => {
    set(patch);
    save();
  };
  const record = (state: MacrosState) => ({
    saved: trim(state.slots),
    writtenMs: state.writtenMs,
    limits: {
      budget: state.budget,
      maxSlots: state.maxSlots,
      minDelayMs: state.minDelayMs,
      maxName: state.maxName,
    },
  });
  const edit = (slot: number, fn: (m: MacroInfo) => MacroInfo) => {
    const draft = get().draft.slice();
    const m = draft[slot];
    if (!m) return;
    draft[slot] = fn(m);
    update({ draft });
  };
  return {
    modelId: null,
    saved: null,
    writtenMs: null,
    limits: DEFAULT_LIMITS,
    draft: [],
    selected: null,
    load: async (modelId) => {
      const r = record(await getMacrosState(modelId));
      const kept = loadPersisted();
      const same = kept.modelId === modelId && Array.isArray(kept.draft);
      update({
        modelId,
        ...r,
        draft: same ? clone(kept.draft as Slots) : clone(r.saved),
        selected: same ? (kept.selected ?? null) : null,
      });
    },
    select: (selected) => update({ selected }),
    create: (name) => {
      const { draft, limits } = get();
      let slot = draft.indexOf(null);
      if (slot === -1) slot = draft.length;
      if (slot >= limits.maxSlots) return null;
      const next = draft.slice();
      next[slot] = { name, steps: [] };
      update({ draft: next, selected: slot });
      return slot;
    },
    rename: (slot, name) => edit(slot, (m) => ({ ...m, name: name.slice(0, get().limits.maxName) })),
    setSteps: (slot, steps) => edit(slot, (m) => ({ ...m, steps })),
    remove: (slot) => {
      const draft = get().draft.slice();
      if (slot >= draft.length) return;
      draft[slot] = null;
      update({ draft: trim(draft), selected: get().selected === slot ? null : get().selected });
    },
    save: async () => {
      const r = record(await applyMacros(trim(get().draft)));
      update({ ...r, draft: clone(r.saved) });
    },
    discard: () => {
      const saved = get().saved;
      if (!saved) return;
      const selected = get().selected;
      update({ draft: clone(saved), selected: selected !== null && saved[selected] ? selected : null });
    },
  };
});

/**
 * Macros from a backup, opened as unsaved edits for `modelId`: nothing is
 * written to the keyboard until the user saves them on the Macros page.
 */
export function stageMacrosImport(modelId: string, slots: Slots) {
  const draft = clone(trim(slots));
  if (useMacros.getState().modelId === modelId) useMacros.setState({ draft, selected: null });
  persist({ modelId, draft, selected: null });
}

/**
 * The record may have changed outside the editor (a profile was loaded):
 * read it again. When it did, the draft (made against the old one) goes.
 */
export async function adoptMacrosRecord(modelId: string) {
  const state = await getMacrosState(modelId);
  const saved = trim(state.slots);
  const store = useMacros.getState();
  if (store.modelId === modelId && store.saved && dirtySlots({ saved: store.saved, draft: saved }).length === 0) return;
  if (store.modelId === modelId) {
    useMacros.setState({ saved, writtenMs: state.writtenMs, draft: clone(saved), selected: null });
  }
  persist({ modelId, draft: clone(saved), selected: null });
}

function sameStep(a: MacroStepInfo, b: MacroStepInfo): boolean {
  return a.kind === b.kind && a.code === b.code && a.delayMs === b.delayMs;
}

function sameMacro(a: MacroInfo | null | undefined, b: MacroInfo | null | undefined): boolean {
  if (!a || !b) return !a && !b;
  return a.name === b.name && a.steps.length === b.steps.length && a.steps.every((s, i) => sameStep(s, b.steps[i]!));
}

/** Slots whose draft differs from what the keyboard has. */
export function dirtySlots(s: Pick<MacroStore, "saved" | "draft">): number[] {
  const saved = s.saved;
  if (!saved) return [];
  const n = Math.max(saved.length, s.draft.length);
  const out: number[] = [];
  for (let i = 0; i < n; i++) if (!sameMacro(s.draft[i], saved[i])) out.push(i);
  return out;
}

/** Bytes of keyboard memory the macros need: `(steps + slots) × 8`. */
export function budgetUsed(slots: Slots): number {
  const used = trim(slots);
  const steps = used.reduce((n, m) => n + (m?.steps.length ?? 0), 0);
  return (steps + used.length) * 8;
}

/** Total play time of a macro, in ms (pauses below the minimum are raised). */
export function duration(m: MacroInfo, minDelayMs: number): number {
  return m.steps.reduce((t, s) => t + Math.max(s.delayMs, minDelayMs), 0);
}
