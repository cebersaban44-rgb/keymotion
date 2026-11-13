// The Keymotion documentation site (roadmap stage 7). Built with Astro and
// Starlight and published to GitHub Pages by .github/workflows/pages.yml.
// English is the default language at the site root; Turkish lives under /tr/.
import { fileURLToPath } from "node:url";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import starlightLinksValidator from "starlight-links-validator";
import starlightLlmsTxt from "starlight-llms-txt";

const REPO = "https://github.com/cebersaban44-rgb/keymotion";
const SITE = "https://cebersaban44-rgb.github.io/keymotion";
// The card that shows when a link to the site is shared (1280×640, made by
// tools/screenshots/social.mjs; the same file is the repository's social preview).
const SHARE_IMAGE = `${SITE}/images/social-preview.png`;
const SHARE_IMAGE_ALT =
  "Keymotion: lighting, key mapping, macros and the little screen of the AJAZZ AK980 PRO, from Windows. Beside it, the keyboard lit in rainbow colors.";

export default defineConfig({
  site: "https://cebersaban44-rgb.github.io",
  base: "/keymotion",
  trailingSlash: "always",
  // `~/components/Screenshot.astro` from any page, whatever its folder depth.
  vite: { resolve: { alias: { "~": fileURLToPath(new URL("./src", import.meta.url)) } } },
  integrations: [
    starlight({
      title: "Keymotion",
      description: "Open-source Windows app for the AJAZZ AK980 PRO keyboard: lighting, key mapping, macros and the keyboard's screen.",
      logo: { src: "./src/assets/logo.svg" },
      favicon: "/favicon.svg",
      // Starlight already writes the title, description, canonical address,
      // language links and the other Open Graph tags; these add the picture.
      head: [
        { tag: "meta", attrs: { property: "og:image", content: SHARE_IMAGE } },
        { tag: "meta", attrs: { property: "og:image:width", content: "1280" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "640" } },
        { tag: "meta", attrs: { property: "og:image:alt", content: SHARE_IMAGE_ALT } },
        { tag: "meta", attrs: { name: "twitter:image", content: SHARE_IMAGE } },
        { tag: "meta", attrs: { name: "twitter:image:alt", content: SHARE_IMAGE_ALT } },
      ],
      defaultLocale: "root",
      locales: {
        root: { label: "English", lang: "en" },
        tr: { label: "Türkçe", lang: "tr" },
      },
      social: [{ icon: "github", label: "GitHub", href: REPO }],
      editLink: { baseUrl: `${REPO}/edit/develop/site/` },
      customCss: [
        "@fontsource-variable/bricolage-grotesque",
        "@fontsource-variable/instrument-sans",
        "@fontsource-variable/jetbrains-mono",
        "./src/styles/theme.css",
      ],
      lastUpdated: false,
      credits: false,
      sidebar: [
        {
          label: "Getting started",
          translations: { tr: "Başlarken" },
          items: ["guide/install", "guide/first-launch", "guide/connecting"],
        },
        {
          label: "Using Keymotion",
          translations: { tr: "Keymotion'ı kullanma" },
          items: [
            "guide/overview",
            "guide/lighting",
            "guide/key-mapping",
            "guide/macros",
            "guide/screen",
            "guide/profiles",
            "guide/focus-timer",
            "guide/system-gauges",
            "guide/reminders",
            "guide/calendar",
            "guide/connection",
            "guide/settings",
          ],
        },
        {
          label: "Help",
          translations: { tr: "Yardım" },
          items: ["help/keyboards", "help/recovery", "help/faq"],
        },
        {
          label: "About",
          translations: { tr: "Hakkında" },
          items: ["about/privacy", "about/code-signing", "about/release-notes"],
        },
        {
          label: "Developers",
          translations: { tr: "Geliştiriciler" },
          collapsed: true,
          items: [
            "developers/contributing",
            "developers/architecture",
            "developers/protocol",
            "developers/adding-a-keyboard",
          ],
        },
      ],
      plugins: [
        starlightLinksValidator({ errorOnLocalLinks: false }),
        // /llms.txt, /llms-full.txt and /llms-small.txt: the English guide as
        // plain text for AI assistants (https://llmstxt.org/).
        starlightLlmsTxt({
          projectName: "Keymotion",
          description:
            "Keymotion is a free, open-source Windows app (Windows 10 and 11, 64-bit) for the AJAZZ AK980 PRO mechanical keyboard. It sets per-key RGB lighting and the 19 built-in effects, remaps keys on the base and Fn layers, records macros, and puts pictures, GIFs, videos and live cards on the keyboard's 240×135 screen. It is an independent project, not made by AJAZZ.",
          details: [
            "- Settings are changed over the USB cable, with the switch on the back of the keyboard in the middle. Over Bluetooth Keymotion only reads the battery level; over the 2.4 GHz receiver it recognizes the keyboard but can't change settings yet.",
            "- Keymotion sends only commands that were measured on this keyboard and never updates its firmware.",
            "- The interface is in English and Turkish. The Turkish guide is at " + SITE + "/tr/.",
            "- Source code, releases and issues: " + REPO + ".",
          ].join("\n"),
          demote: ["developers/**"],
        }),
      ],
    }),
  ],
});
