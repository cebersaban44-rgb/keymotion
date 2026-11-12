// Tests for latest-json.mjs and the release tag check in check-version.mjs.
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { changelogSection, latestJson, releaseAssetUrl, releaseBody } from "./latest-json.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const script = join(root, "tools", "release", "latest-json.mjs");
const SHA = "a".repeat(64);
// Expected failures must not print GitHub Actions error annotations in CI.
const env = { ...process.env, GITHUB_ACTIONS: "" };

const CHANGELOG = `# Changelog

Intro text.

## [Unreleased]

### Added

- Something new.

## [0.2.0] - 2026-12-01

### Fixed

- A bug.
- Another one.

## 0.1.0

First release.

[Unreleased]: https://github.com/o/r/compare/v0.2.0...develop
[0.2.0]: https://github.com/o/r/compare/v0.1.0...v0.2.0
`;

test("changelogSection: the section for the version, up to the next one", () => {
  assert.deepEqual(changelogSection(CHANGELOG, "0.2.0"), {
    label: "0.2.0",
    body: "### Fixed\n\n- A bug.\n- Another one.",
  });
});

test("changelogSection: the last section leaves out the link definitions", () => {
  assert.equal(changelogSection(CHANGELOG, "0.1.0")?.body, "First release.");
});

test("changelogSection: CRLF line endings and a v-prefixed heading", () => {
  const md = "## [v1.2.3] - 2027-01-01\r\n\r\n- Item.\r\n\r\n## [1.2.2]\r\n";
  assert.equal(changelogSection(md, "1.2.3")?.body, "- Item.");
});

test("changelogSection: the version must match exactly", () => {
  assert.equal(changelogSection(CHANGELOG, "0.2"), null);
  assert.equal(changelogSection(CHANGELOG, "0.2.0-rc.1"), null);
});

test("changelogSection: a missing version is null without the fallback", () => {
  assert.equal(changelogSection(CHANGELOG, "0.3.0"), null);
});

test("changelogSection: Unreleased stands in for a missing version when allowed", () => {
  assert.deepEqual(changelogSection(CHANGELOG, "0.3.0", { allowUnreleased: true }), {
    label: "Unreleased",
    body: "### Added\n\n- Something new.",
  });
  // An existing section still wins over the fallback.
  assert.equal(changelogSection(CHANGELOG, "0.2.0", { allowUnreleased: true })?.label, "0.2.0");
});

test("changelogSection: the Turkish file may call it Yayımlanmamış", () => {
  const md = "## [Yayımlanmamış]\n\n- Yeni bir şey.\n";
  assert.deepEqual(changelogSection(md, "0.3.0", { allowUnreleased: true }), {
    label: "Unreleased",
    body: "- Yeni bir şey.",
  });
});

test("changelogSection: no section at all is null even with the fallback", () => {
  assert.equal(changelogSection("# Changelog\n\nNothing yet.\n", "0.1.0", { allowUnreleased: true }), null);
});

test("releaseAssetUrl: GitHub release download URL with the file name encoded", () => {
  assert.equal(
    releaseAssetUrl({ repository: "owner/repo", version: "0.2.0", fileName: "Keymotion_0.2.0_x64-setup.exe" }),
    "https://github.com/owner/repo/releases/download/v0.2.0/Keymotion_0.2.0_x64-setup.exe",
  );
  assert.equal(
    releaseAssetUrl({ repository: "owner/repo", version: "0.2.0-rc.1", fileName: "Key motion+1_0.2.0-rc.1.exe" }),
    "https://github.com/owner/repo/releases/download/v0.2.0-rc.1/Key%20motion%2B1_0.2.0-rc.1.exe",
  );
  assert.throws(() => releaseAssetUrl({ repository: "", version: "0.2.0", fileName: "a.exe" }), /owner\/repo/);
});

const manifestInput = {
  version: "0.2.0",
  notes: { en: "English notes." },
  pubDate: new Date("2026-12-01T10:20:30.456Z"),
  url: "https://github.com/owner/repo/releases/download/v0.2.0/setup.exe",
  signature: "dW50cnVzdGVkIGNvbW1lbnQ=\n",
};

test("latestJson: the Tauri static manifest for windows-x86_64", () => {
  assert.deepEqual(latestJson(manifestInput), {
    version: "0.2.0",
    notes: '{"en":"English notes."}',
    pub_date: "2026-12-01T10:20:30Z",
    platforms: {
      "windows-x86_64": {
        signature: "dW50cnVzdGVkIGNvbW1lbnQ=",
        url: "https://github.com/owner/repo/releases/download/v0.2.0/setup.exe",
      },
    },
  });
});

test("latestJson: notes is a JSON string with Turkish when there is some", () => {
  const m = latestJson({ ...manifestInput, notes: { en: "Fixed.\n\n- \"quoted\"", tr: "Düzeltildi." } });
  assert.equal(typeof m.notes, "string");
  assert.deepEqual(JSON.parse(m.notes), { en: 'Fixed.\n\n- "quoted"', tr: "Düzeltildi." });
  // Empty Turkish notes are left out.
  assert.deepEqual(Object.keys(JSON.parse(latestJson({ ...manifestInput, notes: { en: "x", tr: "" } }).notes)), ["en"]);
});

test("latestJson: rejects a bad version, an empty signature and missing notes", () => {
  assert.throws(() => latestJson({ ...manifestInput, version: "v0.2.0" }), /not a version/);
  assert.throws(() => latestJson({ ...manifestInput, signature: " \n" }), /signature/);
  assert.throws(() => latestJson({ ...manifestInput, notes: { en: "" } }), /English notes/);
});

test("releaseBody: notes, how to install, the SHA-256 line and the policy links", () => {
  const body = releaseBody({ notes: "- A bug.", fileName: "setup.exe", sha256: SHA, repository: "o/r", version: "0.2.0" });
  assert.match(body, /^- A bug\.\n\n---\n\n### Install\n\nDownload \*\*`setup\.exe`\*\* below/);
  assert.match(body, /> \[!NOTE\]\n> This release isn't signed yet/);
  assert.match(body, new RegExp(`SHA-256 of \`setup\\.exe\`: \`${SHA}\``));
  assert.match(body, /\[Code signing policy\]\(https:\/\/cebersaban44-rgb\.github\.io\/keymotion\/about\/code-signing\/\)/);
  assert.match(body, /\[Privacy policy\]\(/);
  assert.match(body, /\[Türkçe sürüm notları\]\(https:\/\/github\.com\/o\/r\/blob\/v0\.2\.0\/CHANGELOG\.tr\.md\)\n$/);
  // Signed releases (stage 9) drop the Windows warning.
  assert.doesNotMatch(releaseBody({ notes: "x", fileName: "s.exe", sha256: SHA, signed: true }), /isn't signed/);
  assert.match(releaseBody({ notes: "x", fileName: "s.exe", sha256: SHA, preface: "Test build." }), /^> Test build\.\n\nx\n/);
  assert.throws(() => releaseBody({ notes: "x", fileName: "s.exe", sha256: "ABC" }), /SHA-256/);
});

test("CLI: writes latest.json and release-notes.md, fails without a section", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "keymotion-release-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const setup = join(dir, "Keymotion_0.2.0_x64-setup.exe");
  writeFileSync(setup, "not really an installer");
  writeFileSync(`${setup}.sig`, "c2lnbmF0dXJl");
  writeFileSync(join(dir, "CHANGELOG.md"), CHANGELOG);
  writeFileSync(join(dir, "tr.md"), "## [0.2.0] - 2026-12-01\n\n- Bir hata.\n");
  const run = (version, ...extra) =>
    spawnSync(
      process.execPath,
      [script, "--version", version, "--setup", setup, "--sha256", SHA, "--repository", "owner/repo",
        "--changelog", join(dir, "CHANGELOG.md"), "--tr-notes", join(dir, "tr.md"), ...extra],
      { encoding: "utf8", env },
    );

  const ok = run("0.2.0");
  assert.equal(ok.status, 0, ok.stdout + ok.stderr);
  const manifest = JSON.parse(readFileSync(join(dir, "latest.json"), "utf8"));
  assert.equal(manifest.platforms["windows-x86_64"].url,
    "https://github.com/owner/repo/releases/download/v0.2.0/Keymotion_0.2.0_x64-setup.exe");
  assert.equal(manifest.platforms["windows-x86_64"].signature, "c2lnbmF0dXJl");
  assert.deepEqual(JSON.parse(manifest.notes), { en: "### Fixed\n\n- A bug.\n- Another one.", tr: "- Bir hata." });
  assert.match(
    readFileSync(join(dir, "release-notes.md"), "utf8"),
    new RegExp(`^### Fixed[\\s\\S]*${SHA}\`\\n\\n\\[User guide\\][\\s\\S]*/owner/repo/blob/v0\\.2\\.0/CHANGELOG\\.tr\\.md\\)\\n$`),
  );

  const missing = run("0.3.0");
  assert.equal(missing.status, 1);
  assert.match(missing.stdout, /has no section for 0\.3\.0/);

  // --check-notes needs only the version and writes nothing.
  rmSync(join(dir, "latest.json"));
  const check = spawnSync(process.execPath, [script, "--check-notes", "--version", "0.2.0",
    "--changelog", join(dir, "CHANGELOG.md"), "--tr-notes", join(dir, "tr.md")], { encoding: "utf8", env });
  assert.equal(check.status, 0, check.stdout + check.stderr);
  assert.match(check.stdout, /CHANGELOG\.md \[0\.2\.0\], Turkish \[0\.2\.0\]/);
  assert.equal(existsSync(join(dir, "latest.json")), false);
  const checkMissing = spawnSync(process.execPath, [script, "--check-notes", "--version", "0.3.0",
    "--changelog", join(dir, "CHANGELOG.md")], { encoding: "utf8", env });
  assert.equal(checkMissing.status, 1);

  const fallback = run("0.3.0", "--allow-unreleased");
  assert.equal(fallback.status, 0, fallback.stdout + fallback.stderr);
  assert.match(readFileSync(join(dir, "release-notes.md"), "utf8"), /^> Test build: 0\.3\.0 .*\n\n### Added/);
  assert.deepEqual(JSON.parse(JSON.parse(readFileSync(join(dir, "latest.json"), "utf8")).notes), {
    en: "### Added\n\n- Something new.",
  });
});

test("check-version --tag: the tag must be v + the app's version", () => {
  const version = JSON.parse(readFileSync(join(root, "app", "src-tauri", "tauri.conf.json"), "utf8")).version;
  const checkVersion = join(root, "tools", "check-version.mjs");
  execFileSync(process.execPath, [checkVersion, "--tag", `v${version}`], { env, stdio: "pipe" });
  const wrong = spawnSync(process.execPath, [checkVersion, "--tag", version], { env, encoding: "utf8" });
  assert.equal(wrong.status, 1);
  assert.match(wrong.stdout, /tag .* \(expected v/);
});
