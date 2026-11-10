// What each key types on the user's Windows keyboard layout, from the
// browser's Keyboard Map API (`navigator.keyboard.getLayoutMap()`; WebView2
// supports it). Used to label keys by the character they type: on a Turkish
// Q layout the key US calls ";" types "ş". Only printable keys are covered;
// others keep their catalogue name.

import { create } from "zustand";
import { codeFromHid } from "./hidCodes";

interface LayoutState {
  /** `KeyboardEvent.code` → character, or `null` when unavailable. */
  map: Map<string, string> | null;
  refresh: () => Promise<void>;
}

interface KeyboardLayoutApi {
  getLayoutMap(): Promise<Map<string, string>>;
}

export const useLayout = create<LayoutState>((set) => ({
  map: null,
  refresh: async () => {
    const api = (navigator as Navigator & { keyboard?: KeyboardLayoutApi }).keyboard;
    if (!api?.getLayoutMap) return;
    try {
      const m = await api.getLayoutMap();
      set({ map: new Map(m) });
    } catch {
      /* not allowed here: keep catalogue names */
    }
  },
}));

const TURKISH = /[ğüşıöç]/;

/** Keycap-style label (upper case) of what HID usage `hid` types, if known. */
export function layoutLabel(map: Map<string, string> | null, hid: number): string | undefined {
  if (!map) return undefined;
  const code = codeFromHid(hid);
  const ch = code ? map.get(code) : undefined;
  if (!ch) return undefined;
  // Turkish needs its own casing: ı → I and i → İ.
  const turkish = [...map.values()].some((v) => TURKISH.test(v));
  return turkish ? ch.toLocaleUpperCase("tr") : ch.toUpperCase();
}

/** `hid → label` for the current layout, re-read when the window regains focus. */
export function useLayoutLabel(): (hid: number) => string | undefined {
  const map = useLayout((s) => s.map);
  return (hid) => layoutLabel(map, hid);
}
