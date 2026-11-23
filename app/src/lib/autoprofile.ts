// Per-app profiles ("Uygulamaya göre"): the backend watches the program in
// front and puts the linked profile's lighting on the keys, or writes the
// profile when a rule asks (app/src-tauri/src/autoprofile.rs). This file
// follows its state and changes its settings; the browser preview keeps a
// made-up state.

import { create } from "zustand";
import type { AutoProfileSettings } from "../bindings/AutoProfileSettings";
import type { AutoProfileState } from "../bindings/AutoProfileState";
import { inTauri, invoke, listen } from "./ipc";

/** The rule that stands for every game (dto.rs `AUTO_RULE_GAMES`). */
export const RULE_GAMES = "games";

export const DEFAULT_AUTO: AutoProfileSettings = { enabled: false, paused: false, rules: [], back: "previous", base: null };

const mock = {
  state: {
    rev: 0,
    settings: { ...DEFAULT_AUTO },
    active: null,
    because: null,
    writesToday: 2,
    writesMax: 20,
    lastError: null,
    recent: [
      { path: "C:\\Program Files\\Microsoft VS Code\\Code.exe", name: "Code" },
      { path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", name: "chrome" },
    ],
  } as AutoProfileState,
};

export function getAuto(): Promise<AutoProfileState> {
  if (import.meta.env.DEV && !inTauri) return Promise.resolve(structuredClone(mock.state));
  return invoke<AutoProfileState>("autoprofile_state");
}

export function configureAuto(settings: AutoProfileSettings): Promise<AutoProfileState> {
  if (import.meta.env.DEV && !inTauri) {
    mock.state.settings = structuredClone(settings);
    return Promise.resolve(structuredClone(mock.state));
  }
  return invoke<AutoProfileState>("autoprofile_configure", { settings });
}

interface AutoStore {
  state: AutoProfileState | null;
  load: () => Promise<void>;
  configure: (patch: Partial<AutoProfileSettings>) => Promise<void>;
}

/** Take a state from the backend unless a newer one is here already (a
 *  reply and an event can cross). */
function accept(next: AutoProfileState) {
  const cur = useAuto.getState().state;
  if (!cur || next.rev >= cur.rev || cur.rev - next.rev > 1_000_000) useAuto.setState({ state: next });
}

export const useAuto = create<AutoStore>((set, get) => ({
  state: null,
  load: async () => accept(await getAuto()),
  configure: async (patch) => {
    const s = get().state;
    const settings = { ...(s?.settings ?? DEFAULT_AUTO), ...patch };
    if (s) set({ state: { ...s, settings } });
    accept(await configureAuto(settings));
  },
}));

/** Follow the backend while the page is open (the tray can pause it too). */
export function followAuto(): () => void {
  useAuto.getState().load().catch(console.error);
  if (import.meta.env.DEV && !inTauri) return () => {};
  const off = listen<AutoProfileState>("auto-profile", (e) => accept(e.payload));
  return () => {
    off.then((f) => f()).catch(console.error);
  };
}
