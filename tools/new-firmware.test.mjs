// node --test tools/new-firmware.test.mjs
// How the page tells an AK980 PRO on one of AJAZZ's new firmwares from one
// on the old firmware among the keyboards the backend doesn't take
// (app/src/lib/newFirmware.ts; Node runs the TypeScript file as it is).
// Firmware 769 (3.01) is supported in beta, so such a keyboard is not in
// that list; what is: an AK980 PRO on a firmware Keymotion doesn't know yet.
import assert from "node:assert/strict";
import { test } from "node:test";
import { firmwareVersion, isAk980Candidate, isNewAk980Firmware, parseDetails } from "../app/src/lib/newFirmware.ts";

const details = (pid, release, pages) =>
  [
    "Keymotion keyboard support request",
    "Keyboard: AK980 PRO",
    "Maker: SONiX",
    `USB ids: VID 0C45, PID ${pid}, release 0x${release}`,
    "Connection: USB",
    "",
    `Collections (${pages.length}):`,
    ...pages.map((p, i) => `  ${i + 1}. usage page 0x${p}, usage 0x0001, interface ${i}`),
  ].join("\n");

test("0C45:8039 the backend didn't take is a new firmware, whatever its details say", () => {
  const k = { id: "0C45:8039:usb", name: "AK980 PRO" };
  assert.equal(isNewAk980Firmware(k), true);
  assert.equal(isNewAk980Firmware(k, parseDetails(details("8039", "0301", ["0001"]))), true);
});

test("0C45:8029 and 0C45:8802 with a usage page 0xFF67 collection are a new firmware", () => {
  const d = parseDetails(details("8029", "0302", ["0001", "000C", "FF67", "FF68"]));
  assert.deepEqual(d, { release: 0x0302, pages: [0x0001, 0x000c, 0xff67, 0xff68] });
  assert.equal(firmwareVersion(d.release), "3.02");
  for (const pid of ["8029", "8802"]) {
    assert.equal(isNewAk980Firmware({ id: `0C45:${pid}:usb`, name: "AK980 PRO" }, d), true, pid);
  }
});

test("0C45:8009 with a usage page 0xFF67 collection is the new firmware", () => {
  const k = { id: "0C45:8009:usb", name: "AK980 PRO" };
  const d = parseDetails(details("8009", "0301", ["0001", "000C", "FF67"]));
  assert.deepEqual(d, { release: 0x0301, pages: [0x0001, 0x000c, 0xff67] });
  assert.equal(isNewAk980Firmware(k, d), true);
  assert.equal(firmwareVersion(d.release), "3.01");
  for (const pid of ["8029", "8802"]) {
    assert.equal(isNewAk980Firmware({ id: `0C45:${pid}:usb`, name: "AK980 PRO" }, d), true, pid);
  }
});

test("0C45:8009 on the old firmware (0xFF13, no 0xFF67) is not", () => {
  const k = { id: "0C45:8009:usb", name: "AK980 PRO" };
  assert.equal(isNewAk980Firmware(k, parseDetails(details("8009", "0106", ["0001", "000C", "FF13"]))), false);
  // Until its details are read, it isn't either.
  assert.equal(isNewAk980Firmware(k), false);
  assert.equal(isNewAk980Firmware(k, null), false);
});

test("other makers and other names are never it", () => {
  const d = parseDetails(details("8009", "0301", ["FF67"]));
  assert.equal(isNewAk980Firmware({ id: "3554:8039:usb", name: "AK980 PRO" }, d), false);
  assert.equal(isNewAk980Firmware({ id: "0C45:8039:usb", name: "AK820 PRO" }, d), false);
  assert.equal(isNewAk980Firmware({ id: "0C45:8039:usb", name: null }, d), false);
  assert.equal(isAk980Candidate({ id: "0c45:8009:usb", name: "ajazz ak980pro" }), true);
});

test("versions read as AJAZZ writes them", () => {
  assert.equal(firmwareVersion(0x0301), "3.01");
  assert.equal(firmwareVersion(0x0106), "1.06");
  assert.equal(firmwareVersion(0x0310), "3.10");
  assert.equal(firmwareVersion(null), null);
  assert.equal(parseDetails("no ids here").release, null);
});
