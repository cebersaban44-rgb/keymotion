// node --test tools/sim-guard.test.mjs
// The keyboard simulator (app feature `sim`, KEYMOTION_SIM=aa769) is a
// development tool: no build that reaches users may contain it. The app
// refuses to compile it without debug assertions (lib.rs); this checks
// that nothing turns it on by default or in a release, Store or CI build.
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (p) => readFileSync(join(root, p), "utf8");

/** Every file under `dir` whose name passes `keep`. */
function files(dir, keep) {
  const out = [];
  for (const name of readdirSync(join(root, dir))) {
    const p = join(dir, name);
    if (statSync(join(root, p)).isDirectory()) {
      if (name !== "node_modules") out.push(...files(p, keep));
    } else if (keep(name)) {
      out.push(p);
    }
  }
  return out;
}

/** Turns the `sim` feature on: `--features sim`, `-F sim`, `features = ["sim"]`. */
const SIM_ON = /(--features|-F)(\s+|=)["']?[\w,\- ]*\bsim\b|features\s*=\s*\[[^\]]*["']sim["']|"features"\s*:\s*\[[^\]]*"sim"/;

test("the app's sim feature exists, off by default, debug only", () => {
  const cargo = read("app/src-tauri/Cargo.toml");
  assert.match(cargo, /^sim = \["kb-core\/sim"\]$/m);
  assert.doesNotMatch(cargo, /^default\s*=.*\bsim\b/m, "not a default feature");
  const lib = read("app/src-tauri/src/lib.rs");
  assert.match(lib, /#\[cfg\(all\(feature = "sim", not\(debug_assertions\)\)\)\]\s*compile_error!/);
  // The CLI and the other crates never ask for it.
  for (const p of files("crates", (n) => n === "Cargo.toml")) {
    assert.doesNotMatch(read(p), SIM_ON, p);
  }
});

test("no workflow, release script or Tauri config builds with the simulator", () => {
  const checked = [
    ...files(".github/workflows", (n) => n.endsWith(".yml")),
    ...files("tools", (n) => n.endsWith(".mjs") && !n.endsWith(".test.mjs")),
    "app/src-tauri/tauri.conf.json",
    "app/src-tauri/tauri.bundle-cli.conf.json",
    "app/package.json",
  ];
  assert.ok(checked.length > 5);
  for (const p of checked) {
    assert.doesNotMatch(read(p), SIM_ON, p);
  }
  // The pattern does find what it looks for.
  for (const on of ["pnpm tauri dev --features sim", "cargo build -F sim", 'features = ["sim"]', '"features": ["sim"]']) {
    assert.match(on, SIM_ON, on);
  }
  assert.doesNotMatch("cargo build --features simd", SIM_ON);
});
