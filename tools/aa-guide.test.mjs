// node --test tools/aa-guide.test.mjs
// The test guide for the AK980 PRO on firmware 3.01 (app/src/lib/
// aaGuideRules.ts; Node runs the TypeScript file as it is): when the
// invitation shows, the progress kept on this computer, the checks
// Keymotion makes itself, and the English text the user shares.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  FORM_DETAILS,
  FORM_FAILS,
  FORM_QUESTION,
  FORM_WORKS,
  GUIDE_CHECK,
  GUIDE_GO,
  GUIDE_STEPS,
  LATER_MAX,
  NOTE_MAX,
  SHARED_MAX,
  SUMMARY,
  answered,
  answeredLater,
  checkHolds,
  checkShown,
  emptyProgress,
  failedSteps,
  formAnswer,
  guideIssue,
  guideState,
  inviteShown,
  newlyChecked,
  nextAt,
  openAt,
  parseProgress,
  progressFor,
  reportParts,
  sharedKeyboard,
  sharedText,
  withNote,
  withResult,
} from "../app/src/lib/aaGuideRules.ts";

const ready = { aaOnCable: true, writable: true, laterNow: false, guideOpen: false };

/** Every step answered with `result`. */
const all = (result) => GUIDE_STEPS.reduce((p, s) => withResult(p, s, result), emptyProgress());

test("nine steps, each with what it opens and what Keymotion checks itself", () => {
  assert.deepEqual(GUIDE_STEPS, ["prepare", "connection", "effect", "live", "perKey", "keymap", "macros", "screen", "restore"]);
  assert.equal(SUMMARY, 9);
  assert.equal(GUIDE_GO.prepare, null);
  assert.deepEqual(GUIDE_GO.effect, { page: "lighting", tab: "effects" });
  assert.deepEqual(GUIDE_GO.perKey, { page: "lighting", tab: "per-key" });
  assert.deepEqual(GUIDE_GO.restore, { page: "connection", backups: true });
  // Nothing Keymotion could tell by itself: live lighting, the screen.
  assert.equal(GUIDE_CHECK.live, null);
  assert.equal(GUIDE_CHECK.screen, null);
  assert.equal(GUIDE_CHECK.prepare, "backup");
  assert.equal(GUIDE_CHECK.connection, "writable");
});

test("the invitation: only an AA keyboard on the cable that takes writes", () => {
  const p = emptyProgress();
  assert.equal(inviteShown(p, ready), true);
  assert.equal(inviteShown(p, { ...ready, aaOnCable: false }), false, "1.06 (FF13), or not on the cable");
  assert.equal(inviteShown(p, { ...ready, writable: false }), false, "read-only");
  assert.equal(inviteShown(p, { ...ready, guideOpen: true }), false, "the guide is open");
});

test("the invitation: Not now twice, then only the Connection page", () => {
  let p = emptyProgress();
  let r = answeredLater(p);
  p = r.progress;
  assert.equal(r.last, false);
  assert.equal(inviteShown(p, { ...ready, laterNow: true }), false, "not again on this connection");
  assert.equal(inviteShown(p, ready), true, "again at the next connection");
  r = answeredLater(p);
  p = r.progress;
  assert.equal(r.last, true);
  assert.equal(p.later, LATER_MAX);
  assert.equal(inviteShown(p, ready), false, "never again");
  assert.equal(answeredLater(p).progress.later, LATER_MAX, "the count stops");
});

test("the invitation: never after Don't show again, opening the guide or sharing", () => {
  assert.equal(inviteShown({ ...emptyProgress(), never: true }, ready), false);
  assert.equal(inviteShown({ ...emptyProgress(), opened: true }, ready), false);
  assert.equal(inviteShown({ ...emptyProgress(), shared: true }, ready), false);
});

test("progress: answers, notes, where Next goes and where the guide opens", () => {
  let p = emptyProgress();
  assert.equal(guideState(p), "new");
  assert.equal(nextAt(p), null, "Choose a result first");
  p = withResult(p, "prepare", "worked");
  assert.equal(nextAt(p), 1);
  assert.equal(guideState(p), "partial");
  assert.equal(answered(p), 1);
  p = withNote(p, "prepare", "x".repeat(NOTE_MAX + 50));
  assert.equal(p.notes.prepare.length, NOTE_MAX);
  p = withNote(p, "prepare", "   ");
  assert.equal(p.notes.prepare, undefined, "an empty note is none");

  p = { ...all("worked"), at: 4 };
  assert.equal(guideState(p), "done");
  assert.equal(openAt(p), SUMMARY, "every step answered: the summary");
  assert.equal(guideState({ ...p, shared: true }), "shared");
  // Left on the summary with a step unanswered (an older progress): the
  // first one without an answer.
  const gap = { ...p, results: { ...p.results, keymap: undefined }, at: SUMMARY };
  delete gap.results.keymap;
  assert.equal(openAt(gap), GUIDE_STEPS.indexOf("keymap"));
  assert.equal(openAt({ ...emptyProgress(), at: 3 }), 3, "where it was left");
  assert.equal(nextAt({ ...p, at: SUMMARY }), null);
  assert.equal(failedSteps(withResult(withResult(p, "macros", "failed"), "screen", "skipped")), 1);
});

test("progress read back from storage: anything that doesn't fit is left out", () => {
  for (const raw of [null, undefined, 3, "x", [], { results: 5 }]) assert.deepEqual(parseProgress(raw), emptyProgress());
  const p = parseProgress({
    results: { prepare: "worked", effect: "maybe", nope: "worked", macros: "failed" },
    notes: { macros: "y".repeat(500), other: "z", live: 7 },
    checked: { effect: true, live: true, prepare: "yes" },
    at: 42,
    opened: true,
    later: 9,
    never: "true",
    shared: true,
  });
  assert.deepEqual(p.results, { prepare: "worked", macros: "failed" });
  assert.equal(p.notes.macros.length, NOTE_MAX);
  assert.deepEqual(Object.keys(p.notes), ["macros"]);
  assert.deepEqual(p.checked, { effect: true }, "only steps that have a check");
  assert.equal(p.at, SUMMARY);
  assert.equal(p.later, LATER_MAX);
  assert.equal(p.opened, true);
  assert.equal(p.never, false);
  assert.equal(p.shared, true);
  // What JSON gives back is what was kept.
  const kept = { ...all("skipped"), notes: { live: "ok" }, checked: { keymap: true }, at: 5, later: 1 };
  assert.deepEqual(parseProgress(JSON.parse(JSON.stringify(kept))), kept);
});

test("Keymotion's own checks: only what happened, only while the guide was open", () => {
  const since = 1_000;
  const facts = { backup: true, writable: true, readBack: { effect: 999, perKey: 1_000, keymap: null, macros: 2_000 } };
  assert.equal(checkHolds("backup", facts, since), true);
  assert.equal(checkHolds("writable", { ...facts, writable: false }, since), false);
  assert.equal(checkHolds("effect", facts, since), false, "saved before the guide opened");
  assert.equal(checkHolds("perKey", facts, since), true);
  assert.equal(checkHolds("keymap", facts, since), false, "never saved");
  assert.equal(checkHolds("restore", facts, since), false, "not reported");
  assert.equal(checkHolds("backup", null, since), false, "no keyboard on the cable");

  let p = emptyProgress();
  assert.deepEqual(newlyChecked(p, facts, since), ["perKey", "macros"], "save checks only");
  p = { ...p, checked: { perKey: true } };
  assert.deepEqual(newlyChecked(p, facts, since), ["macros"]);
  // A save check stays once seen (the keyboard may be unplugged in between);
  // a state check shows only while it holds; a step without one never.
  assert.equal(checkShown(p, "perKey", null), true);
  assert.equal(checkShown(p, "macros", facts), false, "not noted yet");
  assert.equal(checkShown(p, "prepare", facts), true);
  assert.equal(checkShown(p, "prepare", null), false);
  assert.equal(checkShown(p, "connection", { ...facts, writable: false }), false);
  assert.equal(checkShown({ ...p, checked: { live: true } }, "live", facts), false);
});

/** A diagnostics report shaped like app/src-tauri/src/diagnostics.rs's. */
const REPORT = [
  "Keymotion diagnostics",
  "Created: 2026-11-25T10:00:00Z",
  "App version: 1.3.0-beta.3",
  "Windows: Windows 11 Home 25H2 (build 26200.7171)",
  "WebView2: 142.0.3595.80",
  "UI language: tr",
  "",
  "Keyboards:",
  '  AJAZZ AK980 PRO (ajazz-ak980-pro), usb, firmware 3.01, product "AK980 PRO"',
  "",
  '"AA" keyboard:',
  "  ajazz-ak980-pro.aa on USB 8039, firmware 3.01",
  "  full read: done, passed the sanity check",
  "  writes: allowed",
  "  saves read back as written since connected: effect, keymap",
  "",
  "Data files:",
  "  macros.json (1.1 KB)",
  "",
  "Last 2 log lines (addresses and user names removed):",
  "2026-11-25T10:00:01Z  INFO macro saved text=hello secret world",
  "2026-11-25T10:00:02Z  INFO lighting saved mode=wave",
  "",
].join("\n");

test("the report's parts the shared text takes: versions and the AA section only", () => {
  const parts = reportParts(REPORT);
  assert.equal(parts.app, "1.3.0-beta.3");
  assert.equal(parts.windows, "Windows 11 Home 25H2 (build 26200.7171)");
  assert.deepEqual(parts.aa, [
    "  ajazz-ak980-pro.aa on USB 8039, firmware 3.01",
    "  full read: done, passed the sanity check",
    "  writes: allowed",
    "  saves read back as written since connected: effect, keymap",
  ]);
  assert.deepEqual(reportParts("Keymotion diagnostics\r\nApp version: 1.0.0\r\n"), { app: "1.0.0", windows: null, aa: [] });
});

test("the shared text: results, notes, checks, versions and the AA section; nothing else", () => {
  let progress = all("worked");
  progress = withResult(progress, "macros", "failed");
  progress = withResult(progress, "screen", "skipped");
  progress = withNote(progress, "macros", "Typed half of it\nafter the cable came back");
  const checks = { prepare: true, connection: true, effect: true, keymap: true, live: true };
  const text = sharedText({ model: "AJAZZ AK980 PRO", firmware: "3.01", progress, checks, report: REPORT });
  const lines = text.split("\n");
  assert.equal(lines[0], "Keymotion test guide: AJAZZ AK980 PRO, firmware 3.01");
  assert.ok(lines.includes("1. Getting ready: worked. Checked by Keymotion: the backup from the first connection exists."), text);
  assert.ok(lines.includes("3. Lighting effect: worked. Checked by Keymotion: an effect save read back as written."), text);
  assert.ok(lines.includes("4. Live lighting: worked."), "a step without a check never claims one");
  assert.ok(lines.includes("5. Per-key color: worked."), "a check that didn't happen isn't claimed");
  assert.ok(lines.includes("7. Macros: didn't work. Note: Typed half of it after the cable came back"), text);
  assert.ok(lines.includes("8. Screen (optional): skipped."), text);
  assert.ok(lines.includes("App version: 1.3.0-beta.3"));
  assert.ok(lines.includes("Windows: Windows 11 Home 25H2 (build 26200.7171)"));
  assert.ok(text.includes('"AA" keyboard:\n  ajazz-ak980-pro.aa on USB 8039, firmware 3.01\n'), text);
  // Not the log (where a macro's text could be), the data files, the UI
  // language or the keyboard list.
  for (const no of ["secret", "macros.json", "Data files", "UI language", "Last 2 log lines", "product"]) {
    assert.ok(!text.includes(no), `${no}:\n${text}`);
  }
  assert.ok(!text.includes("not answered"));
  assert.ok(!/[ğüşıöçĞÜŞİÖÇ]/.test(text.replace(/Note: .*/g, "")), "English, whatever the UI language");
});

test("the shared text without an AA section, and never too long", () => {
  const short = sharedText({ model: "AJAZZ AK980 PRO", firmware: null, progress: emptyProgress(), checks: {}, report: "" });
  assert.ok(short.startsWith("Keymotion test guide: AJAZZ AK980 PRO, firmware unknown\n"));
  assert.ok(short.includes("1. Getting ready: not answered."));
  assert.ok(short.includes('"AA" keyboard:\n  not connected when this text was made\n'));
  assert.ok(short.includes("App version: unknown"));

  // The longest notes and the backend's longest AA section (6,000
  // characters) stay well under the limit.
  let progress = all("failed");
  for (const s of GUIDE_STEPS) progress = withNote(progress, s, "n".repeat(NOTE_MAX));
  const aa = Array.from({ length: 10 }, (_, i) => `  line ${i} ${"x".repeat(590)}`);
  const report = ['App version: 1.3.0', '"AA" keyboard:', ...aa, ""].join("\n");
  const full = sharedText({ model: "AJAZZ AK980 PRO", firmware: "3.01", progress, checks: {}, report });
  assert.ok(full.length < SHARED_MAX, `${full.length}`);
  assert.ok(full.length < 20_000);
  // And a report far past it is cut, saying so.
  const huge = ['"AA" keyboard:', ...Array.from({ length: 100 }, () => `  ${"y".repeat(500)}`)].join("\n");
  const cut = sharedText({ model: "AJAZZ AK980 PRO", firmware: "3.01", progress, checks: {}, report: huge });
  assert.ok(cut.length <= SHARED_MAX + 60, `${cut.length}`);
  assert.match(cut, /\[… \d+ characters left out\]\n$/);
});

test("GitHub's form: the title and model, the body goes by the clipboard", () => {
  assert.deepEqual(guideIssue("AJAZZ AK980 PRO", "3.01"), {
    title: "[Keyboard] AJAZZ AK980 PRO firmware 3.01 — test guide",
    model: "AJAZZ AK980 PRO firmware 3.01",
  });
  assert.deepEqual(guideIssue("AJAZZ AK980 PRO", null), {
    title: "[Keyboard] AJAZZ AK980 PRO — test guide",
    model: "AJAZZ AK980 PRO",
  });
});

test("GitHub's form: the question, its answers and the box are the form's own words", () => {
  const form = readFileSync(new URL("../.github/ISSUE_TEMPLATE/keyboard.yml", import.meta.url), "utf8");
  for (const words of [FORM_QUESTION, FORM_WORKS, FORM_FAILS, FORM_DETAILS]) assert.ok(form.includes(words), words);
  assert.equal(formAnswer(all("worked")), FORM_WORKS);
  assert.equal(formAnswer(withResult(all("worked"), "screen", "skipped")), FORM_WORKS);
  assert.equal(formAnswer(withResult(all("worked"), "macros", "failed")), FORM_FAILS);
});

const K301 = { modelId: "ajazz-ak980-pro", model: "AJAZZ AK980 PRO", firmware: "3.01" };

test("progress belongs to a model on a firmware: another one starts afresh", () => {
  // The first keyboard met is noted; the same one keeps everything.
  let p = withNote(withResult({ ...emptyProgress(), at: 3, opened: true, later: 1 }, "prepare", "worked"), "prepare", "ok");
  p = progressFor(p, K301);
  assert.deepEqual(p.keyboard, K301);
  assert.equal(p.results.prepare, "worked", "kept when nothing was noted yet");
  assert.equal(progressFor(p, { ...K301 }), p, "the same keyboard: the same progress");
  // Another firmware: nothing of the old answers, the invitation's rules
  // start over; only "Don't show again" stays.
  const shared = { ...all("failed"), keyboard: K301, checked: { effect: true }, opened: true, later: 2, shared: true, at: SUMMARY };
  const next = progressFor(shared, { ...K301, firmware: "3.02" });
  assert.deepEqual(next, { ...emptyProgress(), keyboard: { ...K301, firmware: "3.02" } });
  assert.equal(inviteShown(next, ready), true, "the invitation may show again for that firmware");
  const never = progressFor({ ...shared, never: true }, { ...K301, firmware: "3.02" });
  assert.equal(never.never, true);
  assert.equal(inviteShown(never, ready), false);
  // Another model too.
  assert.equal(answered(progressFor(shared, { ...K301, modelId: "other" })), 0);
  // Kept and read back with the rest.
  assert.deepEqual(parseProgress(JSON.parse(JSON.stringify(shared))).keyboard, K301);
  assert.equal(parseProgress({ keyboard: { modelId: "x", model: "", firmware: "3.01" } }).keyboard, null);
});

test("the shared text names the firmware the answers were collected on", () => {
  const progress = { ...all("worked"), keyboard: K301 };
  assert.deepEqual(sharedKeyboard(progress, "AJAZZ AK980 PRO", "3.02"), { model: "AJAZZ AK980 PRO", firmware: "3.01" });
  assert.deepEqual(sharedKeyboard(emptyProgress(), "AJAZZ AK980 PRO", "3.02"), { model: "AJAZZ AK980 PRO", firmware: "3.02" });
  const text = sharedText({ model: "Something else", firmware: "9.99", progress, checks: {}, report: REPORT });
  assert.ok(text.startsWith("Keymotion test guide: AJAZZ AK980 PRO, firmware 3.01\n\n1. "), text);
});

test("the shared text says when the keyboard was the simulator", () => {
  const sim = REPORT.replace(
    "  ajazz-ak980-pro.aa on USB 8039, firmware 3.01\n",
    "  ajazz-ak980-pro.aa on USB 8039, firmware 3.01\n  simulated keyboard (a development build with KEYMOTION_SIM), not a real one\n",
  );
  const text = sharedText({ model: "AJAZZ AK980 PRO", firmware: "3.01", progress: all("worked"), checks: {}, report: sim });
  assert.ok(text.startsWith("Keymotion test guide: AJAZZ AK980 PRO, firmware 3.01\n(simulated keyboard)\n\n"), text);
  const real = sharedText({ model: "AJAZZ AK980 PRO", firmware: "3.01", progress: all("worked"), checks: {}, report: REPORT });
  assert.ok(!real.includes("simulated"));
});
