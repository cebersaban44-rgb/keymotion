#!/usr/bin/env node
// Guard: the vendor's driver, its files and the keyboard's factory screen
// animation must never be committed (see NOTICE). Fails when a tracked file
//   - has a program, driver or archive extension (.exe, .dll, .sys, .msi,
//     .cab, .rar, .7z),
//   - is a program or archive by content (PE "MZ" header, RAR or 7-Zip
//     signature), whatever its extension,
//   - is a GIF (by extension or content) outside ALLOWED_GIF_DIRS,
//   - has "factory" in its path and an image, video or raw-data extension.
//
// Usage: node tools/check-vendor-files.mjs

import { execFileSync } from "node:child_process";
import { openSync, readSync, closeSync, existsSync, statSync } from "node:fs";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

const FORBIDDEN_EXT = new Set([".exe", ".dll", ".sys", ".msi", ".cab", ".rar", ".7z"]);
const ALLOWED_GIF_DIRS = ["docs/media/"];
const MEDIA_EXT = new Set([
  ".gif", ".png", ".jpg", ".jpeg", ".bmp", ".webp", ".apng", ".avif", ".mp4", ".webm", ".avi", ".mov", ".bin", ".raw",
]);

const files = execFileSync("git", ["ls-files", "-z"], { cwd: root, encoding: "utf8", maxBuffer: 64 << 20 })
  .split("\0")
  .filter(Boolean);

function head(path, n) {
  const full = join(root, path);
  if (!existsSync(full) || !statSync(full).isFile()) return Buffer.alloc(0);
  const fd = openSync(full, "r");
  try {
    const buf = Buffer.alloc(n);
    const read = readSync(fd, buf, 0, n, 0);
    return buf.subarray(0, read);
  } finally {
    closeSync(fd);
  }
}

const startsWith = (buf, bytes) => bytes.every((b, i) => buf[i] === b);
const GIF = [0x47, 0x49, 0x46, 0x38]; // "GIF8"
const RAR = [0x52, 0x61, 0x72, 0x21, 0x1a, 0x07]; // "Rar!" 1A 07
const SEVEN_ZIP = [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c];

function isPortableExecutable(path) {
  const buf = head(path, 4096);
  if (buf.length < 64 || buf[0] !== 0x4d || buf[1] !== 0x5a) return false; // "MZ"
  const peOffset = buf.readUInt32LE(0x3c);
  return peOffset + 4 <= buf.length && startsWith(buf.subarray(peOffset), [0x50, 0x45, 0, 0]); // "PE\0\0"
}

const problems = [];
for (const path of files) {
  const ext = extname(path).toLowerCase();
  const lower = path.toLowerCase();
  const gifAllowed = ALLOWED_GIF_DIRS.some((d) => path.startsWith(d));
  const magic = head(path, 8);

  if (FORBIDDEN_EXT.has(ext)) problems.push([path, `${ext} files (programs, drivers, archives) are not allowed`]);
  else if (isPortableExecutable(path)) problems.push([path, "is a Windows program or library (PE header)"]);
  else if (startsWith(magic, RAR) || startsWith(magic, SEVEN_ZIP)) problems.push([path, "is a RAR or 7-Zip archive"]);

  if ((ext === ".gif" || startsWith(magic, GIF)) && !gifAllowed) {
    problems.push([path, `GIF outside ${ALLOWED_GIF_DIRS.join(", ")}`]);
  }
  if (lower.includes("factory") && MEDIA_EXT.has(ext)) {
    problems.push([path, "looks like the factory screen animation (\"factory\" in the path)"]);
  }
}

const ci = !!process.env.GITHUB_ACTIONS;
for (const [path, message] of problems) {
  console.log(ci ? `::error file=${path}::${message}` : `${path}: ${message}`);
}
if (problems.length) {
  console.log(
    `\nVendor file guard: ${problems.length} problem(s). The vendor driver, its files and the factory` +
      "\nanimation are never distributed with this project (NOTICE). Remove them from the index.",
  );
  process.exit(1);
}
console.log(`Vendor file guard: ${files.length} tracked files checked, none forbidden.`);
