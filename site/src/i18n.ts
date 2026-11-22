// The words the site's own components show, in each of its languages.
// Starlight translates its own interface (search, menus, "Edit page", the
// language list), and the sidebar's group names are in astro.config.mjs;
// everything else a component says is here.
//
// The Russian and Chinese words were written with AI help and wait for a
// native speaker's review; pages in those languages say so (AiNote.astro).

export type SiteLocale = "en" | "tr" | "ru" | "zh-cn";

/**
 * The site language of a page, from `Astro.currentLocale` ("tr", "ru",
 * "zh-CN"). English otherwise, also when there is no page (the llms.txt
 * routes render the English pages without one).
 */
export function siteLocale(current: string | undefined): SiteLocale {
  const tag = (current ?? "").toLowerCase();
  if (tag.startsWith("tr")) return "tr";
  if (tag.startsWith("ru")) return "ru";
  if (tag.startsWith("zh")) return "zh-cn";
  return "en";
}

/** The language tag for `lang` attributes (Chinese fonts need "zh-CN"). */
export const LANG: Record<SiteLocale, string> = { en: "en", tr: "tr", ru: "ru", "zh-cn": "zh-CN" };

interface Strings {
  /** DownloadButtons: the button that opens the latest GitHub release. */
  downloadFromGitHub: string;
  /**
   * StoreBadge: Microsoft's badge in this language
   * (public/images/store/get-it-from-microsoft-<file>-<dark|light>.svg),
   * its width at its 44 px height, and the words printed on it, which
   * screen readers read out instead.
   */
  storeBadge: { file: string; width: number; label: string };
  /**
   * AiNote: languages translated with AI help say so on every page, with a
   * link to report a wrong translation. `reviewedBy` is used once a native
   * speaker has checked the language (REVIEWERS below); `{name}` is their name.
   */
  aiNote?: { text: string; reviewedBy: string; report: string };
}

export const UI: Record<SiteLocale, Strings> = {
  en: {
    downloadFromGitHub: "Download from GitHub",
    storeBadge: { file: "en", width: 161, label: "Download from the Microsoft Store" },
  },
  tr: {
    downloadFromGitHub: "GitHub'dan indir",
    storeBadge: { file: "tr", width: 180, label: "Microsoft Store'dan indirin" },
  },
  ru: {
    downloadFromGitHub: "Скачать с GitHub",
    storeBadge: { file: "ru", width: 161, label: "Скачать из Microsoft Store" },
    aiNote: {
      // A no-break space keeps "ИИ." off a line of its own.
      text: "Русская версия сайта переведена с помощью ИИ.",
      reviewedBy: "Проверка перевода: {name}.",
      report: "Сообщить об ошибке в переводе",
    },
  },
  "zh-cn": {
    downloadFromGitHub: "从 GitHub 下载",
    storeBadge: { file: "zh-cn", width: 161, label: "从 Microsoft Store 下载" },
    aiNote: {
      text: "本站中文版由 AI 辅助翻译。",
      reviewedBy: "译文审校：{name}。",
      report: "报告翻译问题",
    },
  },
};

/**
 * Who checked each AI-translated language, once someone has. For example
 * `ru: { name: "Name", url: "https://github.com/name" }`; `url` is optional.
 */
export const REVIEWERS: Partial<Record<SiteLocale, { name: string; url?: string }>> = {};

/** The guide's issue form (.github/ISSUE_TEMPLATE/docs.yml). */
export const REPORT_FORM = "https://github.com/cebersaban44-rgb/keymotion/issues/new?template=docs.yml";
