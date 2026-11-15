// Tests for msix.mjs and the package manifest it fills.
// Run: node --test "tools/store/*.test.mjs"

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { checkIdentity, checkPackageVersion, fillManifest, singlePackageConfig, storeVersion, TEST_IDENTITY } from "./msix.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const store = join(root, "app", "src-tauri", "store");
const manifest = readFileSync(join(store, "AppxManifest.xml"), "utf8");

test("app versions become Store versions with a fourth part 0", () => {
  assert.equal(storeVersion("1.0.0"), "1.0.0.0");
  assert.equal(storeVersion("2.13.7"), "2.13.7.0");
  assert.equal(storeVersion("1.2.3+build.5"), "1.2.3.0");
  // The Store wants the first part above 0; a test package may have it.
  assert.throws(() => storeVersion("0.1.0"), /first part above 0/);
  assert.equal(storeVersion("0.1.0", { allowZero: true }), "0.1.0.0");
  // No room for a test version, nor for parts above 65535.
  assert.throws(() => storeVersion("1.1.0-rc.1"), /test version/);
  assert.throws(() => storeVersion("1.70000.0"), /65535/);
  assert.throws(() => storeVersion("v1.0.0"), /not a version/);
});

test("package versions given by hand are checked", () => {
  assert.equal(checkPackageVersion("1.2.3.0"), "1.2.3.0");
  assert.throws(() => checkPackageVersion("1.2.3.4"), /fourth part/);
  assert.throws(() => checkPackageVersion("0.1.0.0"), /above 0/);
  assert.equal(checkPackageVersion("0.1.0.0", { allowZero: true }), "0.1.0.0");
  assert.throws(() => checkPackageVersion("1.2.3"), /not a package version/);
});

test("identities are checked as the manifest allows", () => {
  assert.deepEqual(checkIdentity(TEST_IDENTITY), TEST_IDENTITY);
  assert.throws(() => checkIdentity({ ...TEST_IDENTITY, IdentityName: "Key motion" }), /identity name/);
  assert.throws(() => checkIdentity({ ...TEST_IDENTITY, Publisher: "Keymotion" }), /CN=/);
  assert.throws(() => checkIdentity({ ...TEST_IDENTITY, PublisherDisplayName: " " }), /PublisherDisplayName/);
});

test("the manifest is filled, escaped, and every place has a value", () => {
  const values = { ...TEST_IDENTITY, PublisherDisplayName: "A & B <C>", Version: "1.0.0.0" };
  const out = fillManifest(manifest, values);
  assert.doesNotMatch(out, /\{\{\w+\}\}/);
  assert.match(out, /<PublisherDisplayName>A &amp; B &lt;C&gt;<\/PublisherDisplayName>/);
  assert.match(out, /Version="1\.0\.0\.0"/);
  assert.throws(() => fillManifest("{{Nope}}", values), /\{\{Nope\}\}/);
});

test("the manifest keeps what the app relies on", () => {
  // Full trust: HID, gauges, running programs, the tray.
  assert.match(manifest, /<rescap:Capability Name="runFullTrust" \/>/);
  assert.match(manifest, /EntryPoint="Windows\.FullTrustApplication"/);
  // The startup task's id is the one channel.rs asks Windows for.
  const channel = readFileSync(join(root, "app", "src-tauri", "src", "channel.rs"), "utf8");
  const id = /STARTUP_TASK: &str = "([^"]+)"/.exec(channel)?.[1];
  assert.ok(id);
  assert.match(manifest, new RegExp(`TaskId="${id}"`));
  assert.match(manifest, /uap10:Parameters="--hidden"/);
  const tray = readFileSync(join(root, "app", "src-tauri", "src", "tray.rs"), "utf8");
  assert.match(tray, /ARG_HIDDEN: &str = "--hidden"/);
  // The program the manifest starts is the one the build makes.
  assert.match(manifest, /Executable="keymotion\.exe"/);
});

test("every logo the manifest names has its images", () => {
  for (const [, file] of manifest.matchAll(/Assets\\(\w+)\.png/g)) {
    assert.ok(existsSync(join(store, "Assets", `${file}.scale-100.png`)), `${file}.scale-100.png`);
  }
  for (const size of [16, 24, 32, 48, 256]) {
    assert.ok(existsSync(join(store, "Assets", `Square44x44Logo.targetsize-${size}_altform-unplated.png`)));
  }
});

test("the package keeps one resources.pri", () => {
  const xml = `<resources>
	<packaging>
		<autoResourcePackage qualifier="Scale"/>
	</packaging>
	<index root="\\"/>
</resources>`;
  const out = singlePackageConfig(xml);
  assert.doesNotMatch(out, /packaging|autoResourcePackage/);
  assert.match(out, /<index root=/);
});
