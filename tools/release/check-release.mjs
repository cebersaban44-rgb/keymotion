#!/usr/bin/env node
// Is a version's text ready for its release? (docs site: developers/microsoft-store,
// "Making a release"). Fails while any of these is missing, empty or still
// the skeleton tools/release/prepare.mjs wrote:
//   - the version's section in every CHANGELOG (CHANGELOG.md and each
//     translation, such as CHANGELOG.tr.md);
//   - the "## Keymotion X.Y.Z" section of the site's release notes page, in
//     every language that has the page;
//   - the Microsoft Store's "What's new" file of every listing language
//     (store/whats-new/X.Y.Z.<lang>.txt, tools/store/whats-new.mjs).
// A pre-release (1.3.0-rc.1) is only on GitHub and needs none of the last two.
//
// release.yml and store.yml run it first on a release tag, so a tag without
// the text fails in seconds; CI runs it on a pull request into main; and
// tools/release/tag.mjs runs it before it tags.
//
// Usage: node tools/release/check-release.mjs                  the version in the app's files
//        node tools/release/check-release.mjs --version 1.2.0
//        node tools/release/check-release.mjs --tag v1.2.0

import { existsSync, readFileSync, readdirSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { SKELETON_MARKER, readWhatsNew } from "../store/whats-new.mjs";
import { changelogSection } from "./latest-json.mjs";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

/** A release version as prepare.mjs makes them: three numbers. */
export const RELEASE_VERSION = /^\d+\.\d+\.\d+$/;

const CHANGELOG_FILE = /^CHANGELOG(?:\.[A-Za-z]{2,3}(?:-[A-Za-z0-9]+)*)?\.md$/;

/** CHANGELOG.md first, then its translations (CHANGELOG.ru.md, …). */
export function changelogFiles(root) {
  return readdirSync(root)
    .filter((f) => CHANGELOG_FILE.test(f))
    .sort((a, b) => (a === "CHANGELOG.md" ? -1 : b === "CHANGELOG.md" ? 1 : a.localeCompare(b)));
}

const DOCS = ["site", "src", "content", "docs"];
const NOTES_PAGE = ["about", "release-notes.mdx"];

/** The site's release notes page in each language that has one (English first). */
export function releaseNotesPages(root) {
  const docs = join(root, ...DOCS);
  if (!existsSync(docs)) return [];
  const pages = [];
  if (existsSync(join(docs, ...NOTES_PAGE))) pages.push([...DOCS, ...NOTES_PAGE].join("/"));
  for (const entry of readdirSync(docs, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    if (entry.isDirectory() && existsSync(join(docs, entry.name, ...NOTES_PAGE))) {
      pages.push([...DOCS, entry.name, ...NOTES_PAGE].join("/"));
    }
  }
  return pages;
}

/** The heading of a version's section on the release notes page. */
export const notesHeading = (version) => `## Keymotion ${version}`;

/** The body of the page's "## Keymotion <version>" section, or null. */
export function releaseNotesSection(mdx, version) {
  const lines = mdx.replace(/\r\n?/g, "\n").split("\n");
  const start = lines.findIndex((l) => l.trimEnd() === notesHeading(version));
  if (start < 0) return null;
  const body = [];
  for (const l of lines.slice(start + 1)) {
    if (/^#{1,2}\s/.test(l)) break;
    body.push(l);
  }
  return body.join("\n").trim();
}

/** Text a reader would see: without MDX comments ({/* … *\/}). */
const visible = (mdx) => mdx.replace(/\{\/\*[\s\S]*?\*\/\}/g, "").trim();

/**
 * What still keeps `version` from being released: `{ problems, checked }`,
 * where `checked` names what was looked at.
 */
export function releaseProblems(root, version) {
  const problems = [];
  const checked = [];

  for (const file of changelogFiles(root)) {
    checked.push(file);
    const section = changelogSection(readFileSync(join(root, file), "utf8"), version);
    if (!section) problems.push(`${file} has no "## [${version}] - <date>" section`);
    else if (!section.body) problems.push(`${file}: the ${version} section is empty`);
    else if (section.body.includes(SKELETON_MARKER)) problems.push(`${file}: the ${version} section still has ${SKELETON_MARKER}`);
  }
  if (!checked.length) problems.push("no CHANGELOG.md");

  if (version.includes("-")) {
    return { problems, checked, prerelease: true };
  }

  for (const page of releaseNotesPages(root)) {
    checked.push(page);
    const section = releaseNotesSection(readFileSync(join(root, page), "utf8"), version);
    if (section === null) problems.push(`${page} has no "${notesHeading(version)}" section`);
    else if (section.includes(SKELETON_MARKER)) problems.push(`${page}: the ${version} section is still the skeleton (${SKELETON_MARKER})`);
    else if (!visible(section)) problems.push(`${page}: the ${version} section is empty`);
  }

  const whatsNew = readWhatsNew(root, version);
  checked.push(...Object.keys(whatsNew.notes).map((lang) => `What's new (${lang})`));
  problems.push(...whatsNew.problems);
  return { problems, checked, prerelease: false };
}

/** The version in app/src-tauri/tauri.conf.json. */
export const appVersion = (root) => JSON.parse(readFileSync(join(root, "app", "src-tauri", "tauri.conf.json"), "utf8")).version;

function main() {
  const { values: opt } = parseArgs({ options: { version: { type: "string" }, tag: { type: "string" } } });
  if (opt.tag !== undefined && !/^v\d/.test(opt.tag)) throw new Error(`--tag takes a release tag such as v1.2.0, got ${JSON.stringify(opt.tag)}`);
  const version = opt.version ?? opt.tag?.slice(1) ?? appVersion(repoRoot);
  const { problems, checked, prerelease } = releaseProblems(repoRoot, version);
  const ci = !!process.env.GITHUB_ACTIONS;
  if (problems.length) {
    for (const p of problems) console.log(ci ? `::error::Release ${version}: ${p}` : `Release ${version}: ${p}`);
    process.exit(1);
  }
  console.log(
    prerelease
      ? `Release ${version} (pre-release, GitHub only): ${checked.join(", ")} ready.`
      : `Release ${version}: ${checked.join(", ")} ready.`,
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
