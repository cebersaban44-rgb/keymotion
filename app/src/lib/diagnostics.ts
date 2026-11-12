// "Copy diagnostics": the backend builds the report (versions, keyboards,
// the last log lines without addresses or user names); the UI only copies it.

import { getDiagnostics } from "./ipc";
import { useApp } from "./store";

/** Copy `text` to the clipboard; older webviews get the selection fallback. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    /* fall back below */
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  if (!ok) throw new Error("clipboard unavailable");
}

/** Build the diagnostics report and put it on the clipboard. */
export async function copyDiagnostics(): Promise<void> {
  await copyText(await getDiagnostics(useApp.getState().locale));
}
