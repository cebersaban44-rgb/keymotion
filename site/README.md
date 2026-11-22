# Keymotion documentation site

The user guide and developer docs at <https://cebersaban44-rgb.github.io/keymotion/>, built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build). English is the default language; Turkish pages live under `tr/`, Russian under `ru/` and Simplified Chinese under `zh-cn/`.

```bash
pnpm --dir site install
pnpm --dir site dev      # http://localhost:4321/keymotion/
pnpm --dir site build    # also checks every internal link
```

The site is published from `main` by `.github/workflows/pages.yml`, so it always describes the released app.

## Where pages live

| Page | English | Other languages |
|---|---|---|
| Home | `src/content/docs/index.mdx` | `src/content/docs/<lang>/index.mdx` |
| Any other page | `src/content/docs/<section>/<page>.mdx` | `src/content/docs/<lang>/<section>/<page>.mdx` |

`<lang>` is the folder name: `tr`, `ru` or `zh-cn` (the Chinese pages' language tag is `zh-CN`). Every language uses the same file name, so the language switcher finds the pair. A page that is not translated yet shows the English one, under Starlight's notice that it is not in that language yet. The sidebar order is set in `astro.config.mjs`; its labels come from each page's `title`, and the group names have their translations there.

## Words in components

Starlight translates its own interface (search, menus, "Edit page"). The words the site's own components show, such as the GitHub download button and the Store badge's label, are in `src/i18n.ts`, one entry per language. The Store badges in `public/images/store/` are Microsoft's own, one pair per language.

Russian and Chinese pages open with a short note that they were translated with AI help, with a link to report a wrong translation on the guide's issue form (`src/components/AiNote.astro`). Once a native speaker has checked a language, add them to `REVIEWERS` in `src/i18n.ts` and the note names them.

Instrument Sans and Bricolage Grotesque have Latin letters only, so Russian and Chinese text uses system fonts that have its letters (`src/styles/theme.css`); the fonts follow each element's `lang`.

## Writing a page

```mdx
---
title: Lighting
description: One sentence that says what the page covers, for search results and link previews.
---

import Screenshot from "~/components/Screenshot.astro";
import { Aside, Steps } from "@astrojs/starlight/components";

One or two sentences: what this part of the app is for.

<Screenshot name="lighting" alt="The Lighting page with the Wave effect selected" />

## Pick an effect

<Steps>

1. Open **Lighting** in the sidebar.
2. Choose an effect.

</Steps>

<Aside type="note" title="Needs the USB cable">
With the 2.4 GHz receiver or Bluetooth, this page is locked.
</Aside>
```

- **Write for people, not for programmers.** Describe what is on the screen and what happens when you use it. Leave out how it works inside.
- **Name controls exactly as the app does**, in bold: **Save to keyboard**. The app's words are in `app/src/lib/i18n/` (`en.ts`, `tr.ts`, …); use the English text on English pages and each language's own text on its pages.
- **Short sentences, active voice, sentence-case headings.** No marketing words, no emoji. English pages use American spelling, like the app: color, license.
- **Turkish pages are written in Turkish, not translated word by word.** They use Turkish words only (for example "odak zamanlayıcısı", never "Pomodoro"), like the app itself. File formats, key names and brand names stay as they are.
- **Be exact.** Every behavior on a page must match the app. When unsure, check the code or leave it out.
- **No personal data in examples.** Use made-up meeting and reminder names, never a real calendar address.
- **Link between pages with the full path**, including `/keymotion/` and, on pages in another language, its folder (`tr/`, `ru/`, `zh-cn/`): `[Macros](/keymotion/guide/macros/)`, `[Makrolar](/keymotion/tr/guide/macros/)`. The build checks every such link and fails on a broken one.

## Screenshots

`<Screenshot name="…" alt="…" />` shows `src/assets/screens/<lang>/light/<name>.webp` in the light theme and `…/dark/<name>.webp` in the dark theme. The shots are taken by `tools/screenshots` from the app's development build with a stand-in keyboard and sample data, so they can be retaken whenever the app changes. Shots are taken in English and in the dark theme only: pages in the other languages and the light theme show the same picture, unless a shot in that language or a light one exists. Until a shot exists, the page shows a labelled placeholder.

## Search engines and AI assistants

- **Titles and descriptions.** Every page has a `description` in its front matter: one or two sentences that say what the page answers, with the keyboard's name where it fits. Search results and AI assistants show it.
- **Home pages.** Each language's `index.mdx` sets its own `<title>` and Open Graph title (otherwise it would be "Keymotion | Keymotion") and carries a JSON-LD `SoftwareApplication` description of the app. When the app gains or loses a main feature, update its `featureList` in every language.
- **Share picture.** `public/images/social-preview.png` (made by `tools/screenshots/social.mjs`) shows when a link to any page is shared. It is set in `astro.config.mjs`.
- **AI assistants.** `starlight-llms-txt` writes `/keymotion/llms.txt`, `llms-full.txt` and `llms-small.txt` from the English pages. Its summary is in `astro.config.mjs`.
- **Sitemap and IndexNow.** Starlight writes `sitemap-index.xml`. After each publish, `pages.yml` sends the changed pages to IndexNow (`tools/site/indexnow.mjs`).
- **The host root.** `robots.txt`, the IndexNow key file and the root page that sends visitors here live in the [cebersaban44-rgb.github.io](https://github.com/cebersaban44-rgb/cebersaban44-rgb.github.io) repository, because search engines read them only at the root of the host.
