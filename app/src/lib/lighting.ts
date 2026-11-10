import { create } from "zustand";
import type { EffectRequest } from "../bindings/EffectRequest";
import type { LedColor } from "../bindings/LedColor";

export type LightingTab = "effects" | "per-key";

/** led index → "#rrggbb" */
export type ColorMap = Record<number, string>;

interface LightingState {
  tab: LightingTab;
  effect: EffectRequest;
  /** Last effect successfully stored on the keyboard. */
  appliedEffect: EffectRequest | null;
  perKey: ColorMap;
  perKeyBrightness: number;
  brush: string;
  livePreview: boolean;
  setTab: (t: LightingTab) => void;
  setEffect: (patch: Partial<EffectRequest>) => void;
  markApplied: (e: EffectRequest) => void;
  paint: (leds: number[], color: string | null) => void;
  setPerKey: (m: ColorMap) => void;
  setPerKeyBrightness: (b: number) => void;
  setBrush: (c: string) => void;
  setLivePreview: (on: boolean) => void;
}

const KEY = "ks.lighting.v1";

const defaults = {
  tab: "effects" as LightingTab,
  effect: { mode: 11, color: "#ff8a3d", rainbow: true, brightness: 5, speed: 3, direction: 0 } as EffectRequest,
  appliedEffect: null as EffectRequest | null,
  perKey: {} as ColorMap,
  perKeyBrightness: 5,
  brush: "#ff8a3d",
  livePreview: true,
};

type Persisted = typeof defaults;

function load(): Persisted {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...defaults, ...(JSON.parse(raw) as Partial<Persisted>) } : defaults;
  } catch {
    return defaults;
  }
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
    const { tab, effect, appliedEffect, perKey, perKeyBrightness, brush, livePreview } = get();
    persist({ tab, effect, appliedEffect, perKey, perKeyBrightness, brush, livePreview });
  };
  const update = (patch: Partial<LightingState>) => {
    set(patch);
    save();
  };
  return {
    ...load(),
    setTab: (tab) => update({ tab }),
    setEffect: (patch) => update({ effect: { ...get().effect, ...patch } }),
    markApplied: (appliedEffect) => update({ appliedEffect }),
    paint: (leds, color) => {
      const next = { ...get().perKey };
      for (const l of leds) {
        if (color) next[l] = color;
        else delete next[l];
      }
      update({ perKey: next });
    },
    setPerKey: (perKey) => update({ perKey }),
    setPerKeyBrightness: (perKeyBrightness) => update({ perKeyBrightness }),
    setBrush: (brush) => update({ brush }),
    setLivePreview: (livePreview) => update({ livePreview }),
  };
});

export function hexToRgb(hex: string): [number, number, number] {
  const v = parseInt(hex.replace("#", ""), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

export function rgbToHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
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
