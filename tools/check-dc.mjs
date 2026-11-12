// Syntax check of each Claude Design canvas script: extracts the
// data-dc-script block and compiles it with a DCLogic stub.
import { readFileSync } from "node:fs";
let bad = 0;
for (const f of process.argv.slice(2)) {
  const html = readFileSync(f, "utf8");
  const m = html.match(/<script type="text\/x-dc"[^>]*>([\s\S]*?)<\/script>/);
  if (!m) { console.log(f, "no script"); continue; }
  try {
    new Function("DCLogic", m[1] + "\nreturn new Component();")(class { constructor() { this.props = {}; } });
    const C = new Function("DCLogic", m[1] + "\nreturn Component;")(class { constructor() { this.props = {}; } });
    const vals = new C().renderVals();
    console.log(f, "ok", Object.keys(vals).join(","));
  } catch (e) { bad++; console.log(f, "ERROR", e.message); }
}
process.exit(bad ? 1 : 0);
