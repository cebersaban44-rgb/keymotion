// The test guide for a keyboard of AJAZZ's "AA" family on the cable (the
// AK980 PRO on firmware 3.01, "769"): an optional walk through what
// Keymotion does with it, for owners who want to tell us how it works.
// This file holds its rules: the steps, when the invitation on the
// Overview shows, the progress kept on this computer, the checks Keymotion
// makes itself, and the text shared with the developers. The store and
// the side panel are in lib/aaGuide.ts and components/TestGuide.tsx; the
// words for the user are in the dictionaries (lib/i18n).
//
// The shared text is English whatever the UI language: the developers
// read it. Keymotion sends nothing by itself: the user sees the text,
// copies it and pastes it into GitHub's form.
//
// No imports: tools/aa-guide.test.mjs runs this file in Node.

/** The steps, in order. */
export const GUIDE_STEPS = ["prepare", "connection", "effect", "live", "perKey", "keymap", "macros", "screen", "restore"] as const;
export type GuideStep = (typeof GUIDE_STEPS)[number];

/** The summary after the last step (`GuideProgress.at`). */
export const SUMMARY = GUIDE_STEPS.length;

/** A step's answer. */
export type GuideResult = "worked" | "failed" | "skipped";

/** What a step's "Go there" opens; the guide stays open beside it. */
export type GuideGo =
  | { page: "connection"; backups?: true }
  | { page: "lighting"; tab: "effects" | "per-key" }
  | { page: "music" | "keymap" | "macros" }
  | { page: "screen"; tab: "visuals" };

export const GUIDE_GO: Record<GuideStep, GuideGo | null> = {
  prepare: null,
  connection: { page: "connection" },
  effect: { page: "lighting", tab: "effects" },
  live: { page: "music" },
  perKey: { page: "lighting", tab: "per-key" },
  keymap: { page: "keymap" },
  macros: { page: "macros" },
  screen: { page: "screen", tab: "visuals" },
  restore: { page: "connection", backups: true },
};

/** A save the keyboard read back as written (`AaStatus.readBack`). */
export type SaveKind = "effect" | "perKey" | "keymap" | "macros" | "restore";

/** What Keymotion checks itself on a step: a state of the keyboard, or a
 *  save of a kind that read back while the guide was open. Steps without
 *  one (live lighting, the screen) have nothing Keymotion could tell. */
export type GuideCheck = "backup" | "writable" | SaveKind;

export const GUIDE_CHECK: Record<GuideStep, GuideCheck | null> = {
  prepare: "backup",
  connection: "writable",
  effect: "effect",
  live: null,
  perKey: "perKey",
  keymap: "keymap",
  macros: "macros",
  screen: null,
  restore: "restore",
};

/** The longest note kept for a step. */
export const NOTE_MAX = 300;
/** "Not now" on the invitation: it comes back at the next connection
 *  until it was answered so this many times; then only the Connection
 *  page offers the guide. */
export const LATER_MAX = 2;
/** The longest shared text, well under GitHub's 65,536 characters for an
 *  issue (the diagnostics report's own limit is 20,000). */
export const SHARED_MAX = 12_000;

/** The progress, kept on this computer (it survives a restart). */
export interface GuideProgress {
  /** Each answered step's result. */
  results: Partial<Record<GuideStep, GuideResult>>;
  /** Each step's note, as typed (at most `NOTE_MAX` characters). */
  notes: Partial<Record<GuideStep, string>>;
  /** The save checks Keymotion saw hold while the guide was open. */
  checked: Partial<Record<GuideStep, true>>;
  /** The step the guide opens at; `SUMMARY`: the summary. */
  at: number;
  /** The guide was opened at least once. */
  opened: boolean;
  /** Times the invitation was answered with "Not now". */
  later: number;
  /** "Don't show again". */
  never: boolean;
  /** The results were copied to be shared. */
  shared: boolean;
}

export function emptyProgress(): GuideProgress {
  return { results: {}, notes: {}, checked: {}, at: 0, opened: false, later: 0, never: false, shared: false };
}

const RESULTS: readonly string[] = ["worked", "failed", "skipped"];
const isStep = (s: string): s is GuideStep => (GUIDE_STEPS as readonly string[]).includes(s);

/** Progress read back from storage; anything that doesn't fit is left
 *  out (a damaged or foreign value gives a fresh start, never an error). */
export function parseProgress(raw: unknown): GuideProgress {
  const p = emptyProgress();
  if (!raw || typeof raw !== "object") return p;
  const r = raw as Record<string, unknown>;
  const entries = (v: unknown) => (v && typeof v === "object" ? Object.entries(v as Record<string, unknown>) : []);
  for (const [k, v] of entries(r.results)) {
    if (isStep(k) && typeof v === "string" && RESULTS.includes(v)) p.results[k] = v as GuideResult;
  }
  for (const [k, v] of entries(r.notes)) {
    if (isStep(k) && typeof v === "string" && v) p.notes[k] = v.slice(0, NOTE_MAX);
  }
  for (const [k, v] of entries(r.checked)) {
    if (isStep(k) && v === true && GUIDE_CHECK[k]) p.checked[k] = true;
  }
  if (typeof r.at === "number" && Number.isInteger(r.at)) p.at = Math.min(Math.max(r.at, 0), SUMMARY);
  p.opened = r.opened === true;
  if (typeof r.later === "number" && Number.isInteger(r.later)) p.later = Math.min(Math.max(r.later, 0), LATER_MAX);
  p.never = r.never === true;
  p.shared = r.shared === true;
  return p;
}

// ------------------------------------------------------------ invitation

/** What the invitation looks at besides the progress. */
export interface InviteFacts {
  /** The keyboard the UI shows is of the "AA" family, on the cable. */
  aaOnCable: boolean;
  /** Keymotion can write to it (read, sane, backed up). */
  writable: boolean;
  /** "Not now" was answered since it was connected. */
  laterNow: boolean;
  /** The guide is open. */
  guideOpen: boolean;
}

/** The invitation on the Overview shows: only for an "AA" keyboard on the
 *  cable that Keymotion can write to, never once the guide was opened, the
 *  results shared, "Don't show again" answered or "Not now" answered
 *  `LATER_MAX` times, and not again before the next connection after
 *  "Not now". */
export function inviteShown(p: GuideProgress, f: InviteFacts): boolean {
  if (!f.aaOnCable || !f.writable || f.laterNow || f.guideOpen) return false;
  return !p.opened && !p.shared && !p.never && p.later < LATER_MAX;
}

/** After "Not now": the count goes up; whether it is the last time. */
export function answeredLater(p: GuideProgress): { progress: GuideProgress; last: boolean } {
  const later = Math.min(p.later + 1, LATER_MAX);
  return { progress: { ...p, later }, last: later >= LATER_MAX };
}

// --------------------------------------------------------------- progress

/** Steps answered so far. */
export function answered(p: GuideProgress): number {
  return GUIDE_STEPS.filter((s) => p.results[s]).length;
}

/** Steps whose answer is "didn't work". */
export function failedSteps(p: GuideProgress): number {
  return GUIDE_STEPS.filter((s) => p.results[s] === "failed").length;
}

/** What the Connection page says about the guide. */
export type GuideState = "new" | "partial" | "done" | "shared";

export function guideState(p: GuideProgress): GuideState {
  const n = answered(p);
  if (n === GUIDE_STEPS.length) return p.shared ? "shared" : "done";
  return n === 0 ? "new" : "partial";
}

/** `p` with `step` answered. */
export function withResult(p: GuideProgress, step: GuideStep, result: GuideResult): GuideProgress {
  return { ...p, results: { ...p.results, [step]: result } };
}

/** `p` with `step`'s note (cut to `NOTE_MAX`; an empty one is none). */
export function withNote(p: GuideProgress, step: GuideStep, note: string): GuideProgress {
  const notes = { ...p.notes };
  const text = note.slice(0, NOTE_MAX);
  if (text.trim()) notes[step] = text;
  else delete notes[step];
  return { ...p, notes };
}

/** Where "Next step" goes: the next step, or the summary after the last;
 *  `null` while the step has no answer ("Choose a result first"). */
export function nextAt(p: GuideProgress): number | null {
  const step = GUIDE_STEPS[p.at];
  if (!step) return null;
  return p.results[step] ? p.at + 1 : null;
}

/** Where the guide opens: the summary once every step is answered (its
 *  Back leads to the steps), else where it was left. */
export function openAt(p: GuideProgress): number {
  if (answered(p) === GUIDE_STEPS.length) return SUMMARY;
  if (p.at >= SUMMARY) return Math.max(0, GUIDE_STEPS.findIndex((s) => !p.results[s]));
  return p.at;
}

// ------------------------------------------------------------------ checks

/** What Keymotion knows about the keyboard now (`AaStatus`); `null`
 *  without an "AA" keyboard on the cable. */
export interface CheckFacts {
  /** The backup from its first connection exists. */
  backup: boolean;
  /** It was read, passed the check and takes writes. */
  writable: boolean;
  /** Unix ms of the last save of each kind that read back as written. */
  readBack: Partial<Record<SaveKind, number | null>>;
}

/** Whether `check` holds now: the keyboard's state, or a save of its kind
 *  that read back at `sinceMs` or later (the guide opened). */
export function checkHolds(check: GuideCheck, f: CheckFacts | null, sinceMs: number): boolean {
  if (!f) return false;
  if (check === "backup") return f.backup;
  if (check === "writable") return f.writable;
  const at = f.readBack[check];
  return typeof at === "number" && at >= sinceMs;
}

/** The steps whose save check holds now and isn't noted yet (the state
 *  checks are shown as they are, never noted). */
export function newlyChecked(p: GuideProgress, f: CheckFacts | null, sinceMs: number): GuideStep[] {
  return GUIDE_STEPS.filter((s) => {
    const c = GUIDE_CHECK[s];
    return c !== null && c !== "backup" && c !== "writable" && !p.checked[s] && checkHolds(c, f, sinceMs);
  });
}

/** The tick on a step: a state check while it holds, a save check once
 *  it was seen. Never one that didn't happen. */
export function checkShown(p: GuideProgress, step: GuideStep, f: CheckFacts | null): boolean {
  const c = GUIDE_CHECK[step];
  if (c === null) return false;
  if (c === "backup" || c === "writable") return checkHolds(c, f, 0);
  return p.checked[step] === true;
}

// ------------------------------------------------------------- shared text

const STEP_NAME: Record<GuideStep, string> = {
  prepare: "Getting ready",
  connection: "Connection",
  effect: "Lighting effect",
  live: "Live lighting",
  perKey: "Per-key color",
  keymap: "Key mapping",
  macros: "Macros",
  screen: "Screen (optional)",
  restore: "Restore",
};

const CHECK_TEXT: Record<GuideCheck, string> = {
  backup: "the backup from the first connection exists",
  writable: "the keyboard was read and takes writes",
  effect: "an effect save read back as written",
  perKey: "a per-key color save read back as written",
  keymap: "a key mapping save read back as written",
  macros: "a macro save read back as written",
  restore: "a restore read back as written",
};

const RESULT_TEXT: Record<GuideResult, string> = { worked: "worked", failed: "didn't work", skipped: "skipped" };

/** The parts of the diagnostics report (app/src-tauri/src/diagnostics.rs)
 *  the shared text takes: the app and Windows versions and the "AA"
 *  keyboard section, nothing else (not the log, not the data files). */
export function reportParts(report: string): { app: string | null; windows: string | null; aa: string[] } {
  const lines = report.split(/\r?\n/);
  const value = (prefix: string) => lines.find((l) => l.startsWith(prefix))?.slice(prefix.length).trim() || null;
  const aa: string[] = [];
  const start = lines.indexOf('"AA" keyboard:');
  if (start >= 0) {
    for (const line of lines.slice(start + 1)) {
      if (!line.startsWith("  ")) break;
      aa.push(line);
    }
  }
  return { app: value("App version: "), windows: value("Windows: "), aa };
}

/** The GitHub form's title and model for the shared results. */
export function guideIssue(model: string, firmware: string | null): { title: string; model: string } {
  const name = firmware ? `${model} firmware ${firmware}` : model;
  return { title: `[Keyboard] ${name} — test guide`, model: name };
}

/** One line of a note: its line breaks as spaces. */
const oneLine = (s: string) => s.replace(/\s+/g, " ").trim().slice(0, NOTE_MAX);

/** What the shared text is made of. */
export interface ShareInput {
  model: string;
  firmware: string | null;
  progress: GuideProgress;
  /** The tick each step shows (`checkShown`). */
  checks: Partial<Record<GuideStep, boolean>>;
  /** The diagnostics report; only `reportParts` of it go in. */
  report: string;
}

/** The text the user shares: each step's result, note and Keymotion's own
 *  check, the app and Windows versions and the "AA" keyboard section of
 *  the diagnostics report (which never holds macro contents). English, at
 *  most `SHARED_MAX` characters. */
export function sharedText(i: ShareInput): string {
  const { app, windows, aa } = reportParts(i.report);
  const out = [`Keymotion test guide: ${i.model}, firmware ${i.firmware ?? "unknown"}`, ""];
  GUIDE_STEPS.forEach((step, n) => {
    const result = i.progress.results[step];
    let line = `${n + 1}. ${STEP_NAME[step]}: ${result ? RESULT_TEXT[result] : "not answered"}.`;
    const check = GUIDE_CHECK[step];
    if (check && i.checks[step]) line += ` Checked by Keymotion: ${CHECK_TEXT[check]}.`;
    const note = oneLine(i.progress.notes[step] ?? "");
    if (note) line += ` Note: ${note}`;
    out.push(line);
  });
  out.push("", `App version: ${app ?? "unknown"}`, `Windows: ${windows ?? "unknown"}`, "");
  out.push('"AA" keyboard:', ...(aa.length ? aa : ["  not connected when this text was made"]));
  const text = out.join("\n") + "\n";
  return text.length > SHARED_MAX ? `${text.slice(0, SHARED_MAX - 40)}\n[… ${text.length - SHARED_MAX + 40} characters left out]\n` : text;
}
