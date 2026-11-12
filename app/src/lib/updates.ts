// Updates (roadmap stage 6): what the app knows about a newer Keymotion.
// The backend (updates.rs) looks on GitHub and installs; this keeps its
// state for the settings row, the strip above the page and the install
// dialog, and the user's "Later" for this run.

import { create } from "zustand";
import type { UpdateState } from "../bindings/UpdateState";
import type { UpdateStatus } from "../bindings/UpdateStatus";
import type { Locale } from "./i18n";
import { inTauri, invoke, listen } from "./ipc";

const EVENT_UPDATE = "update-state";

/** The release's page on GitHub (its notes, in English). */
export const releasePage = (version: string) => `https://github.com/cebersaban44-rgb/keymotion/releases/tag/v${version}`;

// ----------------------------------------------------------- browser mock

/** Browser preview: `?update=available|none|offline|norelease` (default: none). */
const mockMode = () => new URLSearchParams(location.search).get("update") ?? "none";

const MOCK_NOTES = JSON.stringify({
  en: "- Focus timer: a sequence of focus and break blocks\n- Overview: key letters on the keyboard drawing\n- A tried reminder card goes back to your picture",
  tr: "- Odak zamanlayıcısında odak ve mola bloklarından bir sıra\n- Genel bakış'ta klavye çiziminde tuş yazıları\n- Denenen hatırlatıcı kartından sonra kendi görseliniz geri gelir",
});

const mock = {
  state: { current: "0.1.0", autoCheck: true, lastCheckMs: null, status: { kind: "idle" } } as UpdateState,
  timer: 0,
  emit: null as ((s: UpdateState) => void) | null,
};

function mockSet(status: UpdateStatus, patch: Partial<UpdateState> = {}) {
  mock.state = { ...mock.state, ...patch, status };
  mock.emit?.(mock.state);
}

function mockLook(): UpdateState {
  const now = Date.now();
  const mode = mockMode();
  if (mode === "available") {
    mockSet({ kind: "available", version: "0.1.1", notes: MOCK_NOTES, dateMs: now - 86_400_000 }, { lastCheckMs: now });
  } else if (mode === "offline" || mode === "norelease") {
    mockSet({ kind: "failed", reason: mode === "offline" ? "offline" : "noRelease" }, { lastCheckMs: now });
  } else {
    mockSet({ kind: "upToDate" }, { lastCheckMs: now });
  }
  return mock.state;
}

function mockInstall() {
  const s = mock.state.status;
  if (s.kind !== "available") return;
  const total = 9.8e6;
  let done = 0;
  window.clearInterval(mock.timer);
  mock.timer = window.setInterval(() => {
    done = Math.min(total, done + 0.7e6);
    if (done < total) mockSet({ kind: "downloading", version: s.version, done, total });
    else {
      window.clearInterval(mock.timer);
      mockSet({ kind: "installing", version: s.version });
    }
  }, 250);
}

// ------------------------------------------------------------------- ipc

const devMock = import.meta.env.DEV && !inTauri;

async function getState(): Promise<UpdateState> {
  if (devMock) return mock.state;
  return invoke<UpdateState>("update_state");
}

// ----------------------------------------------------------------- store

interface UpdatesStore {
  state: UpdateState | null;
  /** "Later" on the strip: hidden for this version until the app restarts. */
  laterFor: string | null;
  dialog: boolean;
  /** The last button press failed (shown under the row). */
  error: string | null;
  check: () => Promise<void>;
  setAuto: (on: boolean) => Promise<void>;
  install: () => Promise<void>;
  cancel: () => Promise<void>;
  later: () => void;
  openDialog: (open: boolean) => void;
}

export const useUpdates = create<UpdatesStore>((set, get) => ({
  state: null,
  laterFor: null,
  dialog: false,
  error: null,
  check: async () => {
    set({ error: null });
    try {
      if (devMock) {
        mockSet({ kind: "checking" });
        await new Promise((r) => setTimeout(r, 700));
        set({ state: mockLook() });
        return;
      }
      set({ state: await invoke<UpdateState>("update_check") });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  setAuto: async (on) => {
    if (devMock) {
      mockSet(mock.state.status, { autoCheck: on });
      set({ state: mock.state });
      return;
    }
    set({ state: await invoke<UpdateState>("update_auto", { enabled: on }) });
  },
  install: async () => {
    set({ error: null });
    try {
      if (devMock) {
        mockInstall();
        return;
      }
      set({ state: await invoke<UpdateState>("update_install") });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  cancel: async () => {
    if (devMock) {
      window.clearInterval(mock.timer);
      const s = mock.state.status;
      if (s.kind === "downloading") mockSet({ kind: "available", version: s.version, notes: MOCK_NOTES, dateMs: null });
      return;
    }
    set({ state: await invoke<UpdateState>("update_cancel") });
  },
  later: () => {
    const s = get().state?.status;
    if (s?.kind === "available") set({ laterFor: s.version });
  },
  openDialog: (open) => set({ dialog: open }),
}));

/** Load the state and follow the backend's changes. Returns the stop function. */
export function startUpdates(): () => void {
  let stopped = false;
  const onState = (s: UpdateState) => {
    if (!stopped) useUpdates.setState({ state: s });
  };
  void getState().then(onState, (e) => console.error("update state", e));
  if (devMock) {
    mock.emit = onState;
    // The daily look a moment after starting, as the app does.
    const t = window.setTimeout(() => {
      if (mock.state.autoCheck && mockMode() !== "none") mockLook();
    }, 1500);
    return () => {
      stopped = true;
      window.clearTimeout(t);
      mock.emit = null;
    };
  }
  const unlisten = listen<UpdateState>(EVENT_UPDATE, (e) => onState(e.payload));
  return () => {
    stopped = true;
    void unlisten.then((f) => f());
  };
}

// ----------------------------------------------------------------- notes

/**
 * The release notes in the UI language, as short lines. The release
 * pipeline writes them as a JSON object ({"en": "...", "tr": "..."}) of
 * Markdown lists. Null when there are none in this language (the UI then
 * links to the release page instead of showing another language).
 */
export function notesFor(notes: string | null, locale: Locale): string[] | null {
  if (!notes) return null;
  let text: string | undefined;
  try {
    const parsed: unknown = JSON.parse(notes);
    if (parsed && typeof parsed === "object") text = (parsed as Record<string, unknown>)[locale] as string | undefined;
  } catch {
    // Plain text: written in English.
    text = locale === "en" ? notes : undefined;
  }
  if (typeof text !== "string" || !text.trim()) return null;
  const lines: string[] = [];
  for (const raw of text.split(/\r?\n/)) {
    if (!raw.trim() || /^\s*#/.test(raw)) continue;
    const plain = raw
      .replace(/\*\*|__|`/g, "")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .trim();
    const item = /^\s*[-*]\s+/.test(raw);
    if (item || lines.length === 0) lines.push(plain.replace(/^[-*]\s+/, ""));
    else lines[lines.length - 1] += " " + plain;
  }
  return lines.slice(0, 12);
}

/** Bytes as "6,1 MB" in the UI language. */
export function megabytes(bytes: number, locale: Locale): string {
  return (bytes / 1e6).toLocaleString(locale === "tr" ? "tr-TR" : "en-US", { maximumFractionDigits: 1, minimumFractionDigits: 1 });
}
