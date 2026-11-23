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
import { firmwareVersion, isAk980Candidate, isNewAk980Firmware, parseDetails, type KeyboardDetails } from "./newFirmware";

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

/** Each AK980 PRO candidate's support details, read once while it is
 *  plugged in (lib/newFirmware.ts). */
const useDetails = create<{ byId: Record<string, KeyboardDetails | null> }>(() => ({ byId: {} }));
const asked = new Set<string>();

/** Read the details of the keyboards that may run AJAZZ's new firmware; a
 *  keyboard that went away is read again when it comes back (its firmware
 *  may have changed). */
function readDetails(list: UnsupportedKeyboard[]) {
  const here = new Set(list.map((k) => k.id));
  const gone = [...asked].filter((id) => !here.has(id));
  if (gone.length) {
    gone.forEach((id) => asked.delete(id));
    useDetails.setState((s) => ({ byId: Object.fromEntries(Object.entries(s.byId).filter(([id]) => here.has(id))) }));
  }
  for (const k of list) {
    if (!isAk980Candidate(k) || asked.has(k.id)) continue;
    asked.add(k.id);
    unsupportedReport(k.id)
      .then(parseDetails, () => null)
      .then((d) => useDetails.setState((s) => ({ byId: { ...s.byId, [k.id]: d } })));
  }
}

export interface UnsupportedView {
  /** Every keyboard that looks like a SONiX or AJAZZ one but isn't supported. */
  list: UnsupportedKeyboard[];
  /** AK980 PROs on AJAZZ's new firmware, with its version when known. */
  newFirmware: { keyboard: UnsupportedKeyboard; version: string | null }[];
  /** The rest (an AK980 PRO whose details are still being read waits). */
  others: UnsupportedKeyboard[];
}

/** Keyboards that look like SONiX or AJAZZ ones but aren't supported yet,
 *  with the AK980 PROs on AJAZZ's new firmware told apart. */
export function useUnsupported(): UnsupportedView {
  const [list, setList] = useState<UnsupportedKeyboard[]>([]);
  useEffect(() => follow(getUnsupportedKeyboards, onUnsupportedChanged, setList), []);
  useEffect(() => readDetails(list), [list]);
  const byId = useDetails((s) => s.byId);
  const fresh = list.filter((k) => isNewAk980Firmware(k, byId[k.id]));
  return {
    list,
    newFirmware: fresh.map((k) => ({ keyboard: k, version: firmwareVersion(byId[k.id]?.release) })),
    others: list.filter((k) => !fresh.includes(k) && (!isAk980Candidate(k) || k.id in byId)),
  };
}
