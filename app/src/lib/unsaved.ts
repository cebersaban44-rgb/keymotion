// Changes the user made in the app that are not stored on the keyboard yet.
// Every page with such state contributes one hook below; the close guard
// (UnsavedDialog) and the sidebar read the combined list. Keymap, macros and
// the screen studio plug in here when they exist.

import type { MessageKey } from "./i18n";
import { lightingUnsaved, useLighting } from "./lighting";
import type { PageId } from "./store";

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

/** Every area with unsaved changes, in sidebar order. */
export function useUnsaved(): UnsavedItem[] {
  const lighting = useLightingItem();
  return [lighting].filter((i): i is UnsavedItem => i !== null);
}
