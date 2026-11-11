// Live cards drawn for the keyboard screen (phase C). Same rules as the
// text cards: fonts loaded first, background dithered, text sharp.

import type { CalendarCard } from "../bindings/CalendarCard";
import type { PomodoroCard } from "../bindings/PomodoroCard";
import type { PomodoroPhase } from "../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../bindings/PomodoroSettings";
import type { ReminderCard } from "../bindings/ReminderCard";
import type { ReminderIcon } from "../bindings/ReminderIcon";
import type { TrackInfo } from "../bindings/TrackInfo";
import type { MessageKey } from "./i18n";
import { phaseColor } from "./pomodoro";
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

// -------------------------------------------------------------- pomodoro

const MONO = '"JetBrains Mono Variable", monospace';

/** Words the Pomodoro cards need, in the UI language. */
export interface PomodoroWords {
  /** Small caps line on the card. */
  label: string;
  /** Name of the feature, for the card's name in the app. */
  name: string;
  phase: Record<PomodoroPhase, string>;
  round: string;
  min: string;
  next: string;
  nextFocus: string;
  nextShort: string;
  nextLong: string;
  lastBreak: string;
  doneComplete: string;
  doneStopped: string;
  ended: string;
}

/** The card's words in the current language. */
export function pomodoroWords(tNow: (key: MessageKey) => string): PomodoroWords {
  return {
    label: tNow("pomo.card.label"),
    name: tNow("pomo.card.name"),
    phase: { focus: tNow("pomo.phase.focus"), shortBreak: tNow("pomo.phase.shortBreak"), longBreak: tNow("pomo.phase.longBreak") },
    round: tNow("pomo.card.round"),
    min: tNow("pomo.card.min"),
    next: tNow("pomo.card.next"),
    nextFocus: tNow("pomo.card.nextFocus"),
    nextShort: tNow("pomo.card.nextShort"),
    nextLong: tNow("pomo.card.nextLong"),
    lastBreak: tNow("pomo.card.lastBreak"),
    doneComplete: tNow("pomo.card.doneComplete"),
    doneStopped: tNow("pomo.card.doneStopped"),
    ended: tNow("pomo.card.ended"),
  };
}

function hexRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Largest size from `max` down to `min` at which `text` fits `maxW`. */
function fitFont(ctx: CanvasRenderingContext2D, text: string, font: (px: number) => string, maxW: number, max: number, min: number) {
  let px = max;
  for (; px > min; px--) {
    ctx.font = font(px);
    if (ctx.measureText(text).width <= maxW) break;
  }
  ctx.font = font(px);
  return px;
}

/**
 * Pomodoro card: the phase and its length, the round on a ring, and what
 * comes next; or, when the timer stopped, a summary. Both stay true until
 * the next card, so no countdown is drawn (the keys show the progress).
 */
export async function pomodoroCard(c: PomodoroCard, s: PomodoroSettings, w: PomodoroWords): Promise<Prepared> {
  const done = c.kind === "done";
  // A finished series gets a check; one ended early shows its rounds.
  const check = done && c.complete;
  const color = phaseColor(s, done ? (check ? "shortBreak" : "focus") : c.phase);
  const tone = hexRgb(color);
  const light = tone.map((v) => Math.round(v + (255 - v) * 0.35)) as [number, number, number];
  const endTime = new Date(c.endedMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

  const title = done ? (c.complete ? w.doneComplete : w.doneStopped) : w.phase[c.phase];
  const big = done ? `${c.focusMin} ${w.min}` : `${String(c.minutes).padStart(2, "0")}:00`;
  let foot: string;
  if (done) foot = `${c.focusDone} ${w.round} · ${w.ended} ${endTime}`;
  else if (c.phase === "focus")
    foot = c.focusDone + 1 >= c.rounds ? `${w.next}: ${s.longMin} ${w.min} ${w.nextLong}` : `${w.next}: ${s.shortMin} ${w.min} ${w.nextShort}`;
  else if (c.phase === "shortBreak") foot = `${w.next}: ${s.focusMin} ${w.min} ${w.nextFocus}`;
  else foot = w.lastBreak;
  await Promise.all([fonts(`${w.label} ${title} ${foot}`), document.fonts.load(`700 34px ${MONO}`, big).catch(() => [])]);

  const { bg, fg } = makeLayers();
  const grad = bg.createLinearGradient(0, 0, W * 0.6, H);
  grad.addColorStop(0, rgb(tone, 0.34));
  grad.addColorStop(1, rgb(tone, 0.07));
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  fg.clearRect(0, 0, W, H);
  // Ring on the right: one arc per round, finished rounds filled.
  const r = 40;
  const cx = W - 16 - r;
  const cy = H / 2;
  const rounds = check ? 1 : Math.max(1, done ? s.rounds : c.rounds);
  const filled = check ? 1 : c.focusDone;
  const gap = rounds > 1 ? 0.16 : 0;
  const seg = (Math.PI * 2) / rounds;
  fg.lineWidth = 7;
  fg.lineCap = rounds > 1 ? "round" : "butt";
  for (let i = 0; i < rounds; i++) {
    const a0 = -Math.PI / 2 + i * seg + gap / 2;
    const a1 = a0 + seg - gap;
    fg.strokeStyle = i < filled ? color : rgb(tone, 0.32);
    fg.beginPath();
    fg.arc(cx, cy, r, a0, a1);
    fg.stroke();
    // The round under way: half of its arc.
    if (!done && c.phase === "focus" && i === filled) {
      fg.strokeStyle = rgb(light);
      fg.beginPath();
      fg.arc(cx, cy, r, a0, a0 + (a1 - a0) / 2);
      fg.stroke();
    }
  }
  fg.textAlign = "center";
  fg.fillStyle = "#ffffff";
  if (check) {
    fg.lineWidth = 6;
    fg.lineCap = "round";
    fg.lineJoin = "round";
    fg.strokeStyle = "#ffffff";
    fg.beginPath();
    fg.moveTo(cx - 15, cy + 1);
    fg.lineTo(cx - 4, cy + 12);
    fg.lineTo(cx + 16, cy - 11);
    fg.stroke();
  } else {
    fg.font = `700 24px ${DISPLAY}`;
    fg.fillText(done ? `${c.focusDone}/${s.rounds}` : `${c.round}/${c.rounds}`, cx, cy + 6);
    fg.font = `600 8px ${SANS}`;
    fg.letterSpacing = "1px";
    fg.fillStyle = "rgba(255,255,255,0.6)";
    fg.fillText(w.round.toLocaleUpperCase(), cx, cy + 19);
    fg.letterSpacing = "0px";
  }

  // Text on the left.
  const left = 14;
  const maxW = cx - r - 10 - left;
  fg.textAlign = "left";
  fg.letterSpacing = "1px";
  fg.fillStyle = rgb(light);
  fitFont(fg, w.label, (px) => `600 ${px}px ${SANS}`, maxW, 9, 7);
  fg.fillText(w.label, left, 24);
  fg.letterSpacing = "0px";
  fg.fillStyle = "#ffffff";
  fitFont(fg, title, (px) => `700 ${px}px ${DISPLAY}`, maxW, 26, 15);
  fg.fillText(title, left, 54);
  fg.fillStyle = rgb(light);
  fitFont(fg, big, (px) => `700 ${px}px ${MONO}`, maxW, 34, 20);
  fg.fillText(big, left, 92);
  fg.fillStyle = "rgba(255,255,255,0.62)";
  fitFont(fg, foot, (px) => `500 ${px}px ${SANS}`, maxW + r, 10.5, 8);
  fg.fillText(foot, left, 118);

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return {
    frames: [px565],
    delaysMs: [STILL_DELAY_MS],
    name: done ? `${w.name} · ${title}` : `${w.name} · ${title} ${c.round}/${c.rounds}`,
    kind: "image",
  };
}

// -------------------------------------------------------------- calendar

/** Words the calendar cards need, in the UI language. */
export interface CalendarWords {
  locale: string;
  today: string;
  tomorrow: string;
  /** "Sonra 16:30 …". */
  next: string;
  /** Name of the feature, for the card's name in the app. */
  name: string;
}

export function calendarWords(tNow: (key: MessageKey) => string, locale: string): CalendarWords {
  return {
    locale,
    today: tNow("calendar.card.today"),
    tomorrow: tNow("calendar.card.tomorrow"),
    next: tNow("calendar.card.next"),
    name: tNow("calendar.card.name"),
  };
}

const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/**
 * Calendar card ("Sıradaki", owner's choice 2026-11-11): the start large,
 * the end, the title, where it is and the meeting after it. Only absolute
 * times and the day word, so it stays true from before the meeting until
 * after it starts and goes up once.
 */
export async function calendarCard(c: CalendarCard, calendarName: string, w: CalendarWords): Promise<Prepared> {
  const e = c.event;
  const tone = [e.color[0], e.color[1], e.color[2]] as [number, number, number];
  const light = tone.map((v) => Math.round(v + (255 - v) * 0.45)) as [number, number, number];
  const start = new Date(e.startMs);
  const today = new Date();
  const tomorrow = new Date(today.getTime() + 86_400_000);
  const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  const day = sameDay(start, today)
    ? w.today
    : sameDay(start, tomorrow)
      ? w.tomorrow
      : start.toLocaleDateString(w.locale, { day: "numeric", month: "short" });
  const label = (calendarName ? `${calendarName} · ${day}` : day).toLocaleUpperCase(w.locale);
  const time = clock(e.startMs);
  const end = e.endMs > e.startMs ? `– ${clock(e.endMs)}` : "";
  const title = e.title.trim() || "—";
  const foot = [e.location.trim(), c.next ? `${w.next} ${clock(c.next.startMs)} ${c.next.title.trim()}` : ""]
    .filter(Boolean)
    .join("  ·  ");
  await Promise.all([fonts(`${label} ${title} ${foot}`), document.fonts.load(`700 34px ${MONO}`, `${time} ${end}`).catch(() => [])]);

  const { bg, fg } = makeLayers();
  const grad = bg.createLinearGradient(0, 0, W * 0.6, H);
  grad.addColorStop(0, rgb(tone, 0.34));
  grad.addColorStop(1, rgb(tone, 0.07));
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  fg.clearRect(0, 0, W, H);
  fg.textBaseline = "alphabetic";
  fg.textAlign = "left";
  // A stripe in the calendar's colour on the left edge.
  fg.fillStyle = rgb(tone);
  fg.fillRect(0, 0, 4, H);

  const left = 16;
  const maxW = W - left - 12;
  fg.letterSpacing = "1px";
  fg.fillStyle = rgb(light);
  fitFont(fg, label, (px) => `600 ${px}px ${SANS}`, maxW, 9, 7);
  fg.fillText(label, left, 24);
  fg.letterSpacing = "0px";

  fg.fillStyle = rgb(light);
  fg.font = `700 34px ${MONO}`;
  fg.fillText(time, left, 61);
  if (end) {
    const tw = fg.measureText(time).width;
    fg.font = `500 12px ${MONO}`;
    fg.fillStyle = "rgba(255,255,255,0.55)";
    fg.fillText(end, left + tw + 7, 61);
  }

  // Title: as large as fits in two lines.
  let px = 18;
  let tl: string[] = [];
  for (; px >= 12; px--) {
    fg.font = `700 ${px}px ${DISPLAY}`;
    tl = lines(fg, title, maxW, 2);
    if (!tl.some((l) => l.endsWith("…")) || px === 12) break;
  }
  fg.fillStyle = "#ffffff";
  let y = 84 + px * 0.2;
  for (const l of tl) {
    fg.fillText(l, left, y);
    y += Math.round(px * 1.12);
  }
  if (foot) {
    fg.fillStyle = "rgba(255,255,255,0.62)";
    fitFont(fg, foot, (p) => `500 ${p}px ${SANS}`, maxW, 10.5, 8);
    const cut = lines(fg, foot, maxW, 1)[0] ?? "";
    fg.fillText(cut, left, tl.length > 1 ? 127 : 122);
  }

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return {
    frames: [px565],
    delaysMs: [STILL_DELAY_MS],
    name: `${w.name} · ${time} ${title}`,
    kind: "image",
  };
}

// ------------------------------------------------------------- reminders

/** Words the reminder cards need, in the UI language. */
export interface ReminderWords {
  locale: string;
  /** Small caps line: "Hatırlatma". */
  label: string;
  today: string;
  tomorrow: string;
  /** "Sonraki 11:30". */
  next: string;
  /** Name of the feature, for the card's name in the app. */
  name: string;
}

export function reminderWords(tNow: (key: MessageKey) => string, locale: string): ReminderWords {
  return {
    locale,
    label: tNow("reminder.card.label"),
    today: tNow("reminder.today"),
    tomorrow: tNow("reminder.tomorrow"),
    next: tNow("reminder.card.next"),
    name: tNow("reminder.card.name"),
  };
}

/** The reminder's picture, drawn in `color` around (cx, cy), `s` wide. */
export function drawReminderIcon(ctx: CanvasRenderingContext2D, icon: ReminderIcon, cx: number, cy: number, s: number, color: string) {
  const r = s / 2;
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = Math.max(2, s / 11);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  switch (icon) {
    case "drop":
      ctx.moveTo(cx, cy - r);
      ctx.bezierCurveTo(cx + r * 0.2, cy - r * 0.5, cx + r * 0.75, cy - r * 0.05, cx + r * 0.75, cy + r * 0.3);
      ctx.arc(cx, cy + r * 0.3, r * 0.75, 0, Math.PI);
      ctx.bezierCurveTo(cx - r * 0.75, cy - r * 0.05, cx - r * 0.2, cy - r * 0.5, cx, cy - r);
      ctx.fill();
      break;
    case "stand":
      // Someone stretching, arms up.
      ctx.arc(cx, cy - r * 0.62, r * 0.22, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.moveTo(cx, cy - r * 0.3);
      ctx.lineTo(cx, cy + r * 0.3);
      ctx.moveTo(cx - r * 0.6, cy - r * 0.75);
      ctx.lineTo(cx, cy - r * 0.2);
      ctx.lineTo(cx + r * 0.6, cy - r * 0.75);
      ctx.moveTo(cx - r * 0.4, cy + r);
      ctx.lineTo(cx, cy + r * 0.3);
      ctx.lineTo(cx + r * 0.4, cy + r);
      ctx.stroke();
      break;
    case "eye":
      ctx.moveTo(cx - r, cy);
      ctx.quadraticCurveTo(cx, cy - r * 1.1, cx + r, cy);
      ctx.quadraticCurveTo(cx, cy + r * 1.1, cx - r, cy);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(cx, cy, r * 0.3, 0, Math.PI * 2);
      ctx.fill();
      break;
    case "pill":
      ctx.translate(cx, cy);
      ctx.rotate(-Math.PI / 4);
      ctx.roundRect(-r, -r * 0.38, 2 * r, r * 0.76, r * 0.38);
      ctx.stroke();
      ctx.beginPath();
      ctx.roundRect(-r, -r * 0.38, r, r * 0.76, [r * 0.38, 0, 0, r * 0.38]);
      ctx.fill();
      break;
    default:
      // A bell.
      ctx.moveTo(cx - r * 0.7, cy + r * 0.45);
      ctx.quadraticCurveTo(cx - r * 0.55, cy + r * 0.2, cx - r * 0.55, cy - r * 0.1);
      ctx.arc(cx, cy - r * 0.1, r * 0.55, Math.PI, 0);
      ctx.quadraticCurveTo(cx + r * 0.55, cy + r * 0.2, cx + r * 0.7, cy + r * 0.45);
      ctx.closePath();
      ctx.fill();
      ctx.beginPath();
      ctx.arc(cx, cy + r * 0.62, r * 0.16, 0, Math.PI * 2);
      ctx.fill();
  }
  ctx.restore();
}

/**
 * Reminder card ("saat önde", owner's choice 2026-11-11): the time large,
 * the picture, the title, its note and when it comes again. Only absolute
 * times, like the calendar card, which it sits next to in the order.
 */
export async function reminderCard(c: ReminderCard, w: ReminderWords): Promise<Prepared> {
  const r = c.reminder;
  const tone = [r.color[0], r.color[1], r.color[2]] as [number, number, number];
  const light = tone.map((v) => Math.round(v + (255 - v) * 0.45)) as [number, number, number];
  const day = (ms: number) => {
    const d = new Date(ms);
    const today = new Date();
    const tomorrow = new Date(today.getTime() + 86_400_000);
    if (d.toDateString() === today.toDateString()) return w.today;
    if (d.toDateString() === tomorrow.toDateString()) return w.tomorrow;
    return d.toLocaleDateString(w.locale, { day: "numeric", month: "short" });
  };
  const label = `${w.label} · ${day(c.atMs)}`.toLocaleUpperCase(w.locale);
  const time = clock(c.atMs);
  const title = r.title.trim() || "—";
  const sameDay = c.nextMs != null && new Date(c.nextMs).toDateString() === new Date(c.atMs).toDateString();
  const next = c.nextMs == null ? "" : sameDay ? `${w.next} ${clock(c.nextMs)}` : `${w.next} ${day(c.nextMs).toLocaleLowerCase(w.locale)} ${clock(c.nextMs)}`;
  const foot = [r.note.trim(), next].filter(Boolean).join("  ·  ");
  await Promise.all([fonts(`${label} ${title} ${foot}`), document.fonts.load(`700 34px ${MONO}`, time).catch(() => [])]);

  const { bg, fg } = makeLayers();
  const grad = bg.createLinearGradient(0, 0, W * 0.6, H);
  grad.addColorStop(0, rgb(tone, 0.34));
  grad.addColorStop(1, rgb(tone, 0.07));
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  fg.clearRect(0, 0, W, H);
  fg.textBaseline = "alphabetic";
  fg.textAlign = "left";
  fg.fillStyle = rgb(tone);
  fg.fillRect(0, 0, 4, H);
  drawReminderIcon(fg, r.icon, W - 34, 44, 34, rgb(light));

  const left = 16;
  const maxW = W - left - 12;
  fg.letterSpacing = "1px";
  fg.fillStyle = rgb(light);
  fitFont(fg, label, (px) => `600 ${px}px ${SANS}`, maxW - 50, 9, 7);
  fg.fillText(label, left, 24);
  fg.letterSpacing = "0px";
  fg.font = `700 34px ${MONO}`;
  fg.fillText(time, left, 61);

  // Title: as large as fits in two lines.
  let px = 20;
  let tl: string[] = [];
  for (; px >= 12; px--) {
    fg.font = `700 ${px}px ${DISPLAY}`;
    tl = lines(fg, title, maxW, 2);
    if (!tl.some((l) => l.endsWith("…")) || px === 12) break;
  }
  fg.fillStyle = "#ffffff";
  let y = 86 + px * 0.1;
  for (const l of tl) {
    fg.fillText(l, left, y);
    y += Math.round(px * 1.12);
  }
  if (foot) {
    fg.fillStyle = "rgba(255,255,255,0.62)";
    fitFont(fg, foot, (p) => `500 ${p}px ${SANS}`, maxW, 10.5, 8);
    fg.fillText(lines(fg, foot, maxW, 1)[0] ?? "", left, tl.length > 1 ? 127 : 121);
  }

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return {
    frames: [px565],
    delaysMs: [STILL_DELAY_MS],
    name: `${w.name} · ${time} ${title}`,
    kind: "image",
  };
}
