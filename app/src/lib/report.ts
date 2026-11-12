// Reporting problems on GitHub (docs/yol-haritasi.md, Aşama 4). The app
// sends nothing by itself: the report dialog shows the text first, puts it
// on the clipboard and opens GitHub's issue form in the browser.
//
// Offered from: Settings, a page that failed to draw (ErrorBoundary), an
// unexpected error (errors.ts; each one once a session), a crash note from
// an earlier run (once), and a keyboard that isn't supported yet (Welcome).

import { useEffect, useState } from "react";
import { create } from "zustand";
import type { UnsupportedKeyboard } from "../bindings/UnsupportedKeyboard";
import { getUnsupportedKeyboards, onUnsupportedChanged } from "./ipc";

export interface ReportRequest {
  /** A problem (the "bug" form) or a keyboard support request. */
  kind: "bug" | "keyboard";
  /** The issue's title; empty keeps the form's own. Issues are English. */
  title: string;
  /** What happened, for a problem: an error, a crash note. */
  context?: string;
  /** The unsupported keyboard's id, for a support request. */
  keyboardId?: string;
  /** Its name, for the form's model field. */
  model?: string;
}

interface ReportStore {
  /** The report dialog's request, while it is open. */
  request: ReportRequest | null;
  /** An unexpected error (its original text) offered for reporting. */
  problem: string | null;
  openReport: (request: ReportRequest) => void;
  closeReport: () => void;
  dismissProblem: () => void;
}

export const useReport = create<ReportStore>((set) => ({
  request: null,
  problem: null,
  openReport: (request) => set({ request, problem: null }),
  closeReport: () => set({ request: null }),
  dismissProblem: () => set({ problem: null }),
}));

/** Errors already offered this session, by their text without numbers. */
const offered = new Set<string>();

/** Offer an unexpected error for reporting, once a session. */
export function offerProblem(raw: string): void {
  const key = raw.replace(/\d+/g, "#").slice(0, 300);
  if (offered.has(key)) return;
  offered.add(key);
  if (!useReport.getState().request) useReport.setState({ problem: raw });
}

const TITLE_TEXT = 90;

/** "[Problem] …" with the first line of what happened. */
export function problemTitle(what: string): string {
  const line = what.split("\n").find((l) => l.trim())?.trim() ?? "";
  const short = line.length > TITLE_TEXT ? `${line.slice(0, TITLE_TEXT - 1)}…` : line;
  return short ? `[Problem] ${short}` : "";
}

/** The title for a crash note: its message line. */
export function crashTitle(note: string): string {
  const message = /^Message: (.*)$/m.exec(note)?.[1] ?? "";
  return problemTitle(`Crash: ${message || "no message"}`);
}

/** Keyboards that look like SONiX or AJAZZ ones but aren't supported yet. */
export function useUnsupportedKeyboards(): UnsupportedKeyboard[] {
  const [list, setList] = useState<UnsupportedKeyboard[]>([]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    getUnsupportedKeyboards().then((l) => !gone && setList(l), console.error);
    onUnsupportedChanged((l) => setList(l)).then(
      (off) => (gone ? off() : (stop = off)),
      console.error,
    );
    return () => {
      gone = true;
      stop?.();
    };
  }, []);
  return list;
}
