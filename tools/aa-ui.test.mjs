// node --test tools/aa-ui.test.mjs
// What the app makes of the "AA" driver's English texts (the AK980 PRO on
// firmware 3.01): the sections a restore wrote, why it left a key as it
// was, and the numbers in the upload's cancel and wait messages
// (app/src/lib/aaText.ts; Node runs the TypeScript file as it is). The
// texts are the backend's (kb_core::aa, app/src-tauri/src/aa.rs); a change
// there shows up here.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  betaIssue,
  keptWhy,
  restoredPart,
  restoredParts,
  untestedFirmware,
  uploadCancelledAt,
  uploadWaitSeconds,
} from "../app/src/lib/aaText.ts";

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
