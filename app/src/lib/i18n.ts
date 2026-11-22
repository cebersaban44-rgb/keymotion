// The interface's words in four languages, and the numbers, dates and times
// around them. The text is in i18n/: en.ts is the source and the list of keys,
// tr.ts is complete, ru.ts and zh-CN.ts fill in over time; a key they lack
// shows the English text.

import type { Connection } from "../bindings/Connection";
import { en, type MessageKey, type Messages } from "./i18n/en";
import { intlTag, type Locale } from "./i18n/locales";
import { formatMessage, pickText, type MessageVars } from "./i18n/plural";
import { ru } from "./i18n/ru";
import { tr } from "./i18n/tr";
import { zhCN } from "./i18n/zh-CN";
import { useApp } from "./store";

export type { MessageKey } from "./i18n/en";
export { intlTag, LOCALE_NAMES, LOCALES, localeForWindows, type Locale } from "./i18n/locales";
export type { MessageVars } from "./i18n/plural";

const dictionaries: Record<Locale, Partial<Messages>> = { en, tr, ru, "zh-CN": zhCN };

const current = (): Locale => useApp.getState().locale;

export type Translate = (key: MessageKey, vars?: MessageVars) => string;

/**
 * The message for `key` in `locale`, or in English while `locale` has no text
 * for it (pickText). Each `{name}` is replaced by `vars.name` and each plural
 * ("{count, plural, one {…} other {…}}") is resolved with the rules of the
 * language the text is in. Without `vars` the text is returned as it is (the
 * tray gets its templates this way and fills them in itself).
 */
function message(locale: Locale, key: MessageKey, vars?: MessageVars) {
  const { text, fromOwn } = pickText(dictionaries[locale], en, key, vars?.count);
  return vars ? formatMessage(text, vars, intlTag(fromOwn ? locale : "en")) : text;
}

/**
 * A template fetched earlier without values (`tNow(key)`), filled in now:
 * the cards keep their words and fill in times and counts when they draw.
 * Plural forms use the current locale's rules.
 */
export function fillTemplate(template: string, vars: MessageVars) {
  return formatMessage(template, vars, intlTag(current()));
}

/** Translate outside React (current locale at call time). */
export function tNow(key: MessageKey, vars?: MessageVars) {
  return message(current(), key, vars);
}

// Development only (`pnpm dev` in a browser): tools/screenshots finds buttons
// by their text in the current language. Vite drops this from the installed app.
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as { __keymotionT?: typeof tNow }).__keymotionT = tNow;
}

export function useT(): Translate {
  const locale = useApp((s) => s.locale);
  return (key, vars) => message(locale, key, vars);
}

/**
 * A keyboard model's name in the interface language: the dictionaries may
 * name a model the way it is sold there (key "model.<id>"); otherwise the
 * model file's name. In Chinese the AK980 PRO is 黑爵 AK980 V2（带屏版）.
 */
export function modelTitle(t: Translate, modelId: string, fallback: string): string {
  const key = `model.${modelId}`;
  return key in en ? t(key as MessageKey) : fallback;
}

export const connectionKey = (c: Connection, short = false): MessageKey =>
  `conn.${c}${short ? ".short" : ""}` as MessageKey;

// ------------------------------------------------------------- formatting
//
// Every number, date and time the interface shows goes through these, in the
// interface language (not Windows' own): "40%" / "%40" / "40 %" / "40%",
// "2.8" / "2,8", "Nov 14" / "14 Kas" / "14 нояб." / "11月14日".

const formats = new Map<string, Intl.NumberFormat | Intl.DateTimeFormat | Intl.RelativeTimeFormat>();

function format<T extends Intl.NumberFormat | Intl.DateTimeFormat | Intl.RelativeTimeFormat>(key: string, make: () => T): T {
  let f = formats.get(key);
  if (!f) {
    f = make();
    formats.set(key, f);
  }
  return f as T;
}

/** "40%" in English and Chinese, "%40" in Turkish, "40 %" (no-break space) in Russian. */
export function percent(n: number, locale: Locale = current()) {
  return format(`percent:${locale}`, () =>
    new Intl.NumberFormat(intlTag(locale), { style: "percent", maximumFractionDigits: 2 }),
  ).format(n / 100);
}

/** "2,8" in Turkish and Russian, "2.8" in English and Chinese. `fixed` keeps every digit ("2,20"). */
export function decimal(n: number, digits = 1, fixed = false, locale: Locale = current()) {
  return n.toLocaleString(intlTag(locale), {
    maximumFractionDigits: digits,
    minimumFractionDigits: fixed ? digits : 0,
  });
}

/** Windows' clock, "h23" or "h12" (WebView2 starts with Windows' language). */
function systemHourCycle(): Intl.DateTimeFormatOptions["hourCycle"] {
  try {
    return new Intl.DateTimeFormat(undefined, { hour: "numeric" }).resolvedOptions().hourCycle;
  } catch {
    return undefined;
  }
}

/**
 * A time of day, "14:30". Turkish, Russian and Chinese use the 24-hour clock;
 * English follows Windows' clock ("02:30 PM" on a US Windows, "14:30" on most
 * others), as it did before the interface had its own formats.
 */
export function clockTime(ms: number, locale: Locale = current()) {
  return format(`clock:${locale}`, () =>
    new Intl.DateTimeFormat(intlTag(locale), {
      hour: "2-digit",
      minute: "2-digit",
      ...(locale === "en" ? { hourCycle: systemHourCycle() } : { hourCycle: "h23" as const }),
    }),
  ).format(ms);
}

/** "Nov 14", "14 Kas", "14 нояб.", "11月14日". */
export function shortDate(ms: number, locale: Locale = current()) {
  return format(`day:${locale}`, () => new Intl.DateTimeFormat(intlTag(locale), { day: "numeric", month: "short" })).format(ms);
}

/** "and"-lists: "lighting, key mapping and macros" in the interface language. */
export function listOf(items: string[], locale: Locale = current()): string {
  return new Intl.ListFormat(intlTag(locale), { type: "conjunction" }).format(items);
}

/**
 * How long ago `ms` was, as a whole phrase: "5 min. ago", "5 dk. önce",
 * "5 мин. назад", "5分钟前" (Intl.RelativeTimeFormat, short style).
 * Seconds under a minute, minutes under an hour, hours under two days, then days.
 */
export function timeAgo(ms: number, locale: Locale = current(), now = Date.now()) {
  const f = format(`ago:${locale}`, () => new Intl.RelativeTimeFormat(intlTag(locale), { style: "short", numeric: "always" }));
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return f.format(-s, "second");
  const m = Math.round(s / 60);
  if (m < 60) return f.format(-m, "minute");
  const h = Math.round(m / 60);
  if (h < 48) return f.format(-h, "hour");
  return f.format(-Math.round(h / 24), "day");
}

/** `timeAgo` in the current interface language. */
export function useAgo() {
  const locale = useApp((s) => s.locale);
  return (ms: number) => timeAgo(ms, locale);
}
