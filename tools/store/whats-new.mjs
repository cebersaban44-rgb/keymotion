// The Microsoft Store's "What's new" text of each version (docs site:
// developers/microsoft-store). One plain-text file per Store listing
// language, written by hand for the Store copy: store/whats-new/<version>.<lang>.txt.
//
//   - tools/release/prepare.mjs puts a skeleton there (a line with
//     SKELETON_MARKER), which a person replaces.
//   - tools/release/check-release.mjs fails the release while a file is
//     missing, empty, still the skeleton or longer than the Store takes.
//   - tools/store/submit.mjs sends each file as its listing's release notes.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The languages of Keymotion's Store listing. A file for each is required, and
 * submit.mjs maps each to the listing whose language it is ("en" → "en-us").
 * Add a language here when the listing gets one in Partner Center.
 */
export const STORE_LANGUAGES = ["en", "tr"];

/** The Store's limit for "What's new", in characters. */
export const WHATS_NEW_MAX = 1500;

/** What a skeleton line starts with, in every file prepare.mjs writes. */
export const SKELETON_MARKER = "TODO(release)";

const LANGUAGE_NAME = { en: "English", tr: "Turkish" };

/** store/whats-new/<version>.<lang>.txt under `root`. */
export const whatsNewPath = (root, version, lang) => join(root, "store", "whats-new", `${version}.${lang}.txt`);

/** The repository-relative form of the same path, for messages. */
export const whatsNewFile = (version, lang) => `store/whats-new/${version}.${lang}.txt`;

/** The skeleton prepare.mjs writes: one line that says what to write. */
export function whatsNewSkeleton(version, lang) {
  const language = LANGUAGE_NAME[lang] ?? lang;
  return (
    `${SKELETON_MARKER}: What's new in Keymotion ${version} for the Microsoft Store listing, in ${language}. ` +
    `Plain text, no Markdown, at most ${WHATS_NEW_MAX} characters. Only what the Store copy gets: ` +
    `leave out what concerns the GitHub copy alone (its own update options). Replace this line.\n`
  );
}

/** The text as the Store gets it: line endings as LF, no blank edges. */
export const whatsNewText = (raw) => raw.replace(/\r\n?/g, "\n").trim();

/** What is wrong with one file's text, or null. */
export function whatsNewProblem(raw) {
  const text = whatsNewText(raw);
  if (!text) return "is empty";
  if (text.includes(SKELETON_MARKER)) return `is still the skeleton (remove the ${SKELETON_MARKER} line)`;
  const length = [...text].length;
  if (length > WHATS_NEW_MAX) return `has ${length} characters; the Store takes at most ${WHATS_NEW_MAX}`;
  return null;
}

/**
 * Every Store language's text for `version`: `{ notes: { en: "...", ... },
 * problems: ["store/whats-new/1.2.0.tr.txt is missing", ...] }`.
 */
export function readWhatsNew(root, version, languages = STORE_LANGUAGES) {
  const notes = {};
  const problems = [];
  for (const lang of languages) {
    const file = whatsNewFile(version, lang);
    const path = whatsNewPath(root, version, lang);
    if (!existsSync(path)) {
      problems.push(`${file} is missing (node tools/release/prepare.mjs writes a skeleton)`);
      continue;
    }
    const raw = readFileSync(path, "utf8");
    const problem = whatsNewProblem(raw);
    if (problem) problems.push(`${file} ${problem}`);
    else notes[lang] = whatsNewText(raw);
  }
  return { notes, problems };
}
