// node --test tools/aa-ui.test.mjs
// What the app makes of the "AA" driver's English texts (the AK980 PRO on
// firmware 3.01): the sections a restore wrote, why it left a key as it
// was, the numbers in the upload's cancel and wait messages, and which
// message each of its errors gets (app/src/lib/aaText.ts and
// errorRules.ts; Node runs the TypeScript files as they are). The
// texts are the backend's (kb_core::aa, app/src-tauri/src/aa.rs); a change
// there shows up here.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  betaIssue,
  draftFits,
  freeMacroSlot,
  IMPORTED,
  keptKeys,
  keptWhy,
  liveNoticeKind,
  notSaneBody,
  restoredPart,
  restoredParts,
  restoreHeldBy,
  untestedFirmware,
  withoutFixed,
} from "../app/src/lib/aaText.ts";
import { EXPECTED, errorRule, uploadCancelledAt, uploadWaitSeconds } from "../app/src/lib/errorRules.ts";

test("every section a restore writes has a name, in the order shown", () => {
  for (const [name, part] of [
    ["base layer", "base"],
    ["Fn layer", "fn"],
    ["effect", "effect"],
    ["per-key table", "perKey"],
    ["macros", "macros"],
  ]) {
    assert.equal(restoredPart(name), part, name);
  }
  assert.equal(restoredPart("options"), null);
  assert.deepEqual(restoredParts(["macros", "effect", "base layer", "effect", "something new"]), ["base", "effect", "macros"]);
  assert.deepEqual(restoredParts([]), []);
});

test("why a key was kept, from the driver's reasons", () => {
  assert.equal(keptWhy("the Fn key is never reassigned"), "fn");
  assert.equal(keptWhy("the knob is never reassigned"), "knob");
  assert.equal(keptWhy("slot 126 (end of the table) must stay as read"), "fixed");
  assert.equal(keptWhy("keymap slot 40: page 0x0D entries are never written (only kept where they are)"), "function");
  assert.equal(keptWhy("keymap slot 9: firmware function 3 (reset, test or mode switch) is never written"), "function");
  assert.equal(keptWhy("keymap slot 9: its factory function is firmware function 7, which is never brought back"), "function");
  assert.equal(keptWhy("table is 500 bytes; expected 512"), "other");
});

test("a cancelled upload says how far it got", () => {
  assert.deepEqual(uploadCancelledAt("screen upload cancelled after 399 of 586 parts"), { done: 399, total: 586 });
  assert.deepEqual(uploadCancelledAt("keyboard write failed: screen upload cancelled after 0 of 16 parts"), { done: 0, total: 16 });
  assert.equal(uploadCancelledAt("screen upload cancelled before it started"), null);
});

test("the wait after a cut upload, in whole seconds", () => {
  assert.equal(uploadWaitSeconds("a screen upload stopped part-way; no commands for another 4200 ms"), 5);
  assert.equal(uploadWaitSeconds("a screen upload stopped part-way; no commands for another 5000 ms"), 5);
  assert.equal(uploadWaitSeconds("a screen upload stopped part-way; no commands for another 12 ms"), 1);
  assert.equal(uploadWaitSeconds("screen upload stopped"), null);
});

test("live lights that wait on the untested firmware question", () => {
  assert.equal(untestedFirmware("untested keyboard firmware 3.01"), "3.01");
  assert.equal(untestedFirmware("another program is using the keyboard (ajazz-driver)"), null);
});

test("the beta's issue form names the model and firmware as the docs do", () => {
  assert.deepEqual(betaIssue("AJAZZ AK980 PRO", "3.01"), {
    title: "[Keyboard] AJAZZ AK980 PRO firmware 3.01",
    model: "AJAZZ AK980 PRO firmware 3.01",
  });
  assert.deepEqual(betaIssue("AJAZZ AK980 PRO", null), { title: "[Keyboard] AJAZZ AK980 PRO", model: "AJAZZ AK980 PRO" });
});

test("the AA driver's failures say what to do, not 'something went wrong'", () => {
  for (const [raw, key] of [
    ["no answer to command 0x13", "err.noAnswer"],
    ["keyboard write failed: no answer to command 0x12", "err.noAnswer"],
    ["only 32 of 64 bytes were written", "err.noAnswer"],
    ["the effect read back differs from what was written", "err.verify"],
    ["the keymap read back differs from what was written", "err.verify"],
    ["channel not usable: the device was removed", "err.noKeyboard"],
    // The restore's own failure stays first: only part of it is back.
    ["the restore stopped while writing the macros (already written: base layer): no answer to command 0x15", "err.aa.restoreStopped"],
    ["the keyboard does not look as expected, so nothing is written: no answer to command 0x10", "err.aa.notSane"],
  ]) {
    assert.equal(errorRule(raw).key, key, raw);
  }
});

test("a request the keyboard did not answer in time may still happen", () => {
  const r = errorRule("keyboard did not answer in time");
  assert.equal(r.key, "err.busy");
  assert.ok(EXPECTED.has("err.busy"), "no problem report for a busy keyboard");
});

test("macros that don't fit say how much they need, before 'too large'", () => {
  const raw = "invalid value: 3 macros with 200 steps take 812 bytes; the keyboard takes 512 (and at most 99 macros)";
  assert.deepEqual(errorRule(raw), { key: "err.aa.macroMemory", vars: { bytes: 812, limit: 512 } });
  assert.ok(EXPECTED.has("err.aa.macroMemory"));
  assert.equal(errorRule("something else").key, "err.generic");
});

test("a read that did not finish is not a report-worthy check failure", () => {
  assert.equal(notSaneBody(true), "aa.ro.notSane.read");
  assert.equal(notSaneBody(false), "aa.ro.notSane.body");
});

test("'comes back in about 30 seconds' only when something goes back", () => {
  assert.equal(liveNoticeKind({ liveLightsOff: null, liveArmed: true, liveRestores: true }), "armed");
  // Per-key lighting saved from Keymotion: nothing was switched.
  assert.equal(liveNoticeKind({ liveLightsOff: null, liveArmed: true, liveRestores: false }), null);
  assert.equal(liveNoticeKind({ liveLightsOff: null, liveArmed: false, liveRestores: false }), null);
  assert.equal(liveNoticeKind({ liveLightsOff: { kind: "read-only" }, liveArmed: false, liveRestores: false }), "off");
});

test("no restore while the keyboard takes no writes or an upload runs", () => {
  assert.equal(restoreHeldBy("only read", true), "read-only");
  assert.equal(restoreHeldBy(null, true), "upload");
  assert.equal(restoreHeldBy(null, false), null);
});

test("macros and keys set in another program are left alone", () => {
  // Slot 1 holds a macro made elsewhere: a new one takes slot 2.
  assert.equal(freeMacroSlot([{ name: "a" }], [1], 100), 2);
  assert.equal(freeMacroSlot([{ name: "a" }, null, null], [], 100), 1);
  assert.equal(freeMacroSlot([null], [0], 1), null);
  assert.deepEqual(withoutFixed({ 3: "x", 7: "y" }, [7]), { 3: "x" });
});

test("a save against keys or macros changed elsewhere says what happened", () => {
  assert.equal(
    errorRule("the keyboard's keys changed in another program since Keymotion read them; nothing was written").key,
    "err.aa.keysChanged",
  );
  assert.equal(
    errorRule("the keyboard's macros changed in another program since Keymotion read them; nothing was written").key,
    "err.aa.macrosChanged",
  );
  assert.deepEqual(
    errorRule("macro 2 on the keyboard was made in another program and is not replaced; nothing was written"),
    { key: "err.aa.macroElsewhere", vars: { slot: 2 } },
  );
  assert.equal(
    errorRule("the profile's macros were not loaded: they would replace a macro made in another program").key,
    "err.aa.profileMacros",
  );
  for (const k of ["err.aa.keysChanged", "err.aa.macrosChanged", "err.aa.macroElsewhere", "err.aa.profileMacros"]) {
    assert.ok(EXPECTED.has(k), k);
  }
});

test("an unsaved draft from before goes when the keyboard changed since", () => {
  // "04 xx": the app's own record, nothing to compare.
  assert.equal(draftFits(undefined, null), true);
  // "AA": the same read of the keyboard, or a draft opened from a backup.
  assert.equal(draftFits("abc", "abc"), true);
  assert.equal(draftFits(IMPORTED, "abc"), true);
  // Changed in AJAZZ's tool since, or a draft from before sources existed.
  assert.equal(draftFits("abc", "def"), false);
  assert.equal(draftFits(undefined, "def"), false);
  assert.equal(draftFits(null, "def"), false);
});

test("the restore's done step names only places that are keys", () => {
  const kept = [
    { layer: "base", slot: 85, led: 96, reason: "the Fn key is never reassigned" },
    { layer: "fn", slot: 126, led: null, reason: "slot 126 (end of the table) must stay as read" },
  ];
  assert.deepEqual(keptKeys(kept), [kept[0]]);
});
