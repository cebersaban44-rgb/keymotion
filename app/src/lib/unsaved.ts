// Changes the user made in the app that are not stored on the keyboard yet.
// Every page with such state contributes one hook below; the close guard
// (UnsavedDialog) and the sidebar read the combined list. A running screen
// upload counts too: closing would leave the GIF page half written. So does
// any other flash write the app runs by itself (a live card, going back to
// the own picture), though the sidebar does not mark it.

import { useEffect } from "react";
import type { MessageKey } from "./i18n";
import { dirtyLayers, useKeymap } from "./keymap";
import { dirtySlots, useMacros } from "./macros";
import { lightingUnsaved, useLighting } from "./lighting";
import { useScreen } from "./screen";
import type { PageId } from "./store";
import { startWriting, useWriting, writingDone } from "./writing";

export interface UnsavedItem {
  id: string;
  page: PageId;
  /** Page name. */
  area: MessageKey;
  /** What is pending, in one line. */
  detail: MessageKey;
  /** Saving writes to the keyboard over USB. */
  needsUsb: boolean;
  /** Store the pending changes on the keyboard. */
  save: () => Promise<void>;
  /** Drop the pending changes. */
  discard: () => void;
  /** A write the app runs by itself: closing asks, the sidebar shows no mark. */
  background?: boolean;
}

/** Lighting: the most recently edited setup (an effect or a per-key design). */
function useLightingItem(): UnsavedItem | null {
  const pending = useLighting(lightingUnsaved);
  if (!pending) return null;
  return {
    id: `lighting:${pending}`,
    page: "lighting",
    area: "nav.lighting",
    detail: pending === "effect" ? "unsaved.lighting.effect" : "unsaved.lighting.per-key",
    needsUsb: true,
    save: () => {
      const s = useLighting.getState();
      return pending === "effect" ? s.applyEffectDraft() : s.savePerKeyDraft();
    },
    discard: () => useLighting.getState().discardDraft(),
  };
}

/** Keymap: layers edited but not written to the keyboard. */
function useKeymapItem(): UnsavedItem | null {
  const dirty = useKeymap((s) => dirtyLayers(s).join(","));
  if (!dirty) return null;
  return {
    id: `keymap:${dirty}`,
    page: "keymap",
    area: "nav.keymap",
    detail: "unsaved.keymap",
    needsUsb: true,
    save: () => useKeymap.getState().save(),
    discard: () => useKeymap.getState().discard(),
  };
}

/** Macros: edits not written to the keyboard. */
function useMacrosItem(): UnsavedItem | null {
  const dirty = useMacros((s) => dirtySlots(s).length > 0);
  if (!dirty) return null;
  return {
    id: "macros",
    page: "macros",
    area: "nav.macros",
    detail: "unsaved.macros",
    needsUsb: true,
    save: () => useMacros.getState().save(),
    discard: () => useMacros.getState().discard(),
  };
}

/** Screen: an upload is running. "Save" waits for it to finish. */
function useScreenItem(): UnsavedItem | null {
  const uploading = useScreen((s) => s.upload !== null);
  if (!uploading) return null;
  return {
    id: "screen",
    page: "screen",
    area: "nav.screen",
    detail: "unsaved.screen",
    needsUsb: true,
    save: () => useScreen.getState().running ?? Promise.resolve(),
    discard: () => {},
  };
}

/** Any other flash write in progress (the backend's `flash-write`): a live
 *  card, going back to the own picture, or a short settings write. "Save"
 *  waits for it to end; it cannot be undone. */
function useWriteItem(manualUpload: boolean): UnsavedItem | null {
  useEffect(startWriting, []);
  const kind = useWriting((s) => s.kind);
  // A manual upload has its own item above.
  if (!kind || (kind === "screen" && manualUpload)) return null;
  return {
    id: `write:${kind}`,
    page: kind === "screen" ? "screen" : "overview",
    area: kind === "screen" ? "nav.screen" : "unsaved.write.area",
    detail: kind === "screen" ? "unsaved.screen" : "unsaved.write",
    needsUsb: false,
    save: writingDone,
    discard: () => {},
    background: true,
  };
}

/** Every area with unsaved changes, in sidebar order. */
export function useUnsaved(): UnsavedItem[] {
  const lighting = useLightingItem();
  const keymap = useKeymapItem();
  const macros = useMacrosItem();
  const screen = useScreenItem();
  const write = useWriteItem(screen !== null);
  return [lighting, keymap, macros, screen, write].filter((i): i is UnsavedItem => i !== null);
}
