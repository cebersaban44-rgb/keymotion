#!/usr/bin/env node
// Release files for the Tauri updater and the GitHub release page
// (.github/workflows/release.yml, roadmap stage 6).
//
// Writes two files next to the setup .exe:
//   - latest.json: the static updater manifest the app downloads. Its `notes`
//     is a JSON string, {"en": "...", "tr": "..."}; the app parses it and
//     shows the notes in the interface language ("tr" only when there are
//     Turkish notes).
//   - release-notes.md: the body of the draft release: the CHANGELOG.md
//     section for the version and the SHA-256 of the setup file.
//
// Release notes:
//   - English: the CHANGELOG.md section whose heading is the version, in Keep
//     a Changelog form: "## [0.2.0] - 2026-12-01" ("## 0.2.0" and "## [v0.2.0]"
//     work too). A tag build fails when there is none.
//   - Turkish (optional): the same kind of section in docs/surum-notlari.md.
//   - --allow-unreleased (test builds from a branch): when the version has no
//     section, the "## [Unreleased]" section is used instead ("Yayımlanmamış"
//     is accepted as the same heading in the Turkish file).
//
// Usage:
//   node tools/release/latest-json.mjs --version 0.2.0 \
//     --setup release-out/Keymotion_0.2.0_x64-setup.exe --sha256 <hex> \
//     [--repository owner/repo] [--allow-unreleased] [--out-dir dir] \
//     [--changelog CHANGELOG.md] [--tr-notes docs/surum-notlari.md]
//   node tools/release/latest-json.mjs --check-notes --version 0.2.0 \
//     [--allow-unreleased]     only checks that the notes exist (before the
//                              build, so a tag without notes fails early)
//
// <setup>.sig must exist (tauri build writes it when
// bundle.createUpdaterArtifacts is true). --repository defaults to
// GITHUB_REPOSITORY, --out-dir to the setup file's folder.

import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const root = fileURLToPath(new URL("../..", import.meta.url));

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const UNRELEASED = new Set(["unreleased", "yayımlanmamış"]);
// "## [0.2.0] - 2026-12-01", "## 0.2.0", "## [v0.2.0]", "## [Unreleased]".
const SECTION_HEADING = /^##\s+\[?(?:v(?=\d))?([^\]\s]+)\]?(?:\s.*)?$/;
const NEXT_SECTION = /^#{1,2}\s/;
// "[0.2.0]: https://..." link definitions, which Keep a Changelog puts after
// the last section.
const LINK_DEFINITION = /^\s{0,3}\[[^\]]+\]:\s*\S/;

const isUnreleased = (label) => UNRELEASED.has(label.toLowerCase());

/** The body of the "## <label>" section, or null when there is none. */
function section(markdown, matches) {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const start = lines.findIndex((l) => {
    const m = SECTION_HEADING.exec(l.trimEnd());
    return m !== null && matches(m[1]);
  });
  if (start < 0) return null;
  const body = [];
  for (const l of lines.slice(start + 1)) {
    if (NEXT_SECTION.test(l)) break;
    body.push(l);
  }
  while (body.length && (body.at(-1).trim() === "" || LINK_DEFINITION.test(body.at(-1)))) body.pop();
  return body.join("\n").trim();
}

/**
 * The changelog section for `version`: `{ label, body }`, where `label` is the
 * version or "Unreleased", or null when there is no section. With
 * `allowUnreleased`, the Unreleased section stands in for a missing version.
 */
export function changelogSection(markdown, version, { allowUnreleased = false } = {}) {
  const body = section(markdown, (label) => label === version);
  if (body !== null) return { label: version, body };
  if (!allowUnreleased) return null;
  const unreleased = section(markdown, isUnreleased);
  return unreleased === null ? null : { label: "Unreleased", body: unreleased };
}

/** Where GitHub serves a release file: .../releases/download/v<version>/<file>. */
export function releaseAssetUrl({ repository, version, fileName }) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "")) {
    throw new Error(`repository must be "owner/repo", got ${JSON.stringify(repository)}`);
  }
  return `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(fileName)}`;
}

/** RFC 3339 in UTC, without milliseconds: 2026-11-12T09:30:00Z. */
const rfc3339 = (date) => new Date(date).toISOString().replace(/\.\d{3}Z$/, "Z");

/** The Tauri static updater manifest (latest.json) for the Windows build. */
export function latestJson({ version, notes, pubDate, url, signature }) {
  if (!SEMVER.test(version ?? "")) throw new Error(`not a version: ${JSON.stringify(version)}`);
  if (!notes?.en) throw new Error("English notes are required");
  if (!signature?.trim()) throw new Error("the updater signature is empty");
  if (!url?.startsWith("https://")) throw new Error(`not an https URL: ${JSON.stringify(url)}`);
  const text = notes.tr ? { en: notes.en, tr: notes.tr } : { en: notes.en };
  return {
    version,
    notes: JSON.stringify(text),
    pub_date: rfc3339(pubDate),
    platforms: {
      "windows-x86_64": { signature: signature.trim(), url },
    },
  };
}

const SITE = "https://cebersaban44-rgb.github.io/keymotion";

/**
 * The draft release's body: the notes, how to install, the setup file's
 * SHA-256 and the links SignPath asks a download page to carry (code signing
 * policy, privacy policy). `signed` drops the Windows warning once releases
 * are signed (roadmap stage 9).
 */
export function releaseBody({ notes, fileName, sha256, preface, repository, version, signed = false }) {
  if (!/^[0-9a-f]{64}$/.test(sha256 ?? "")) throw new Error(`not a SHA-256: ${JSON.stringify(sha256)}`);
  const parts = [];
  if (preface) parts.push(`> ${preface}`);
  const install = [
    `Download **\`${fileName}\`** below and run it. It works on Windows 10 and 11, 64-bit, and installs for your user account without administrator rights. Connect the keyboard with the USB cable, with the switch on its back in the middle.`,
  ];
  if (!signed) {
    install.push(
      `> [!NOTE]\n> This release isn't signed yet, so Windows may say "Windows protected your PC". Select **More info**, then **Run anyway**.`,
    );
  }
  install.push(
    "Already using Keymotion? It finds the new version by itself, or open **Settings › About** and select **Check for updates**.",
  );
  const links = [
    `[User guide](${SITE}/guide/install/)`,
    `[Code signing policy](${SITE}/about/code-signing/)`,
    `[Privacy policy](${SITE}/about/privacy/)`,
  ];
  if (repository && version) {
    links.push(`[Türkçe sürüm notları](https://github.com/${repository}/blob/v${version}/docs/surum-notlari.md)`);
  }
  parts.push(notes, "---", "### Install", ...install, `SHA-256 of \`${fileName}\`: \`${sha256}\``, links.join(" · "));
  return `${parts.join("\n\n")}\n`;
}

function main() {
  const { values: opt } = parseArgs({
    options: {
      version: { type: "string" },
      setup: { type: "string" },
      sha256: { type: "string" },
      repository: { type: "string", default: process.env.GITHUB_REPOSITORY },
      "allow-unreleased": { type: "boolean", default: false },
      "check-notes": { type: "boolean", default: false },
      signed: { type: "boolean", default: false },
      "out-dir": { type: "string" },
      changelog: { type: "string", default: join(root, "CHANGELOG.md") },
      "tr-notes": { type: "string", default: join(root, "docs", "surum-notlari.md") },
    },
  });
  const required = opt["check-notes"] ? ["version"] : ["version", "setup", "sha256", "repository"];
  for (const name of required) {
    if (!opt[name]) throw new Error(`--${name} is required`);
  }
  const { version, setup } = opt;
  const allowUnreleased = opt["allow-unreleased"];

  const en = changelogSection(readFileSync(opt.changelog, "utf8"), version, { allowUnreleased });
  if (!en) {
    throw new Error(
      `${opt.changelog} has no section for ${version}. Add "## [${version}] - <date>" before tagging` +
        (allowUnreleased ? " (there is no Unreleased section either)." : "."),
    );
  }
  if (!en.body) throw new Error(`the ${en.label} section of ${opt.changelog} is empty`);
  const tr = existsSync(opt["tr-notes"])
    ? changelogSection(readFileSync(opt["tr-notes"], "utf8"), version, { allowUnreleased })
    : null;
  const found = `CHANGELOG.md [${en.label}]${tr?.body ? `, Turkish [${tr.label}]` : ", no Turkish notes"}`;
  if (opt["check-notes"]) {
    console.log(`Release notes for ${version}: ${found}.`);
    return;
  }

  const fileName = basename(setup);
  const sigPath = `${setup}.sig`;
  if (!existsSync(setup)) throw new Error(`${setup} not found`);
  if (!existsSync(sigPath)) {
    throw new Error(`${sigPath} not found; is bundle.createUpdaterArtifacts true in tauri.conf.json?`);
  }
  const manifest = latestJson({
    version,
    notes: { en: en.body, tr: tr?.body || undefined },
    pubDate: new Date(),
    url: releaseAssetUrl({ repository: opt.repository, version, fileName }),
    signature: readFileSync(sigPath, "utf8"),
  });
  const preface =
    en.label === version
      ? undefined
      : `Test build: ${version} has no CHANGELOG.md section yet, so these are the Unreleased notes.`;
  const body = releaseBody({
    notes: en.body,
    fileName,
    sha256: opt.sha256,
    preface,
    repository: opt.repository,
    version,
    signed: opt.signed,
  });

  const outDir = opt["out-dir"] ?? dirname(setup);
  writeFileSync(join(outDir, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  writeFileSync(join(outDir, "release-notes.md"), body);
  console.log(`Notes: ${found}.`);
  console.log(`Wrote ${join(outDir, "latest.json")} (${manifest.platforms["windows-x86_64"].url}).`);
  console.log(`Wrote ${join(outDir, "release-notes.md")}.`);
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
