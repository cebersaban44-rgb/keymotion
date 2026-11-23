#!/usr/bin/env node
// Release files for the Tauri updater and the GitHub release page
// (.github/workflows/release.yml, roadmap stage 6).
//
// Writes two files next to the setup .exe:
//   - latest.json: the static updater manifest the app downloads. Its `notes`
//     is a JSON string, {"en": "...", "tr": "...", "ru": "...", "zh-CN": "..."}
//     (English always, the others when there are notes in that language); the
//     app parses it and shows the notes in the interface language.
//   - release-notes.md: the body of the draft release: a download button for
//     the setup file, the CHANGELOG.md section for the version (folded when
//     long) and the SHA-256 of the setup file.
//
// Release notes:
//   - English: the CHANGELOG.md section whose heading is the version, in Keep
//     a Changelog form: "## [0.2.0] - 2026-12-01" ("## 0.2.0" and "## [v0.2.0]"
//     work too). A tag build fails when there is none.
//   - The other interface languages (optional): the same kind of section in
//     CHANGELOG.tr.md, CHANGELOG.ru.md and CHANGELOG.zh-CN.md, when the file
//     exists (--notes ru=path reads another file; --tr-notes path is the same
//     as --notes tr=path).
//   - --allow-unreleased (test builds from a branch): when the version has no
//     section, the "## [Unreleased]" section is used instead ("Yayımlanmamış",
//     "Не выпущено" and "未发布" are accepted as the same heading).
//
// Usage:
//   node tools/release/latest-json.mjs --version 0.2.0 \
//     --setup release-out/Keymotion_0.2.0_x64-setup.exe --sha256 <hex> \
//     [--repository owner/repo] [--allow-unreleased] [--out-dir dir] \
//     [--changelog CHANGELOG.md] [--notes ru=CHANGELOG.ru.md ...] [--tr-notes CHANGELOG.tr.md]
//   node tools/release/latest-json.mjs --check-notes --version 0.2.0 \
//     [--allow-unreleased]     only checks that the notes exist (before the
//                              build, so a tag without notes fails early)
//
// <setup>.sig must exist (tauri build writes it when
// bundle.createUpdaterArtifacts is true). --repository defaults to
// GITHUB_REPOSITORY, --out-dir to the setup file's folder.

import { existsSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const root = fileURLToPath(new URL("../..", import.meta.url));

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const UNRELEASED = new Set(["unreleased", "yayımlanmamış", "не выпущено", "未发布"]);
/** The interface languages besides English, in the order the notes list them. */
export const NOTE_LANGUAGES = ["tr", "ru", "zh-CN"];
// "## [0.2.0] - 2026-12-01", "## 0.2.0", "## [v0.2.0]", "## [Unreleased]",
// "## [Не выпущено]" (a label in brackets may have spaces).
const SECTION_HEADING = /^##\s+(?:\[(?:v(?=\d))?([^\]]+)\]|(?:v(?=\d))?([^\]\s]+))(?:\s.*)?$/;
const NEXT_SECTION = /^#{1,2}\s/;
// "[0.2.0]: https://..." link definitions, which Keep a Changelog puts after
// the last section.
const LINK_DEFINITION = /^\s{0,3}\[[^\]]+\]:\s*\S/;

/** Whether a CHANGELOG heading's label is the Unreleased one, in any language. */
export const isUnreleased = (label) => UNRELEASED.has(label.toLowerCase());

/** The body of the "## <label>" section, or null when there is none. */
function section(markdown, matches) {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const start = lines.findIndex((l) => {
    const m = SECTION_HEADING.exec(l.trimEnd());
    return m !== null && matches(m[1] ?? m[2]);
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

const order = (lang) => {
  const i = NOTE_LANGUAGES.indexOf(lang);
  return i < 0 ? NOTE_LANGUAGES.length : i;
};

/** The Tauri static updater manifest (latest.json) for the Windows build. */
export function latestJson({ version, notes, pubDate, url, signature }) {
  if (!SEMVER.test(version ?? "")) throw new Error(`not a version: ${JSON.stringify(version)}`);
  if (!notes?.en) throw new Error("English notes are required");
  if (!signature?.trim()) throw new Error("the updater signature is empty");
  if (!url?.startsWith("https://")) throw new Error(`not an https URL: ${JSON.stringify(url)}`);
  // English first, then the others that have notes (empty ones are left out).
  const others = Object.keys(notes)
    .filter((lang) => lang !== "en" && typeof notes[lang] === "string" && notes[lang].trim())
    .sort((a, b) => order(a) - order(b) || a.localeCompare(b));
  const text = Object.fromEntries([["en", notes.en], ...others.map((lang) => [lang, notes[lang]])]);
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

// Notes with more items than this are folded under "Everything in this
// release", so the download button stays near the top of the page.
const FOLD_AFTER_ITEMS = 8;

/** Text for a shields.io static badge: "-" and "_" doubled, spaces as "_". */
const badgeText = (text) => encodeURIComponent(text.replace(/-/g, "--").replace(/_/g, "__").replace(/ /g, "_"));

/** 5357915 → "5.1 MB" (MiB, as GitHub and Windows show it). */
export const formatSize = (bytes) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;

/**
 * The notes with a long list folded: the lines before the first heading or
 * list item stay visible, the rest goes into a <details> block when it has
 * more than FOLD_AFTER_ITEMS top-level items.
 */
export function foldNotes(notes) {
  const lines = notes.split("\n");
  const start = lines.findIndex((l) => /^(#{3,6}\s|[-*+]\s)/.test(l));
  if (start < 0) return notes;
  const items = lines.slice(start).filter((l) => /^[-*+]\s/.test(l)).length;
  if (items <= FOLD_AFTER_ITEMS) return notes;
  const intro = lines.slice(0, start).join("\n").trim();
  const list = lines.slice(start).join("\n").trim();
  const folded = `<details>\n<summary><b>Everything in this release</b> (${items} items)</summary>\n\n${list}\n\n</details>`;
  return intro ? `${intro}\n\n${folded}` : folded;
}

/** A line that starts a block of its own: a heading, a list item, a quote,
 *  a table row, HTML, a rule or a code fence. */
const BLOCK_START = /^\s*(#{1,6}\s|[-*+]\s|\d+[.)]\s|>|\||<|---|```|~~~)/;

/**
 * The notes with each paragraph and list item on one line. The CHANGELOGs
 * wrap their lines at about 75 characters, and GitHub shows every line break
 * in a release's text as a break, so the release page showed the notes as a
 * narrow, ragged column. A line joins the one before when both are plain
 * text; blank lines, the lines that start a block (BLOCK_START) and
 * everything inside a code block stay as they are.
 */
export function unwrapNotes(notes) {
  const out = [];
  let code = false;
  let joinable = false;
  for (const line of notes.split("\n")) {
    if (/^\s*(```|~~~)/.test(line)) {
      code = !code;
      out.push(line);
      joinable = false;
      continue;
    }
    if (code || !line.trim()) {
      out.push(line);
      joinable = false;
      continue;
    }
    const starts = BLOCK_START.test(line);
    if (joinable && !starts) {
      out[out.length - 1] = `${out[out.length - 1].trimEnd()} ${line.trim()}`;
      continue;
    }
    out.push(line);
    // Paragraphs and list items take the next line; headings, tables, quotes and HTML don't.
    joinable = !starts || /^\s*([-*+]\s|\d+[.)]\s)/.test(line);
  }
  return out.join("\n");
}

/** Keymotion in the Microsoft Store: the Store app opens on its page. */
export const STORE_URL = "https://apps.microsoft.com/detail/9NL03GDZZ0ZS?referrer=appbadge&mode=full";
/** Each language's name in the script's own messages. */
const LANGUAGE_NAME = { tr: "Turkish", ru: "Russian", "zh-CN": "Chinese" };

/** The link to each language's CHANGELOG under the release notes. */
const NOTES_LINK = { tr: "Türkçe sürüm notları", ru: "Русский", "zh-CN": "简体中文" };

/** Microsoft's own badge, unchanged: dark on light pages, light on dark. */
const STORE_BADGE = (theme) => `https://get.microsoft.com/images/en-us%20${theme}.svg`;

/**
 * The draft release's body. GitHub always lists the release files at the
 * bottom, so the page starts with a download button for the setup file (with
 * its size and what it needs) and, beside it, Microsoft's "Get it from
 * Microsoft" badge: the same version comes out in the Microsoft Store the
 * same day (pre-releases don't). While releases are unsigned, the Windows
 * warning follows; then the notes, one line per paragraph (unwrapNotes) and
 * folded when long; then how to install,
 * the setup file's SHA-256 and the links SignPath asks a download page to
 * carry (code signing policy, privacy policy), and links to the notes in
 * `languages` (CHANGELOG.<lang>.md; Turkish by default). `signed` drops the
 * Windows warning once releases are signed (roadmap stage 9). The buttons
 * need `repository` and `version`. A pre-release says how it differs: a
 * normal Keymotion never updates to it (the updater reads GitHub's latest
 * release), and next to the Store copy it steps aside (handoff.rs).
 */
export function releaseBody({ notes, fileName, sha256, size, preface, repository, version, signed = false, languages = ["tr"] }) {
  if (!/^[0-9a-f]{64}$/.test(sha256 ?? "")) throw new Error(`not a SHA-256: ${JSON.stringify(sha256)}`);
  const parts = [];
  if (preface) parts.push(`> ${preface}`);
  const prerelease = /^\d+\.\d+\.\d+-/.test(version ?? "");
  const store = Boolean(version) && !prerelease;
  if (repository && version) {
    const url = releaseAssetUrl({ repository, version, fileName });
    const badge = `https://img.shields.io/badge/${badgeText("Download for Windows")}-${badgeText(`Keymotion ${version}`)}-d65f12?style=for-the-badge`;
    const facts = ["Windows 10 and 11, 64-bit", size ? formatSize(size) : null, "no administrator rights"];
    const storeBadge = store
      ? ` &nbsp; <a href="${STORE_URL.replace("&", "&amp;")}"><picture>` +
        `<source media="(prefers-color-scheme: dark)" srcset="${STORE_BADGE("light")}">` +
        `<img alt="Get it from Microsoft" src="${STORE_BADGE("dark")}" height="40"></picture></a>`
      : "";
    parts.push(
      `<p align="center"><a href="${url}"><img alt="Download Keymotion ${version} for Windows" src="${badge}" height="40"></a>${storeBadge}</p>\n` +
        `<p align="center"><sub>${facts.filter(Boolean).join(" · ")}</sub></p>`,
    );
  }
  if (!signed) {
    const storeLine = store ? " The Microsoft Store copy of this version is signed by Microsoft and installs without it." : "";
    parts.push(
      `> [!NOTE]\n> This release isn't signed yet, so Windows may say "Windows protected your PC". Select **More info**, then **Run anyway**.${storeLine}`,
    );
  }
  const install = [
    `Download **\`${fileName}\`** with the button above or from the files below, and run it. It works on Windows 10 and 11, 64-bit, and installs for your user account without administrator rights. Connect the keyboard with the USB cable, with the switch on its back in the middle.`,
    `Of the files below, only the \`.exe\` is needed: the \`.sig\` file and \`latest.json\` are for the app's own updates.`,
    ...(store
      ? [`Prefer the Microsoft Store? The same version is there, signed by Microsoft: [Keymotion in the Microsoft Store](${STORE_URL}).`]
      : []),
    ...(prerelease
      ? [
          "This is a pre-release, for trying the next version early. Keymotion from a normal release never updates to it by itself: install it from this page. Once installed, it finds the next pre-release and the next release by itself, or open **Settings › About** and select **Check for updates**.",
          "Pre-releases are only on GitHub. If Keymotion from the Microsoft Store is installed, this copy steps aside for it and opens the Store copy instead: to try the pre-release, remove the Store copy first (your settings stay).",
        ]
      : ["Already using Keymotion? It finds the new version by itself, or open **Settings › About** and select **Check for updates**."]),
  ];
  const links = [
    `[User guide](${SITE}/guide/install/)`,
    `[Code signing policy](${SITE}/about/code-signing/)`,
    `[Privacy policy](${SITE}/about/privacy/)`,
  ];
  if (repository && version) {
    for (const lang of languages) {
      links.push(`[${NOTES_LINK[lang] ?? lang}](https://github.com/${repository}/blob/v${version}/CHANGELOG.${lang}.md)`);
    }
  }
  parts.push(foldNotes(unwrapNotes(notes)), "---", "### Install", ...install, `SHA-256 of \`${fileName}\`: \`${sha256}\``, links.join(" · "));
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
      notes: { type: "string", multiple: true, default: [] },
      "tr-notes": { type: "string" },
    },
  });
  // Where each language's notes are: CHANGELOG.<lang>.md next to CHANGELOG.md
  // unless named.
  const notesFiles = Object.fromEntries(
    NOTE_LANGUAGES.map((lang) => [lang, join(dirname(opt.changelog), `CHANGELOG.${lang}.md`)]),
  );
  if (opt["tr-notes"]) notesFiles.tr = opt["tr-notes"];
  for (const pair of opt.notes) {
    const m = /^([A-Za-z]{2,3}(?:-[A-Za-z0-9]+)*)=(.+)$/.exec(pair);
    if (!m) throw new Error(`--notes takes lang=path (ru=CHANGELOG.ru.md), got ${JSON.stringify(pair)}`);
    notesFiles[m[1]] = m[2];
  }
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
  // The other languages: their section for the version, when the file exists.
  const translations = {};
  const files = [];
  for (const [lang, file] of Object.entries(notesFiles)) {
    if (!existsSync(file)) continue;
    files.push(lang);
    const s = changelogSection(readFileSync(file, "utf8"), version, { allowUnreleased });
    if (s?.body) translations[lang] = s;
  }
  const langs = Object.keys(translations);
  const found =
    `CHANGELOG.md [${en.label}]` +
    (langs.length ? `, ${langs.map((l) => `${LANGUAGE_NAME[l] ?? l} [${translations[l].label}]`).join(", ")}` : ", no other languages");
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
    notes: { en: en.body, ...Object.fromEntries(langs.map((l) => [l, translations[l].body])) },
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
    size: statSync(setup).size,
    preface,
    repository: opt.repository,
    version,
    signed: opt.signed,
    // A link for each language whose CHANGELOG is in the repository.
    languages: files.sort((a, b) => order(a) - order(b)),
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
