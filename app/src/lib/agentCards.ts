// Agents' cards for the keyboard screen (step 7): the backend asks for one
// (`agent-card`), the page draws it like the other live cards and hands it
// back (`agents_card_drawn`, the picture as `live_submit` takes it), or
// says why it could not (`agents_card_failed`). The backend waits 15 s,
// and asks only while the page says it listens (`agents_card_drawer_ready`).
//
// Browser preview: `?agentCard=text|image|picture|longTask` (with
// `?mock=usb`) draws a sample card once the app has started, and
// `window.__keymotionAgentCard(kind)` draws another; the card goes to the
// mock GIF page, as a live card would.

import type { AgentCard } from "../bindings/AgentCard";
import type { LiveState } from "../bindings/LiveState";
import { inTauri, invoke, listen } from "./ipc";
import { useLive } from "./live";
import { mockLiveUploaded, useScreen } from "./screen";
import { toBlob, type Prepared } from "./screenImport";

/** Drawing may take this long; the answer must reach the app within 15 s. */
const DRAW_MS = 12_000;

/** Draw one card, or fail with a reason the agent can read. */
async function drawWithin(draw: (c: AgentCard) => Promise<Prepared>, c: AgentCard): Promise<Prepared> {
  let timer = 0;
  try {
    return await Promise.race([
      draw(c),
      new Promise<never>((_, reject) => {
        timer = window.setTimeout(() => reject(new Error("drawing the card took too long")), DRAW_MS);
      }),
    ]);
  } finally {
    window.clearTimeout(timer);
  }
}

const reason = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Draw agents' cards for the whole app session: one at a time, in the
 * order they come. `draw` turns a card into a picture (kept out of here so
 * this module has no canvas code). Returns a function that stops listening.
 */
export function startAgentCards(draw: (c: AgentCard) => Promise<Prepared>): () => void {
  let queue: Promise<void> = Promise.resolve();
  const drawAndSend = async (c: AgentCard) => {
    let card: Prepared;
    try {
      card = await drawWithin(draw, c);
    } catch (e) {
      console.error("agent card not drawn", e);
      if (import.meta.env.DEV && !inTauri) return;
      await invoke("agents_card_failed", { id: c.id, error: reason(e) }).catch(console.error);
      return;
    }
    if (import.meta.env.DEV && !inTauri) {
      mockDrawn(card);
      return;
    }
    try {
      const state = await invoke<LiveState>("agents_card_drawn", toBlob(card), { headers: { "x-card": c.id } });
      useLive.setState({ state });
    } catch (e) {
      // The app refused the picture (or the card was given up): it tells the agent.
      console.error("agent card not taken", e);
    }
  };
  const onCard = (c: AgentCard) => {
    queue = queue.then(() => drawAndSend(c));
  };

  if (import.meta.env.DEV && !inTauri) {
    mock.onCard = onCard;
    (window as unknown as { __keymotionAgentCard?: (k: MockKind) => void }).__keymotionAgentCard = (k) =>
      void mockAgentCard(k);
    const kind = new URLSearchParams(location.search).get("agentCard") as MockKind | null;
    const timer = kind ? window.setTimeout(() => void mockAgentCard(kind), 600) : 0;
    return () => {
      window.clearTimeout(timer);
      mock.onCard = null;
    };
  }
  const off = listen<AgentCard>("agent-card", (e) => onCard(e.payload));
  // The app asks for cards only while a drawer listens: say so once the
  // listener is live, and say it stops when it goes away (the app also
  // forgets it when the page loads again or the window closes).
  let stopped = false;
  off.then(() => (stopped ? undefined : invoke("agents_card_drawer_ready"))).catch(console.error);
  const gone = () => {
    invoke("agents_card_drawer_ready", { ready: false }).catch(() => {});
  };
  window.addEventListener("pagehide", gone);
  return () => {
    stopped = true;
    window.removeEventListener("pagehide", gone);
    gone();
    off.then((f) => f()).catch(console.error);
  };
}

// ------------------------------------------------------- browser preview

type MockKind = "text" | "image" | "picture" | "longTask";

const mock = {
  onCard: null as ((c: AgentCard) => void) | null,
  seq: 0,
};

/** Browser preview: the card reaches the mock GIF page. */
function mockDrawn(card: Prepared) {
  (window as unknown as { __keymotionAgentCardLast?: Prepared }).__keymotionAgentCardLast = card;
  mockLiveUploaded(toBlob(card));
  const modelId = useScreen.getState().modelId;
  if (modelId) useScreen.getState().load(modelId).catch(console.error);
}

/** A small picture drawn here, as an agent might send one (PNG, base64). */
async function mockPicture(kind: "chart" | "pixel"): Promise<AgentCard["image"]> {
  const c = document.createElement("canvas");
  const x = c.getContext("2d")!;
  if (kind === "pixel") {
    // 16 × 16 pixel art: a smiling face, sent small and drawn sharp.
    const art = [
      "....yyyyyyyy....",
      "..yyyyyyyyyyyy..",
      ".yyyyyyyyyyyyyy.",
      ".yyykkyyyykkyyy.",
      "yyyykkyyyykkyyyy",
      "yyyykkyyyykkyyyy",
      "yyyyyyyyyyyyyyyy",
      "yyyyyyyyyyyyyyyy",
      "yykyyyyyyyyyykyy",
      "yyykyyyyyyyykyyy",
      ".yyykkyyyykkyyy.",
      ".yyyyykkkkyyyyy.",
      "..yyyyyyyyyyyy..",
      "....yyyyyyyy....",
      "................",
      "................",
    ];
    c.width = c.height = 16;
    art.forEach((row, y) =>
      Array.from(row).forEach((p, i) => {
        if (p === ".") return;
        x.fillStyle = p === "y" ? "#ffd23f" : "#3a2a10";
        x.fillRect(i, y, 1, 1);
      }),
    );
  } else {
    // A little bar chart, as a build script might draw.
    c.width = 120;
    c.height = 120;
    x.fillStyle = "#14202c";
    x.fillRect(0, 0, 120, 120);
    const bars = [38, 52, 47, 70, 64, 88, 96];
    bars.forEach((v, i) => {
      x.fillStyle = i === bars.length - 1 ? "#3fcf8e" : "#4c9dff";
      x.fillRect(10 + i * 15, 108 - v, 10, v);
    });
    x.fillStyle = "#ffffff55";
    x.fillRect(6, 108, 108, 2);
  }
  const url = c.toDataURL("image/png");
  return { mime: "image/png", data: url.slice(url.indexOf(",") + 1), width: c.width, height: c.height, animated: false };
}

/** Browser preview: an agent asks for a card. */
async function mockAgentCard(kind: MockKind) {
  const base: AgentCard = {
    id: `mock-${++mock.seq}`,
    kind: "text",
    agent: "claude-code",
    title: null,
    text: null,
    image: null,
    color: null,
    minutes: null,
    label: null,
    showS: 300,
  };
  const card: AgentCard =
    kind === "longTask"
      ? { ...base, kind: "longTask", color: [63, 207, 142], minutes: 14, label: "keymotion" }
      : kind === "image"
        ? { ...base, title: "Bundle size", text: "Down 12% this week\nfrom 412 KB", image: await mockPicture("chart"), color: [76, 157, 255] }
        : kind === "picture"
          ? { ...base, image: await mockPicture("pixel"), color: [255, 210, 63] }
          : { ...base, title: "Tests", text: "All 214 passed\nReady to merge", color: [63, 207, 142] };
  mock.onCard?.(card);
}
