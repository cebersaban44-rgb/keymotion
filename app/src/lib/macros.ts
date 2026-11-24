import { create } from "zustand";
import type { MacroBudgetRule } from "../bindings/MacroBudgetRule";
import type { MacroFixed } from "../bindings/MacroFixed";
import type { MacroInfo } from "../bindings/MacroInfo";
import type { MacroStepInfo } from "../bindings/MacroStepInfo";
import type { MacrosState } from "../bindings/MacrosState";
import { freeMacroSlot } from "./aaText";
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
  /** Record id (`KeyboardInfo.recordId`) of the keyboards the state below
   *  belongs to. */
  recordId: string | null;
  /** What the keyboard has: what the app last wrote ("04 xx"), or what the
   *  keyboard itself holds ("AA", `readMs`); `null` until loaded. */
  saved: Slots | null;
  writtenMs: number | null;
  /** Macros made in another program that Keymotion can neither show nor
   *  change ("AA" family): their slots stay as they are, and a save keeps
   *  them. */
  fixed: MacroFixed[];
  /** When `saved` was read from the keyboard itself; `null`: the app's own
   *  record. */
  readMs: number | null;
  limits: MacroLimits;
  /** Edits being made; equals `saved` when nothing is pending. */
  draft: Slots;
  selected: number | null;
  load: (recordId: string) => Promise<void>;
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
  /** The record id. Named after the model id it was until 1.2.x: the same
   *  value for a keyboard of the "04 xx" family, so drafts saved then still
   *  match. */
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
    const { recordId, draft, selected } = get();
    if (recordId) persist({ modelId: recordId, draft, selected });
  };
  const update = (patch: Partial<MacroStore>) => {
    set(patch);
    save();
  };
  const record = (state: MacrosState) => ({
    saved: trim(state.slots),
    writtenMs: state.writtenMs,
    fixed: state.fixed,
    readMs: state.readMs,
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
    recordId: null,
    saved: null,
    writtenMs: null,
    fixed: [],
    readMs: null,
    limits: DEFAULT_LIMITS,
    draft: [],
    selected: null,
    load: async (recordId) => {
      const r = record(await getMacrosState(recordId));
      const kept = loadPersisted();
      const same = kept.modelId === recordId && Array.isArray(kept.draft);
      update({
        recordId,
        ...r,
        draft: same ? withoutFixed(clone(kept.draft as Slots), r.fixed) : clone(r.saved),
        selected: same ? (kept.selected ?? null) : null,
      });
    },
    select: (selected) => update({ selected }),
    create: (name) => {
      const { draft, limits, fixed } = get();
      // Not into a slot that holds a macro made in another program.
      const slot = freeMacroSlot(
        draft,
        fixed.map((f) => f.slot),
        limits.maxSlots,
      );
      if (slot === null) return null;
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
 * Macros from a backup, opened as unsaved edits for the keyboards of
 * `recordId`: nothing is written to the keyboard until the user saves them
 * on the Macros page.
 */
export function stageMacrosImport(recordId: string, slots: Slots) {
  const store = useMacros.getState();
  const draft = withoutFixed(clone(trim(slots)), store.recordId === recordId ? store.fixed : []);
  if (store.recordId === recordId) useMacros.setState({ draft, selected: null });
  persist({ modelId: recordId, draft, selected: null });
}

/**
 * The record may have changed outside the editor (a profile was loaded):
 * read it again. When it did, the draft (made against the old one) goes.
 */
export async function adoptMacrosRecord(recordId: string) {
  const state = await getMacrosState(recordId);
  const saved = trim(state.slots);
  const store = useMacros.getState();
  const sameFixed = JSON.stringify(store.fixed) === JSON.stringify(state.fixed);
  if (store.recordId === recordId && store.saved && sameFixed && dirtySlots({ saved: store.saved, draft: saved }).length === 0) {
    // Read again, the same macros: only the time changes.
    useMacros.setState({ readMs: state.readMs });
    return;
  }
  if (store.recordId === recordId) {
    useMacros.setState({
      saved,
      writtenMs: state.writtenMs,
      fixed: state.fixed,
      readMs: state.readMs,
      draft: clone(saved),
      selected: null,
    });
  }
  persist({ modelId: recordId, draft: clone(saved), selected: null });
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

/** Bytes of keyboard memory the macros need, by the keyboard's rule
 *  (`KeyboardInfo.limits.macroBudget`): `(steps + slots) × 8` in the "04 xx"
 *  family, `(steps + slots) × 4` in the "AA" family. */
export function budgetUsed(slots: Slots, rule: MacroBudgetRule = "ff13", fixed: readonly MacroFixed[] = []): number {
  const used = trim(slots);
  // Macros made in another program take memory too (and are kept).
  const steps = used.reduce((n, m) => n + (m?.steps.length ?? 0), 0) + fixed.reduce((n, f) => n + f.steps, 0);
  const macros = used.filter((m) => m !== null).length + fixed.length;
  return rule === "aa512" ? (steps + macros) * 4 : (steps + used.length) * 8;
}

/** Slots without the macros made in another program (never changed from
 *  here). */
function withoutFixed(slots: Slots, fixed: readonly MacroFixed[]): Slots {
  return trim(slots.map((m, i) => (fixed.some((f) => f.slot === i) ? null : m)));
}

/** The keyboard's macro memory in bytes: the record's (`MacrosState.budget`,
 *  the "04 xx" family's) or 512 in the "AA" family. */
export function budgetOf(rule: MacroBudgetRule, recordBudget: number): number {
  return rule === "aa512" ? 512 : recordBudget;
}

/** Total play time of a macro, in ms (pauses below the minimum are raised). */
export function duration(m: MacroInfo, minDelayMs: number): number {
  return m.steps.reduce((t, s) => t + Math.max(s.delayMs, minDelayMs), 0);
}
