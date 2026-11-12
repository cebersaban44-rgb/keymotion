# Keymotion documentation site

The user guide and developer docs at <https://cebersaban44-rgb.github.io/keymotion/>, built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build). English is the default language; Turkish pages live under `tr/`.

```bash
pnpm --dir site install
pnpm --dir site dev      # http://localhost:4321/keymotion/
pnpm --dir site build    # also checks every internal link
```

The site is published from `main` by `.github/workflows/pages.yml`, so it always describes the released app.

## Where pages live

| Page | English | Turkish |
|---|---|---|
| Home | `src/content/docs/index.mdx` | `src/content/docs/tr/index.mdx` |
| Any other page | `src/content/docs/<section>/<page>.mdx` | `src/content/docs/tr/<section>/<page>.mdx` |

Both languages use the same file name, so the language switcher finds the pair. The sidebar order is set in `astro.config.mjs`; its labels come from each page's `title`.

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
- **Name controls exactly as the app does**, in bold: **Save to keyboard**. The app's words are in `app/src/lib/i18n.ts`; use the English text on English pages and the Turkish text on Turkish pages.
- **Short sentences, active voice, sentence-case headings.** No marketing words, no emoji. English pages use American spelling, like the app: color, license.
- **Turkish pages are written in Turkish, not translated word by word.** They use Turkish words only (for example "odak zamanlayıcısı", never "Pomodoro"), like the app itself. File formats, key names and brand names stay as they are.
- **Be exact.** Every behavior on a page must match the app. When unsure, check the code or leave it out.
- **No personal data in examples.** Use made-up meeting and reminder names, never a real calendar address.
- **Link between pages with the full path**, including `/keymotion/` and, on Turkish pages, `tr/`: `[Macros](/keymotion/guide/macros/)`, `[Makrolar](/keymotion/tr/guide/macros/)`. The build checks every such link and fails on a broken one.

## Screenshots

`<Screenshot name="…" alt="…" />` shows `src/assets/screens/<lang>/light/<name>.webp` in the light theme and `…/dark/<name>.webp` in the dark theme. The shots are taken by `tools/screenshots` from the app's development build with a stand-in keyboard and sample data, so they can be retaken whenever the app changes. Shots are taken in English and in the dark theme only: Turkish pages and the light theme show the same picture, unless a Turkish or light one exists. Until a shot exists, the page shows a labelled placeholder.

## Search engines and AI assistants

- **Titles and descriptions.** Every page has a `description` in its front matter: one or two sentences that say what the page answers, with the keyboard's name where it fits. Search results and AI assistants show it.
- **Home pages.** `index.mdx` and `tr/index.mdx` set their own `<title>` and Open Graph title (otherwise it would be "Keymotion | Keymotion") and carry a JSON-LD `SoftwareApplication` description of the app. When the app gains or loses a main feature, update its `featureList` in both.
- **Share picture.** `public/images/social-preview.png` (made by `tools/screenshots/social.mjs`) shows when a link to any page is shared. It is set in `astro.config.mjs`.
- **AI assistants.** `starlight-llms-txt` writes `/keymotion/llms.txt`, `llms-full.txt` and `llms-small.txt` from the English pages. Its summary is in `astro.config.mjs`.
- **Sitemap and IndexNow.** Starlight writes `sitemap-index.xml`. After each publish, `pages.yml` sends the changed pages to IndexNow (`tools/site/indexnow.mjs`).
- **The host root.** `robots.txt`, the IndexNow key file and the root page that sends visitors here live in the [cebersaban44-rgb.github.io](https://github.com/cebersaban44-rgb/cebersaban44-rgb.github.io) repository, because search engines read them only at the root of the host.
