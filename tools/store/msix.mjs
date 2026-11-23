#!/usr/bin/env node
// Packs keymotion.exe as the Microsoft Store's MSIX (docs site:
// developers/microsoft-store). The program is the one `tauri build` makes
// for GitHub; at run time it sees its package and lets the Store update it
// (app/src-tauri/src/channel.rs). keymotion-cli.exe, the agents'
// command-line tool, goes beside it (the manifest's second application,
// with its app execution alias): the one next to keymotion.exe, or --cli.
//
//   node tools/store/msix.mjs --exe target/release/keymotion.exe --out target/store
//       The Store package. Its identity comes from Partner Center › the app ›
//       Product identity, in STORE_IDENTITY_NAME, STORE_PUBLISHER and
//       STORE_PUBLISHER_DISPLAY_NAME (GitHub: repository variables).
//   node tools/store/msix.mjs --exe … --out … --test
//       A package for trying on this computer, with its own made-up
//       identity ("Keymotion (test)"), so it never mixes with the Store's.
//       A pre-release app version (1.3.0-beta.1) packs as its three
//       numbers (1.3.0.0); a Store package refuses it.
//   --version 1.2.3.0   another package version than the app's: test
//                       packages only. A Store package always carries the
//                       app's own version, so GitHub and the Store never
//                       show different numbers for the same release.
//   --check             only check the identity and the version (CI runs
//                       it before the build), make nothing.
//
// Writes <out>/layout (the package's files; with Windows' developer mode,
// `Add-AppxPackage -Register <out>\layout\AppxManifest.xml` installs it
// as it is) and <out>/Keymotion_<version>_x64.msix, unsigned: the Store
// signs what it publishes. Needs the Windows SDK (makepri, makeappx).

import { execFileSync } from "node:child_process";
import { appendFileSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../..", import.meta.url));
const STORE_DIR = join(root, "app", "src-tauri", "store");
const TAURI_CONF = join(root, "app", "src-tauri", "tauri.conf.json");

/** The identity of a package for trying on this computer only. */
export const TEST_IDENTITY = {
  IdentityName: "Keymotion.StoreTest",
  Publisher: "CN=Keymotion Store Test",
  PublisherDisplayName: "Keymotion test",
  DisplayName: "Keymotion (test)",
};

/**
 * The Store's version for an app version: "1.2.3" → "1.2.3.0". The Store
 * keeps the fourth part and wants the first above 0; it has no room for a
 * test version's "-rc.1". A test package (`allowPrerelease`) of a test
 * version takes its three numbers: 1.3.0-beta.1 → 1.3.0.0, so CI packs one
 * on a beta branch too.
 */
export function storeVersion(appVersion, { allowZero = false, allowPrerelease = false } = {}) {
  const m = /^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/.exec(appVersion);
  if (!m) throw new Error(`"${appVersion}" is not a version like 1.2.3`);
  if (m[4] && !allowPrerelease) throw new Error(`${appVersion} is a test version; the Store takes released versions only`);
  const parts = m.slice(1, 4).map(Number);
  if (parts.some((p) => p > 65535)) throw new Error(`${appVersion}: each part must be 65535 or less`);
  if (parts[0] === 0 && !allowZero) {
    throw new Error(`${appVersion}: the Store wants the first part above 0 (release 1.0.0, or pass --version)`);
  }
  return `${parts.join(".")}.0`;
}

/** A package version given by hand: four parts, the last 0. */
export function checkPackageVersion(v, { allowZero = false } = {}) {
  const m = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(v);
  if (!m) throw new Error(`"${v}" is not a package version like 1.2.3.0`);
  const parts = m.slice(1).map(Number);
  if (parts.some((p) => p > 65535)) throw new Error(`${v}: each part must be 65535 or less`);
  if (parts[3] !== 0) throw new Error(`${v}: the fourth part is the Store's and must be 0`);
  if (parts[0] === 0 && !allowZero) throw new Error(`${v}: the Store wants the first part above 0`);
  return v;
}

/** Partner Center's values, as the manifest allows them. */
export function checkIdentity(id) {
  if (!/^[A-Za-z0-9.-]{3,50}$/.test(id.IdentityName ?? "")) {
    throw new Error(`identity name "${id.IdentityName ?? ""}": 3 to 50 letters, digits, dots or dashes`);
  }
  if (!/^CN=/.test(id.Publisher ?? "") || id.Publisher.length > 8192) {
    throw new Error(`publisher "${id.Publisher ?? ""}" must start with CN= (Partner Center › Product identity)`);
  }
  for (const key of ["PublisherDisplayName", "DisplayName"]) {
    if (!id[key]?.trim() || id[key].length > 256) throw new Error(`${key} is empty or too long`);
  }
  return id;
}

const xmlEscape = (s) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&apos;");

/** The manifest with its {{Name}} places filled; every place must be. */
export function fillManifest(template, values) {
  const out = template.replace(/\{\{(\w+)\}\}/g, (all, key) => {
    if (!(key in values)) throw new Error(`the manifest asks for {{${key}}}, which has no value`);
    return xmlEscape(String(values[key]));
  });
  return out;
}

/** A tool of the newest Windows SDK (x64). */
export function sdkTool(name, kits = "C:\\Program Files (x86)\\Windows Kits\\10\\bin") {
  const versions = existsSync(kits)
    ? readdirSync(kits)
        .filter((d) => /^10\.\d+\.\d+\.\d+$/.test(d))
        .sort((a, b) => {
          const pa = a.split(".").map(Number);
          const pb = b.split(".").map(Number);
          for (let i = 0; i < 4; i++) if (pa[i] !== pb[i]) return pb[i] - pa[i];
          return 0;
        })
    : [];
  for (const v of versions) {
    const tool = join(kits, v, "x64", name);
    if (existsSync(tool)) return tool;
  }
  throw new Error(`${name} not found under ${kits}: install the Windows SDK`);
}

function args(argv) {
  const out = { test: false, check: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--test") out.test = true;
    else if (a === "--check") out.check = true;
    else if (["--exe", "--cli", "--out", "--version"].includes(a)) out[a.slice(2)] = argv[++i];
    else throw new Error(`unknown argument ${a}`);
  }
  if (out.version && !out.test) {
    throw new Error("--version is for test packages only: a Store package carries the app's own version");
  }
  if (!out.check && (!out.exe || !out.out)) {
    throw new Error("usage: msix.mjs --exe <keymotion.exe> [--cli <keymotion-cli.exe>] --out <folder> [--test] [--version 1.2.3.0] | --check [--test]");
  }
  return out;
}

function identity(test) {
  if (test) return { ...TEST_IDENTITY };
  const env = process.env;
  const missing = ["STORE_IDENTITY_NAME", "STORE_PUBLISHER", "STORE_PUBLISHER_DISPLAY_NAME"].filter((k) => !env[k]);
  if (missing.length) {
    throw new Error(`${missing.join(", ")} not set: copy them from Partner Center › the app › Product identity (or pass --test)`);
  }
  return checkIdentity({
    IdentityName: env.STORE_IDENTITY_NAME,
    Publisher: env.STORE_PUBLISHER,
    PublisherDisplayName: env.STORE_PUBLISHER_DISPLAY_NAME,
    DisplayName: "Keymotion",
  });
}

/** The package's programs: keymotion.exe, and keymotion-cli.exe from
 *  `cli` or beside it. Each must be there, under its own name. */
export function programs(exe, cli) {
  const app = resolve(exe);
  if (basename(app).toLowerCase() !== "keymotion.exe" || !existsSync(app)) throw new Error(`${app}: not a keymotion.exe`);
  const tool = resolve(cli ?? join(dirname(app), "keymotion-cli.exe"));
  if (basename(tool).toLowerCase() !== "keymotion-cli.exe" || !existsSync(tool)) {
    throw new Error(`${tool}: not a keymotion-cli.exe (build it: cargo build -p keymotion-cli, with --release for a release)`);
  }
  return { app, tool };
}

/** What `keymotion-cli --version` said is the app's version: the tool
 *  beside the app is from the same build, never an older one left there. */
export function checkCliVersion(said, appVersion) {
  const line = String(said).trim();
  if (!line.startsWith(`keymotion-cli ${appVersion} `)) {
    throw new Error(`keymotion-cli says "${line}", the app is ${appVersion}: build both from the same commit`);
  }
  return line;
}

/** Run an SDK tool; its file-by-file lines are left out of the log. */
function run(tool, list) {
  const out = execFileSync(tool, list, { stdio: ["ignore", "pipe", "inherit"], encoding: "utf8" });
  for (const line of out.split(/\r?\n/)) {
    if (line.trim() && !/^Processing |^Using /.test(line)) console.log(line);
  }
}

/** One resources.pri for the whole package: makepri's default config would
 *  split the sizes into resource packages, which only a bundle has. */
export function singlePackageConfig(xml) {
  return xml.replace(/\s*<packaging>[\s\S]*?<\/packaging>/, "");
}

function main() {
  const opt = args(process.argv.slice(2));
  const id = identity(opt.test);
  const appVersion = JSON.parse(readFileSync(TAURI_CONF, "utf8")).version;
  const version = opt.version
    ? checkPackageVersion(opt.version, { allowZero: opt.test })
    : storeVersion(appVersion, { allowZero: opt.test, allowPrerelease: opt.test });
  if (opt.check) {
    console.log(`Keymotion ${appVersion} → package ${version} (${id.IdentityName}): ready to pack`);
    return;
  }
  const { app, tool } = programs(opt.exe, opt.cli);
  checkCliVersion(execFileSync(tool, ["--version"], { encoding: "utf8" }), appVersion);

  const out = resolve(opt.out);
  const layout = join(out, "layout");
  rmSync(layout, { recursive: true, force: true });
  mkdirSync(layout, { recursive: true });
  copyFileSync(app, join(layout, "keymotion.exe"));
  copyFileSync(tool, join(layout, "keymotion-cli.exe"));
  cpSync(join(STORE_DIR, "Assets"), join(layout, "Assets"), { recursive: true });
  const template = readFileSync(join(STORE_DIR, "AppxManifest.xml"), "utf8");
  writeFileSync(join(layout, "AppxManifest.xml"), fillManifest(template, { ...id, Version: version }));

  // resources.pri: lets Windows pick each logo's size and contrast.
  const makepri = sdkTool("makepri.exe");
  const priconfig = join(out, "priconfig.xml");
  // The languages AppxManifest.xml's <Resources> declares.
  run(makepri, ["createconfig", "/cf", priconfig, "/dq", "en-US_tr-TR_ru-RU_zh-CN", "/pv", "10.0.0", "/o"]);
  writeFileSync(priconfig, singlePackageConfig(readFileSync(priconfig, "utf8")));
  run(makepri, ["new", "/pr", layout, "/cf", priconfig, "/mn", join(layout, "AppxManifest.xml"), "/of", join(layout, "resources.pri"), "/o"]);

  const msix = join(out, `Keymotion_${version}_x64.msix`);
  run(sdkTool("makeappx.exe"), ["pack", "/d", layout, "/p", msix, "/o"]);
  console.log(`\nKeymotion ${appVersion} → package ${version} (${id.IdentityName})\n  ${msix}\n  ${layout}`);
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `msix=${msix}\npackage_version=${version}\n`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error(process.env.GITHUB_ACTIONS ? `::error::${msg}` : msg);
    process.exit(1);
  }
}
