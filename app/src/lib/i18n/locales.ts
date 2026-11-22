// The interface languages and how they map to Windows' languages and to the
// Intl APIs. No imports: the store reads this while it is created, and the
// i18n checks (tools/check-i18n.mjs) load it in Node.

/** English is the source text; a key another language lacks shows in English. */
export const LOCALES = ["en", "tr", "ru", "zh-CN"] as const;
export type Locale = (typeof LOCALES)[number];

/** Each language in its own words, for the language picker. */
export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  tr: "Türkçe",
  ru: "Русский",
  "zh-CN": "简体中文",
};

const TAGS: Record<Locale, string> = { en: "en-US", tr: "tr-TR", ru: "ru-RU", "zh-CN": "zh-CN" };

/** The BCP 47 tag for `Intl` (dates, numbers, plural rules, lists). */
export function intlTag(locale: Locale): string {
  return TAGS[locale];
}

/**
 * The language to start in for a Windows display language (`navigator.language`
 * in WebView2), until the user picks one in Settings: Turkish and Russian as
 * they are; Simplified Chinese for zh-Hans, zh-CN and zh-SG; English for
 * everything else, Traditional Chinese (zh-TW, zh-HK, zh-Hant), Ukrainian,
 * Belarusian and Kazakh included (owner's decisions, 2026-11-12 and
 * 2026-11-22).
 */
export function localeForWindows(tag: string): Locale {
  let parsed: Intl.Locale;
  try {
    // maximize() adds the likely script: zh-CN → zh-Hans-CN, zh-HK → zh-Hant-HK.
    parsed = new Intl.Locale(tag).maximize();
  } catch {
    return "en";
  }
  switch (parsed.language) {
    case "tr":
      return "tr";
    case "ru":
      return "ru";
    case "zh":
      return parsed.script === "Hans" ? "zh-CN" : "en";
    default:
      return "en";
  }
}
