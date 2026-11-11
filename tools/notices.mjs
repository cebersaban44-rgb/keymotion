#!/usr/bin/env node
// Writes app/public/third-party-notices.txt: the licences of all third-party
// software that ships inside the app. The app can show the file (it is served
// from app/public), and the installer can carry it.
//
//   Part 1  Rust crates, from cargo-about (about.toml, tools/notices/about.hbs)
//   Part 2  The HIDAPI C library, built into the hidapi crate
//   Part 3  JavaScript packages bundled into the UI (app/package.json
//           "dependencies" and everything they depend on)
//   Part 4  Fonts bundled into the UI (SIL Open Font License 1.1, full text)
//
// Requirements:
//   cargo install --locked --features cli cargo-about
//   pnpm --dir app install
// Usage (repository root):
//   node tools/notices.mjs           write the file
//   node tools/notices.mjs --check   fail if the file is out of date
//
// The output depends only on Cargo.lock, app/pnpm-lock.yaml and the installed
// packages, so running it twice gives the same file. Run it whenever a
// dependency is added, removed or updated, and commit the result.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const appDir = join(root, "app");
const outFile = join(appDir, "public", "third-party-notices.txt");
const TARGET = "x86_64-pc-windows-msvc";
const RULE = "=".repeat(72);
const THIN = "-".repeat(72);

function run(cmd, args) {
  try {
    return execFileSync(cmd, args, { cwd: root, encoding: "utf8", maxBuffer: 256 << 20, stdio: ["ignore", "pipe", "inherit"] });
  } catch (e) {
    console.error(`\nnotices: \`${cmd} ${args.join(" ")}\` failed.`);
    if (cmd === "cargo" && args[0] === "about") {
      console.error("Install cargo-about with: cargo install --locked --features cli cargo-about");
    }
    process.exit(1);
  }
}

/** LF line endings, no BOM, no trailing spaces, at most one blank line in a row. */
function tidy(text) {
  return text
    .replace(/^\uFEFF/, "")
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((l) => l.trimEnd())
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/^\n+|\n+$/g, "");
}

// ---------------------------------------------------------------- Rust crates
// Crate sources must be in the local cargo cache; then cargo-about runs
// offline and does not depend on what a remote repository serves today.
run("cargo", ["fetch", "--locked", "--target", TARGET]);
const rust = run("cargo", ["about", "generate", "--frozen", "--workspace", "tools/notices/about.hbs"]);
const rustCrates = new Set(
  [...rust.matchAll(/^ {2}(\S+) (\S+)$/gm)].map((m) => `${m[1]} ${m[2]}`),
);

// --------------------------------------------------------------- HIDAPI (C)
const metadata = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET]));
const hidapi = metadata.packages.find((p) => p.name === "hidapi");
let hidapiPart = "";
if (hidapi) {
  const dir = join(dirname(hidapi.manifest_path), "etc", "hidapi");
  const bsd = join(dir, "LICENSE-bsd.txt");
  if (!existsSync(bsd)) {
    console.error(`notices: ${bsd} not found; the hidapi crate layout changed.`);
    process.exit(1);
  }
  hidapiPart = [
    `The hidapi crate (${hidapi.version}) compiles the HIDAPI C library into the app.`,
    "HIDAPI may be used under one of three licences (GPL-3.0, a BSD-style licence",
    "or the original HIDAPI licence). Keymotion uses it under the BSD-style licence:",
    "",
    tidy(readFileSync(bsd, "utf8")),
  ].join("\n");
}

// ------------------------------------------------------ JavaScript packages
const LICENSE_FILE = /^(licen[cs]e|copying|notice)(\.|-|_|$)/i;

/** Resolve `name` the way Node does, starting from the package in `fromDir`. */
function resolvePackage(name, fromDir) {
  for (let dir = fromDir; ; dir = dirname(dir)) {
    if (basename(dir) !== "node_modules") {
      const candidate = join(dir, "node_modules", name);
      if (existsSync(join(candidate, "package.json"))) return realpathSync(candidate);
    }
    if (dirname(dir) === dir) return null;
  }
}

const packages = new Map(); // "name@version" -> { name, version, dir, pkg }
function walk(name, fromDir, optional) {
  const dir = resolvePackage(name, fromDir);
  if (!dir) {
    if (optional) return;
    console.error(`notices: ${name} is not installed. Run: pnpm --dir app install`);
    process.exit(1);
  }
  const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const key = `${pkg.name}@${pkg.version}`;
  if (packages.has(key)) return;
  packages.set(key, { name: pkg.name, version: pkg.version, dir, pkg });
  for (const dep of Object.keys(pkg.dependencies ?? {})) walk(dep, dir, false);
  for (const dep of Object.keys(pkg.optionalDependencies ?? {})) walk(dep, dir, true);
}

const appPkg = JSON.parse(readFileSync(join(appDir, "package.json"), "utf8"));
for (const dep of Object.keys(appPkg.dependencies ?? {})) walk(dep, appDir, false);

const licenseOf = (pkg) =>
  typeof pkg.license === "string"
    ? pkg.license
    : pkg.license?.type ?? (Array.isArray(pkg.licenses) ? pkg.licenses.map((l) => l.type ?? l).join(" OR ") : "UNKNOWN");
const sourceOf = (pkg) => {
  const repo = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url;
  return (repo ?? pkg.homepage ?? `https://www.npmjs.com/package/${pkg.name}`)
    .replace(/^git\+/, "")
    .replace(/\.git$/, "")
    .replace(/^git:\/\//, "https://");
};
const licenseFiles = (dir) =>
  readdirSync(dir)
    .filter((f) => LICENSE_FILE.test(f))
    .sort()
    .map((f) => tidy(readFileSync(join(dir, f), "utf8")));

const isFont = (name) => name.startsWith("@fontsource");
const sorted = [...packages.values()].sort((a, b) => (a.name === b.name ? 0 : a.name < b.name ? -1 : 1));
const missing = [];

function packageBlock({ name, version, dir, pkg }) {
  const texts = licenseFiles(dir);
  if (!texts.length) missing.push(`${name}@${version}`);
  return [
    RULE,
    `${name} ${version}`,
    `Licence: ${licenseOf(pkg)}`,
    `Source: ${sourceOf(pkg)}`,
    THIN,
    "",
    texts.length ? texts.join(`\n\n${THIN}\n\n`) : `The package has no licence file. It is distributed under ${licenseOf(pkg)}.`,
    "",
  ].join("\n");
}

const jsPart = sorted.filter((p) => !isFont(p.name)).map(packageBlock).join("\n");
const fonts = sorted.filter((p) => isFont(p.name));
for (const f of fonts) {
  const text = licenseFiles(f.dir).join("\n");
  if (!/SIL OPEN FONT LICENSE Version 1\.1/.test(text) || !/END OF TERMS AND CONDITIONS|DISCLAIMER/.test(text)) {
    console.error(`notices: ${f.name} has no full SIL Open Font License 1.1 text.`);
    process.exit(1);
  }
}
const fontPart = fonts.map(packageBlock).join("\n");

// ----------------------------------------------------------------- Output
const header = [
  "Keymotion: third-party notices",
  "",
  "Keymotion is licensed under MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).",
  "It includes the third-party software listed below, each under its own",
  "licence. This file is generated by tools/notices.mjs; do not edit it by hand.",
  "",
  "The source code of each Rust crate is available from https://crates.io and",
  "of each JavaScript package from https://www.npmjs.com, under the name and",
  "version given here.",
  "",
  `Part 1. Rust crates (${rustCrates.size})`,
  "Part 2. HIDAPI C library",
  `Part 3. JavaScript packages (${sorted.length - fonts.length})`,
  `Part 4. Fonts (${fonts.length})`,
].join("\n");

const output =
  [
    header,
    `${RULE}\n${RULE}\nPart 1. Rust crates\n${RULE}\n${RULE}`,
    tidy(rust),
    `${RULE}\n${RULE}\nPart 2. HIDAPI C library\n${RULE}\n${RULE}`,
    hidapiPart || "Not used.",
    `${RULE}\n${RULE}\nPart 3. JavaScript packages\n${RULE}\n${RULE}`,
    tidy(jsPart),
    `${RULE}\n${RULE}\nPart 4. Fonts\n${RULE}\n${RULE}`,
    [
      "The fonts below are bundled into the app and used for its interface and for",
      "the cards it draws for the keyboard's screen. Each is licensed under the SIL",
      "Open Font License, Version 1.1; the full licence text follows each font.",
    ].join("\n"),
    tidy(fontPart),
  ].join("\n\n") + "\n";

if (missing.length) {
  console.warn(`notices: no licence file in ${missing.join(", ")} (licence named from package.json).`);
}

const check = process.argv.includes("--check");
const current = existsSync(outFile) ? readFileSync(outFile, "utf8") : "";
if (check) {
  if (current !== output) {
    console.error("notices: app/public/third-party-notices.txt is out of date. Run: node tools/notices.mjs");
    process.exit(1);
  }
  console.log("notices: up to date.");
} else {
  writeFileSync(outFile, output, "utf8");
  console.log(
    `notices: wrote app/public/third-party-notices.txt (${rustCrates.size} crates, ` +
      `${sorted.length - fonts.length} JavaScript packages, ${fonts.length} fonts, ${(output.length / 1024).toFixed(0)} KiB).`,
  );
}
