#!/usr/bin/env node
// Text hygiene for tracked source and documentation files.
//
// Fails on:
//   - a UTF-8 byte order mark at the start of a file,
//   - bytes that are not valid UTF-8,
//   - the replacement character U+FFFD (a character was lost in a conversion),
//   - mojibake: UTF-8 text that was read with a Windows code page and written
//     back as UTF-8. Each letter turns into two or three characters:
//       - Turkish, code page 1252/1254: "ı" (C4 B1) becomes U+00C4 U+00B1,
//         "ş" (C5 9F) becomes U+00C5 U+0178, "ç" (C3 A7) becomes U+00C3
//         U+00A7, and an em dash (E2 80 94) starts with U+00E2 U+20AC (a
//         PowerShell 5.1 edit did this);
//       - Russian, code page 1252/1254: "т" (D1 82) becomes U+00D1 U+201A,
//         "а" (D0 B0) U+00D0 U+00B0 (U+011E U+00B0 with 1254);
//       - Russian, code page 1251 (a Russian Windows): "т" becomes U+0421
//         U+201A, "а" U+0420 U+00B0;
//       - Chinese (three bytes, E4-E9 then two of 80-BF), code page 1252/1254:
//         "上" (E4 B8 8A) becomes U+00E4 U+00B8 U+0160, "，" (EF BC 8C) U+00EF
//         U+00BC U+0152; code page 1251: "上" becomes U+0434 U+0451 U+0409.
//     The Russian and Chinese patterns only use characters that never follow
//     those letters in real Turkish, Russian or Chinese text
//     (tools/check-text.test.mjs tries both ways).
//
// Lockfiles and the generated TypeScript bindings are skipped. In Markdown, an
// inline code span that contains nothing but a mojibake sample documents the
// problem and is allowed (so notes can list the patterns this way).
//
// Usage: node tools/check-text.mjs            (every tracked file)
//        node tools/check-text.mjs FILE...    (only these files)

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, realpathSync } from "node:fs";
import { extname, basename, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

// .nsh: the installer's scripts and language files. Tauri writes the language
// files again with a BOM of its own, so a BOM in the source makes two and NSIS
// fails (release run, 2026-11-12).
const EXTENSIONS = new Set([
  ".rs", ".ts", ".tsx", ".mjs", ".cjs", ".js", ".md", ".json", ".toml", ".css", ".html", ".yml", ".yaml", ".nsh",
]);
const SKIP_NAMES = new Set(["Cargo.lock", "pnpm-lock.yaml", "package-lock.json", "yarn.lock"]);
const SKIP_PREFIXES = ["app/src/bindings/"];

// Written with escapes so this file does not match itself. Each constant is
// how a UTF-8 lead byte looks when it is decoded with code page 1252/1254.
const C = {
  A_TILDE: "\u00C3", // lead byte C3: ç ö ü Ç Ö Ü ×
  A_DIAER: "\u00C4", // lead byte C4: ı İ ğ Ğ
  A_RING: "\u00C5", // lead byte C5: ş Ş
  A_CIRC: "\u00C2", // lead byte C2: · ° and the no-break space
  a_circ: "\u00E2", // lead byte E2: dashes, curly quotes, ellipsis
};
// Code page 1252/1254, bytes 80-9F: the characters other than C1 controls
// (those are caught on their own). Quotes, dashes and the ellipsis are left
// out of the Russian and Chinese patterns: real text puts them after a letter.
const CP1252_HIGH = "\u20AC\u201A\u0192\u201E\u2020\u2021\u02C6\u2030\u0160\u2039\u0152\u017D\u2022\u02DC\u2122\u0161\u203A\u0153\u017E\u0178";
const CP1252_PUNCT = "\u2026\u2018\u2019\u201C\u201D\u2013\u2014";
// Code page 1251, bytes 80-BF: all of them, and those that are not letters
// of Russian, Ukrainian or Belarusian, quotes, dashes, the ellipsis or spaces.
const CP1251_HIGH =
  "\u0402\u0403\u201A\u0453\u201E\u2026\u2020\u2021\u20AC\u2030\u0409\u2039\u040A\u040C\u040B\u040F\u0452\u2018\u2019\u201C\u201D\u2022\u2013\u2014\u2122\u0459\u203A\u045A\u045C\u045B\u045F" +
  "\u00A0\u040E\u045E\u0408\u00A4\u0490\u00A6\u00A7\u0401\u00A9\u0404\u00AB\u00AC\u00AD\u00AE\u0407\u00B0\u00B1\u0406\u0456\u0491\u00B5\u00B6\u00B7\u0451\u2116\u0454\u00BB\u0458\u0405\u0455\u0457";
const CP1251_ODD =
  "\u0402\u0403\u0453\u0452\u0409\u040A\u040C\u040B\u040F\u0459\u045A\u045C\u045B\u045F\u0408\u0458\u00A4\u00A6\u00A7\u00A9\u00AC\u00AE\u00B0\u00B1\u00B5\u00B6\u00B7\u2116\u0405\u0455\u201A\u2030\u20AC\u2020\u2021\u2122\u203A\u2039";

const MOJIBAKE = new RegExp(
  [
    // Turkish (and any Latin-1 letter).
    C.A_TILDE,
    `${C.A_DIAER}[\u00B0\u00B1\u0178\u017E]`,
    `${C.A_RING}[\u0178\u017E]`,
    `${C.A_CIRC}[\u00A0-\u00BF]`,
    `${C.a_circ}\u20AC`,
    // Russian, code page 1252/1254: lead byte D0 ("Ð", or "Ğ" in
    // 1254) before 90-BF, lead byte D1 ("Ñ") before 80-91.
    `[\u00D0\u011E][\u00A1-\u00BF${CP1252_HIGH}]`,
    `\u00D1[\u20AC\u201A\u0192\u201E\u2026\u2020\u2021\u02C6\u2030\u0160\u2039\u0152\u017D\u2018]`,
    // Russian, code page 1251: "Р" and "С" (lead bytes D0, D1).
    `[\u0420\u0421][${CP1251_ODD}]`,
    // Chinese, code page 1252/1254: lead bytes E3-E9 and EF ("ã"-"é",
    // "ï"), then two continuation bytes.
    `[\u00E3-\u00E9\u00EF][\u00A1-\u00BF${CP1252_HIGH}][\u00A0-\u00BF${CP1252_HIGH}${CP1252_PUNCT}]`,
    // Chinese, code page 1251: lead bytes E3-E9 and EF are "г"-"й"
    // and "п"; one of the two bytes after them is an odd one.
    `[\u0433-\u0439\u043F][${CP1251_HIGH}]?[${CP1251_ODD}]`,
    // C1 control characters never appear in real text. Code page 1254 has no
    // character for byte 9E, the second byte of "Ğ" and "Ş".
    "[\u0080-\u009F]",
  ].join("|"),
  "gu",
);
const MOJIBAKE_ONLY = new RegExp(`^(?:${MOJIBAKE.source})+$`, "u");

/** The mojibake samples in one line of text (empty when there are none). */
export function findMojibake(line) {
  return [...line.matchAll(MOJIBAKE)].map((m) => m[0]);
}

/** `line` without Markdown code spans that only show a mojibake sample. */
export function withoutSamples(line) {
  return line.replace(/`([^`\n]{1,12})`/g, (m, inner) => (MOJIBAKE_ONLY.test(inner) ? "" : m));
}

function trackedFiles() {
  const out = execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8", maxBuffer: 64 << 20 });
  return out.split("\0").filter(Boolean);
}

function wanted(path) {
  if (!EXTENSIONS.has(extname(path).toLowerCase())) return false;
  if (SKIP_NAMES.has(basename(path))) return false;
  return !SKIP_PREFIXES.some((p) => path.startsWith(p));
}

function main() {
  const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  const problems = [];
  const report = (path, line, message) => problems.push({ path, line, message });

  // Explicit arguments are paths relative to the current directory; tracked
  // files are relative to the repository root.
  const args = process.argv.slice(2);
  const files = args.length
    ? args.map((a) => ({ path: a, full: resolve(a) }))
    : trackedFiles().filter(wanted).map((p) => ({ path: p, full: resolve(root, p) }));

  let checked = 0;
  for (const { path, full } of files) {
    if (!existsSync(full)) continue; // deleted in the working tree
    checked++;
    const bytes = readFileSync(full);
    if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
      report(path, 1, "starts with a UTF-8 byte order mark (BOM)");
    }
    let text;
    try {
      text = decoder.decode(bytes);
    } catch {
      report(path, 1, "is not valid UTF-8");
      continue;
    }
    const markdown = extname(path).toLowerCase() === ".md";
    text.split("\n").forEach((raw, i) => {
      if (raw.includes("\uFFFD")) report(path, i + 1, "contains U+FFFD (a character was lost in a conversion)");
      const found = findMojibake(markdown ? withoutSamples(raw) : raw);
      if (found.length) {
        const shown = [...new Set(found)].map((s) =>
          JSON.stringify(s).replace(/[\u0080-\u009F]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`),
        );
        report(path, i + 1, `broken characters (mojibake): ${shown.join(", ")}`);
      }
    });
  }

  const ci = !!process.env.GITHUB_ACTIONS;
  for (const p of problems) {
    if (ci) console.log(`::error file=${p.path},line=${p.line}::${p.message}`);
    else console.log(`${p.path}:${p.line}: ${p.message}`);
  }
  if (problems.length) {
    console.log(
      `\nText hygiene: ${problems.length} problem(s) in ${new Set(problems.map((p) => p.path)).size} file(s).` +
        "\nEdit files with an editor or `python -I` (encoding=\"utf-8\"), never PowerShell Get-Content/Set-Content." +
        '\nA mojibake file can usually be restored with text.encode("cp1254").decode("utf-8").',
    );
    return 1;
  }
  console.log(`Text hygiene: ${checked} files checked, no BOM, invalid UTF-8 or broken characters.`);
  return 0;
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) process.exitCode = main();
