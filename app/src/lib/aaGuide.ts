// The test guide for a keyboard of AJAZZ's "AA" family on the cable (rules:
// lib/aaGuideRules.ts; the panel and its two ways in:
// components/TestGuide.tsx). Here: its store, kept on this computer, and
// what follows the keyboard while the app runs.
//
// - The progress (answers, notes, the checks seen, the invitation's
//   answers) is in the webview's local storage, as the UI's other
//   preferences: it survives a restart, never leaves the computer and is
//   shared only when the user copies the results. It belongs to one model
//   on one firmware: another firmware starts afresh (`progressFor`).
// - The panel stays open beside every page: "Go there" opens a page, the
//   guide waits at the side.
// - The guide writes nothing to the keyboard. It only opens the pages that
//   do, and the user saves there.

import { useEffect } from "react";
import { create } from "zustand";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { aaStatusOf } from "./aa";
import {
  answeredLater,
  emptyProgress,
  inviteShown,
  newlyChecked,
  openAt,
  parseProgress,
  progressFor,
  withNote,
  withResult,
  type CheckFacts,
  type GuideGo,
  type GuideProgress,
  type GuideResult,
  type GuideStep,
} from "./aaGuideRules";
import { useLighting } from "./lighting";
import { useActiveKeyboard, useApp } from "./store";

const KEY = "ks.aaGuide.v1";

function load(): GuideProgress {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? parseProgress(JSON.parse(raw)) : emptyProgress();
  } catch {
    return emptyProgress();
  }
}

function persist(p: GuideProgress) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    /* storage unavailable: the progress lasts until the app closes */
  }
}

interface GuideStore {
  progress: GuideProgress;
  /** The panel is open. */
  open: boolean;
  /** When it opened: only saves from then on are its own checks. */
  openedAtMs: number;
  /** "Not now" was answered since the keyboard was connected. */
  laterNow: boolean;
  /** "The guide is closed, your progress is kept", for a moment. */
  closedNotice: boolean;
  /** Bumped when "Open the guide" is clicked while it is open: the panel
   *  draws the eye to itself. */
  attention: number;
}

export const useGuide = create<GuideStore>(() => ({
  progress: load(),
  open: false,
  openedAtMs: 0,
  laterNow: false,
  closedNotice: false,
  attention: 0,
}));

function update(change: (p: GuideProgress) => GuideProgress) {
  const progress = change(useGuide.getState().progress);
  persist(progress);
  useGuide.setState({ progress });
}

/** Open the panel where the guide was left (the summary once every step
 *  is answered). */
export function openGuide() {
  if (useGuide.getState().open) {
    useGuide.setState((s) => ({ attention: s.attention + 1 }));
    return;
  }
  update((p) => ({ ...p, opened: true, at: openAt(p) }));
  useGuide.setState({ open: true, openedAtMs: Date.now(), closedNotice: false });
}

/** Close the panel; the progress stays. `notice`: say so for a moment
 *  (Cancel, not Close after the summary). */
export function closeGuide(notice: boolean) {
  useGuide.setState({ open: false, closedNotice: notice });
}

export function answerStep(step: GuideStep, result: GuideResult) {
  update((p) => withResult(p, step, result));
}

export function noteStep(step: GuideStep, note: string) {
  update((p) => withNote(p, step, note));
}

/** Show step `at` (`SUMMARY`: the summary). */
export function showStep(at: number) {
  update((p) => ({ ...p, at }));
}

/** "Not now" on the invitation; whether it was the last time it shows. */
export function inviteLater(): boolean {
  const { progress, last } = answeredLater(useGuide.getState().progress);
  persist(progress);
  useGuide.setState({ progress, laterNow: true });
  return last;
}

/** "Don't show again" on the invitation. */
export function inviteNever() {
  update((p) => ({ ...p, never: true }));
}

/** The results were copied: the invitation never shows again. */
export function markShared() {
  update((p) => ({ ...p, shared: true }));
}

/** Open the page a step's "Go there" names; the panel stays open. */
export function goThere(go: GuideGo) {
  const app = useApp.getState();
  if (go.page === "lighting") {
    useLighting.getState().setTab(go.tab);
    app.setPage("lighting");
  } else if (go.page === "screen") {
    app.openScreen(go.tab);
  } else {
    app.setPage(go.page);
  }
  if (go.page === "connection" && go.backups) scrollWhenShown("[data-keyboard-backups]");
}

/** Scroll `selector` into view once the page that holds it has drawn (a
 *  page change fades the old one out first). */
function scrollWhenShown(selector: string) {
  let tries = 0;
  const look = () => {
    const el = document.querySelector(selector);
    if (el) el.scrollIntoView({ block: "start", behavior: "smooth" });
    else if (++tries < 20) window.setTimeout(look, 100);
  };
  window.setTimeout(look, 250);
}

/** What the guide's checks look at, from the keyboard the UI shows. */
export function checkFacts(kb: KeyboardInfo | null | undefined): CheckFacts | null {
  const s = kb?.protocol === "ajazz-aa" ? aaStatusOf(kb) : null;
  if (!s) return null;
  return { backup: s.backup !== null, writable: s.readOnly === null, readBack: s.readBack };
}

/** An "AA" keyboard on the cable that Keymotion can write to. */
function writableAa(kb: KeyboardInfo | null): { aaOnCable: boolean; writable: boolean } {
  const aaOnCable = kb?.protocol === "ajazz-aa" && kb.connection === "usb";
  return { aaOnCable, writable: aaOnCable && !!kb?.aaStatus && kb.aaStatus.readOnly === null };
}

/** Whether the invitation shows on the Overview now. */
export function useGuideInvite(): boolean {
  const kb = useActiveKeyboard();
  const progress = useGuide((s) => s.progress);
  const laterNow = useGuide((s) => s.laterNow);
  const guideOpen = useGuide((s) => s.open);
  return inviteShown(progress, { ...writableAa(kb), laterNow, guideOpen });
}

/** The panel's width (components/TestGuide.tsx), and where a notice in the
 *  window's bottom or top right corner goes meanwhile: left of the panel,
 *  never over its buttons. */
export const GUIDE_WIDTH = "w-[340px]";

export function useCornerRight(): string {
  return useGuide((s) => s.open) ? "right-[364px]" : "right-6";
}

/** For the whole session (App): the progress follows the "AA" keyboard on
 *  the cable (another firmware starts afresh), "Not now" lasts until the
 *  keyboard is connected again, and while the panel is open the saves that
 *  read back are noted as its checks. */
export function useGuideFollow() {
  const kb = useActiveKeyboard();
  const onCable = kb?.protocol === "ajazz-aa" && kb.connection === "usb";
  useEffect(() => {
    if (!onCable && useGuide.getState().laterNow) useGuide.setState({ laterNow: false });
  }, [onCable]);
  const modelId = onCable ? kb.modelId : null;
  const model = onCable ? kb.modelName : null;
  const firmware = onCable ? (kb.firmware?.trim() ?? "") : "";
  useEffect(() => {
    if (!modelId || !model || !firmware) return;
    const now = useGuide.getState().progress;
    const next = progressFor(now, { modelId, model, firmware });
    if (next !== now) update(() => next);
  }, [modelId, model, firmware]);
  const open = useGuide((s) => s.open);
  const since = useGuide((s) => s.openedAtMs);
  const facts = checkFacts(kb);
  const key = JSON.stringify(facts?.readBack ?? null);
  useEffect(() => {
    if (!open) return;
    const seen = newlyChecked(useGuide.getState().progress, checkFacts(kb), since);
    if (seen.length) update((p) => ({ ...p, checked: { ...p.checked, ...Object.fromEntries(seen.map((s) => [s, true])) } }));
    // The read-back times are what changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, since, key]);
}
