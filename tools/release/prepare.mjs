#!/usr/bin/env node
// Prepares a release on a branch from develop (docs site: developers/microsoft-store,
// "Making a release"):
//   - the version number in Cargo.toml ([workspace.package]), app/package.json,
//     app/src-tauri/tauri.conf.json and the workspace crates' entries in
//     Cargo.lock (tools/check-version.mjs checks them before and after);
//   - every CHANGELOG (CHANGELOG.md and its translations): the Unreleased
//     section becomes "## [X.Y.Z] - <date>" under a new, empty Unreleased
//     section, and the link definitions at the end get [Unreleased] compared
//     with the new tag and a link for every version heading that has none;
//   - a skeleton "## Keymotion X.Y.Z" section on the site's release notes
//     page, in every language that has the page;
//   - a skeleton Microsoft Store "What's new" file for every listing language
//     (store/whats-new/X.Y.Z.<lang>.txt, tools/store/whats-new.mjs).
// People then write the summaries where the skeletons say TODO(release);
// tools/release/check-release.mjs fails until they have.
//
// A pre-release (1.3.0-beta.1, on a beta/1.3.0-beta.1 branch from develop;
// docs site: developers/contributing, "Beta (pre-release)") is only on
// GitHub. It gets the version number and, in every CHANGELOG, a
// "## [1.3.0-beta.1] - <date>" section that is a copy of Unreleased:
// Unreleased stays as it is, so the release after the pre-release (1.3.0)
// still lists everything. No release notes or What's new skeletons.
//
// Nothing is written unless every file can be prepared: an empty Unreleased
// section in any CHANGELOG stops it. Files that already have the version's
// section (release notes, What's new) are left as they are.
//
// Usage: node tools/release/prepare.mjs <X.Y.Z|X.Y.Z-beta.N|minor|patch> [--dry-run] [--date YYYY-MM-DD]
//        node tools/release/prepare.mjs     the bump CHANGELOG.md suggests
//   minor is suggested when Unreleased has anything besides Fixed and
//   Security (Added, Changed, …), patch when it has only those. From a
//   pre-release, name the version: 1.3.0 releases 1.3.0-beta.1.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { STORE_LANGUAGES, SKELETON_MARKER, whatsNewFile, whatsNewPath, whatsNewSkeleton } from "../store/whats-new.mjs";
import {
  appVersion,
  changelogFiles,
  isPrerelease,
  notesHeading,
  PRERELEASE_VERSION,
  releaseNotesPages,
  RELEASE_VERSION,
} from "./check-release.mjs";
import { isUnreleased } from "./latest-json.mjs";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

/** Where the CHANGELOG links point when a file has none to copy from. */
export const REPOSITORY_URL = "https://github.com/cebersaban44-rgb/keymotion";

/** The three numbers of a version: "1.3.0-beta.1" → "1.3.0". */
const core = (v) => v.split(/[-+]/)[0];
const parse = (v) => core(v).split(".").map(Number);
/** The pre-release part of a version: "1.3.0-beta.1" → "beta.1", "1.3.0" → "". */
const preId = (v) => {
  const s = v.split("+")[0];
  const dash = s.indexOf("-");
  return dash < 0 ? "" : s.slice(dash + 1);
};

/**
 * Below 0, 0 or above 0 as version `a` comes before, with or after `b`, in
 * Semantic Versioning's order: by the three numbers, then a pre-release
 * before its release (1.3.0-beta.1 < 1.3.0), then the pre-release parts one
 * by one (beta.2 < beta.10 < rc.1).
 */
export function compareVersions(a, b) {
  const [x, y] = [parse(a), parse(b)];
  const cmp = x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
  if (cmp) return cmp;
  const [p, q] = [preId(a), preId(b)];
  if (!p || !q) return (p ? -1 : 0) - (q ? -1 : 0);
  const [ps, qs] = [p.split("."), q.split(".")];
  for (let i = 0; i < Math.min(ps.length, qs.length); i++) {
    if (ps[i] === qs[i]) continue;
    const [pn, qn] = [/^\d+$/.test(ps[i]), /^\d+$/.test(qs[i])];
    if (pn && qn) return Number(ps[i]) - Number(qs[i]);
    if (pn !== qn) return pn ? -1 : 1;
    return ps[i] < qs[i] ? -1 : 1;
  }
  return ps.length - qs.length;
}

/**
 * The next version: "minor" and "patch" count up from `current`; a version
 * named (X.Y.Z, or a pre-release such as X.Y.Z-beta.1) must be above it.
 * From a pre-release, the version must be named: 1.3.0 releases
 * 1.3.0-beta.1, and "minor" would have skipped it.
 */
export function nextVersion(current, wanted) {
  if (!RELEASE_VERSION.test(current) && !PRERELEASE_VERSION.test(current)) {
    throw new Error(`the app's version ${current} is not X.Y.Z or a pre-release such as X.Y.Z-beta.1`);
  }
  const [major, minor, patch] = parse(current);
  if (wanted === "minor" || wanted === "patch") {
    if (isPrerelease(current)) {
      throw new Error(`the app's version ${current} is a pre-release: name the version instead of ${wanted} (${core(current)} releases it)`);
    }
    return wanted === "minor" ? `${major}.${minor + 1}.0` : `${major}.${minor}.${patch + 1}`;
  }
  if (!RELEASE_VERSION.test(wanted ?? "") && !PRERELEASE_VERSION.test(wanted ?? "")) {
    throw new Error(`expected X.Y.Z, a pre-release such as X.Y.Z-beta.1, minor or patch, got ${JSON.stringify(wanted)}`);
  }
  if (compareVersions(wanted, current) <= 0) throw new Error(`${wanted} is not above the app's version ${current}`);
  return wanted;
}

/**
 * How far `version` is from `current`: "major", "minor" or "patch" by the
 * three numbers; with the same three, "pre-release" (1.3.0-beta.1 →
 * 1.3.0-beta.2) or "release" (1.3.0-beta.1 → 1.3.0).
 */
export function bumpKind(current, version) {
  const [a, b] = [parse(version), parse(current)];
  if (a[0] !== b[0]) return "major";
  if (a[1] !== b[1]) return "minor";
  if (a[2] !== b[2]) return "patch";
  return isPrerelease(version) ? "pre-release" : "release";
}

const HEADING = /^##\s+\[([^\]]+)\](.*)$/;
const NEXT_SECTION = /^#{1,2}\s/;
const LINK_DEFINITION = /^\[([^\]]+)\]:\s*(\S+)\s*$/;

/** The Unreleased section: its heading's line, label and body lines. */
function unreleased(lines) {
  const start = lines.findIndex((l) => {
    const m = HEADING.exec(l.trimEnd());
    return m !== null && isUnreleased(m[1]);
  });
  if (start < 0) return null;
  let end = start + 1;
  while (end < lines.length && !NEXT_SECTION.test(lines[end])) end++;
  // The link definitions after the last section are not part of it.
  let last = end;
  while (last > start + 1 && (lines[last - 1].trim() === "" || LINK_DEFINITION.test(lines[last - 1]))) last--;
  return { start, label: HEADING.exec(lines[start].trimEnd())[1], body: lines.slice(start + 1, last) };
}

/** The groups of a section ("### Added") and how many items each has. */
export function changeGroups(body) {
  const groups = [];
  let current = null;
  for (const line of body) {
    const h = /^###\s+(.+?)\s*$/.exec(line);
    if (h) {
      current = { name: h[1], items: 0 };
      groups.push(current);
    } else if (/^[-*+]\s/.test(line)) {
      if (!current) {
        current = { name: "", items: 0 };
        groups.push(current);
      }
      current.items++;
    } else if (line.trim() && !current) {
      // A summary paragraph before the first group.
      current = { name: "", items: 0 };
      groups.push(current);
    }
  }
  return groups.filter((g) => g.items > 0);
}

const PATCH_GROUPS = new Set(["fixed", "security"]);

/**
 * The bump CHANGELOG.md's Unreleased section asks for: "minor" when it has
 * anything besides Fixed and Security, "patch" when only those, null when
 * it is empty.
 */
export function suggestBump(changelog) {
  const section = unreleased(changelog.replace(/\r\n?/g, "\n").split("\n"));
  if (!section) return null;
  const groups = changeGroups(section.body);
  if (!groups.length) return null;
  const bump = groups.every((g) => PATCH_GROUPS.has(g.name.toLowerCase())) ? "patch" : "minor";
  return { bump, groups: groups.map((g) => g.name || "(no group)") };
}

/**
 * The CHANGELOG with its Unreleased section released as `version`:
 * `{ text, groups, label }`. A pre-release (1.3.0-beta.1) gets a copy of the
 * section below it, and Unreleased stays as it is: the release after the
 * pre-release lists everything again. Throws when there is no Unreleased
 * section, when it is empty, or when the version already has one.
 */
export function releaseChangelog(changelog, { version, date, file = "CHANGELOG.md" }) {
  const prerelease = isPrerelease(version);
  const lines = changelog.replace(/\r\n?/g, "\n").split("\n");
  if (lines.some((l) => HEADING.exec(l.trimEnd())?.[1].replace(/^v/, "") === version)) {
    throw new Error(`${file} already has a section for ${version}`);
  }
  const section = unreleased(lines);
  if (!section) throw new Error(`${file} has no Unreleased section`);
  const groups = changeGroups(section.body);
  if (!groups.length) {
    throw new Error(`${file}: the [${section.label}] section is empty; write its changes before preparing ${version}`);
  }
  if (prerelease) {
    lines.splice(section.start + 1 + section.body.length, 0, "", `## [${version}] - ${date}`, ...section.body);
  } else {
    lines.splice(section.start, 1, `## [${section.label}]`, "", `## [${version}] - ${date}`);
  }

  // The link definitions: [Unreleased] compares the new tag with develop,
  // and every version heading has a link (the previous one was once missed).
  // After a pre-release, Unreleased still compares with the last release.
  const defs = new Map();
  for (const l of lines) {
    const m = LINK_DEFINITION.exec(l);
    if (m) defs.set(m[1], { line: l, url: m[2] });
  }
  const base = /^(https:\/\/github\.com\/[^/]+\/[^/]+)\//.exec(defs.get(section.label)?.url ?? "")?.[1] ?? REPOSITORY_URL;
  const labels = lines.map((l) => HEADING.exec(l.trimEnd())?.[1]).filter((label) => label !== undefined);
  const lastRelease = labels.map((label) => label.replace(/^v/, "")).find((label) => RELEASE_VERSION.test(label));
  const block = labels.map((label) => {
    if (label === section.label) {
      if (prerelease) return defs.get(label)?.line ?? `[${label}]: ${base}/compare/v${lastRelease ?? version}...develop`;
      return `[${label}]: ${base}/compare/v${version}...develop`;
    }
    return defs.get(label)?.line ?? `[${label}]: ${base}/releases/tag/v${label.replace(/^v/, "")}`;
  });
  const managed = new Set(labels);
  const kept = lines.filter((l) => !managed.has(LINK_DEFINITION.exec(l)?.[1]));
  while (kept.length && kept.at(-1).trim() === "") kept.pop();
  return { text: `${kept.join("\n")}\n\n${block.join("\n")}\n`, groups, label: section.label };
}

/** The release notes page with a skeleton section for `version` above the newest one, or null when it has one. */
export function addNotesSection(mdx, version) {
  const lines = mdx.split("\n");
  if (lines.some((l) => l.trimEnd() === notesHeading(version))) return null;
  const at = lines.findIndex((l) => /^## Keymotion \d/.test(l));
  if (at < 0) throw new Error(`no "## Keymotion <version>" section to put ${version} above`);
  const skeleton = [
    notesHeading(version),
    "",
    `{/* ${SKELETON_MARKER}: a sentence or two on what ${version} brings, then a bullet for each main change, ` +
      `as in the sections below (CHANGELOG has the details). Also add ${version} to the description at the top. ` +
      "Then remove this comment. */}",
    "",
  ];
  lines.splice(at, 0, ...skeleton);
  return lines.join("\n");
}

/** A JSON file's top-level "version", changed in place (the rest of the text stays as it is). */
export function setJsonVersion(text, version) {
  const out = text.replace(/^(\s*"version"\s*:\s*")[^"]*(")/m, `$1${version}$2`);
  const [before, after] = [JSON.parse(text), JSON.parse(out)];
  if (after.version !== version || JSON.stringify({ ...before, version }) !== JSON.stringify(after)) {
    throw new Error(`could not change only the top-level "version"`);
  }
  return out;
}

/** Cargo.toml with [workspace.package] version = "<version>". */
export function setWorkspaceVersion(toml, version) {
  const lines = toml.split("\n");
  const start = lines.findIndex((l) => l.trim() === "[workspace.package]");
  if (start < 0) throw new Error("Cargo.toml has no [workspace.package]");
  for (let i = start + 1; i < lines.length && !/^\s*\[/.test(lines[i]); i++) {
    if (/^\s*version\s*=\s*"[^"]*"/.test(lines[i])) {
      lines[i] = lines[i].replace(/"[^"]*"/, `"${version}"`);
      return lines.join("\n");
    }
  }
  throw new Error("[workspace.package] in Cargo.toml has no version");
}

/** The [package] name of a crate and whether it takes the workspace's version. */
function crate(manifest) {
  const lines = manifest.split(/\r?\n/);
  const start = lines.findIndex((l) => l.trim() === "[package]");
  const table = [];
  for (const l of lines.slice(start + 1)) {
    if (/^\s*\[/.test(l)) break;
    table.push(l);
  }
  const body = table.join("\n");
  return {
    name: /^\s*name\s*=\s*"([^"]+)"/m.exec(body)?.[1],
    version: /^\s*version\s*=\s*"([^"]+)"/m.exec(body)?.[1],
    inherits: /^\s*version\.workspace\s*=\s*true|^\s*version\s*=\s*\{\s*workspace\s*=\s*true\s*\}/m.test(body),
  };
}

/** The workspace's crates (crates/* and the app, as in check-version.mjs) that carry the app's version. */
export function workspaceCrates(root, current) {
  const manifests = [join(root, "app", "src-tauri", "Cargo.toml")];
  const crates = join(root, "crates");
  if (existsSync(crates)) {
    for (const d of readdirSync(crates, { withFileTypes: true })) {
      if (d.isDirectory() && existsSync(join(crates, d.name, "Cargo.toml"))) manifests.push(join(crates, d.name, "Cargo.toml"));
    }
  }
  return manifests
    .map((path) => ({ path, ...crate(readFileSync(path, "utf8")) }))
    .filter((c) => c.name && (c.inherits || c.version === current));
}

/** Cargo.lock with the workspace crates (no `source`) at `version`. */
export function setLockVersions(lock, names, from, version) {
  let count = 0;
  const out = lock.replace(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\n(?!source = )/g, (all, name, v) => {
    if (!names.includes(name) || v !== from) return all;
    count++;
    return all.replace(`version = "${v}"`, `version = "${version}"`);
  });
  return { text: out, count };
}

const rel = (root, path) => relative(root, path).replace(/\\/g, "/");

/** Today in the computer's time zone, as YYYY-MM-DD. */
const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

/**
 * Everything prepare would write, without writing: `{ current, version,
 * suggestion, changes: [{ path, file, text, what }], kept, warnings }`.
 * Throws with every problem at once when a file can't be prepared.
 */
export function planRelease({ root = repoRoot, wanted, date = today() }) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error(`--date takes YYYY-MM-DD, got ${JSON.stringify(date)}`);
  const current = appVersion(root);
  const changelogs = changelogFiles(root);
  if (!changelogs.includes("CHANGELOG.md")) throw new Error("no CHANGELOG.md");
  const suggestion = suggestBump(readFileSync(join(root, "CHANGELOG.md"), "utf8"));
  const version = nextVersion(current, wanted);
  const prerelease = isPrerelease(version);
  const warnings = [];
  const kind = bumpKind(current, version);
  if (suggestion && kind === "patch" && suggestion.bump === "minor") {
    warnings.push(`CHANGELOG.md's Unreleased has ${suggestion.groups.join(", ")}: that is a minor version, not a patch`);
  }

  const changes = [];
  const kept = [];
  const problems = [];
  const change = (path, text, what) => changes.push({ path, file: rel(root, path), text, what });

  // The version number.
  const cargo = join(root, "Cargo.toml");
  change(cargo, setWorkspaceVersion(readFileSync(cargo, "utf8"), version), `version ${current} → ${version} ([workspace.package])`);
  for (const path of [join(root, "app", "package.json"), join(root, "app", "src-tauri", "tauri.conf.json")]) {
    change(path, setJsonVersion(readFileSync(path, "utf8"), version), `version ${current} → ${version}`);
  }
  const crates = workspaceCrates(root, current);
  for (const c of crates.filter((c) => !c.inherits)) {
    const text = readFileSync(c.path, "utf8").replace(/^(\s*version\s*=\s*")[^"]*(")/m, `$1${version}$2`);
    change(c.path, text, `version ${current} → ${version}`);
  }
  const lockPath = join(root, "Cargo.lock");
  if (existsSync(lockPath)) {
    const lock = setLockVersions(readFileSync(lockPath, "utf8"), crates.map((c) => c.name), current, version);
    if (lock.count !== crates.length) {
      problems.push(`Cargo.lock: found ${lock.count} of the ${crates.length} workspace crates at ${current}; run tools/check-version.mjs`);
    }
    change(lockPath, lock.text, `${lock.count} workspace crates ${current} → ${version}`);
  }

  // The CHANGELOGs.
  for (const file of changelogs) {
    const path = join(root, file);
    try {
      const out = releaseChangelog(readFileSync(path, "utf8"), { version, date, file });
      const groups = out.groups.map((g) => `${g.name || "intro"} ${g.items}`).join(", ");
      change(
        path,
        out.text,
        prerelease
          ? `[${out.label}] copied to [${version}] - ${date} (${groups}), [${out.label}] kept; links`
          : `[${out.label}] → [${version}] - ${date} (${groups}); links`,
      );
    } catch (e) {
      problems.push(e.message);
    }
  }

  // The site's release notes and the Store's What's new describe what both
  // channels get: a pre-release is only on GitHub, so it has neither.
  if (prerelease) {
    kept.push("the release notes pages and the Store's What's new (a pre-release is only on GitHub)");
  } else {
    for (const page of releaseNotesPages(root)) {
      const path = join(root, page);
      try {
        const text = addNotesSection(readFileSync(path, "utf8"), version);
        if (text === null) kept.push(`${page} (has "${notesHeading(version)}")`);
        else change(path, text, `skeleton section "${notesHeading(version).slice(3)}"`);
      } catch (e) {
        problems.push(`${page}: ${e.message}`);
      }
    }

    for (const lang of STORE_LANGUAGES) {
      const path = whatsNewPath(root, version, lang);
      if (existsSync(path)) kept.push(`${whatsNewFile(version, lang)} (exists)`);
      else change(path, whatsNewSkeleton(version, lang), "new skeleton");
    }
  }

  if (problems.length) {
    const e = new Error(`Can't prepare ${version}:\n${problems.map((p) => `  - ${p}`).join("\n")}\nNothing was written.`);
    e.problems = problems;
    throw e;
  }
  return { current, version, kind, prerelease, suggestion, date, changes, kept, warnings };
}

/** tools/check-version.mjs of `root`, which checks the files it lives beside. */
function checkVersion(root) {
  execFileSync(process.execPath, [join(root, "tools", "check-version.mjs")], { stdio: "inherit", env: { ...process.env, GITHUB_ACTIONS: "" } });
}

function main() {
  const { values: opt, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      "dry-run": { type: "boolean", default: false },
      date: { type: "string" },
      root: { type: "string" },
    },
  });
  const root = opt.root ?? repoRoot;
  const usage = "usage: prepare.mjs <X.Y.Z|X.Y.Z-beta.N|minor|patch> [--dry-run] [--date YYYY-MM-DD]";
  if (positionals.length > 1) throw new Error(usage);
  const [wanted] = positionals;
  if (!wanted) {
    const current = appVersion(root);
    const s = suggestBump(readFileSync(join(root, "CHANGELOG.md"), "utf8"));
    console.log(
      s
        ? `CHANGELOG.md's Unreleased has ${s.groups.join(", ")}: suggested ${s.bump}, ${current} → ${nextVersion(current, s.bump)}.\n` +
            `Run: node tools/release/prepare.mjs ${s.bump} --dry-run`
        : "CHANGELOG.md's Unreleased section is empty: there is nothing to release.",
    );
    console.log(usage);
    process.exit(1);
  }

  // The files must agree before anything changes.
  checkVersion(root);
  const plan = planRelease({ root, wanted, ...(opt.date ? { date: opt.date } : {}) });
  const why = plan.suggestion ? `; CHANGELOG.md suggests ${plan.suggestion.bump} (${plan.suggestion.groups.join(", ")})` : "";
  const kind = plan.prerelease && plan.kind !== "pre-release" ? `${plan.kind} pre-release` : plan.kind;
  console.log(`\nKeymotion ${plan.current} → ${plan.version} (${kind}${why}), dated ${plan.date}.`);
  for (const w of plan.warnings) console.log(`Warning: ${w}.`);
  const width = Math.max(...plan.changes.map((c) => c.file.length)) + 2;
  console.log(`\n${opt["dry-run"] ? "Would change" : "Changing"}:`);
  for (const c of plan.changes) console.log(`  ${c.file.padEnd(width)}${c.what}`);
  if (plan.kept.length) {
    console.log("Left as they are:");
    for (const k of plan.kept) console.log(`  ${k}`);
  }
  if (opt["dry-run"]) {
    console.log("\nDry run: nothing written.");
    return;
  }
  for (const c of plan.changes) {
    mkdirSync(dirname(c.path), { recursive: true });
    writeFileSync(c.path, c.text);
  }
  checkVersion(root);
  if (plan.prerelease) {
    console.log(
      [
        "",
        "Next:",
        `  1. Optionally, a one-line summary under each CHANGELOG's [${plan.version}] heading. Leave Unreleased`,
        `     as it is: the release after ${plan.version} lists everything again.`,
        "  2. node tools/release/check-release.mjs   (the CHANGELOG sections are all a pre-release needs)",
        `  3. Commit on beta/${plan.version} and push it; no pull request, the branch is never merged.`,
        "     Once CI is green on it: node tools/release/tag.mjs   (on that branch)",
      ].join("\n"),
    );
    return;
  }
  console.log(
    [
      "",
      "Next:",
      `  1. Write the text where the skeletons say ${SKELETON_MARKER}: the release notes pages (and their description line)`,
      "     and the Store's What's new files. Optionally, a one-line summary under each CHANGELOG heading.",
      "  2. node tools/release/check-release.mjs   (passes once nothing is left to write)",
      `  3. Commit on release/${plan.version}, open the pull request into develop, then develop → main;`,
      "     then, on main: node tools/release/tag.mjs",
    ].join("\n"),
  );
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  try {
    main();
  } catch (e) {
    console.log(`${process.env.GITHUB_ACTIONS ? "::error::" : ""}${e.message}`);
    process.exit(1);
  }
}
