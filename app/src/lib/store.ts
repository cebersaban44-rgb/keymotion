import { create } from "zustand";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ThemePreference } from "../bindings/ThemePreference";
import type { UiEnvironment } from "../bindings/UiEnvironment";
import type { Locale } from "./i18n";

export type PageId = "overview" | "lighting" | "keymap" | "macros" | "screen" | "connection" | "settings";

interface AppState {
  keyboards: KeyboardInfo[];
  loaded: boolean;
  env: UiEnvironment | null;
  page: PageId;
  theme: ThemePreference;
  locale: Locale;
  setKeyboards: (k: KeyboardInfo[]) => void;
  setEnv: (e: UiEnvironment) => void;
  setPage: (p: PageId) => void;
  setTheme: (t: ThemePreference) => void;
  setLocale: (l: Locale) => void;
}

// UI preferences are per-machine conveniences; storage may be unavailable.
function load<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

function save(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

export const useApp = create<AppState>((set) => ({
  keyboards: [],
  loaded: false,
  env: null,
  page: "overview",
  theme: load<ThemePreference>("ks.theme", ["system", "dark", "light"], "system"),
  locale: load<Locale>("ks.locale", ["tr", "en"], "tr"),
  setKeyboards: (keyboards) => set({ keyboards, loaded: true }),
  setEnv: (env) => set({ env }),
  setPage: (page) => set({ page }),
  setTheme: (theme) => {
    save("ks.theme", theme);
    set({ theme });
  },
  setLocale: (locale) => {
    save("ks.locale", locale);
    set({ locale });
  },
}));

/** The keyboard the UI is focused on: prefer the richest connection. */
export function useActiveKeyboard(): KeyboardInfo | null {
  return useApp((s) => {
    const rank = { usb: 0, dongle: 1, bluetooth: 2 } as const;
    return [...s.keyboards].sort((a, b) => rank[a.connection] - rank[b.connection])[0] ?? null;
  });
}
