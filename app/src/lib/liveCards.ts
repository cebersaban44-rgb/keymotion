// Live cards drawn for the keyboard screen (phase C). Same rules as the
// text cards: fonts loaded first, background dithered, text sharp.

import type { TrackInfo } from "../bindings/TrackInfo";
import { H, STILL_DELAY_MS, W, type Prepared } from "./screenImport";
import { compose565, makeLayers } from "./screenText";

const DISPLAY = '"Bricolage Grotesque Variable", "Segoe UI Emoji", sans-serif';
const SANS = '"Instrument Sans Variable", "Segoe UI Emoji", sans-serif';

async function fonts(text: string) {
  try {
    await Promise.all([document.fonts.load(`700 18px ${DISPLAY}`, text), document.fonts.load(`500 12px ${SANS}`, text)]);
  } catch {
    // Fallback fonts still draw.
  }
}

/** Lines of at most `maxW`, at most `max` of them; the last one ends in
 *  "…" when text is left over, and words wider than a line are cut. */
function lines(ctx: CanvasRenderingContext2D, text: string, maxW: number, max: number): string[] {
  const width = (s: string) => ctx.measureText(s).width;
  const cut = (s: string) => {
    let c = s;
    while (c.length > 1 && width(`${c}…`) > maxW) c = c.slice(0, -1);
    return `${c.replace(/[\s,.;:–-]+$/, "")}…`;
  };
  const out: string[] = [];
  let line = "";
  let rest = false;
  for (const w of text.split(/\s+/).filter(Boolean)) {
    const next = line ? `${line} ${w}` : w;
    if (!line || width(next) <= maxW) {
      line = next;
      continue;
    }
    out.push(line);
    line = w;
    if (out.length === max) {
      rest = true;
      break;
    }
  }
  if (!rest && line) {
    if (out.length < max) out.push(line);
    else rest = true;
  }
  const fitted = out.map((l) => (width(l) <= maxW ? l : cut(l)));
  if (rest && fitted.length) {
    const last = fitted[fitted.length - 1]!;
    if (!last.endsWith("…")) fitted[fitted.length - 1] = cut(`${last} `);
  }
  return fitted;
}

/** Average colour of the cover, darkened for a background behind text. */
function coverTone(bmp: ImageBitmap): [number, number, number] {
  const c = document.createElement("canvas");
  c.width = c.height = 16;
  const x = c.getContext("2d", { willReadFrequently: true })!;
  x.drawImage(bmp, 0, 0, 16, 16);
  const d = x.getImageData(0, 0, 16, 16).data;
  let r = 0;
  let g = 0;
  let b = 0;
  let wsum = 0;
  for (let i = 0; i < d.length; i += 4) {
    const max = Math.max(d[i]!, d[i + 1]!, d[i + 2]!);
    const min = Math.min(d[i]!, d[i + 1]!, d[i + 2]!);
    // Colourful pixels count more than greys.
    const w = 0.2 + (max ? (max - min) / max : 0);
    r += d[i]! * w;
    g += d[i + 1]! * w;
    b += d[i + 2]! * w;
    wsum += w;
  }
  return [r / wsum, g / wsum, b / wsum];
}

function rgb([r, g, b]: [number, number, number], k = 1) {
  return `rgb(${Math.round(r * k)}, ${Math.round(g * k)}, ${Math.round(b * k)})`;
}

/** "Çalan şarkı": cover on the left, title and artist on the right. Stays
 *  true until the next track, so it carries no time or progress. */
export async function nowPlayingCard(t: TrackInfo, cover: Blob | null, label: string): Promise<Prepared> {
  const title = t.title.trim() || "—";
  const artist = t.artist.trim();
  await fonts(`${label} ${title} ${artist} ${t.app}`);
  const bmp = cover ? await createImageBitmap(cover).catch(() => null) : null;
  const tone: [number, number, number] = bmp ? coverTone(bmp) : [26, 60, 120];
  // Lighter version of the tone for the label.
  const max = Math.max(...tone, 1);
  const accent = rgb(tone.map((v) => 120 + (v / max) * 135) as [number, number, number]);

  const { bg, fg } = makeLayers();
  // Background: the cover's tone, dark, fading to black.
  const grad = bg.createLinearGradient(0, 0, W, H);
  grad.addColorStop(0, rgb(tone, 0.45));
  grad.addColorStop(1, rgb(tone, 0.12));
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  const size = 104;
  const cx = 14;
  const cy = Math.round((H - size) / 2);
  bg.save();
  bg.beginPath();
  bg.roundRect(cx, cy, size, size, 8);
  bg.clip();
  if (bmp) {
    // Centre-crop to a square.
    const s = Math.min(bmp.width, bmp.height);
    bg.drawImage(bmp, (bmp.width - s) / 2, (bmp.height - s) / 2, s, s, cx, cy, size, size);
  } else {
    bg.fillStyle = rgb(tone, 0.9);
    bg.fillRect(cx, cy, size, size);
  }
  bg.restore();
  bmp?.close();

  fg.clearRect(0, 0, W, H);
  fg.textBaseline = "alphabetic";
  fg.textAlign = "left";
  if (!cover) {
    fg.fillStyle = "rgba(255,255,255,0.9)";
    fg.font = `700 56px ${DISPLAY}`;
    fg.textAlign = "center";
    fg.fillText("♪", cx + size / 2, cy + size / 2 + 20);
    fg.textAlign = "left";
  }

  const left = cx + size + 12;
  const maxW = W - left - 10;
  fg.font = `600 9px ${SANS}`;
  fg.letterSpacing = "1px";
  const labelY = 30;
  fg.fillStyle = accent;
  fg.fillText(label, left, labelY);
  fg.letterSpacing = "0px";

  // Title: as large as fits in two lines.
  let px = 18;
  let tl: string[] = [];
  for (; px >= 12; px--) {
    fg.font = `700 ${px}px ${DISPLAY}`;
    tl = lines(fg, title, maxW, 2);
    if (!tl.some((l) => l.endsWith("…")) || px === 12) break;
  }
  fg.fillStyle = "#ffffff";
  const tH = Math.round(px * 1.15);
  let y = labelY + 8 + px;
  for (const l of tl) {
    fg.fillText(l, left, y);
    y += tH;
  }
  if (artist) {
    fg.font = `500 12px ${SANS}`;
    fg.fillStyle = "rgba(255,255,255,0.78)";
    y += 3;
    for (const l of lines(fg, artist, maxW, 2)) {
      fg.fillText(l, left, y);
      y += 15;
    }
  }
  if (t.app) {
    fg.font = `500 10px ${SANS}`;
    fg.fillStyle = "rgba(255,255,255,0.5)";
    fg.fillText(t.app, left, H - 14);
  }

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return {
    frames: [px565],
    delaysMs: [STILL_DELAY_MS],
    name: artist ? `${artist} — ${title}` : title,
    kind: "image",
  };
}
