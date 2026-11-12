// The Keymotion documentation site (roadmap stage 7). Built with Astro and
// Starlight and published to GitHub Pages by .github/workflows/pages.yml.
// English is the default language at the site root; Turkish lives under /tr/.
import { fileURLToPath } from "node:url";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import starlightLinksValidator from "starlight-links-validator";

const REPO = "https://github.com/cebersaban44-rgb/keymotion";

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
      plugins: [starlightLinksValidator({ errorOnLocalLinks: false })],
    }),
  ],
});
