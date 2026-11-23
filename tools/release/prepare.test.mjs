// Tests for prepare.mjs and check-release.mjs, on a small made-up repository.
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { SKELETON_MARKER, STORE_LANGUAGES } from "../store/whats-new.mjs";
import { changelogFiles, releaseNotesPages, releaseNotesSection, releaseProblems } from "./check-release.mjs";
import {
  addNotesSection,
  bumpKind,
  nextVersion,
  planRelease,
  releaseChangelog,
  setJsonVersion,
  setLockVersions,
  setWorkspaceVersion,
  suggestBump,
} from "./prepare.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const script = join(root, "tools", "release", "prepare.mjs");
// Expected failures must not print GitHub Actions error annotations in CI.
const env = { ...process.env, GITHUB_ACTIONS: "" };

const REPO = "https://github.com/o/r";

const CHANGELOG = `# Changelog

Intro.

## [Unreleased]

### Added

- **Music mode:** new.

### Fixed

- A bug.

## [1.1.0] - 2026-11-16

One line.

### Fixed

- Old bug.

## [1.0.0] - 2026-11-15

- First.

[Unreleased]: ${REPO}/compare/v1.1.0...develop
[1.1.0]: ${REPO}/releases/tag/v1.1.0
`;

const CHANGELOG_TR = `# Sürüm notları

## [Yayımlanmamış]

### Eklenenler

- **Müzik modu:** yeni.

## [1.1.0] - 2026-11-16

- Eski.

[Yayımlanmamış]: ${REPO}/compare/v1.1.0...develop
[1.1.0]: ${REPO}/releases/tag/v1.1.0
[1.0.0]: ${REPO}/releases/tag/v1.0.0
`;

const NOTES = (title) => `---
title: ${title}
description: A summary of 1.1.0.
---

Intro.

## GitHub and the Microsoft Store

Text.

## Keymotion 1.1.0

What 1.1.0 brought.

- **Updates.** Text.

## Keymotion 1.0.0

Text.
`;

const LOCK = `version = 4

[[package]]
name = "kb-core"
version = "1.1.0"
dependencies = [
 "serde",
]

[[package]]
name = "keymotion"
version = "1.1.0"

[[package]]
name = "other"
version = "1.1.0"

[[package]]
name = "serde"
version = "1.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"
`;

/** A repository with everything prepare.mjs touches, at version 1.1.0. */
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "keymotion-prepare-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const files = {
    "Cargo.toml": '[workspace]\nmembers = ["crates/*", "app/src-tauri"]\n\n[workspace.package]\nversion = "1.1.0"\nedition = "2021"\n\n[workspace.dependencies]\nserde = { version = "1.1.0" }\n',
    "crates/kb-core/Cargo.toml": '[package]\nname = "kb-core"\nversion.workspace = true\n',
    "crates/other/Cargo.toml": '[package]\nname = "other"\nversion = "1.1.0"\n',
    "app/src-tauri/Cargo.toml": '[package]\nname = "keymotion"\nversion.workspace = true\n',
    "app/package.json": '{\n  "name": "keymotion-ui",\n  "private": true,\n  "version": "1.1.0",\n  "type": "module"\n}\n',
    "app/src-tauri/tauri.conf.json": '{\n  "productName": "Keymotion",\n  "version": "1.1.0",\n  "identifier": "dev.keymotion"\n}\n',
    "Cargo.lock": LOCK,
    "CHANGELOG.md": CHANGELOG,
    "CHANGELOG.tr.md": CHANGELOG_TR,
    "site/src/content/docs/about/release-notes.mdx": NOTES("Release notes"),
    "site/src/content/docs/tr/about/release-notes.mdx": NOTES("Sürüm notları"),
  };
  for (const [file, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, file)), { recursive: true });
    writeFileSync(join(dir, file), text);
  }
  // check-version.mjs checks the files beside its own folder.
  mkdirSync(join(dir, "tools"));
  copyFileSync(join(root, "tools", "check-version.mjs"), join(dir, "tools", "check-version.mjs"));
  return dir;
}

const read = (dir, file) => readFileSync(join(dir, file), "utf8");

test("nextVersion: minor and patch count up, X.Y.Z must be above", () => {
  assert.equal(nextVersion("1.1.0", "minor"), "1.2.0");
  assert.equal(nextVersion("1.1.3", "minor"), "1.2.0");
  assert.equal(nextVersion("1.1.3", "patch"), "1.1.4");
  assert.equal(nextVersion("1.1.0", "2.0.0"), "2.0.0");
  assert.throws(() => nextVersion("1.1.0", "1.1.0"), /not above/);
  assert.throws(() => nextVersion("1.1.0", "1.0.9"), /not above/);
  assert.throws(() => nextVersion("1.1.0", "v1.2.0"), /expected X\.Y\.Z/);
  assert.throws(() => nextVersion("1.1.0", "1.2.0-rc.1"), /expected X\.Y\.Z/);
  assert.throws(() => nextVersion("1.1.0", undefined), /expected X\.Y\.Z/);
  assert.equal(bumpKind("1.1.0", "1.1.1"), "patch");
  assert.equal(bumpKind("1.1.0", "1.2.0"), "minor");
  assert.equal(bumpKind("1.1.0", "2.0.0"), "major");
});

test("suggestBump: minor for Added or Changed, patch for Fixed and Security only", () => {
  assert.deepEqual(suggestBump(CHANGELOG), { bump: "minor", groups: ["Added", "Fixed"] });
  const fixedOnly = "## [Unreleased]\n\n### Fixed\n\n- A.\n\n### Security\n\n- B.\n\n## [1.0.0]\n\n### Added\n\n- C.\n";
  assert.deepEqual(suggestBump(fixedOnly), { bump: "patch", groups: ["Fixed", "Security"] });
  assert.equal(suggestBump("## [Unreleased]\n\n### Changed\n\n- A.\n").bump, "minor");
  // A group heading without items doesn't count; an empty section suggests nothing.
  assert.equal(suggestBump("## [Unreleased]\n\n### Added\n\n### Fixed\n\n- A.\n").bump, "patch");
  assert.equal(suggestBump("## [Unreleased]\n\n## [1.0.0]\n\n- A.\n"), null);
  assert.equal(suggestBump("# Changelog\n"), null);
});

test("releaseChangelog: Unreleased becomes the version, a new empty Unreleased stays on top", () => {
  const { text, groups, label } = releaseChangelog(CHANGELOG, { version: "1.2.0", date: "2026-11-30" });
  assert.equal(label, "Unreleased");
  assert.deepEqual(groups, [{ name: "Added", items: 1 }, { name: "Fixed", items: 1 }]);
  assert.match(text, /\n## \[Unreleased\]\n\n## \[1\.2\.0\] - 2026-11-30\n\n### Added\n\n- \*\*Music mode:\*\* new\.\n/);
  // [Unreleased] compares the new tag; every version heading has a link, in
  // order, the forgotten 1.0.0 one included; nothing else moves.
  assert.ok(
    text.endsWith(
      `- First.\n\n[Unreleased]: ${REPO}/compare/v1.2.0...develop\n[1.2.0]: ${REPO}/releases/tag/v1.2.0\n` +
        `[1.1.0]: ${REPO}/releases/tag/v1.1.0\n[1.0.0]: ${REPO}/releases/tag/v1.0.0\n`,
    ),
    text,
  );
  assert.equal(text.match(/^\[Unreleased\]:/gm).length, 1);
});

test("releaseChangelog: the translations keep their own Unreleased label", () => {
  const { text, label } = releaseChangelog(CHANGELOG_TR, { version: "1.2.0", date: "2026-11-30", file: "CHANGELOG.tr.md" });
  assert.equal(label, "Yayımlanmamış");
  assert.match(text, /## \[Yayımlanmamış\]\n\n## \[1\.2\.0\] - 2026-11-30\n\n### Eklenenler/);
  assert.match(text, new RegExp(`\\n\\[Yayımlanmamış\\]: ${REPO}/compare/v1\\.2\\.0\\.\\.\\.develop\\n\\[1\\.2\\.0\\]: `));
  // A file that starts with this version (no older headings) gets its links too;
  // CRLF comes out as LF.
  const ru = "# Журнал\r\n\r\n## [Не выпущено]\r\n\r\n### Добавлено\r\n\r\n- Новое.\r\n\r\n[Не выпущено]: https://github.com/a/b/compare/v1.1.0...develop\r\n";
  assert.equal(
    releaseChangelog(ru, { version: "1.2.0", date: "2026-11-30" }).text,
    "# Журнал\n\n## [Не выпущено]\n\n## [1.2.0] - 2026-11-30\n\n### Добавлено\n\n- Новое.\n\n" +
      "[Не выпущено]: https://github.com/a/b/compare/v1.2.0...develop\n[1.2.0]: https://github.com/a/b/releases/tag/v1.2.0\n",
  );
});

test("releaseChangelog: stops on an empty section, a missing one or a version already there", () => {
  const empty = "## [Unreleased]\n\n### Added\n\n## [1.1.0] - 2026-11-16\n\n- A.\n";
  assert.throws(() => releaseChangelog(empty, { version: "1.2.0", date: "2026-11-30", file: "CHANGELOG.ru.md" }), /CHANGELOG\.ru\.md: the \[Unreleased\] section is empty/);
  assert.throws(() => releaseChangelog("# Changelog\n\n## [1.1.0]\n\n- A.\n", { version: "1.2.0", date: "2026-11-30" }), /no Unreleased section/);
  assert.throws(() => releaseChangelog(CHANGELOG, { version: "1.1.0", date: "2026-11-30" }), /already has a section for 1\.1\.0/);
});

test("addNotesSection: a skeleton above the newest version, once", () => {
  const out = addNotesSection(NOTES("Release notes"), "1.2.0");
  assert.match(out, /## GitHub and the Microsoft Store\n\nText\.\n\n## Keymotion 1\.2\.0\n\n\{\/\* TODO\(release\): [^\n]*\*\/\}\n\n## Keymotion 1\.1\.0\n/);
  assert.equal(addNotesSection(out, "1.2.0"), null);
  assert.throws(() => addNotesSection("# Notes\n\nNothing.\n", "1.2.0"), /no "## Keymotion <version>" section/);
});

test("the version files change only where the version is", () => {
  const pkg = '{\n  "name": "x",\n  "version": "1.1.0",\n  "dependencies": { "a": "1.1.0" }\n}\n';
  assert.equal(setJsonVersion(pkg, "1.2.0"), pkg.replace('"version": "1.1.0"', '"version": "1.2.0"'));
  // A nested "version" before the top-level one is refused rather than changed.
  assert.throws(() => setJsonVersion('{\n  "engines": {\n    "version": "1"\n  },\n  "version": "1.1.0"\n}\n', "1.2.0"), /only the top-level/);
  const toml = '[workspace]\nmembers = []\n\n[workspace.package]\nedition = "2021"\nversion = "1.1.0"\n\n[workspace.dependencies]\nx = { version = "1.1.0" }\n';
  assert.equal(setWorkspaceVersion(toml, "1.2.0"), toml.replace('version = "1.1.0"\n\n', 'version = "1.2.0"\n\n'));
  assert.throws(() => setWorkspaceVersion("[workspace]\n", "1.2.0"), /no \[workspace\.package\]/);
  // A registry crate of the same version stays; so does a member at another version.
  const lock = setLockVersions(LOCK, ["kb-core", "keymotion", "serde"], "1.1.0", "1.2.0");
  assert.equal(lock.count, 2);
  assert.match(lock.text, /name = "kb-core"\nversion = "1\.2\.0"/);
  assert.match(lock.text, /name = "keymotion"\nversion = "1\.2\.0"/);
  assert.match(lock.text, /name = "other"\nversion = "1\.1\.0"/);
  assert.match(lock.text, /name = "serde"\nversion = "1\.1\.0"\nsource/);
});

test("planRelease: every file, the CHANGELOGs, the notes pages and What's new", (t) => {
  const dir = fixture(t);
  const plan = planRelease({ root: dir, wanted: "minor", date: "2026-11-30" });
  assert.equal(plan.version, "1.2.0");
  assert.equal(plan.suggestion.bump, "minor");
  assert.deepEqual(
    plan.changes.map((c) => c.file),
    [
      "Cargo.toml",
      "app/package.json",
      "app/src-tauri/tauri.conf.json",
      "crates/other/Cargo.toml",
      "Cargo.lock",
      "CHANGELOG.md",
      "CHANGELOG.tr.md",
      "site/src/content/docs/about/release-notes.mdx",
      "site/src/content/docs/tr/about/release-notes.mdx",
      ...STORE_LANGUAGES.map((l) => `store/whats-new/1.2.0.${l}.txt`),
    ],
  );
  assert.match(plan.changes.find((c) => c.file === "Cargo.lock").what, /^3 workspace crates/);
  // A patch with Added items is allowed, with a warning.
  assert.match(planRelease({ root: dir, wanted: "patch", date: "2026-11-30" }).warnings[0], /minor version, not a patch/);
  assert.throws(() => planRelease({ root: dir, wanted: "minor", date: "30.11.2026" }), /YYYY-MM-DD/);
});

test("CLI: --dry-run writes nothing; a run prepares everything and check-version passes", (t) => {
  const dir = fixture(t);
  const run = (...args) => spawnSync(process.execPath, [script, ...args, "--root", dir, "--date", "2026-11-30"], { encoding: "utf8", env });

  const before = read(dir, "CHANGELOG.md");
  const dry = run("1.2.0", "--dry-run");
  assert.equal(dry.status, 0, dry.stdout + dry.stderr);
  assert.match(dry.stdout, /Keymotion 1\.1\.0 → 1\.2\.0 \(minor; CHANGELOG\.md suggests minor \(Added, Fixed\)\), dated 2026-11-30\./);
  assert.match(dry.stdout, /Would change:\n {2}Cargo\.toml +version 1\.1\.0 → 1\.2\.0/);
  assert.match(dry.stdout, /CHANGELOG\.tr\.md +\[Yayımlanmamış\] → \[1\.2\.0\] - 2026-11-30 \(Eklenenler 1\); links/);
  assert.match(dry.stdout, /Dry run: nothing written\./);
  assert.equal(read(dir, "CHANGELOG.md"), before);
  assert.equal(existsSync(join(dir, "store")), false);

  const ok = run("minor");
  assert.equal(ok.status, 0, ok.stdout + ok.stderr);
  assert.match(ok.stdout, /Version check: 1\.2\.0 in /);
  assert.equal(JSON.parse(read(dir, "app/package.json")).version, "1.2.0");
  assert.equal(JSON.parse(read(dir, "app/src-tauri/tauri.conf.json")).version, "1.2.0");
  assert.match(read(dir, "Cargo.toml"), /\[workspace\.package\]\nversion = "1\.2\.0"/);
  assert.match(read(dir, "crates/other/Cargo.toml"), /version = "1\.2\.0"/);
  assert.match(read(dir, "CHANGELOG.md"), /## \[1\.2\.0\] - 2026-11-30/);
  assert.match(read(dir, "site/src/content/docs/tr/about/release-notes.mdx"), /## Keymotion 1\.2\.0\n\n\{\/\* TODO\(release\)/);
  assert.match(read(dir, "store/whats-new/1.2.0.tr.txt"), /^TODO\(release\): .*in Turkish/);

  // Prepared once: a second run stops, and changes nothing.
  const changelog = read(dir, "CHANGELOG.md");
  const again = run("1.2.0");
  assert.equal(again.status, 1);
  assert.match(again.stdout, /1\.2\.0 is not above the app's version 1\.2\.0/);
  assert.equal(read(dir, "CHANGELOG.md"), changelog);
});

test("CLI: an empty Unreleased section in any CHANGELOG stops it before anything is written", (t) => {
  const dir = fixture(t);
  writeFileSync(join(dir, "CHANGELOG.tr.md"), CHANGELOG_TR.replace("- **Müzik modu:** yeni.\n", ""));
  const res = spawnSync(process.execPath, [script, "minor", "--root", dir], { encoding: "utf8", env });
  assert.equal(res.status, 1);
  assert.match(res.stdout, /CHANGELOG\.tr\.md: the \[Yayımlanmamış\] section is empty/);
  assert.match(res.stdout, /Nothing was written\./);
  assert.equal(JSON.parse(read(dir, "app/package.json")).version, "1.1.0");
  assert.equal(read(dir, "CHANGELOG.md"), CHANGELOG);
});

test("CLI: without a version it suggests one and stops", (t) => {
  const dir = fixture(t);
  const res = spawnSync(process.execPath, [script, "--root", dir], { encoding: "utf8", env });
  assert.equal(res.status, 1);
  assert.match(res.stdout, /Unreleased has Added, Fixed: suggested minor, 1\.1\.0 → 1\.2\.0\./);
});

test("check-release: skeletons fail until the text is written", (t) => {
  const dir = fixture(t);
  assert.deepEqual(changelogFiles(dir), ["CHANGELOG.md", "CHANGELOG.tr.md"]);
  assert.deepEqual(releaseNotesPages(dir), [
    "site/src/content/docs/about/release-notes.mdx",
    "site/src/content/docs/tr/about/release-notes.mdx",
  ]);

  // Before prepare: nothing for 1.2.0.
  const none = releaseProblems(dir, "1.2.0").problems;
  assert.ok(none.includes('CHANGELOG.md has no "## [1.2.0] - <date>" section'), none.join("\n"));
  assert.ok(none.includes('site/src/content/docs/tr/about/release-notes.mdx has no "## Keymotion 1.2.0" section'));
  assert.ok(none.some((p) => /^store\/whats-new\/1\.2\.0\.en\.txt is missing/.test(p)));

  spawnSync(process.execPath, [script, "1.2.0", "--root", dir], { encoding: "utf8", env });
  const skeletons = releaseProblems(dir, "1.2.0").problems;
  assert.deepEqual(skeletons, [
    `site/src/content/docs/about/release-notes.mdx: the 1.2.0 section is still the skeleton (${SKELETON_MARKER})`,
    `site/src/content/docs/tr/about/release-notes.mdx: the 1.2.0 section is still the skeleton (${SKELETON_MARKER})`,
    `store/whats-new/1.2.0.en.txt is still the skeleton (remove the ${SKELETON_MARKER} line)`,
    `store/whats-new/1.2.0.tr.txt is still the skeleton (remove the ${SKELETON_MARKER} line)`,
  ]);

  // A section with only a comment left is empty; What's new has a length limit.
  for (const page of releaseNotesPages(dir)) {
    const path = join(dir, page);
    writeFileSync(path, readFileSync(path, "utf8").replace(/\{\/\* TODO\(release\)[^\n]*\*\/\}/, "{/* later */}"));
  }
  writeFileSync(join(dir, "store/whats-new/1.2.0.en.txt"), "x".repeat(1501));
  writeFileSync(join(dir, "store/whats-new/1.2.0.tr.txt"), "\n  \n");
  assert.deepEqual(releaseProblems(dir, "1.2.0").problems, [
    "site/src/content/docs/about/release-notes.mdx: the 1.2.0 section is empty",
    "site/src/content/docs/tr/about/release-notes.mdx: the 1.2.0 section is empty",
    "store/whats-new/1.2.0.en.txt has 1501 characters; the Store takes at most 1500",
    "store/whats-new/1.2.0.tr.txt is empty",
  ]);

  for (const page of releaseNotesPages(dir)) {
    const path = join(dir, page);
    writeFileSync(path, readFileSync(path, "utf8").replace("{/* later */}", "Music mode and more.\n\n- **Music mode.** Lights."));
  }
  writeFileSync(join(dir, "store/whats-new/1.2.0.en.txt"), "Music mode: the keys move with the sound.\n");
  writeFileSync(join(dir, "store/whats-new/1.2.0.tr.txt"), "Müzik modu: tuşlar sesle hareket eder.\n");
  const ready = releaseProblems(dir, "1.2.0");
  assert.deepEqual(ready.problems, []);
  assert.deepEqual(ready.checked.slice(-2), ["What's new (en)", "What's new (tr)"]);
  assert.equal(releaseNotesSection(read(dir, "site/src/content/docs/about/release-notes.mdx"), "1.2.0"), "Music mode and more.\n\n- **Music mode.** Lights.");
});

test("check-release: a pre-release needs only its CHANGELOG sections", (t) => {
  const dir = fixture(t);
  writeFileSync(join(dir, "CHANGELOG.md"), CHANGELOG.replace("## [Unreleased]", "## [Unreleased]\n\n## [1.2.0-rc.1] - 2026-11-30"));
  const r = releaseProblems(dir, "1.2.0-rc.1");
  assert.equal(r.prerelease, true);
  assert.deepEqual(r.problems, ['CHANGELOG.tr.md has no "## [1.2.0-rc.1] - <date>" section']);
});
