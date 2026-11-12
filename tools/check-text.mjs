#!/usr/bin/env node
// Text hygiene for tracked source and documentation files.
//
// Fails on:
//   - a UTF-8 byte order mark at the start of a file,
//   - bytes that are not valid UTF-8,
//   - the replacement character U+FFFD (a character was lost in a conversion),
//   - mojibake: UTF-8 text that was read with a Windows code page (1252/1254)
//     and written back as UTF-8. Each two-byte letter turns into two
//     characters: "ı" (C4 B1) becomes U+00C4 U+00B1, "ş" (C5 9F) becomes
//     U+00C5 U+0178, "ç" (C3 A7) becomes U+00C3 U+00A7, and an em dash
//     (E2 80 94) starts with U+00E2 U+20AC (docs/DURUM.md §6, lesson 52).
//
// Lockfiles and the generated TypeScript bindings are skipped. In Markdown, an
// inline code span that contains nothing but a mojibake sample documents the
// problem and is allowed (DURUM.md lists the patterns this way).
//
// Usage: node tools/check-text.mjs            (every tracked file)
//        node tools/check-text.mjs FILE...    (only these files)

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { extname, basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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
const MOJIBAKE = new RegExp(
  [
    C.A_TILDE,
    `${C.A_DIAER}[\u00B0\u00B1\u0178\u017E]`,
    `${C.A_RING}[\u0178\u017E]`,
    `${C.A_CIRC}[\u00A0-\u00BF]`,
    `${C.a_circ}\u20AC`,
    // C1 control characters never appear in real text. Code page 1254 has no
    // character for byte 9E, the second byte of "\u011E" and "\u015E".
    "[\u0080-\u009F]",
  ].join("|"),
  "gu",
);
const MOJIBAKE_ONLY = new RegExp(`^(?:${MOJIBAKE.source})+$`, "u");

function trackedFiles() {
  const out = execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8", maxBuffer: 64 << 20 });
  return out.split("\0").filter(Boolean);
}

function wanted(path) {
  if (!EXTENSIONS.has(extname(path).toLowerCase())) return false;
  if (SKIP_NAMES.has(basename(path))) return false;
  return !SKIP_PREFIXES.some((p) => path.startsWith(p));
}

const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const problems = [];

function report(path, line, message) {
  problems.push({ path, line, message });
}

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
    const line = markdown ? raw.replace(/`([^`\n]{1,12})`/g, (m, inner) => (MOJIBAKE_ONLY.test(inner) ? "" : m)) : raw;
    const found = [...line.matchAll(MOJIBAKE)].map((m) => m[0]);
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
  process.exit(1);
}
console.log(`Text hygiene: ${checked} files checked, no BOM, invalid UTF-8 or broken characters.`);
