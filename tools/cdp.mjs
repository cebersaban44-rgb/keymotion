// Evaluate a JS expression in the app's WebView2 over CDP.
// usage: node cdp.mjs "<expression>"   (awaits promises, returns JSON)
import { readFileSync } from "node:fs";
const arg = process.argv[2];
// "@path" reads the expression from a file (avoids shell quoting issues).
const expr = arg.startsWith("@") ? readFileSync(arg.slice(1), "utf8") : arg;
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = list.find((t) => t.type === "page");
if (!page) throw new Error("no page target: " + JSON.stringify(list));
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => {
  ws.onopen = r;
  ws.onerror = j;
});
const id = 1;
ws.send(
  JSON.stringify({
    id,
    method: "Runtime.evaluate",
    params: { expression: expr, awaitPromise: true, returnByValue: true },
  }),
);
const msg = await new Promise((r) => {
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id === id) r(m);
  };
});
ws.close();
const res = msg.result?.result;
if (msg.result?.exceptionDetails) {
  console.log("EXCEPTION", JSON.stringify(msg.result.exceptionDetails.exception?.description ?? msg.result.exceptionDetails));
} else {
  console.log(typeof res?.value === "string" ? res.value : JSON.stringify(res?.value, null, 1));
}
console.error("[target]", page.url);
