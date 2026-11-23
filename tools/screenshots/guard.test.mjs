// node --test tools/screenshots/guard.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { blocked, DENY, guard } from "./guard.mjs";

function fakeWindow() {
  const calls = [];
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke(cmd, args) {
        calls.push([cmd, args]);
        return Promise.resolve("ok");
      },
    },
  };
  return calls;
}

test("refuses the commands that write, lets the rest through", async () => {
  const calls = fakeWindow();
  assert.equal(guard([DENY, []]), "on");
  await assert.rejects(window.__TAURI_INTERNALS__.invoke("lighting_save", {}), /screenshot guard: "lighting_save"/);
  await assert.rejects(window.__TAURI_INTERNALS__.invoke("screen_upload", {}), /screen_upload/);
  assert.equal(await window.__TAURI_INTERNALS__.invoke("keyboards", {}), "ok");
  assert.deepEqual(calls.map((c) => c[0]), ["keyboards"]);
  assert.deepEqual(blocked(), ["lighting_save", "screen_upload"]);
  assert.deepEqual(blocked(), []);
});

test("allows a refused command only while it is allowed", async () => {
  const calls = fakeWindow();
  assert.equal(guard([DENY, ["agents_connect"]]), "on");
  assert.equal(await window.__TAURI_INTERNALS__.invoke("agents_connect", { token: "t" }), "ok");
  assert.equal(guard([DENY, []]), "on");
  await assert.rejects(window.__TAURI_INTERNALS__.invoke("agents_connect", { token: "t" }));
  assert.equal(calls.length, 1);
});

test("says when there is nothing to guard or it cannot", () => {
  globalThis.window = {};
  assert.equal(guard([DENY, []]), "no-tauri");
  globalThis.window = { __TAURI_INTERNALS__: Object.freeze({ invoke: () => Promise.resolve() }) };
  assert.equal(guard([DENY, []]), "failed");
});

test("every flash write in the app is on the list", () => {
  for (const cmd of ["apply_effect", "apply_per_key", "lighting_save", "apply_keymap", "apply_macros", "screen_upload", "profiles_load", "live_submit"]) {
    assert.ok(DENY.includes(cmd), cmd);
  }
});
