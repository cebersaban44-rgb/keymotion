#!/usr/bin/env node
// The app's version is kept in three files, and this check fails when they
// disagree (docs site: developers/contributing):
//   - app/src-tauri/Cargo.toml   (usually `version.workspace = true`, which
//                                 reads [workspace.package] in Cargo.toml)
//   - app/package.json
//   - app/src-tauri/tauri.conf.json
// It also checks that Cargo.lock has been updated for the app crate and that
// every other workspace crate either inherits the workspace version or has the
// same one.
//
// Usage: node tools/check-version.mjs
//        node tools/check-version.mjs --tag v0.2.0     also: the release tag is
//                                                      "v" + that version
//        node tools/check-version.mjs --github-output  in GitHub Actions: on
//                                                      success, write
//                                                      version=<version> to
//                                                      $GITHUB_OUTPUT

import { appendFileSync, readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const { values: args } = parseArgs({
  options: { tag: { type: "string" }, "github-output": { type: "boolean", default: false } },
});

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (p) => readFileSync(join(root, p), "utf8");

/** The body of a TOML table (`[name]`) up to the next table header. */
function tomlTable(text, name) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((l) => l.trim() === `[${name}]`);
  if (start < 0) return null;
  const body = [];
  for (const l of lines.slice(start + 1)) {
    if (/^\s*\[/.test(l)) break;
    body.push(l);
  }
  return body.join("\n");
}

const stringKey = (table, key) => table?.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"`, "m"))?.[1] ?? null;
const inheritsWorkspace = (table, key) =>
  new RegExp(`^\\s*${key}\\.workspace\\s*=\\s*true|^\\s*${key}\\s*=\\s*\\{\\s*workspace\\s*=\\s*true\\s*\\}`, "m").test(
    table ?? "",
  );

const workspaceVersion = stringKey(tomlTable(read("Cargo.toml"), "workspace.package"), "version");

function crateVersion(manifest) {
  const pkg = tomlTable(read(manifest), "package");
  if (inheritsWorkspace(pkg, "version")) return { name: stringKey(pkg, "name"), version: workspaceVersion, inherited: true };
  return { name: stringKey(pkg, "name"), version: stringKey(pkg, "version"), inherited: false };
}

const app = crateVersion("app/src-tauri/Cargo.toml");
const pkgJson = JSON.parse(read("app/package.json")).version ?? null;
let tauri = JSON.parse(read("app/src-tauri/tauri.conf.json")).version ?? null;
// Tauri also accepts a path to a package.json as "version".
if (tauri && tauri.endsWith(".json")) {
  tauri = JSON.parse(readFileSync(resolve(root, "app/src-tauri", tauri), "utf8")).version ?? null;
}

const sources = [
  [`app/src-tauri/Cargo.toml${app.inherited ? " (workspace)" : ""}`, app.version],
  ["app/package.json", pkgJson],
  ["app/src-tauri/tauri.conf.json", tauri],
];
const problems = [];
const expected = app.version;
for (const [file, v] of sources) {
  if (!v) problems.push(`${file}: no version found`);
  else if (v !== expected) problems.push(`${file}: ${v} (expected ${expected})`);
}

// Other crates: inherit, or match.
for (const dir of readdirSync(join(root, "crates"), { withFileTypes: true })) {
  const manifest = `crates/${dir.name}/Cargo.toml`;
  if (!dir.isDirectory() || !existsSync(join(root, manifest))) continue;
  const c = crateVersion(manifest);
  if (c.version !== expected) problems.push(`${manifest}: ${c.version} (expected ${expected} or version.workspace = true)`);
}

// Cargo.lock must follow the manifest, or the next build rewrites it.
if (existsSync(join(root, "Cargo.lock")) && app.name) {
  const lock = read("Cargo.lock");
  const m = lock.match(new RegExp(`\\[\\[package\\]\\]\\r?\\nname = "${app.name}"\\r?\\nversion = "([^"]+)"`));
  if (!m) problems.push(`Cargo.lock: no entry for ${app.name}`);
  else if (m[1] !== expected) problems.push(`Cargo.lock: ${app.name} ${m[1]} (expected ${expected}; run cargo update -w)`);
}

// A release tag names the version it builds.
if (args.tag !== undefined && args.tag !== `v${expected}`) {
  problems.push(`tag ${args.tag} (expected v${expected}, the version in the files above)`);
}

if (problems.length) {
  const ci = !!process.env.GITHUB_ACTIONS;
  for (const p of problems) console.log(ci ? `::error::Version mismatch: ${p}` : `Version mismatch: ${p}`);
  process.exit(1);
}
console.log(
  `Version check: ${expected} in ${sources.map(([f]) => f).join(", ")} and Cargo.lock` +
    (args.tag !== undefined ? `, and tag ${args.tag}.` : "."),
);
if (args["github-output"]) {
  if (!process.env.GITHUB_OUTPUT) {
    console.log("--github-output: GITHUB_OUTPUT is not set (it is set in GitHub Actions).");
    process.exit(1);
  }
  appendFileSync(process.env.GITHUB_OUTPUT, `version=${expected}\n`);
}
