// Reporting problems on GitHub. The app sends nothing by itself: the
// report dialog shows the text first, puts it on the clipboard and opens
// GitHub's issue form in the browser.
//
// Offered from: Settings, a page that failed to draw (ErrorBoundary), an
// unexpected error (errors.ts; each one once a session), a crash note from
// an earlier run (once), and a keyboard that isn't supported yet (Welcome).

import { useEffect, useState } from "react";
import { create } from "zustand";
import type { UnsupportedKeyboard } from "../bindings/UnsupportedKeyboard";
import { follow } from "./follow";
import { getUnsupportedKeyboards, onUnsupportedChanged, unsupportedReport } from "./ipc";

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

/**
 * AJAZZ's newest firmware for the AK980 PRO ("AK980 Led v2 Firmware 769",
 * 3.01) moves the keyboard to another command set, under its own USB id
 * (0C45:8039). Keymotion doesn't speak it yet, so the backend lists the
 * keyboard as unknown and sends it nothing; the page says what it is.
 */
export function isNewAk980Firmware(k: UnsupportedKeyboard): boolean {
  const [vid, pid] = k.id.toUpperCase().split(":");
  return vid === "0C45" && pid === "8039" && /AK980\s*PRO/i.test(k.name ?? "");
}

/** The firmware's version ("3.01") from the keyboard's USB release number,
 *  read from Windows for the support request; `null` until it is read or
 *  when it can't be. */
export function useFirmwareVersion(id: string | null): string | null {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    setVersion(null);
    if (!id) return;
    let live = true;
    unsupportedReport(id)
      .then((text) => {
        const m = /release 0x([0-9A-F]{2})([0-9A-F]{2})/i.exec(text);
        if (live && m) setVersion(`${parseInt(m[1]!, 16)}.${m[2]}`);
      })
      .catch(console.error);
    return () => {
      live = false;
    };
  }, [id]);
  return version;
}

/** Keyboards that look like SONiX or AJAZZ ones but aren't supported yet. */
export function useUnsupportedKeyboards(): UnsupportedKeyboard[] {
  const [list, setList] = useState<UnsupportedKeyboard[]>([]);
  useEffect(() => follow(getUnsupportedKeyboards, onUnsupportedChanged, setList), []);
  return list;
}
