// Live cards drawn for the keyboard screen (phase C). Same rules as the
// text cards: fonts loaded first, background dithered, text sharp.

import type { AgentCard } from "../bindings/AgentCard";
import type { CalendarCard } from "../bindings/CalendarCard";
import type { GameCard } from "../bindings/GameCard";
import type { PomodoroCard } from "../bindings/PomodoroCard";
import type { PomodoroPhase } from "../bindings/PomodoroPhase";
import type { PomodoroSettings } from "../bindings/PomodoroSettings";
import type { ReminderCard } from "../bindings/ReminderCard";
import type { ReminderIcon } from "../bindings/ReminderIcon";
import type { TrackInfo } from "../bindings/TrackInfo";
import { clockTime, fillTemplate, percent, type MessageKey, type Translate } from "./i18n";
import { iconTone } from "./icons";
import { phaseColor } from "./pomodoro";
import { H, STILL_DELAY_MS, W, type Prepared } from "./screenImport";
import { append, pieces, trimBeforeEllipsis } from "./lineBreak";
import { compose565, makeLayers, SCRIPT_FONTS } from "./screenText";

// Russian and Chinese letters come from SCRIPT_FONTS (the card fonts are Latin only).
const DISPLAY = `"Bricolage Grotesque Variable", "Segoe UI Emoji", ${SCRIPT_FONTS}, sans-serif`;
const SANS = `"Instrument Sans Variable", "Segoe UI Emoji", ${SCRIPT_FONTS}, sans-serif`;

async function fonts(text: string) {
  try {
    await Promise.all([document.fonts.load(`700 18px ${DISPLAY}`, text), document.fonts.load(`500 12px ${SANS}`, text)]);
  } catch {
    // Fallback fonts still draw.
  }
}

/** Lines of at most `maxW`, at most `max` of them; the last one ends in
 *  "…" when text is left over, and words wider than a line are cut. Chinese
 *  breaks between words (lineBreak.ts). */
function lines(ctx: CanvasRenderingContext2D, text: string, maxW: number, max: number): string[] {
  const width = (s: string) => ctx.measureText(s).width;
  const cut = (s: string) => {
    // By characters, so an emoji or a rare Chinese character is never split.
    const chars = Array.from(s);
    while (chars.length > 1 && width(`${chars.join("")}…`) > maxW) chars.pop();
    return `${trimBeforeEllipsis(chars.join(""))}…`;
  };
  const out: string[] = [];
  let line = "";
  let rest = false;
  for (const p of pieces(text)) {
    const next = append(line, p);
    if (!line || width(next) <= maxW) {
      line = next;
      continue;
    }
    out.push(line);
    line = p.text;
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

// Times and minutes; a Chinese unit after them ("25 分钟") comes from YaHei.
const MONO = '"JetBrains Mono Variable", "Microsoft YaHei UI", monospace';

/** Words the Pomodoro cards need, in the UI language. */
export interface PomodoroWords {
  /** Small caps line on the card. */
  label: string;
  /** Name of the feature, for the card's name in the app. */
  name: string;
  phase: Record<PomodoroPhase, string>;
  round: string;
  /** "3 focus blocks", for the finished series. */
  rounds: (n: number) => string;
  min: string;
  next: string;
  nextFocus: string;
  nextBreak: string;
  last: string;
  doneComplete: string;
  doneStopped: string;
  ended: string;
}

/** The card's words in the current language. */
export function pomodoroWords(tNow: Translate): PomodoroWords {
  return {
    label: tNow("pomo.card.label"),
    name: tNow("pomo.card.name"),
    phase: { focus: tNow("pomo.phase.focus"), break: tNow("pomo.phase.break") },
    round: tNow("pomo.card.round"),
    rounds: (n) => tNow("pomo.card.rounds", { count: n }),
    min: tNow("pomo.card.min"),
    next: tNow("pomo.card.next"),
    nextFocus: tNow("pomo.card.nextFocus"),
    nextBreak: tNow("pomo.card.nextBreak"),
    last: tNow("pomo.card.last"),
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
  const color = phaseColor(s, done ? (check ? "break" : "focus") : c.phase);
  const tone = hexRgb(color);
  const light = tone.map((v) => Math.round(v + (255 - v) * 0.35)) as [number, number, number];
  const endTime = clockTime(c.endedMs);

  const title = done ? (c.complete ? w.doneComplete : w.doneStopped) : w.phase[c.phase];
  const big = done ? `${c.focusMin} ${w.min}` : `${String(c.minutes).padStart(2, "0")}:00`;
  let foot: string;
  if (done) foot = `${w.rounds(c.focusDone)} · ${w.ended} ${endTime}`;
  else if (c.next) foot = `${w.next}: ${c.next.minutes} ${w.min} ${c.next.phase === "focus" ? w.nextFocus : w.nextBreak}`;
  else foot = w.last;
  await Promise.all([fonts(`${w.label} ${title} ${foot}`), document.fonts.load(`700 34px ${MONO}`, big).catch(() => [])]);

  const { bg, fg } = makeLayers();
  const grad = bg.createLinearGradient(0, 0, W * 0.6, H);
  grad.addColorStop(0, rgb(tone, 0.34));
  grad.addColorStop(1, rgb(tone, 0.07));
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  fg.clearRect(0, 0, W, H);
  // Ring on the right: one arc per focus block of the sequence, the ones
  // done in this run filled. A finished series: one arc per focus block done.
  const r = 40;
  const cx = W - 16 - r;
  const cy = H / 2;
  const rounds = check ? 1 : Math.max(1, done ? c.focusDone : c.rounds);
  const filled = check ? 1 : done ? c.focusDone : c.phase === "focus" ? c.round - 1 : Math.min(c.round, c.focusDone);
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
    fg.fillText(done ? `${c.focusDone}` : `${c.round}/${c.rounds}`, cx, cy + 6);
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

const clock = (ms: number) => clockTime(ms);

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

// ----------------------------------------------------------------- games

/** Words the game cards need, in the UI language. */
export interface GameWords {
  /** Name of the feature, for the card's name in the app. */
  name: string;
  started: string;
  /** "Ended {time}". */
  endedAt: string;
  today: string;
  /** "{h} h {min} min", "{min} min" and "under 1 min". */
  hours: string;
  minutes: string;
  less: string;
  gpu: string;
  temp: string;
  cpu: string;
  avg: string;
  peak: string;
  todayTotal: string;
  win: string;
  lose: string;
  cs: string;
  kda: string;
}

export function gameWords(tNow: (key: MessageKey) => string): GameWords {
  return {
    name: tNow("games.card.name"),
    started: tNow("games.card.started"),
    endedAt: tNow("games.card.endedAt"),
    today: tNow("games.card.today"),
    hours: tNow("games.time.hours"),
    minutes: tNow("games.time.minutes"),
    less: tNow("games.time.less"),
    gpu: tNow("games.card.gpu"),
    temp: tNow("games.card.temp"),
    cpu: tNow("games.card.cpu"),
    avg: tNow("games.card.avg"),
    peak: tNow("games.card.peak"),
    todayTotal: tNow("games.card.todayTotal"),
    win: tNow("games.card.win"),
    lose: tNow("games.card.lose"),
    cs: tNow("games.card.cs"),
    kda: tNow("games.card.kda"),
  };
}

function hhmm(ms: number) {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "2 h 14 min", "45 min", "0 min" from words with `{h}` and `{min}`; a
 *  moment under a minute is "under 1 min", not "0 min". */
export function playTime(ms: number, w: Pick<GameWords, "hours" | "minutes" | "less">): string {
  if (ms > 0 && ms < 60_000) return w.less;
  const total = Math.max(0, Math.floor(ms / 60_000));
  const h = Math.floor(total / 60);
  const min = total % 60;
  return h > 0 ? fillTemplate(w.hours, { h, min }) : fillTemplate(w.minutes, { min });
}

/** The game starts: its Steam picture (or its name on a dark tone), when
 *  it started, and today's play time. A still picture: it stays true for
 *  the whole session, nothing moves while the game runs. `programIcon` is
 *  asked only when there is no cover, or one that can't be read. */
export async function gameStartCard(
  c: GameCard,
  cover: Blob | null,
  w: GameWords,
  programIcon: () => Promise<ImageData | null> = () => Promise.resolve(null),
): Promise<Prepared> {
  const label = `${w.started} · ${hhmm(c.startedMs)}`;
  const today = fillTemplate(w.today, { time: playTime(c.todayMs, w) });
  await fonts(`${label} ${c.name} ${today}`);
  const bmp = cover ? await createImageBitmap(cover).catch(() => null) : null;
  // No cover: the program's icon, on its own colour.
  const icon = bmp ? null : await programIcon().catch(() => null);
  const mark = icon ? await createImageBitmap(icon).catch(() => null) : null;
  const tone: [number, number, number] = bmp ? coverTone(bmp) : ((icon && iconTone(icon)) ?? [92, 52, 150]);
  const max = Math.max(...tone, 1);
  const accent = rgb(tone.map((v) => 120 + (v / max) * 135) as [number, number, number]);

  const { bg, fg } = makeLayers();
  bg.fillStyle = rgb(tone, 0.22);
  bg.fillRect(0, 0, W, H);
  const top = 82;
  if (bmp) {
    // Centre-crop the wide picture to the top band.
    const want = W / top;
    let sw = bmp.width;
    let sh = sw / want;
    if (sh > bmp.height) {
      sh = bmp.height;
      sw = sh * want;
    }
    bg.drawImage(bmp, (bmp.width - sw) / 2, (bmp.height - sh) / 2, sw, sh, 0, 0, W, top);
    bmp.close();
  } else {
    bg.fillStyle = rgb(tone, 0.55);
    bg.fillRect(0, 0, W, top);
  }

  fg.clearRect(0, 0, W, H);
  fg.textBaseline = "alphabetic";
  fg.textAlign = "left";
  const left = 12;
  const maxW = W - left * 2;
  if (!bmp) {
    // No picture: the name, large, in the band (after the icon, if any).
    const iconSize = 44;
    const nameLeft = mark ? left + iconSize + 10 : left;
    const nameW = W - nameLeft - left;
    if (mark) {
      fg.imageSmoothingQuality = "high";
      fg.drawImage(mark, left, Math.round((top - iconSize) / 2), iconSize, iconSize);
      mark.close();
    }
    let px = 26;
    let tl: string[] = [];
    for (; px >= 14; px--) {
      fg.font = `700 ${px}px ${DISPLAY}`;
      tl = lines(fg, c.name, nameW, 2);
      if (!tl.some((l) => l.endsWith("…")) || px === 14) break;
    }
    fg.fillStyle = "#ffffff";
    const lh = Math.round(px * 1.1);
    let y = Math.round((top - lh * tl.length) / 2 + px * 0.85);
    for (const l of tl) {
      fg.fillText(l, nameLeft, y);
      y += lh;
    }
  }
  fg.letterSpacing = "1px";
  fg.fillStyle = accent;
  fitFont(fg, label, (px) => `600 ${px}px ${SANS}`, maxW, 9, 7);
  fg.fillText(label.toLocaleUpperCase(document.documentElement.lang || undefined), left, top + 17);
  fg.letterSpacing = "0px";
  fg.fillStyle = "#ffffff";
  if (bmp) {
    fitFont(fg, c.name, (px) => `700 ${px}px ${DISPLAY}`, maxW, 15, 10);
    fg.fillText(lines(fg, c.name, maxW, 1)[0] ?? "", left, top + 34);
  }
  fg.fillStyle = "rgba(255,255,255,0.72)";
  fg.font = `500 11px ${SANS}`;
  fg.fillText(today, left, H - 9);

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return { frames: [px565], delaysMs: [STILL_DELAY_MS], name: `${w.name} · ${c.name}`, kind: "image" };
}

/** The session ended: how long, the graphics card and processor load,
 *  the hottest the card got, and today's total. Readings the computer did
 *  not give are left out. */
export async function gameSummaryCard(c: GameCard, w: GameWords): Promise<Prepared> {
  const end = c.endedMs ?? Date.now();
  const lol = c.lol;
  const who = lol?.champion ? `${c.name} · ${lol.champion}` : c.name;
  const label = `${who} · ${fillTemplate(w.endedAt, { time: hhmm(end) })}`;
  const length = playTime(end - c.startedMs, w);
  // A League of Legends match: the result first, then K/D/A.
  const result = lol?.win == null ? null : lol.win ? w.win : w.lose;
  const time = result ?? length;
  const pct = (v: number | null) => (v == null ? null : percent(Math.round(v)));
  const cells: [string, string][] = [];
  if (lol) {
    cells.push([w.kda, `${lol.kills} / ${lol.deaths} / ${lol.assists}`]);
    cells.push([w.cs, `${lol.cs}${result ? ` · ${length}` : ""}`]);
  }
  if (c.gpuAvg != null) {
    const peak = pct(c.gpuPeak);
    cells.push([w.gpu, `${w.avg} ${pct(c.gpuAvg)}${peak ? ` · ${w.peak} ${peak}` : ""}`]);
  }
  if (c.tempPeak != null) cells.push([w.temp, `${w.peak} ${Math.round(c.tempPeak)} °C`]);
  if (c.cpuAvg != null) {
    const peak = pct(c.cpuPeak);
    cells.push([w.cpu, `${w.avg} ${pct(c.cpuAvg)}${peak ? ` · ${w.peak} ${peak}` : ""}`]);
  }
  cells.push([w.todayTotal, playTime(c.todayMs, w)]);
  await fonts(`${label} ${time} ${cells.flat().join(" ")}`);

  const { bg, fg } = makeLayers();
  const grad = bg.createLinearGradient(0, 0, W, H);
  grad.addColorStop(0, "rgb(30, 34, 46)");
  grad.addColorStop(1, "rgb(12, 14, 20)");
  bg.fillStyle = grad;
  bg.fillRect(0, 0, W, H);

  fg.clearRect(0, 0, W, H);
  fg.textBaseline = "alphabetic";
  fg.textAlign = "left";
  const left = 12;
  const maxW = W - left * 2;
  fg.fillStyle = "rgba(255,255,255,0.6)";
  fitFont(fg, label, (px) => `600 ${px}px ${SANS}`, maxW, 10, 7);
  fg.fillText(lines(fg, label, maxW, 1)[0] ?? "", left, 20);
  fg.fillStyle = lol?.win === true ? "#f0c75e" : lol?.win === false ? "#ff6b5e" : "#ffffff";
  fitFont(fg, time, (px) => `700 ${px}px ${DISPLAY}`, maxW, 32, 18);
  fg.fillText(time, left, 54);

  // Two columns of readings.
  const colW = (W - left * 2 - 10) / 2;
  cells.slice(0, 4).forEach(([k, v], i) => {
    const x = left + (i % 2) * (colW + 10);
    const y = 78 + Math.floor(i / 2) * 30;
    fg.fillStyle = "rgba(255,255,255,0.55)";
    fg.font = `500 9.5px ${SANS}`;
    fg.fillText(k, x, y);
    fg.fillStyle = "#ffffff";
    fitFont(fg, v, (px) => `600 ${px}px ${SANS}`, colW, 11.5, 8);
    fg.fillText(v, x, y + 14);
  });

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return { frames: [px565], delaysMs: [STILL_DELAY_MS], name: `${w.name} · ${c.name} · ${time}`, kind: "image" };
}

// ---------------------------------------------------------------- agents

/** Words the agents' cards need, in the UI language. */
export interface AgentCardWords {
  locale: string;
  /** Name of the feature, for the card's name in the app. */
  name: string;
  done: string;
  hours: string;
  minutes: string;
  less: string;
}

export function agentCardWords(tNow: (key: MessageKey) => string, locale: string): AgentCardWords {
  return {
    locale,
    name: tNow("ag.card.name"),
    done: tNow("ag.card.done"),
    hours: tNow("games.time.hours"),
    minutes: tNow("games.time.minutes"),
    less: tNow("games.time.less"),
  };
}

/** An agent's card when it chose no colour: Keymotion's orange. */
const AGENT_TONE: [number, number, number] = [255, 154, 85];
const PICTURE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

/**
 * An agent's words as plain text for the canvas: never read as markup (they
 * are only ever drawn with `fillText`), control and direction-changing
 * characters left out, line breaks kept, and at most `max` characters.
 */
export function agentText(s: string | null | undefined, max: number): string {
  if (!s) return "";
  const hidden = (c: number) =>
    c < 0x20 ||
    (c >= 0x7f && c <= 0x9f) ||
    c === 0x200b ||
    c === 0x200e ||
    c === 0x200f ||
    (c >= 0x202a && c <= 0x202e) ||
    (c >= 0x2066 && c <= 0x2069) ||
    c === 0xfeff;
  const chars = Array.from(s.replace(/\r\n?/g, "\n").replace(/\t/g, " ")).filter(
    (ch) => ch === "\n" || !hidden(ch.codePointAt(0)!),
  );
  return Array.from(chars.join("").trim()).slice(0, max).join("");
}

/** The agent's picture, decoded (an animated one gives its first frame). */
async function agentPicture(c: AgentCard): Promise<ImageBitmap | null> {
  const img = c.image;
  if (!img) return null;
  if (!PICTURE_TYPES.has(img.mime)) throw new Error(`unsupported picture type ${img.mime}`);
  let bytes: Uint8Array<ArrayBuffer>;
  try {
    const bin = atob(img.data);
    bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  } catch {
    throw new Error("the picture's data is not valid base64");
  }
  try {
    return await createImageBitmap(new Blob([bytes], { type: img.mime }));
  } catch {
    throw new Error("the picture could not be decoded");
  }
}

/**
 * The picture inside a box, whole (letterboxed), centred. A small picture
 * that grows at least twice (pixel art, an icon) is scaled by whole steps
 * without smoothing and drawn on the text layer, so its edges stay sharp;
 * a photo is smoothed and dithered with the background.
 */
function drawPicture(
  bg: CanvasRenderingContext2D,
  fg: CanvasRenderingContext2D,
  bmp: ImageBitmap,
  x: number,
  y: number,
  w: number,
  h: number,
) {
  const k = Math.min(w / bmp.width, h / bmp.height);
  if (k >= 2) {
    const s = Math.floor(k);
    const dw = bmp.width * s;
    const dh = bmp.height * s;
    fg.save();
    fg.imageSmoothingEnabled = false;
    fg.drawImage(bmp, Math.round(x + (w - dw) / 2), Math.round(y + (h - dh) / 2), dw, dh);
    fg.restore();
    return;
  }
  const dw = bmp.width * k;
  const dh = bmp.height * k;
  bg.save();
  bg.imageSmoothingEnabled = true;
  bg.imageSmoothingQuality = "high";
  bg.drawImage(bmp, x + (w - dw) / 2, y + (h - dh) / 2, dw, dh);
  bg.restore();
}

/** Paragraphs (the agent's line breaks) wrapped to `maxW`, at most `max`
 *  lines in all; the last one ends in "…" when text is left over. */
function paragraphs(ctx: CanvasRenderingContext2D, text: string, maxW: number, max: number): string[] {
  const out: string[] = [];
  const parts = text.split("\n").filter((p) => p.trim());
  for (let i = 0; i < parts.length && out.length < max; i++) {
    out.push(...lines(ctx, parts[i]!, maxW, max - out.length));
    if (out.length >= max && i < parts.length - 1) {
      const last = out[max - 1]!;
      if (!last.endsWith("…")) out[max - 1] = lines(ctx, `${last} …`, maxW, 1)[0] ?? last;
    }
  }
  return out.slice(0, max);
}

/** The largest size from `max` down to `min` at which `text` fits in
 *  `rows` lines without being cut. */
function fitLines(
  ctx: CanvasRenderingContext2D,
  text: string,
  font: (px: number) => string,
  maxW: number,
  rows: number,
  max: number,
  min: number,
) {
  let px = max;
  let ls: string[] = [];
  for (; px >= min; px--) {
    ctx.font = font(px);
    ls = paragraphs(ctx, text, maxW, rows);
    if (!ls.some((l) => l.endsWith("…")) || px === min) break;
  }
  return { px: Math.max(px, min), lines: ls };
}

/** Lines drawn inside a clip of the area they may use. */
function drawLines(
  ctx: CanvasRenderingContext2D,
  ls: string[],
  x: number,
  y: number,
  step: number,
  clip: [number, number, number, number],
) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(...clip);
  ctx.clip();
  for (const l of ls) {
    ctx.fillText(l, x, y);
    y += step;
  }
  ctx.restore();
}

/** A ring with a check mark (a long task's card). */
function drawCheck(ctx: CanvasRenderingContext2D, cx: number, cy: number, r: number, color: string) {
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = 3;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(cx - r * 0.42, cy + r * 0.02);
  ctx.lineTo(cx - r * 0.1, cy + r * 0.34);
  ctx.lineTo(cx + r * 0.45, cy - r * 0.3);
  ctx.stroke();
  ctx.restore();
}

/** The live cards' background: the colour fading, a stripe on the left. */
function agentLayers(tone: [number, number, number]) {
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
  return { bg, fg };
}

/**
 * An agent's card (`agent-card`), in the live cards' style: a stripe and a
 * background in the agent's colour, its title in that colour, its text in
 * white as large as fits, and its picture whole. A long task's card (F)
 * says "Done" and how long it took, with the project folder above.
 */
export async function agentCard(c: AgentCard, w: AgentCardWords): Promise<Prepared> {
  const tone = c.color ?? AGENT_TONE;
  const light = tone.map((v) => Math.round(v + (255 - v) * 0.45)) as [number, number, number];
  if (c.kind === "longTask") return longTaskCard(c, w, tone, light);

  const title = agentText(c.title, 48).replace(/\n/g, " ");
  const text = agentText(c.text, 160);
  const bmp = await agentPicture(c);
  await fonts(`${title} ${text}`);
  const { bg, fg } = agentLayers(tone);

  if (bmp && !title && !text) {
    // The picture alone: as large as the screen allows.
    drawPicture(bg, fg, bmp, 4, 0, W - 4, H);
    bmp.close();
  } else if (bmp) {
    // The picture on the left, the words beside it.
    const size = 104;
    const px = 14;
    const py = Math.round((H - size) / 2);
    bg.save();
    bg.beginPath();
    bg.roundRect(px, py, size, size, 8);
    bg.fillStyle = rgb(tone, 0.16);
    bg.fill();
    bg.clip();
    drawPicture(bg, fg, bmp, px, py, size, size);
    bg.restore();
    bmp.close();
    const left = px + size + 12;
    const maxW = W - left - 10;
    let top = py;
    if (title) {
      fg.fillStyle = rgb(light);
      fitFont(fg, title, (p) => `700 ${p}px ${DISPLAY}`, maxW, 15, 10);
      drawLines(fg, lines(fg, title, maxW, 1), left, top + 13, 0, [left, 0, maxW, top + 20]);
      top += 24;
    }
    if (text) {
      const fit = fitLines(fg, text, (p) => `600 ${p}px ${SANS}`, maxW, title ? 4 : 5, 15, 10);
      fg.fillStyle = "#ffffff";
      drawLines(fg, fit.lines, left, top + fit.px, Math.round(fit.px * 1.22), [left, top, maxW, H - top - 4]);
    }
  } else {
    // Words only: the title above, the text large.
    const left = 16;
    const maxW = W - left - 12;
    if (title && text) {
      fg.fillStyle = rgb(light);
      fitFont(fg, title, (p) => `700 ${p}px ${DISPLAY}`, maxW, 17, 11);
      drawLines(fg, lines(fg, title, maxW, 1), left, 30, 0, [left, 0, maxW, 40]);
    }
    const body = text || title;
    const top = title && text ? 42 : 10;
    const fit = fitLines(fg, body, (p) => `700 ${p}px ${DISPLAY}`, maxW, 3, 26, 12);
    const step = Math.round(fit.px * 1.15);
    const block = step * (fit.lines.length - 1) + fit.px;
    const y = Math.round(top + (H - 8 - top - block) / 2 + fit.px * 0.85);
    fg.fillStyle = "#ffffff";
    drawLines(fg, fit.lines, left, y, step, [left, top, maxW, H - top - 2]);
  }

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  const first = (title || text.split("\n")[0] || "").trim();
  return { frames: [px565], delaysMs: [STILL_DELAY_MS], name: first ? `${w.name} · ${first}` : w.name, kind: "image" };
}

/** Behaviour F: a long task finished. "Done", how long it took, and the
 *  project folder; it stays true however long it shows. */
async function longTaskCard(
  c: AgentCard,
  w: AgentCardWords,
  tone: [number, number, number],
  light: [number, number, number],
): Promise<Prepared> {
  // A folder's name, as it is written (in capitals "keymotion" would
  // become "KEYMOTİON" in Turkish).
  const label = agentText(c.label, 64).replace(/\n/g, " ");
  const length = playTime(Math.max(0, c.minutes ?? 0) * 60_000, w);
  await fonts(`${label} ${w.done} ${length}`);
  const { bg, fg } = agentLayers(tone);
  drawCheck(fg, W - 38, 46, 20, rgb(light));

  const left = 16;
  const maxW = W - left - 12;
  const beside = maxW - 56;
  if (label) {
    fg.fillStyle = rgb(light);
    fitFont(fg, label, (px) => `600 ${px}px ${SANS}`, beside, 11, 8);
    drawLines(fg, lines(fg, label, beside, 1), left, 25, 0, [left, 0, beside, 32]);
  }
  fg.fillStyle = "#ffffff";
  fitFont(fg, w.done, (px) => `700 ${px}px ${DISPLAY}`, beside, 32, 18);
  drawLines(fg, [w.done], left, 70, 0, [left, 30, beside, 50]);
  fg.fillStyle = rgb(light);
  fitFont(fg, length, (px) => `700 ${px}px ${DISPLAY}`, maxW, 24, 14);
  drawLines(fg, [length], left, 108, 0, [left, 80, maxW, 40]);

  const px565 = compose565(bg.getImageData(0, 0, W, H).data, fg.getImageData(0, 0, W, H).data, true);
  return { frames: [px565], delaysMs: [STILL_DELAY_MS], name: `${w.done} · ${length}`, kind: "image" };
}
