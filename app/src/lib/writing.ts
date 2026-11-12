// A write to the keyboard's flash in progress, as the backend reports it
// (`flash-write`): manual and live screen uploads, the live mode going back
// to the user's own picture, lighting, keymap and macros. Closing the app
// asks while one runs (unsaved.ts). The backend also asks Windows to hold a
// shutdown meanwhile; the reason Windows shows comes from here, in the UI
// language.

import { create } from "zustand";
import type { FlashWrite } from "../bindings/FlashWrite";
import { tNow } from "./i18n";
import { follow } from "./follow";
import { inTauri, invoke, listen } from "./ipc";
import { useApp } from "./store";

interface WritingStore {
  kind: FlashWrite | null;
}

export const useWriting = create<WritingStore>(() => ({ kind: null }));

let started = false;

/** Follow the backend's flash writes for the whole session (once). */
export function startWriting() {
  if (started || !inTauri) return;
  started = true;
  follow(
    () => invoke<FlashWrite | null>("flash_write_state"),
    (cb) => listen<FlashWrite | null>("flash-write", (e) => cb(e.payload)),
    (kind) => useWriting.setState({ kind }),
  );
  const sendReason = () => invoke("set_write_reason", { reason: tNow("write.reason") }).catch(console.error);
  sendReason();
  useApp.subscribe((s, prev) => {
    if (s.locale !== prev.locale) sendReason();
  });
}

/** Resolves when no flash write runs any more. */
export function writingDone(): Promise<void> {
  if (!useWriting.getState().kind) return Promise.resolve();
  return new Promise((resolve) => {
    const off = useWriting.subscribe((s) => {
      if (!s.kind) {
        off();
        resolve();
      }
    });
  });
}
