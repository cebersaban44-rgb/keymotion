// Every picture of the app the site and the README use, and how to get each
// one from the real app (tools/screenshots/capture.mjs). The site shows the
// English dark-theme shots in every language (owner's decisions, 2026-11-12
// and 2026-11-23: dark only), so each entry is taken once, in English, dark.
//
// Fields:
//   name     the file name (site: src/assets/screens/en/dark/<name>.webp,
//            the name the guide's <Screenshot name="…"> uses)
//   pages    the site pages that show it (for the list; new shots have none yet)
//   state    what the picture shows, in words
//   source   "app"   taken from the real app by capture.mjs
//            "prep"  the same, after the operator sets the keyboard up (`prep`)
//            "mock"  only the browser stand-in can show it safely (an update
//                    waiting, an interrupted upload, a game being played…):
//                    take it with shoot.mjs --only <name>
//   prep     what the operator does first (capture.mjs --interactive waits)
//   mock     the stand-in keyboard for --target mock ("usb", "bluetooth", "none")
//   query    more stand-in switches for --target mock
//   page, screenTab, viewport, clip, settle, steps(page, km): as in shots.mjs
//
// Steps here never write to the keyboard's memory, never connect a coding
// app outside a test folder and never open a browser window: in the real app
// capture.mjs also refuses those calls (its guard), so a step that tries one
// fails instead of writing.

import {
  addReminder,
  button,
  calibrationCard,
  click,
  dialogClip,
  find,
  gamesOn,
  gaugesOn,
  go,
  keyboardCard,
  L,
  libraryCard,
  lightsRow,
  musicOn,
  openScreen,
  padded,
  recordGreeting,
  scrollTo,
  scrollToBottom,
  studioSteps,
  t,
  volumeUpOnSlash,
  waitText,
} from "./shots.mjs";

// ---------------------------------------------------------------- helpers

/** Make the window as tall as the page, so all of it shows (up to `max`). */
async function fitHeight(page, max = 1600) {
  const height = await page.evaluate(() => document.querySelector("main").scrollHeight);
  await page.setViewportSize({ width: page.viewportSize().width, height: Math.min(max, height) });
  await page.waitForTimeout(500);
}

/** Calendar › Add calendar › Other calendar › `km.ics` › Add. The real app
 *  reads it from the internet: a public calendar, never the owner's. */
async function addCalendar(page, km) {
  await go(page, "calendar");
  await click(page, "calendar.add");
  await click(page, "calendar.p.other", { start: true, wait: 500 });
  await page.getByLabel(await t(page, "calendar.address"), { exact: true }).fill(km.ics);
  await click(page, "calendar.add.do", { wait: 2500 });
}

/** Profiles: "Work" and "Gaming", made from what the keyboard has now (no
 *  effect is saved to the keyboard first, unlike shots.mjs). */
async function twoProfiles(page, { lang }) {
  for (const name of [L(lang, "Work", "İş"), L(lang, "Gaming", "Oyun")]) {
    await go(page, "profiles");
    await click(page, "prof.create", { wait: 500 });
    await page.getByLabel(await t(page, "prof.create.name"), { exact: true }).fill(name);
    await click(page, "prof.create.button", { wait: 1500 });
  }
}

/** The Overview of a keyboard in use: a reminder, a calendar and the gauges. */
async function busyOverview(page, km) {
  await addReminder(page, "reminder.tpl.stand", L(km.lang, "Stretch your legs", "Bacaklarını esnet"));
  await addCalendar(page, km);
  await gaugesOn(page);
  await go(page, "overview", 1500);
}

/** Screen › Images › Text card (the command bar), `template`, Preview, Save
 *  to library: a card in the library only, nothing uploaded. */
async function savedCard(page, template) {
  await openScreen(page, "visuals");
  await page.locator('[data-tile="text"]').click();
  await page.waitForTimeout(900);
  await click(page, template, { role: "radio", wait: 600 });
  await click(page, "sc.step.preview", { wait: 1200 });
  await click(page, "sc.saveOnly", { wait: 1500 });
}

/** A section of the AI agents page by its title. */
const agentsSection = async (page, key) =>
  page.locator("main section").filter({ has: page.locator("h2 > span").getByText(await t(page, key), { exact: true }) }).first();

/** AI agents: turn access on, which opens the connect step (access off and
 *  nothing connected yet), with Details open. */
async function connectStep(page) {
  await go(page, "agents", 1200);
  const access = await find(page, "switch", "ag.access");
  if ((await access.getAttribute("aria-checked")) !== "true") await access.click();
  await page.locator("[data-agents-flow]").waitFor({ timeout: 10_000 });
  await page.waitForTimeout(800);
}

/** A sample card as an agent's request would come, drawn by the page's own
 *  card code (no keyboard involved), shown three times its size. */
function agentCardShot(name, card) {
  return {
    name,
    pages: ["guide/ai-agents"],
    state: `An agent's ${card.kind === "longTask" ? "long task card (F)" : "card"} as the keyboard's screen shows it, three times its size.`,
    source: "app",
    page: "overview",
    viewport: { width: 760, height: 445 },
    clip: { x: 20, y: 20, width: 720, height: 405 },
    steps: async (page) => {
      const url = await page.evaluate(async (c) => {
        const entry = performance.getEntriesByType("resource").find((r) => r.name.includes("/src/lib/liveCards.ts"));
        const lc = await import(/* @vite-ignore */ entry ? entry.name : "/src/lib/liveCards.ts");
        const si = await import(/* @vite-ignore */ "/src/lib/screenImport.ts");
        const i18n = await import(/* @vite-ignore */ "/src/lib/i18n.ts");
        if (!lc.agentCard) throw new Error("this build has no agent cards (PR #162)");
        const p = await lc.agentCard(c, lc.agentCardWords(i18n.tNow, "en"));
        const cv = document.createElement("canvas");
        cv.width = 240;
        cv.height = 135;
        cv.getContext("2d").putImageData(new ImageData(si.toRgba(p.frames[0]), 240, 135), 0, 0);
        return cv.toDataURL("image/png");
      }, card);
      await page.evaluate((src) => {
        const img = document.createElement("img");
        img.src = src;
        img.id = "ks-card-shot";
        img.style.cssText =
          "position:fixed;left:20px;top:20px;width:720px;height:405px;image-rendering:pixelated;z-index:2147483647";
        document.body.append(img);
      }, url);
      await page.locator("#ks-card-shot").waitFor();
      await page.waitForTimeout(300);
    },
  };
}

const CARD = { id: "shot", kind: "text", agent: "claude-code", title: null, text: null, image: null, color: null, minutes: null, label: null, showS: 300 };

// ------------------------------------------------------------------ list

export const CAPTURES = [
  // Getting started and basics
  {
    name: "welcome",
    pages: ["guide/first-launch"],
    state: "No keyboard: the welcome page with the steps to connect one.",
    source: "prep",
    prep: "Unplug the keyboard's USB cable (and turn it off, or take out the 2.4 GHz receiver).",
    mock: "none",
    page: "overview",
  },
  {
    name: "overview",
    pages: ["guide/overview", "index"],
    state: "Overview of a keyboard in use: the glass strip, the keyboard, a reminder, a public calendar and the gauges on; the window as tall as the page.",
    source: "prep",
    prep: "Keyboard on the USB cable, its lighting as the picture should show it (nothing is saved to it).",
    page: "overview",
    steps: async (page, km) => {
      await busyOverview(page, km);
      await fitHeight(page);
    },
  },
  {
    name: "overview-bluetooth",
    pages: ["guide/overview"],
    state: "Overview over Bluetooth: the battery at the top and the card about the features that need the cable.",
    source: "prep",
    prep: "Unplug the cable and switch the keyboard to Bluetooth (BT), connected to this computer.",
    mock: "bluetooth",
    page: "overview",
    steps: (page) => fitHeight(page),
  },
  {
    name: "connection",
    pages: ["guide/connection"],
    state: "Connection page with the keyboard on the USB cable.",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "connection",
  },
  {
    name: "connection-bluetooth",
    pages: ["guide/connecting"],
    state: "Connection page over Bluetooth.",
    source: "prep",
    prep: "Keyboard on Bluetooth, the cable unplugged.",
    mock: "bluetooth",
    page: "connection",
  },
  { name: "settings", pages: ["guide/settings"], state: "Settings, from the top.", source: "app", page: "settings" },
  {
    name: "settings-about",
    pages: ["guide/settings"],
    state: "Settings › About after Check for updates (\"Checked just now\").",
    source: "app",
    page: "settings",
    steps: async (page) => {
      await click(page, "update.check", { wait: 1500 });
      await waitText(page, "update.justNow", { timeout: 20_000 });
      await scrollTo(page, await find(page, "button", "update.check"), 120);
    },
  },
  { name: "update-strip", pages: ["guide/install"], state: "The strip that says a new version waits.", source: "mock", query: { update: "available" }, page: "overview" },
  { name: "update-dialog", pages: ["guide/settings"], state: "The update dialog before installing.", source: "mock", query: { update: "available" }, page: "settings" },
  { name: "update-whats-new", pages: ["about/release-notes"], state: "What's new in the waiting version.", source: "mock", query: { update: "available" }, page: "settings" },
  {
    name: "report-dialog",
    pages: ["guide/settings"],
    state: "Report a problem: the dialog with the details that would be sent (nothing is sent).",
    source: "app",
    page: "settings",
    clip: dialogClip(),
    steps: async (page) => {
      await scrollToBottom(page);
      await click(page, "report.settings.problem", { wait: 900 });
      await page.locator("[data-report-text]").waitFor();
      await page.waitForTimeout(600);
    },
  },
  {
    name: "profiles",
    pages: ["guide/profiles"],
    state: "Profiles with \"Work\" and \"Gaming\".",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "profiles",
    steps: async (page, km) => {
      await twoProfiles(page, km);
      await go(page, "profiles");
    },
  },
  {
    name: "profile-load",
    pages: ["guide/profiles"],
    state: "Loading \"Work\": the question before anything is written. Needs a profile that differs from the keyboard, which takes a save to the keyboard.",
    source: "mock",
    page: "profiles",
  },


  // Lighting, keys, macros
  { name: "lighting-effects", pages: ["guide/lighting"], state: "Lighting › Effects: the gallery and the effect's settings.", source: "prep", prep: "Keyboard on the USB cable.", page: "lighting" },
  {
    name: "lighting-per-key",
    pages: ["guide/lighting"],
    state: "Lighting › Per-key color with the Rainbow preset (a preview, not saved).",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "lighting",
    steps: async (page) => {
      await click(page, "light.tab.perKey", { role: "tab", wait: 700 });
      await click(page, "light.preset.rainbow", { wait: 900 });
    },
  },
  {
    name: "color-calibration",
    pages: ["guide/lighting"],
    state: "Settings › Color calibration with the ramp pattern (the card alone).",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "settings",
    viewport: { height: 1200 },
    clip: async (page) => padded(await (await calibrationCard(page)).boundingBox(), 20),
    steps: async (page) => {
      await scrollTo(page, await calibrationCard(page), 24);
      await click(page, "cal.pattern.ramp", { role: "radio", wait: 900 });
    },
  },
  {
    name: "key-mapping",
    pages: ["guide/key-mapping"],
    state: "Key mapping: the number pad's / chosen as Volume up (not saved).",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "keymap",
    steps: async (page) => {
      await volumeUpOnSlash(page);
      await scrollTo(page, keyboardCard(page), 14);
    },
  },
  {
    name: "macros",
    pages: ["guide/macros"],
    state: "Macros: \"Greeting\" recorded (Shift+H, I, a left click), not saved.",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "macros",
    steps: (page, km) => recordGreeting(page, km),
  },
  {
    name: "key-mapping-macro",
    pages: ["guide/macros"],
    state: "Key mapping: / plays macro M1 three times. Needs a macro saved on the keyboard.",
    source: "mock",
    page: "macros",
  },

  // Screen and tools
  { name: "screen-studio", pages: ["guide/screen"], state: "Screen › Images: both screen pages and the three sources.", source: "prep", prep: "Keyboard on the USB cable.", page: "screen", screenTab: "visuals", viewport: { height: 985 } },
  {
    name: "screen-text-card",
    pages: ["guide/screen"],
    state: "Screen › Text card with the Name template.",
    source: "app",
    page: "screen",
    screenTab: "visuals",
    steps: async (page) => {
      await page.locator('[data-tile="text"]').click();
      await page.waitForTimeout(900);
      await click(page, "sc.tx.t.name", { role: "radio", wait: 700 });
      await scrollTo(page, await studioSteps(page), 44);
    },
  },
  {
    name: "screen-library",
    pages: ["guide/screen"],
    state: "The picture library with two saved cards (saved to the library only; nothing uploaded).",
    source: "app",
    page: "screen",
    screenTab: "visuals",
    clip: async (page) => padded(await libraryCard(page).boundingBox(), 20),
    steps: async (page) => {
      await savedCard(page, "sc.tx.t.name");
      await savedCard(page, "sc.tx.t.quote");
      await scrollTo(page, libraryCard(page), 40);
    },
  },
  {
    name: "screen-match-lights",
    pages: ["guide/screen"],
    state: "Match lights for a mountain card (a preview on the keys, nothing saved).",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "screen",
    screenTab: "visuals",
    clip: dialogClip(28, "[role=dialog] .dlg"),
    steps: async (page) => {
      await savedCard(page, "sc.tx.t.mountain");
      // Choose the new card, then Match lights in the command bar.
      // A card just saved is chosen already; a click would let it go.
      const item = page.locator("[data-library-item]", { hasText: await t(page, "sc.tx.sample.mountain.t") }).first();
      const pick = item.locator("button").first();
      if ((await pick.getAttribute("aria-pressed")) !== "true") await pick.click();
      await page.waitForTimeout(400);
      await click(page, "sc.lm.action", { within: page.locator("[data-library-bar]"), wait: 900 });
      await click(page, "sc.lm.gradient", { role: "radio", start: true, wait: 900 });
    },
  },
  { name: "screen-live", pages: ["guide/screen"], state: "Screen › Live screen on, with its budget and a card shown. Turning it on uploads cards.", source: "mock", page: "screen", screenTab: "live" },
  { name: "screen-incomplete-upload", pages: ["guide/screen", "help/recovery"], state: "The notice after an upload was cut off.", source: "mock", page: "screen", screenTab: "visuals" },
  { name: "screen-no-backup", pages: ["help/recovery"], state: "No backup of the factory animation, and none in the official driver.", source: "mock", query: { driver: "" }, page: "screen", screenTab: "visuals" },
  {
    name: "focus-timer",
    pages: ["guide/focus-timer"],
    state: "The focus timer running (just started) and its keys.",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    query: { pomoskip: "7" },
    page: "focus",
    steps: async (page) => {
      await click(page, "pomo.start", { wait: 2500 });
    },
  },
  { name: "focus-timer-cards", pages: ["guide/focus-timer"], state: "The focus timer's Keyboard screen row with the live screen on.", source: "mock", page: "focus" },
  {
    name: "system-gauges",
    pages: ["guide/system-gauges"],
    state: "System gauges on (this computer's own readings).",
    source: "prep",
    prep: "Keyboard on the USB cable.",
    page: "gauges",
    steps: async (page) => {
      await gaugesOn(page);
      await page.waitForTimeout(2500);
    },
  },
  {
    name: "reminders",
    pages: ["guide/reminders"],
    state: "Reminders with \"Drink water\" and \"Take a pill\".",
    source: "app",
    page: "reminders",
    steps: async (page) => {
      await addReminder(page, "reminder.tpl.water");
      await addReminder(page, "reminder.tpl.pill");
      await go(page, "reminders");
    },
  },
  {
    name: "reminders-alert-style",
    pages: ["guide/reminders"],
    state: "Adding a reminder, step 2: how it alerts.",
    source: "app",
    page: "reminders",
    steps: async (page) => {
      await click(page, "reminder.add");
      await click(page, "reminder.tpl.pill", { start: true, wait: 500 });
      await click(page, "reminder.nextStep", { wait: 900 });
      await scrollTo(page, page.locator("main").getByText(await t(page, "reminder.list"), { exact: true }).first(), 20);
    },
  },
  {
    name: "calendar",
    pages: ["guide/calendar"],
    state: "Calendar with a public calendar added (its events, not the owner's).",
    source: "app",
    page: "calendar",
    steps: async (page, km) => {
      await addCalendar(page, km);
      await go(page, "calendar", 1500);
      await fitHeight(page);
    },
  },
  {
    name: "calendar-add-google",
    pages: ["guide/calendar"],
    state: "Add calendar › Google: the steps to copy the secret address.",
    source: "app",
    page: "calendar",
    steps: async (page) => {
      await click(page, "calendar.add");
      await click(page, "calendar.p.google", { start: true, wait: 700 });
      await scrollTo(page, page.locator("main").getByText(await t(page, "calendar.sources"), { exact: true }).first(), 20);
    },
  },

  // Games, profiles by app, music
  { name: "game-session", pages: ["guide/game-session"], state: "A game being played: the session, the profile each game gets, the start card.", source: "mock", page: "games" },
  {
    name: "game-session-lights",
    pages: ["guide/game-session"],
    state: "Game session › Lights while playing: the MOBA keys.",
    source: "app",
    page: "games",
    viewport: { height: 1000 },
    clip: async (page) => padded(await lightsRow(page).boundingBox(), 0),
    steps: async (page) => {
      await gamesOn(page);
      await click(page, "games.lights.keys", { role: "radio", wait: 500 });
      await click(page, "games.keys.moba", { wait: 600 });
      await scrollTo(page, lightsRow(page), 20);
    },
  },
  { name: "game-session-lol", pages: ["guide/game-session"], state: "League of Legends lights: health and mana. Needs League of Legends in the games list.", source: "mock", page: "games" },
  { name: "game-session-lol-events", pages: ["guide/game-session"], state: "League of Legends lights: the match's events.", source: "mock", page: "games" },
  { name: "profiles-by-app", pages: ["guide/profiles"], state: "Profiles › By app: every game \"Gaming\", the code editor \"Work\". Needs per-key profiles saved on the keyboard.", source: "mock", page: "profiles" },
  {
    name: "music",
    pages: ["guide/music-mode"],
    state: "Music mode on with Bars, while music plays.",
    source: "prep",
    prep: "Keyboard on the USB cable, and music playing on this computer (speakers or headphones).",
    page: "music",
    steps: async (page) => {
      await musicOn(page);
      await page.waitForTimeout(3000);
    },
  },
  {
    name: "music-modes",
    pages: ["guide/music-mode"],
    state: "Music mode's mode and appearance settings.",
    source: "app",
    page: "music",
    viewport: { height: 1000 },
    clip: async (page) => {
      const rows = page.locator("[data-music-settings] > div");
      const first = await rows.nth(0).boundingBox();
      const second = await rows.nth(1).boundingBox();
      return { x: first.x, y: first.y, width: first.width, height: second.y + second.height - first.y };
    },
    steps: async (page) => {
      await musicOn(page);
      await scrollTo(page, page.locator("[data-music-settings]"), 20);
    },
  },

  // New in this version
  {
    name: "ai-agents-connect",
    pages: ["guide/ai-agents"],
    // Details stays closed: in the real app its paths hold the Windows user
    // name and, in a development build, the build folder.
    state: "AI agents: turning access on, step 1 (Things to connect), Details closed. Access off and nothing connected before.",
    source: "prep",
    prep: "Start the app with KEYMOTION_CLI_TEST_HOME set to an empty test folder holding .claude and .codex folders (see capture.mjs); agent access off.",
    page: "agents",
    steps: connectStep,
  },
  {
    name: "ai-agents",
    pages: ["guide/ai-agents"],
    state: "AI agents: access on, Claude Code and Codex connected (in the test folder), the signals.",
    source: "prep",
    prep: "As for ai-agents-connect; capture.mjs approves the connect step only with --agents-home matching the paths it shows.",
    query: { agents: "on" },
    page: "agents",
    steps: async (page, km) => {
      await go(page, "agents", 1200);
      const access = await find(page, "switch", "ag.access");
      if ((await access.getAttribute("aria-checked")) !== "true") {
        await connectStep(page);
        if (!(await km.approveConnect(page))) throw new Error("the connect step was not approved (see --agents-home)");
      }
      await go(page, "agents", 1200);
    },
  },
  {
    name: "ai-agents-recent",
    pages: ["guide/ai-agents"],
    state: "AI agents › Recent actions: a signal, a settings change with Undo and a refused request.",
    source: "prep",
    prep: "Agent access on. capture.mjs runs keymotion-cli (--cli) for the entries: a done signal, a color change, a refused request.",
    query: { agents: "on" },
    page: "agents",
    steps: async (page, km) => {
      await km.agentActions();
      await go(page, "agents", 1500);
      const section = await agentsSection(page, "ag.log");
      await scrollTo(page, section, 20);
    },
  },
  agentCardShot("agent-card-text", { ...CARD, title: "Tests", text: "All 214 passed\nReady to merge", color: [63, 207, 142] }),
  agentCardShot("agent-card-long-task", { ...CARD, kind: "longTask", minutes: 14, label: "keymotion", color: [63, 207, 142] }),
  {
    name: "welcome-new-firmware",
    pages: ["guide/first-launch"],
    state: "An AK980 PRO on an AJAZZ firmware Keymotion doesn't know yet: the welcome page's card about it.",
    source: "mock",
    mock: "newfw",
    page: "overview",
  },
];

/** The README's pictures, still made by shoot.mjs and hero.mjs from the
 *  browser stand-in (docs/images). Listed so nothing is forgotten. */
export const README_IMAGES = [
  { file: "docs/images/hero-dark.webp", state: "The keyboard beside the Overview (hero.mjs from shoot.mjs's HERO capture).", source: "mock" },
  ...["lighting", "key-mapping", "macros", "screen", "live-screen", "focus", "music", "game-session", "profiles"].map((n) => ({
    file: `docs/images/features/${n}.webp`,
    state: `The README's animated "${n}" row (shoot.mjs --clips).`,
    source: "mock",
  })),
];
