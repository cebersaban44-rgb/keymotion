// Drive the running app's WebView2 over CDP (start the app with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222).
//
//   node cdp.mjs "<expression>"     evaluate JS (awaits promises, prints JSON)
//   node cdp.mjs @file.js           same, expression read from a file
//   node cdp.mjs --shot out.png     save a screenshot of the window
//   node cdp.mjs --file "<css selector>" <path>   put a file in an <input type=file>
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const [arg, out, extra] = process.argv.slice(2);
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = list.find((t) => t.type === "page");
if (!page) throw new Error("no page target: " + JSON.stringify(list));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = j;
});

const call = (method, params) =>
  new Promise((r) => {
    const id = Math.floor(Math.random() * 1e9);
    ws.addEventListener("message", function on(e) {
      const m = JSON.parse(e.data);
      if (m.id !== id) return;
      ws.removeEventListener("message", on);
      r(m);
    });
    ws.send(JSON.stringify({ id, method, params }));
  });

if (arg === "--file") {
  const doc = await call("DOM.getDocument", { depth: -1, pierce: true });
  const q = await call("DOM.querySelector", { nodeId: doc.result.root.nodeId, selector: out });
  if (!q.result?.nodeId) throw new Error("no element for " + out);
  const r = await call("DOM.setFileInputFiles", { nodeId: q.result.nodeId, files: [resolve(extra)] });
  console.log(r.error ? "ERROR " + JSON.stringify(r.error) : "file set: " + resolve(extra));
} else if (arg === "--shot") {
  const msg = await call("Page.captureScreenshot", { format: "png" });
  if (!msg.result?.data) throw new Error("screenshot failed: " + JSON.stringify(msg.error ?? msg));
  writeFileSync(out ?? "shot.png", Buffer.from(msg.result.data, "base64"));
  console.log(out ?? "shot.png");
} else {
  // "@path" reads the expression from a file (avoids shell quoting issues).
  const expr = arg.startsWith("@") ? readFileSync(arg.slice(1), "utf8") : arg;
  const msg = await call("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  const res = msg.result?.result;
  if (msg.result?.exceptionDetails) {
    console.log("EXCEPTION", JSON.stringify(msg.result.exceptionDetails.exception?.description ?? msg.result.exceptionDetails));
  } else {
    console.log(typeof res?.value === "string" ? res.value : JSON.stringify(res?.value, null, 1));
  }
}
ws.close();
console.error("[target]", page.url);
