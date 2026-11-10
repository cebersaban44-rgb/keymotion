import { create } from "zustand";
import type { EffectRequest } from "../bindings/EffectRequest";
import type { LedColor } from "../bindings/LedColor";
import { applyEffect, applyPerKey } from "./ipc";

export type LightingTab = "effects" | "per-key";
/** Per-key tools: paint what you touch, or select keys and then colour them. */
export type PaintTool = "brush" | "select";
/** The lighting setup with edits not yet stored on the keyboard (last edited wins). */
export type LightingDraft = "effect" | "per-key" | null;

/** led index → "#rrggbb" */
export type ColorMap = Record<number, string>;

export interface LightingState {
  tab: LightingTab;
  effect: EffectRequest;
  /** Last effect successfully stored on the keyboard. */
  appliedEffect: EffectRequest | null;
  perKey: ColorMap;
  perKeyBrightness: number;
  /**
   * Per-key design last stored on the keyboard by this app, or `null` when
   * the keyboard is not showing one (nothing saved yet, or an effect was
   * applied since).
   */
  savedPerKey: { colors: ColorMap; brightness: number } | null;
  brush: string;
  tool: PaintTool;
  livePreview: boolean;
  /**
   * Which setup was edited since the last store. The keyboard holds one
   * lighting setup at a time, so only the most recently edited one counts.
   */
  draft: LightingDraft;
  setTab: (t: LightingTab) => void;
  setTool: (t: PaintTool) => void;
  setEffect: (patch: Partial<EffectRequest>) => void;
  markApplied: (e: EffectRequest) => void;
  markPerKeySaved: (colors: ColorMap, brightness: number) => void;
  revertPerKey: () => void;
  paint: (leds: number[], color: string | null) => void;
  setPerKey: (m: ColorMap) => void;
  setPerKeyBrightness: (b: number) => void;
  setBrush: (c: string) => void;
  setLivePreview: (on: boolean) => void;
  /** Store the effect being edited on the keyboard (flash write). */
  applyEffectDraft: () => Promise<void>;
  /** Store the per-key design on the keyboard (flash write). */
  savePerKeyDraft: () => Promise<void>;
  /** Drop unsaved edits: back to what the keyboard stores, where known. */
  discardDraft: () => void;
}

const KEY = "ks.lighting.v1";

const defaults = {
  tab: "effects" as LightingTab,
  effect: { mode: 11, color: "#ff8a3d", rainbow: true, brightness: 5, speed: 3, direction: 0 } as EffectRequest,
  appliedEffect: null as EffectRequest | null,
  perKey: {} as ColorMap,
  perKeyBrightness: 5,
  savedPerKey: null as { colors: ColorMap; brightness: number } | null,
  brush: "#ff8a3d",
  tool: "brush" as PaintTool,
  livePreview: true,
  draft: null as LightingDraft,
};

type Persisted = typeof defaults;

function load(): Persisted {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return defaults;
    const stored = JSON.parse(raw) as Partial<Persisted>;
    const state = { ...defaults, ...stored };
    // State saved before drafts were tracked: infer pending edits once, so
    // changes left unsaved then are still flagged.
    if (!("draft" in stored)) state.draft = inferDraft(state);
    return state;
  } catch {
    return defaults;
  }
}

function inferDraft(s: Persisted): LightingDraft {
  const saved = s.savedPerKey;
  if (saved && (saved.brightness !== s.perKeyBrightness || !sameColors(saved.colors, s.perKey))) return "per-key";
  if (s.appliedEffect && !sameEffect(s.effect, s.appliedEffect)) return "effect";
  return null;
}

function persist(s: Persisted) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* per-machine convenience only */
  }
}

export const useLighting = create<LightingState>((set, get) => {
  const save = () => {
    const { tab, effect, appliedEffect, perKey, perKeyBrightness, savedPerKey, brush, tool, livePreview, draft } = get();
    persist({ tab, effect, appliedEffect, perKey, perKeyBrightness, savedPerKey, brush, tool, livePreview, draft });
  };
  const update = (patch: Partial<LightingState>) => {
    set(patch);
    save();
  };
  return {
    ...load(),
    setTab: (tab) => update({ tab }),
    setTool: (tool) => update({ tool }),
    setEffect: (patch) => update({ effect: { ...get().effect, ...patch }, draft: "effect" }),
    // The keyboard shows one lighting setup at a time: storing an effect
    // replaces the per-key design and vice versa.
    markApplied: (appliedEffect) => update({ appliedEffect, savedPerKey: null, draft: null }),
    markPerKeySaved: (colors, brightness) =>
      update({ savedPerKey: { colors: { ...colors }, brightness }, appliedEffect: null, draft: null }),
    revertPerKey: () => {
      const saved = get().savedPerKey;
      if (saved) update({ perKey: { ...saved.colors }, perKeyBrightness: saved.brightness, draft: null });
    },
    paint: (leds, color) => {
      const next = { ...get().perKey };
      for (const l of leds) {
        if (color) next[l] = color;
        else delete next[l];
      }
      update({ perKey: next, draft: "per-key" });
    },
    setPerKey: (perKey) => update({ perKey, draft: "per-key" }),
    setPerKeyBrightness: (perKeyBrightness) => update({ perKeyBrightness, draft: "per-key" }),
    setBrush: (brush) => update({ brush }),
    setLivePreview: (livePreview) => update({ livePreview }),
    applyEffectDraft: async () => {
      const effect = { ...get().effect };
      await applyEffect(effect);
      get().markApplied(effect);
    },
    savePerKeyDraft: async () => {
      const { perKey, perKeyBrightness } = get();
      const colors = { ...perKey };
      await applyPerKey(toLedColors(colors), perKeyBrightness);
      get().markPerKeySaved(colors, perKeyBrightness);
    },
    discardDraft: () => {
      const { draft, appliedEffect, savedPerKey } = get();
      if (draft === "effect" && appliedEffect) update({ effect: { ...appliedEffect }, draft: null });
      else if (draft === "per-key" && savedPerKey)
        update({ perKey: { ...savedPerKey.colors }, perKeyBrightness: savedPerKey.brightness, draft: null });
      else update({ draft: null });
    },
  };
});

export function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.replace("#", ""), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

export function rgbToHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

function sameEffect(a: EffectRequest, b: EffectRequest): boolean {
  return (
    a.mode === b.mode &&
    a.color.toLowerCase() === b.color.toLowerCase() &&
    a.rainbow === b.rainbow &&
    a.brightness === b.brightness &&
    a.speed === b.speed &&
    a.direction === b.direction
  );
}

/**
 * Which lighting edits are not on the keyboard yet, or `null`. Edits that
 * end up equal to what is stored do not count.
 */
export function lightingUnsaved(
  s: Pick<LightingState, "draft" | "effect" | "appliedEffect" | "perKey" | "perKeyBrightness" | "savedPerKey">,
): LightingDraft {
  if (s.draft === "effect") return s.appliedEffect && sameEffect(s.effect, s.appliedEffect) ? null : "effect";
  if (s.draft === "per-key") {
    const saved = s.savedPerKey;
    const same = saved
      ? saved.brightness === s.perKeyBrightness && sameColors(saved.colors, s.perKey)
      : Object.keys(s.perKey).length === 0;
    return same ? null : "per-key";
  }
  return null;
}

/** Same colours on the same keys (case-insensitive hex). */
export function sameColors(a: ColorMap, b: ColorMap): boolean {
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => b[Number(k)]?.toLowerCase() === a[Number(k)]?.toLowerCase());
}

export function toLedColors(map: ColorMap): LedColor[] {
  return Object.entries(map).map(([led, hex]) => {
    const [r, g, b] = hexToRgb(hex);
    return { led: Number(led), r, g, b };
  });
}

/** Effects that react to key presses rather than animating on their own. */
export const REACTIVE_EFFECTS = new Set([2, 3, 13, 14, 15]);

export const SWATCHES = [
  "#ffffff",
  "#ff3b30",
  "#ff8a3d",
  "#ffcc00",
  "#a8e43a",
  "#34c759",
  "#2ee6d6",
  "#38b6ff",
  "#3a5bff",
  "#8b5cf6",
  "#e040fb",
  "#ff4fa3",
];
