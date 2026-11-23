// Tests for msix.mjs and the package manifest it fills.
// Run: node --test "tools/store/*.test.mjs"

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";

import {
  checkCliVersion,
  checkIdentity,
  checkPackageVersion,
  fillManifest,
  programs,
  singlePackageConfig,
  storeVersion,
  TEST_IDENTITY,
} from "./msix.mjs";

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

test("keymotion-cli is a console app with its alias, the app keeps its id", () => {
  const apps = [...manifest.matchAll(/<Application Id="(\w+)" Executable="([^"]+)"([^>]*)>/g)];
  assert.deepEqual(apps.map((a) => [a[1], a[2]]), [["Keymotion", "keymotion.exe"], ["Cli", "keymotion-cli.exe"]]);
  // keymotion-cli starts the app by the id "Keymotion".
  const launch = readFileSync(join(root, "crates", "keymotion-cli", "src", "launch.rs"), "utf8");
  assert.equal(/pub const APP_ID: &str = "([^"]+)"/.exec(launch)?.[1], apps[0][1]);
  // A console program, no tile, nothing in the Start menu.
  assert.match(apps[1][3], /desktop4:Subsystem="console"/);
  const cli = manifest.slice(manifest.indexOf('<Application Id="Cli"'));
  assert.match(cli, /AppListEntry="none"/);
  assert.match(cli, /<uap5:AppExecutionAlias desktop4:Subsystem="console">/);
  // The alias is where the app and the hooks look for it.
  const alias = /<uap5:ExecutionAlias Alias="([^"]+)" \/>/.exec(cli)?.[1];
  assert.equal(alias, "keymotion-cli.exe");
  const apps_rs = readFileSync(join(root, "app", "src-tauri", "src", "agent", "apps.rs"), "utf8");
  assert.ok(apps_rs.includes("Microsoft\\WindowsApps\\" + alias), "app/src-tauri/src/agent/apps.rs");
  const setup = readFileSync(join(root, "crates", "kb-hooks", "src", "setup.rs"), "utf8");
  assert.ok(setup.includes(`join("WindowsApps").join("${alias}")`), "crates/kb-hooks/src/setup.rs");
  // Every prefix it uses is declared, and ignorable on older Windows.
  for (const ns of ["uap5", "desktop4"]) {
    assert.match(manifest, new RegExp(`xmlns:${ns}="http://schemas\\.microsoft\\.com/appx/manifest/`));
    assert.match(manifest, new RegExp(`IgnorableNamespaces="[^"]*\\b${ns}\\b`));
  }
  // Nothing new to ask for: full trust is all.
  assert.deepEqual([...manifest.matchAll(/Capability Name="(\w+)"/g)].map((m) => m[1]), ["runFullTrust"]);
});

test("the package takes keymotion-cli.exe from beside keymotion.exe, or --cli", () => {
  const dir = mkdtempSync(join(tmpdir(), "km-msix-"));
  try {
    const app = join(dir, "keymotion.exe");
    writeFileSync(app, "");
    assert.throws(() => programs(app), /not a keymotion-cli\.exe/);
    writeFileSync(join(dir, "keymotion-cli.exe"), "");
    assert.deepEqual(programs(app), { app, tool: join(dir, "keymotion-cli.exe") });
    const other = join(dir, "other");
    mkdirSync(other);
    writeFileSync(join(other, "kb.exe"), "");
    assert.throws(() => programs(app, join(other, "kb.exe")), /not a keymotion-cli\.exe/);
    assert.throws(() => programs(join(dir, "keymotion-cli.exe")), /not a keymotion\.exe/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("keymotion-cli must say the app's version", () => {
  assert.equal(checkCliVersion("keymotion-cli 1.2.0 (protocol 1)\n", "1.2.0"), "keymotion-cli 1.2.0 (protocol 1)");
  assert.throws(() => checkCliVersion("keymotion-cli 1.1.0 (protocol 1)", "1.2.0"), /same commit/);
  assert.throws(() => checkCliVersion("keymotion-cli 1.2.0-rc.1 (protocol 1)", "1.2.0"), /same commit/);
  assert.throws(() => checkCliVersion("", "1.2.0"), /same commit/);
});

test("every logo the manifest names has its images", () => {
  for (const [, file] of manifest.matchAll(/Assets\\(\w+)\.png/g)) {
    assert.ok(existsSync(join(store, "Assets", `${file}.scale-100.png`)), `${file}.scale-100.png`);
  }
  for (const size of [16, 24, 32, 48, 256]) {
    assert.ok(existsSync(join(store, "Assets", `Square44x44Logo.targetsize-${size}_altform-unplated.png`)));
  }
});

test("a Store package never takes another version than the app's", () => {
  const env = { ...process.env, GITHUB_ACTIONS: "", STORE_IDENTITY_NAME: "a.Keymotion", STORE_PUBLISHER: "CN=X", STORE_PUBLISHER_DISPLAY_NAME: "x" };
  const script = join(root, "tools", "store", "msix.mjs");
  const refused = spawnSync(process.execPath, [script, "--check", "--version", "9.9.9.0"], { env, encoding: "utf8" });
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /test packages only/);
  const test = spawnSync(process.execPath, [script, "--check", "--test", "--version", "9.9.9.0"], { env, encoding: "utf8" });
  assert.equal(test.status, 0, test.stderr);
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
