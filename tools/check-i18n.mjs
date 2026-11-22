#!/usr/bin/env node
// Checks the interface's dictionaries (app/src/lib/i18n/: en.ts is the
// source; tr.ts, ru.ts, zh-CN.ts are its translations). The rules are the
// "Denetimler" section of the plan for Russian and Chinese (2026-11-22):
//
//   1. Keys: no key that English lacks; no `.one` key without its base key;
//      Turkish has every key; Russian and Chinese write counts with plural
//      syntax, so they have no `.one` keys. A key Russian or Chinese lacks
//      shows in English: a warning, an error with --release.
//   2. Placeholders: the same {names} as the English text ("#" aside).
//   3. Plurals: every text parses (the app's own parser, plural.ts); every
//      Russian plural has one, few, many and other; Chinese has no plurals;
//      Russian should pluralize where English has a `.one` form (warning).
//   4. Not translated: a Russian or Chinese text equal to the English one,
//      unless the key is in SAME_IN_EVERY_LANGUAGE or has no letters.
//   5. Script: Russian text has Cyrillic letters, Chinese text has Chinese
//      characters, neither has Turkish letters (or the other's script); a
//      Latin word in Russian that the English text lacks is a warning.
//   6. Punctuation and tone: no "!" anywhere; Chinese uses full-width
//      punctuation, "…" and no 您, and has a space between Chinese and
//      Latin letters or digits (warning); Russian uses " — " for a dash, a
//      no-break space before "%" and «» quotes (warnings), and no
//      "пожалуйста"; a label without a full stop in English has none in the
//      translation.
//   7. Glossary: words the plan forbids (трей, донгл, 托盘, 您). TODO: the
//      approved terms, once app/src/lib/i18n/glossary.json exists.
//   8. Length: a short English label (under 20 characters) whose Russian
//      text is over 1.5 times as long, or whose Chinese text is wider (a
//      Chinese character counts twice): warnings, listed with --details.
//   9. Stale translations: i18n/meta.json keeps a hash of the English text
//      each Russian and Chinese text was translated from (`--stamp ru`
//      writes it for the keys there now); a changed English text makes the
//      translation stale: a warning, an error with --release.
//  10. The Riot Games legal line (games.lol.legal) stays in English,
//      character for character, in every language.
//
// Rule 11 (broken characters: Cyrillic and Chinese mojibake) is in
// check-text.mjs, which reads every file.
//
// Usage: node tools/check-i18n.mjs              warnings for what is missing
//        node tools/check-i18n.mjs --release    missing and stale are errors
//        node tools/check-i18n.mjs --details    list every warning
//        node tools/check-i18n.mjs --stamp ru   record the English behind ru.ts

import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

// Node loads the app's TypeScript as it is (type stripping), so the checks
// use the parser the app uses.
import { parseMessage, placeholderNames, plurals } from "../app/src/lib/i18n/plural.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
export const DICT_DIR = join(root, "app", "src", "lib", "i18n");

/** Each language's file and the name it exports. English is the source. */
export const LANGUAGES = [
  { lang: "en", file: "en.ts", name: "en" },
  { lang: "tr", file: "tr.ts", name: "tr" },
  { lang: "ru", file: "ru.ts", name: "ru" },
  { lang: "zh-CN", file: "zh-CN.ts", name: "zhCN" },
];
/** Filled in over time: a missing key shows in English. */
const PARTIAL = new Set(["ru", "zh-CN"]);

/**
 * Keys whose text may be the same as the English one in Russian and Chinese:
 * names (of the app, of other products and services), file formats, key
 * names printed on the keyboard, placeholders only. Add a key here when its
 * English text is right in every language.
 */
export const SAME_IN_EVERY_LANGUAGE = new Set([
  "app.name",
  "conn.usb.short",
  "conn.dongle.short",
  "conn.bluetooth.short",
  "sc.drop.formats",
  "sc.tx.sample.name.s",
  "calendar.p.outlook",
  "calendar.p.apple",
  "calendar.placeholder.google",
  "calendar.placeholder.outlook",
  "calendar.placeholder.apple",
  "calendar.placeholder.other",
  "games.source.steam",
  "games.source.epic",
  "games.source.gog",
  "games.source.xbox",
  "games.source.riot",
  "games.source.windows",
  "games.keys.wasd",
  "games.keys.moba",
  "games.card.cs",
  "games.card.kda",
  "games.lol.legal",
]);
/** And in Russian only (Chinese writes 蓝牙). */
const SAME_IN_RUSSIAN = new Set(["conn.bluetooth"]);

/** Stays in English, character for character (Riot's policy). */
export const ENGLISH_ONLY = new Set(["games.lol.legal"]);

/** Words the plan rules out (glossary: "yasak" column). */
const FORBIDDEN = {
  ru: [
    { re: /трей/iu, why: "трей (say where the icon is: значок Keymotion рядом с часами)" },
    { re: /донгл/iu, why: "донгл (USB-приёмник)" },
    { re: /пожалуйста/iu, why: "пожалуйста (no 'please' in the interface)" },
  ],
  "zh-CN": [
    { re: /托盘/u, why: "托盘 (say where the icon is: 任务栏时钟旁的 Keymotion 图标)" },
    { re: /您/u, why: "您 (use 你, or no pronoun)" },
  ],
};

const RUSSIAN_PLURAL_FORMS = ["one", "few", "many", "other"];
const HAN = /\p{Script=Han}/u;
const CYRILLIC = /\p{Script=Cyrillic}/u;
const TURKISH = /[ğĞıİşŞ]/u;
const LETTER = /\p{L}/u;

/** The literal text of parsed parts (all plural forms, no placeholders). */
function literals(parts) {
  const out = [];
  const walk = (ps) => {
    for (const p of ps) {
      if (typeof p === "string") out.push(p);
      else if (p.kind === "plural") Object.values(p.forms).forEach(walk);
    }
  };
  walk(parts);
  return out;
}

const sorted = (set) => [...set].sort().join(", ") || "none";
const isLabel = (en) => !/[.?!…:。]\s*$/u.test(en) && !/\}\s*$/u.test(en);
/** Display width: a Chinese (or other wide) character counts twice. */
const width = (s) => [...s].reduce((n, ch) => n + (/[\p{Script=Han}\u3000-\u303F\uFF00-\uFFEF]/u.test(ch) ? 2 : 1), 0);

export const hashOf = (text) => createHash("sha256").update(text).digest("hex").slice(0, 16);

/**
 * Findings for the dictionaries `dicts` ({ en, tr, ru, "zh-CN" }: key → text).
 * Each is { level: "error" | "warning", lang, key, rule, message, bulk? };
 * `bulk` warnings are summed up unless listed with --details.
 * `meta` is i18n/meta.json's content, or null when there is none yet.
 */
export function checkDictionaries(dicts, { release = false, meta = null } = {}) {
  const findings = [];
  const add = (level, lang, key, rule, message, bulk = false) => findings.push({ level, lang, key, rule, message, bulk });
  const en = dicts.en;
  const enKeys = new Set(Object.keys(en));
  const parsed = new Map();

  // Every text parses (rule 3); the parts are kept for the other rules.
  for (const [lang, dict] of Object.entries(dicts)) {
    for (const [key, text] of Object.entries(dict)) {
      if (typeof text !== "string") {
        add("error", lang, key, "keys", "is not a string");
        continue;
      }
      try {
        parsed.set(`${lang}\0${key}`, parseMessage(text));
      } catch (e) {
        add("error", lang, key, "plural", `does not parse: ${e.message}`);
      }
    }
  }
  const partsOf = (lang, key) => parsed.get(`${lang}\0${key}`);

  // English: a `.one` key needs its base key.
  for (const key of enKeys) {
    if (key.endsWith(".one") && !enKeys.has(key.slice(0, -4))) {
      add("error", "en", key, "keys", `"${key.slice(0, -4)}" does not exist: a misspelled plural key?`);
    }
  }

  for (const { lang } of LANGUAGES.slice(1)) {
    const dict = dicts[lang] ?? {};
    const partial = PARTIAL.has(lang);
    const stamps = meta?.[lang] ?? null;

    // 1. Keys.
    for (const key of Object.keys(dict)) {
      if (!enKeys.has(key)) add("error", lang, key, "keys", "is not an English key (misspelled, or removed from en.ts)");
      else if (partial && key.endsWith(".one")) {
        add("error", lang, key, "keys", `write the count in "${key.slice(0, -4)}" with plural syntax instead of a .one key`);
      }
    }
    const missing = [...enKeys].filter((k) => !(k in dict) && !(partial && k.endsWith(".one")));
    for (const key of missing) {
      if (partial) add(release ? "error" : "warning", lang, key, "missing", "not translated yet (shows in English)", !release);
      else add("error", lang, key, "missing", "missing");
    }

    for (const [key, text] of Object.entries(dict)) {
      if (!enKeys.has(key) || typeof text !== "string") continue;
      const source = en[key];
      const parts = partsOf(lang, key);
      const enParts = partsOf("en", key);
      if (!parts || !enParts) continue;
      const lits = literals(parts);
      const plain = lits.join(" ");

      // 10. The Riot line.
      if (ENGLISH_ONLY.has(key)) {
        if (text !== source) add("error", lang, key, "riot", "must stay exactly as the English text");
        continue;
      }

      // 2. Placeholders.
      const want = placeholderNames(enParts);
      const have = new Set(placeholderNames(parts));
      // English may show a count only through its `.one` form ("time",
      // "times"); Russian then picks its form with {count, plural, …}.
      if (`${key}.one` in en && !want.has("count") && plurals(parts).some((p) => p.name === "count")) {
        have.delete("count");
      }
      if (sorted(want) !== sorted(have)) {
        add("error", lang, key, "placeholders", `placeholders {${sorted(have)}} differ from English {${sorted(want)}}`);
      }
      if (source !== "" && text.trim() === "") add("error", lang, key, "missing", "is empty");

      // 3. Plurals.
      const forms = plurals(parts);
      if (lang === "ru") {
        for (const p of forms) {
          const lacking = RUSSIAN_PLURAL_FORMS.filter((f) => !p.forms.includes(f));
          if (lacking.length) add("error", lang, key, "plural", `the plural of {${p.name}} needs ${lacking.join(", ")} (one/few/many/other)`);
        }
        const counted = new Set(plurals(enParts).map((p) => p.name));
        if (`${key}.one` in en) counted.add("count");
        for (const name of counted) {
          if (want.has(name) && !forms.some((p) => p.name === name)) {
            add("warning", lang, key, "plural", `English changes with {${name}}; Russian usually needs {${name}, plural, one {…} few {…} many {…} other {…}}`);
          }
        }
      }
      if (lang === "zh-CN" && forms.length) add("error", lang, key, "plural", "Chinese has one form after a number: write it without plural syntax");

      if (!partial) continue;

      // 4. Not translated.
      const mayBeSame = SAME_IN_EVERY_LANGUAGE.has(key) || (lang === "ru" && SAME_IN_RUSSIAN.has(key));
      const hasLetters = LETTER.test(plain);
      if (text === source && hasLetters && !mayBeSame) add("error", lang, key, "untranslated", "is the English text");

      // 5. Script.
      if (hasLetters && !mayBeSame) {
        if (lang === "ru" && !CYRILLIC.test(plain)) add("error", lang, key, "script", "has no Cyrillic letters");
        if (lang === "zh-CN" && !HAN.test(plain)) add("error", lang, key, "script", "has no Chinese characters");
      }
      if (TURKISH.test(plain)) add("error", lang, key, "script", "has Turkish letters");
      if (lang === "ru" && HAN.test(plain)) add("error", lang, key, "script", "has Chinese characters");
      if (lang === "zh-CN" && CYRILLIC.test(plain)) add("error", lang, key, "script", "has Cyrillic letters");
      if (lang === "ru") {
        const enWords = new Set((source.match(/[A-Za-z][A-Za-z0-9]*/g) ?? []).map((w) => w.toLowerCase()));
        const foreign = [...new Set(plain.match(/[A-Za-z][A-Za-z0-9]*/g) ?? [])].filter((w) => !enWords.has(w.toLowerCase()));
        if (foreign.length) add("warning", lang, key, "script", `Latin words the English text lacks: ${foreign.join(", ")}`);
      }

      // 6. Punctuation and tone.
      if (lits.some((l) => l.includes("!"))) add("error", lang, key, "tone", 'has "!"');
      // A Russian abbreviation keeps its point ("ср." for "avg").
      const abbreviation = lang === "ru" && /(?:^|\s)\p{L}{1,4}\.$/u.test(text);
      if (isLabel(source) && /[.。]\s*$/u.test(text) && !abbreviation) {
        add("error", lang, key, "punctuation", "ends with a full stop; the English label has none");
      }
      if (lang === "zh-CN") {
        if (lits.some((l) => /\p{Script=Han}[,.;:?!()]/u.test(l) || /[,;:?!(](?=\p{Script=Han})/u.test(l))) {
          add("error", lang, key, "punctuation", "ASCII punctuation next to Chinese: use ，。；：？（）");
        }
        if (text.includes("...")) add("error", lang, key, "punctuation", 'has "..." (use "…")');
        if (lits.some((l) => /\p{Script=Han}[A-Za-z0-9]|[A-Za-z0-9]\p{Script=Han}/u.test(l))) {
          add("warning", lang, key, "punctuation", "no space between Chinese and Latin letters or digits");
        }
        if (/[—–]/u.test(plain)) add("warning", lang, key, "punctuation", "has a dash; Chinese text avoids them");
        if (isLabel(source) && plain.includes("请")) add("warning", lang, key, "tone", "请 in a label or button");
      }
      if (lang === "ru") {
        if (lits.some((l) => / - /.test(l))) add("warning", lang, key, "punctuation", 'has " - " (use " — ")');
        if (/(?:\d|\}| )%/u.test(text)) add("warning", lang, key, "punctuation", 'needs a no-break space before "%" ("40\u00A0%")');
        if (/["“”„]/u.test(plain)) add("warning", lang, key, "punctuation", "quotes: «…» (the plan's choice, until the reviewer decides)");
      }

      // 7. Forbidden words.
      for (const f of FORBIDDEN[lang] ?? []) {
        if (f.re.test(plain)) add("error", lang, key, "glossary", `uses ${f.why}`);
      }

      // 8. Length.
      if (source.length < 20 && LETTER.test(source)) {
        if (lang === "ru" && text.length > source.length * 1.5) {
          add("warning", lang, key, "length", `${text.length} characters, English ${source.length}`, true);
        }
        if (lang === "zh-CN" && width(text) > source.length) {
          add("warning", lang, key, "length", `${width(text)} wide, English ${source.length}`, true);
        }
      }

      // 9. Stale.
      if (stamps) {
        if (!(key in stamps)) add("warning", lang, key, "stale", "no record of the English it was translated from (--stamp)", true);
        else if (stamps[key] !== hashOf(source)) {
          add(release ? "error" : "warning", lang, key, "stale", "the English text changed since it was translated");
        }
      }
    }
  }
  return findings;
}

/** The dictionaries in `dir`, and where each key is (file, line). */
export async function loadDictionaries(dir = DICT_DIR) {
  const dicts = {};
  const where = {};
  for (const { lang, file, name } of LANGUAGES) {
    const path = join(dir, file);
    const mod = await import(pathToFileURL(path).href);
    dicts[lang] = mod[name];
    if (!dicts[lang] || typeof dicts[lang] !== "object") throw new Error(`${path} does not export "${name}"`);
    const lines = {};
    readFileSync(path, "utf8")
      .split("\n")
      .forEach((l, i) => {
        const m = /^\s*"([^"]+)"\s*:/.exec(l);
        if (m && !(m[1] in lines)) lines[m[1]] = i + 1;
      });
    where[lang] = { file: path.slice(root.length).replace(/\\/g, "/"), lines };
  }
  return { dicts, where };
}

function main() {
  const { values: opt } = parseArgs({
    options: {
      release: { type: "boolean", default: false },
      details: { type: "boolean", default: false },
      stamp: { type: "string", multiple: true, default: [] },
    },
  });
  return loadDictionaries().then(({ dicts, where }) => {
    const metaPath = join(DICT_DIR, "meta.json");
    const meta = existsSync(metaPath) ? JSON.parse(readFileSync(metaPath, "utf8")) : null;

    if (opt.stamp.length) {
      const next = meta ?? {};
      for (const lang of opt.stamp) {
        if (!PARTIAL.has(lang)) throw new Error(`--stamp takes ${[...PARTIAL].join(" or ")}, not ${lang}`);
        next[lang] = Object.fromEntries(
          Object.keys(dicts[lang]).filter((k) => k in dicts.en).sort().map((k) => [k, hashOf(dicts.en[k])]),
        );
      }
      writeFileSync(metaPath, `${JSON.stringify(next, null, 2)}\n`);
      console.log(`Recorded the English behind ${opt.stamp.join(", ")} in ${metaPath}.`);
      return 0;
    }

    const findings = checkDictionaries(dicts, { release: opt.release, meta });
    const ci = !!process.env.GITHUB_ACTIONS;
    const shown = findings.filter((f) => !f.bulk || opt.details);
    for (const f of shown) {
      const { file, lines } = where[f.lang];
      const line = lines[f.key] ?? 1;
      const text = `${f.key}: ${f.message} [${f.rule}]`;
      if (ci) console.log(`::${f.level} file=${file},line=${line}::${text}`);
      else console.log(`${file}:${line}: ${f.level}: ${text}`);
    }
    const errors = findings.filter((f) => f.level === "error").length;
    const warnings = findings.length - errors;
    const total = Object.keys(dicts.en).length;
    console.log("");
    for (const lang of PARTIAL) {
      const done = Object.keys(dicts[lang]).filter((k) => k in dicts.en).length;
      const mine = findings.filter((f) => f.lang === lang);
      const missing = mine.filter((f) => f.rule === "missing" && f.message.startsWith("not translated")).length;
      const hidden = opt.details ? [] : mine.filter((f) => f.bulk);
      const other = hidden.filter((f) => f.rule !== "missing").length;
      console.log(
        `${lang}: ${done} of ${total} texts translated` +
          (missing ? `, ${missing} still show in English` : "") +
          (other ? `, ${other} more warnings` : "") +
          (hidden.length ? " (--details lists them)" : "") +
          ".",
      );
    }
    console.log(`Dictionaries: ${errors} error(s), ${warnings} warning(s)${meta ? "" : "; no meta.json yet, so no stale check"}.`);
    return errors ? 1 : 0;
  });
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  // exitCode, not exit(): exiting while output is still being written trips
  // a libuv assertion on Windows.
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (e) => {
      console.log(`${process.env.GITHUB_ACTIONS ? "::error::" : ""}${e.message}`);
      process.exitCode = 1;
    },
  );
}
