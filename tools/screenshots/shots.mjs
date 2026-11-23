// What tools/screenshots/shoot.mjs captures. Each item opens the app's
// development build with the stand-in keyboard (`mock`, default "usb", plus
// `query` switches), goes to `page`, waits `settle` ms and runs
// `steps(page, { lang, theme })` before the picture. `name` is the file name
// the guide's <Screenshot name="…"> uses. `clip` is the area to capture: a
// rectangle, or a function of the page that returns one.
//
// Steps find buttons by their text in the page's language: `t(page, key)`
// asks the app for its own string (app/src/lib/i18n.ts), so the same steps
// work in English and Turkish. Sample text that the steps type in is chosen
// per language with `L(lang, en, tr)`.

export const L = (lang, en, tr) => (lang === "tr" ? tr : en);

// ---------------------------------------------------------------- helpers

/** The app's text for `key` in the current language. */
export const t = (page, key, vars) => page.evaluate(([k, v]) => window.__keymotionT(k, v ?? undefined), [key, vars ?? null]);

export const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** An element by its role and the app's text for `key`. `start` matches
 *  names that begin with the text (a card with a second line, say). */
export async function find(page, role, key, { vars, start = false, within } = {}) {
  const text = await t(page, key, vars);
  const name = start ? new RegExp(`^\\s*${escape(text)}`) : text;
  return (within ?? page).getByRole(role, { name, exact: !start }).first();
}
export const button = (page, key, opts) => find(page, "button", key, opts);
export async function click(page, key, opts = {}) {
  const el = await find(page, opts.role ?? "button", key, opts);
  await el.click();
  await page.waitForTimeout(opts.wait ?? 350);
}

/** Open a page of the app as the sidebar does. */
export async function go(page, id, wait = 800) {
  await page.evaluate((p) => {
    window.__keymotionStore.getState().setPage(p);
    document.querySelector("main").scrollTo({ top: 0, behavior: "instant" });
  }, id);
  await page.waitForTimeout(wait);
}
export async function openScreen(page, tab, wait = 900) {
  await page.evaluate((p) => {
    window.__keymotionStore.getState().openScreen(p);
    document.querySelector("main").scrollTo({ top: 0, behavior: "instant" });
  }, tab);
  await page.waitForTimeout(wait);
}

/** Scroll the page so `locator` sits `offset` px below the top of the window. */
export async function scrollTo(page, locator, offset = 28) {
  await locator.evaluate((el, off) => {
    const main = document.querySelector("main");
    const top = el.getBoundingClientRect().top - main.getBoundingClientRect().top + main.scrollTop - off;
    main.scrollTo({ top, behavior: "instant" });
  }, offset);
  await page.waitForTimeout(350);
}
export async function scrollToBottom(page) {
  await page.evaluate(() => {
    const main = document.querySelector("main");
    main.scrollTo({ top: main.scrollHeight, behavior: "instant" });
  });
  await page.waitForTimeout(350);
}

/** The open dialog with a margin of the dimmed page around it. */
export const dialogClip =
  (pad = 40, selector = "[role=dialog]") =>
  async (page) => {
    const box = await page.locator(selector).first().boundingBox();
    const vp = page.viewportSize();
    const x = Math.max(0, Math.floor(box.x - pad));
    const y = Math.max(0, Math.floor(box.y - pad));
    return {
      x,
      y,
      width: Math.min(vp.width, Math.ceil(box.x + box.width + pad)) - x,
      height: Math.min(vp.height, Math.ceil(box.y + box.height + pad)) - y,
    };
  };

/** Wait until an element with this text shows. */
export async function waitText(page, key, opts = {}) {
  const text = await t(page, key, opts.vars);
  await page.getByText(text, { exact: opts.exact ?? false }).first().waitFor({ timeout: opts.timeout ?? 10_000 });
}

// ------------------------------------------------------------ app actions

/** Lighting › Effects: pick `effect` (fx.N key) and save it to the keyboard. */
async function saveEffect(page, effect = "fx.8") {
  await go(page, "lighting");
  await click(page, effect, { start: true });
  await click(page, "bar.save", { wait: 300 });
  await waitText(page, "bar.saved");
  await page.waitForTimeout(400);
}

/** Reminders › Add reminder › `template`, an optional new title, Next, Save. */
export async function addReminder(page, template, title) {
  await go(page, "reminders");
  await click(page, "reminder.add");
  await click(page, template, { start: true, wait: 500 });
  if (title) await page.getByLabel(await t(page, "reminder.title"), { exact: true }).fill(title);
  await click(page, "reminder.nextStep", { wait: 500 });
  await click(page, "reminder.save", { wait: 700 });
}

const SAMPLE_CALENDAR = "https://calendar.example.com/team/basic.ics";

/** Calendar › Add calendar › Other calendar › the sample address › Add. */
async function addCalendar(page) {
  await go(page, "calendar");
  await click(page, "calendar.add");
  await click(page, "calendar.p.other", { start: true, wait: 500 });
  await page.getByLabel(await t(page, "calendar.address"), { exact: true }).fill(SAMPLE_CALENDAR);
  await click(page, "calendar.add.do", { wait: 1500 });
}

/** System gauges › Show on keys. */
export async function gaugesOn(page) {
  await go(page, "gauges");
  await click(page, "gauges.on", { role: "switch", wait: 600 });
}

/** Music mode › Show on keys. */
export async function musicOn(page) {
  await go(page, "music");
  await click(page, "music.on", { role: "switch", wait: 600 });
}

/** The made-up song's frame the music shots show (50 ms each from its
 *  start): 1.55 s, just after a snare. */
const MUSIC_FRAME = 31;

/** The Overview of a keyboard in use: a reminder, a calendar, the gauges
 *  and a saved effect. */
async function busyOverview(page, { lang }) {
  await addReminder(page, "reminder.tpl.stand", L(lang, "Stretch your legs", "Bacaklarını esnet"));
  await addCalendar(page);
  await gaugesOn(page);
  await saveEffect(page, "fx.8");
  await go(page, "overview", 1500);
}

// ------------------------------------------------------------------ shots

/** The pictures the guide pages use. */
export const SHOTS = [
  // Getting started and basics
  { name: "welcome", mock: "none", page: "overview" },
  // An AK980 PRO on AJAZZ's firmware 769.
  { name: "welcome-new-firmware", mock: "fw769", page: "overview" },
  // Every tile: the window as tall as the page.
  {
    name: "overview",
    page: "overview",
    steps: async (page, km) => {
      await busyOverview(page, km);
      await fitHeight(page);
    },
  },
  // The battery at the top and the "Unlock every feature" card at the bottom.
  { name: "overview-bluetooth", mock: "bluetooth", page: "overview", steps: (page) => fitHeight(page) },
  { name: "connection", page: "connection" },
  { name: "connection-bluetooth", mock: "bluetooth", page: "connection" },
  { name: "settings", page: "settings" },
  {
    name: "settings-about",
    page: "settings",
    steps: async (page) => {
      await click(page, "update.check", { wait: 1200 });
      await waitText(page, "update.justNow");
      // From the Version row down: Updates has three choices now, so the
      // section no longer fits to its end.
      await scrollTo(page, await find(page, "button", "update.check"), 120);
    },
  },
  {
    name: "update-strip",
    query: { update: "available" },
    page: "overview",
    steps: async (page) => {
      await page.locator("[data-update-strip=available]").waitFor();
      await page.waitForTimeout(600);
    },
  },
  {
    name: "update-dialog",
    query: { update: "available" },
    page: "settings",
    clip: dialogClip(),
    steps: async (page) => {
      await page.locator("[data-update-strip=available]").waitFor();
      await page.waitForTimeout(500);
      await click(page, "update.installNow", { wait: 700 });
    },
  },
  {
    name: "update-whats-new",
    query: { update: "available" },
    page: "settings",
    clip: dialogClip(),
    settle: 0,
    steps: async (page) => {
      await scrollToBottom(page);
      // The daily look finds the new version 1.5 s after the start; before
      // that the row offers Check for updates.
      const check = await button(page, "update.check");
      if (await check.isVisible()) await check.click();
      const whatsNew = await button(page, "update.whatsNew");
      await whatsNew.waitFor();
      await whatsNew.click();
      await page.waitForTimeout(700);
    },
  },
  {
    name: "report-dialog",
    page: "settings",
    clip: dialogClip(),
    steps: async (page) => {
      await scrollToBottom(page);
      await click(page, "report.settings.problem", { wait: 900 });
      await page.locator("[data-report-text]").waitFor();
      await page.waitForTimeout(400);
    },
  },
  { name: "profiles", page: "profiles", steps: twoProfiles },
  {
    name: "profile-load",
    page: "profiles",
    steps: async (page, km) => {
      await twoProfiles(page, km);
      const row = page.locator("li", { hasText: L(km.lang, "Work", "İş") }).filter({ has: await button(page, "prof.load") });
      await (await button(page, "prof.load", { within: row.last() })).click();
      await page.locator("[data-profile-confirm]").waitFor();
      await page.waitForTimeout(500);
    },
  },

  // Lighting, keys, macros
  { name: "lighting-effects", page: "lighting" },
  {
    name: "lighting-per-key",
    page: "lighting",
    steps: async (page) => {
      await click(page, "light.tab.perKey", { role: "tab", wait: 700 });
      await click(page, "light.preset.rainbow", { wait: 700 });
    },
  },
  {
    name: "color-calibration",
    page: "settings",
    // The whole card with its keyboard picture: a taller window, the card alone.
    viewport: { height: 1200 },
    clip: async (page) => padded(await (await calibrationCard(page)).boundingBox(), 20),
    steps: async (page) => {
      await scrollTo(page, await calibrationCard(page), 24);
      await click(page, "cal.pattern.ramp", { role: "radio", wait: 700 });
    },
  },
  {
    name: "key-mapping",
    page: "keymap",
    steps: async (page) => {
      await volumeUpOnSlash(page);
      await scrollTo(page, keyboardCard(page), 14);
    },
  },
  {
    name: "macros",
    page: "macros",
    steps: async (page, km) => {
      await recordGreeting(page, km);
    },
  },
  {
    name: "key-mapping-macro",
    page: "macros",
    steps: async (page, km) => {
      await recordGreeting(page, km);
      await click(page, "bar.save", { wait: 300 });
      await waitText(page, "bar.saved");
      await go(page, "keymap");
      await page.locator('[data-key="kp-slash"]').first().click();
      await click(page, "km.cat.macro", { role: "radio", wait: 400 });
      await click(page, "km.play.times", { role: "radio", wait: 300 });
      await page.locator('main input[type="number"]').first().fill("3");
      await page.waitForTimeout(200);
      await page.getByRole("button", { name: /^\s*M1\b/ }).first().click();
      await page.waitForTimeout(500);
      await scrollTo(page, keyboardCard(page), 14);
    },
  },

  // Screen and tools
  // The title, both screen pages and the three sources: a slightly taller window.
  { name: "screen-studio", page: "screen", screenTab: "visuals", viewport: { height: 985 } },
  {
    name: "screen-text-card",
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
    page: "screen",
    screenTab: "visuals",
    // The library card alone.
    clip: async (page) => padded(await libraryCard(page).boundingBox(), 20),
    steps: async (page) => {
      await makeCard(page, "sc.tx.t.name", "save");
      await makeCard(page, "sc.tx.t.quote", "upload");
      await scrollTo(page, libraryCard(page), 40);
    },
  },
  {
    name: "screen-match-lights",
    page: "screen",
    screenTab: "visuals",
    clip: dialogClip(28, "[role=dialog] .dlg"),
    steps: async (page) => {
      await makeCard(page, "sc.tx.t.mountain", "save", "sc.tx.pal.sunset");
      // A card just saved is chosen already; Match lights is in the command bar.
      const item = page.locator("[data-library-item]", { hasText: await t(page, "sc.tx.sample.mountain.t") }).first();
      const pick = item.locator("button").first();
      if ((await pick.getAttribute("aria-pressed")) !== "true") await pick.click();
      await click(page, "sc.lm.action", { within: page.locator("[data-library-bar]"), wait: 900 });
      await click(page, "sc.lm.gradient", { role: "radio", start: true, wait: 900 });
    },
  },
  {
    name: "screen-live",
    page: "screen",
    screenTab: "live",
    steps: async (page) => {
      await liveOn(page);
      await page.waitForTimeout(5000);
      await fitHeight(page);
    },
  },
  {
    name: "screen-incomplete-upload",
    page: "screen",
    screenTab: "visuals",
    steps: async (page) => {
      await makeCard(page, "sc.tx.t.badge", "upload");
      await page.evaluate(() => window.__ksMockIncomplete());
      await page.waitForTimeout(1200);
      // The studio's "uploaded" note would contradict the notice: close it.
      const close = await button(page, "sc.close");
      if (await close.isVisible()) await close.click();
      await page.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
      await page.waitForTimeout(600);
    },
  },
  { name: "screen-no-backup", query: { driver: "" }, page: "screen", screenTab: "visuals", settle: 1500 },
  {
    name: "focus-timer",
    // The first block starts 7 minutes in: the ring and the keys are part
    // empty and the ring shows a real time.
    query: { pomoskip: "7" },
    page: "focus",
    steps: async (page) => {
      await click(page, "pomo.start", { wait: 1500 });
    },
  },
  {
    name: "focus-timer-cards",
    page: "focus",
    // The Keyboard screen row alone, from its title to the next row.
    clip: async (page) => {
      const title = page.locator("main").getByText(await t(page, "pomo.screen"), { exact: true }).first();
      const top = (await title.boundingBox()).y - 34;
      const bottom = (await page.locator("main").getByText(await t(page, "pomo.sound"), { exact: true }).first().boundingBox()).y - 34;
      const card = await page.locator("main section.card", { has: page.getByText(await t(page, "pomo.screen"), { exact: true }) }).last().boundingBox();
      return { x: card.x - 16, y: top, width: card.width + 32, height: bottom - top };
    },
    steps: async (page) => {
      await liveOn(page);
      await go(page, "focus");
      await scrollTo(page, page.locator("main").getByText(await t(page, "pomo.screen"), { exact: true }).first(), 60);
    },
  },
  {
    name: "system-gauges",
    page: "gauges",
    steps: async (page) => {
      await gaugesOn(page);
      await page.waitForTimeout(1500);
    },
  },
  {
    name: "reminders",
    page: "reminders",
    steps: async (page) => {
      await addReminder(page, "reminder.tpl.water");
      await addReminder(page, "reminder.tpl.pill");
      await go(page, "reminders");
    },
  },
  {
    name: "reminders-alert-style",
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
    page: "calendar",
    steps: async (page) => {
      await addCalendar(page);
      await go(page, "calendar", 1500);
      await fitHeight(page);
    },
  },
  // Games and profiles by app
  // A game being played, the profile each game gets, and the start card on
  // the keyboard's screen: the top of the page down to the games list.
  {
    name: "game-session",
    page: "games",
    viewport: { height: 1000 },
    clip: async (page) => {
      const list = await page.locator("[data-games-list]").boundingBox();
      return { x: 0, y: 0, width: page.viewportSize().width, height: Math.min(page.viewportSize().height, list.y + list.height + 16) };
    },
    steps: async (page, km) => {
      await byApp(page, km);
      await liveOn(page);
      await gamesOn(page);
      await page.evaluate(() => window.__ksMockGame(true));
      await page.waitForTimeout(1500);
    },
  },
  {
    name: "game-session-lights",
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
  {
    name: "game-session-lol",
    page: "games",
    // The panel from its title to the end of the health settings.
    viewport: { height: 1200 },
    clip: async (page) => {
      const panel = await page.locator("[data-lol]").boundingBox();
      const bars = await page.locator("[data-lol-bars]").boundingBox();
      return { x: panel.x - 12, y: panel.y - 12, width: panel.width + 24, height: bars.y + bars.height - panel.y + 16 };
    },
    steps: async (page) => {
      await gamesOn(page);
      await page.locator("[data-game='League of Legends'] [data-game-special]").click();
      await page.waitForTimeout(900);
      await scrollTo(page, page.locator("[data-lol]"), 24);
    },
  },
  {
    name: "game-session-lol-events",
    page: "games",
    viewport: { height: 1200 },
    // The animations from their switch down to the Riot notice.
    clip: async (page) => {
      const panel = await page.locator("[data-lol]").boundingBox();
      const title = await page.locator("[data-lol]").getByText(await t(page, "games.lol.events"), { exact: true }).first().boundingBox();
      return { x: panel.x - 12, y: title.y - 9, width: panel.width + 24, height: panel.y + panel.height - title.y + 21 };
    },
    steps: async (page) => {
      await gamesOn(page);
      await page.locator("[data-game='League of Legends'] [data-game-special]").click();
      await page.waitForTimeout(900);
      await scrollTo(page, await find(page, "switch", "games.lol.events"), 40);
    },
  },
  {
    name: "profiles-by-app",
    page: "profiles",
    viewport: { height: 1000 },
    clip: async (page) => padded(await page.locator("[data-auto-profiles]").boundingBox(), 16),
    steps: async (page, km) => {
      await gamesOn(page);
      await byApp(page, km);
      await scrollTo(page, page.locator("[data-auto-profiles]"), 30);
    },
  },
  // Music mode
  // The page with Bars on the keys, at the same moment of the made-up song
  // every time (just after a snare, so bass and middle stand tall).
  {
    name: "music",
    page: "music",
    steps: async (page) => {
      await musicOn(page);
      await page.evaluate((n) => window.__ksMockMusicStep(n, true), MUSIC_FRAME);
      await page.waitForTimeout(300);
    },
  },
  {
    // The mode and appearance settings.
    name: "music-modes",
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
      await page.evaluate((n) => window.__ksMockMusicStep(n, true), MUSIC_FRAME);
      await scrollTo(page, page.locator("[data-music-settings]"), 20);
    },
  },
  {
    name: "calendar-add-google",
    page: "calendar",
    steps: async (page) => {
      await click(page, "calendar.add");
      await click(page, "calendar.p.google", { start: true, wait: 700 });
      await scrollTo(page, page.locator("main").getByText(await t(page, "calendar.sources"), { exact: true }).first(), 20);
    },
  },
];

/** The Screen page's library card. */
export const libraryCard = (page) => page.locator("section", { has: page.locator("[data-library]") }).last();

/** The Screen page's studio, by its first step's name ("Source"). */
export const studioSteps = async (page) => page.locator("main").getByText(await t(page, "sc.step.source"), { exact: true }).first();

/** Make the window as tall as the page, so all of it shows (up to `max`). */
export async function fitHeight(page, max = 1600) {
  const height = await page.evaluate(() => {
    const main = document.querySelector("main");
    return main.scrollHeight;
  });
  await page.setViewportSize({ width: page.viewportSize().width, height: Math.min(max, height) });
  await page.waitForTimeout(500);
}

/** Screen › Live screen › on (and its one-time warning about the factory animation). */
export async function liveOn(page) {
  await openScreen(page, "live");
  await click(page, "sc.live.mode", { role: "switch", wait: 700 });
  const ack = await button(page, "sc.live.ack.go");
  if (await ack.isVisible()) await ack.click();
  await page.waitForTimeout(800);
}

/** Screen › Images › Make a card, `template`, Preview, then save it to the
 *  library or upload it. */
export async function makeCard(page, template, then, palette) {
  await openScreen(page, "visuals");
  await page.locator('[data-tile="text"]').click();
  await page.waitForTimeout(900);
  await click(page, template, { role: "radio", wait: 600 });
  // The colour themes' names follow a small "Aa" sample.
  if (palette) {
    const name = new RegExp(`${escape(await t(page, palette))}$`);
    await page.getByRole("radio", { name }).first().click();
    await page.waitForTimeout(400);
  }
  await click(page, "sc.step.preview", { wait: 1200 });
  if (then === "save") {
    await click(page, "sc.saveOnly", { wait: 1500 });
  } else {
    await click(page, "sc.uploadAction", { wait: 900 });
    const confirm = await button(page, "sc.confirm.go");
    if (await confirm.isVisible()) await confirm.click();
    await page.locator("[data-library]").first().waitFor();
    await page.waitForTimeout(4000);
  }
}

/** The plate around the big keyboard picture (Key mapping). */
export const keyboardCard = (page) => page.locator("section", { has: page.locator('[data-key="esc"]') }).last();

/** Grow a rectangle by `pad` on every side. */
export const padded = (box, pad) => ({ x: box.x - pad, y: box.y - pad, width: box.width + 2 * pad, height: box.height + 2 * pad });

/** Settings › Color calibration, the card. */
export const calibrationCard = async (page) =>
  page.locator("section").filter({ has: page.getByText(await t(page, "cal.title"), { exact: true }) }).first();

/** Key mapping › Base layer: the number pad's "/" becomes Volume up. */
export async function volumeUpOnSlash(page) {
  await page.locator('[data-key="kp-slash"]').first().click();
  await page.waitForTimeout(300);
  await click(page, "km.cat.media", { role: "radio", wait: 400 });
  await click(page, "km.media.volume-up", { wait: 500 });
}

/** Macros › Add, renamed, recorded: Shift+H, I, then a left click. */
export async function recordGreeting(page, { lang }) {
  await click(page, "mc.new", { wait: 500 });
  await page.getByLabel(await t(page, "mc.name"), { exact: true }).fill(L(lang, "Greeting", "Selamlama"));
  await click(page, "mc.record", { wait: 400 });
  const key = async (code, hold = 90, after = 120) => {
    await page.keyboard.down(code);
    await page.waitForTimeout(hold);
    await page.keyboard.up(code);
    await page.waitForTimeout(after);
  };
  await page.keyboard.down("Shift");
  await page.waitForTimeout(120);
  await key("KeyH", 90, 80);
  await page.keyboard.up("Shift");
  await page.waitForTimeout(150);
  await key("KeyI", 90, 300);
  await click(page, "mc.stop", { wait: 400 });
  await click(page, "mc.mouse.left", { wait: 500 });
}

/** Profiles: "Work" with one effect, then "Gaming" with another (on the keyboard). */
async function twoProfiles(page, { lang }) {
  const create = async (name) => {
    await go(page, "profiles");
    await click(page, "prof.create", { wait: 500 });
    await page.getByLabel(await t(page, "prof.create.name"), { exact: true }).fill(name);
    await click(page, "prof.create.button", { wait: 1200 });
  };
  await saveEffect(page, "fx.7");
  await create(L(lang, "Work", "İş"));
  await saveEffect(page, "fx.15");
  await create(L(lang, "Gaming", "Oyun"));
}

/** Game session › Notice games. */
export async function gamesOn(page) {
  await go(page, "games");
  await click(page, "games.on", { role: "switch", wait: 700 });
}

/** Game session's "Lights while playing" row. */
export const lightsRow = (page) => page.locator("[data-games] div.border-t > div").filter({ has: page.getByRole("radiogroup") }).first();

/** Lighting › Per-key color › `preset`, saved, then a profile `name` from it. */
async function perKeyProfile(page, preset, name) {
  await go(page, "lighting");
  await click(page, "light.tab.perKey", { role: "tab", wait: 700 });
  await click(page, preset, { wait: 600 });
  await click(page, "bar.save", { wait: 300 });
  await waitText(page, "bar.saved");
  await go(page, "profiles");
  await click(page, "prof.create", { wait: 500 });
  await page.getByLabel(await t(page, "prof.create.name"), { exact: true }).fill(name);
  await click(page, "prof.create.button", { wait: 1200 });
}

/** Profiles › By app: every game gets "Gaming", the code editor "Work". */
async function byApp(page, { lang }) {
  const work = L(lang, "Work", "İş");
  const gaming = L(lang, "Gaming", "Oyun");
  await perKeyProfile(page, "light.preset.home", work);
  await perKeyProfile(page, "light.preset.wasd", gaming);
  await go(page, "profiles");
  await click(page, "auto.title", { role: "switch", wait: 600 });
  const add = async (program, profile) => {
    await page.getByRole("combobox", { name: await t(page, "auto.add"), exact: true }).selectOption({ label: program });
    await click(page, "auto.add", { wait: 500 });
    await page.getByRole("combobox", { name: await t(page, "auto.profile"), exact: true }).last().selectOption({ label: profile });
    await page.waitForTimeout(400);
  };
  await add(await t(page, "auto.allGames"), gaming);
  await add("Code", work);
}

/** The README's main picture: the Overview page of a keyboard in use. */
export const HERO = { name: "hero", page: "overview", steps: busyOverview };

// ------------------------------------------------------------------ clips

/** Click without scrolling the page (a clip's frame stays put). */
async function press(page, role, key, opts = {}) {
  await (await find(page, role, key, opts)).dispatchEvent("click");
  if (opts.wait) await page.waitForTimeout(opts.wait);
}

/** A box around `locator` grown by `pad`, cut off at `height` if given. */
async function boxOf(locator, pad = 12, height) {
  const b = await locator.boundingBox();
  return { x: b.x - pad, y: b.y - pad, width: b.width + 2 * pad, height: height ?? b.height + 2 * pad };
}

const LIGHTING_TOUR = ["fx.11", "fx.12", "fx.9", "fx.16", "fx.4"];
/** The "music" clip's modes, each shown this many frames (70 ms apiece). */
const MUSIC_MODES = ["music.mode.bars", "music.mode.beat", "music.mode.level", "music.mode.wave"];
const MUSIC_MODE_FRAMES = 14;
/** Health and mana, in percent, for each frame of the "game-session" clip. */
const LOL_TOUR = [
  [100, 90],
  [86, 78],
  [72, 66],
  [58, 52],
  [44, 40],
  [32, 30],
  [20, 18],
  [9, 8],
];
const tours = new WeakMap();

/** Short animations for the README's feature table (docs/images/features).
 *  Each shows `frames` captures of `clip`; `frame(page, i)` acts before
 *  capture i. */
export const CLIPS = [
  {
    // The keyboard picture on the Effects tab shows only what the keyboard
    // reports, which the browser stand-in doesn't, so this shows the effect
    // list with its moving previews while the choice steps through five.
    name: "lighting",
    page: "lighting",
    motion: true,
    viewport: { height: 1300 },
    fps: 12,
    quality: 75,
    // The gallery down to the row with Flow (the last one chosen).
    clip: async (page) => {
      const gallery = await page.locator("[data-effects]").boundingBox();
      const flow = await page.locator('[data-effects] [data-effect="16"]').boundingBox();
      return { x: gallery.x - 12, y: gallery.y - 8, width: gallery.width + 24, height: flow.y + flow.height - gallery.y + 20 };
    },
    steps: async (page) => {
      await scrollTo(page, page.locator("[data-effects]"), 60);
      // Looked up before recording: the page is slow while 20 previews move.
      tours.set(page, await Promise.all(LIGHTING_TOUR.map((fx) => find(page, "button", fx, { start: true }))));
    },
    // Rolling, Rotating, Outward wave, Flow, Starlight: about 0.8 s each.
    record: async (page) => {
      for (const effect of tours.get(page)) {
        await effect.dispatchEvent("click");
        await page.waitForTimeout(650);
      }
    },
  },
  {
    name: "key-mapping",
    page: "keymap",
    frames: 4,
    delay: [900, 800, 800, 1700],
    clip: async (page) => {
      const plate = await keyboardCard(page).boundingBox();
      return { x: plate.x - 8, y: plate.y - 8, width: plate.width + 16, height: 610 };
    },
    steps: async (page) => {
      await scrollTo(page, keyboardCard(page), 14);
    },
    // The number pad's "/", then Media, then Volume up (all in view: no scrolling).
    frame: async (page, i) => {
      if (i === 1) await page.locator('[data-key="kp-slash"]').first().click();
      if (i === 2) await click(page, "km.cat.media", { role: "radio", wait: 0 });
      if (i === 3) await click(page, "km.media.volume-up", { wait: 0 });
      await page.mouse.move(1, 1);
      await page.waitForTimeout(450);
    },
  },
  {
    name: "macros",
    page: "macros",
    viewport: { height: 1000 },
    frames: 10,
    delay: (i) => (i === 0 ? 700 : 330),
    hold: 1800,
    clip: async (page) => boxOf(page.locator("main .card").filter({ has: page.getByRole("button", { name: await t(page, "mc.record") }) }).last(), 10, 640),
    steps: async (page, { lang }) => {
      await click(page, "mc.new", { wait: 500 });
      await page.getByLabel(await t(page, "mc.name"), { exact: true }).fill(L(lang, "Greeting", "Selamlama"));
      await page.evaluate(() => document.activeElement?.blur());
      await page.waitForTimeout(300);
    },
    // Record, then one key event per frame: Shift+H, I, then a left click.
    frame: async (page, i) => {
      const k = page.keyboard;
      const steps = [
        async () => {},
        async () => press(page, "button", "mc.record"),
        async () => k.down("Shift"),
        async () => k.down("KeyH"),
        async () => k.up("KeyH"),
        async () => k.up("Shift"),
        async () => k.down("KeyI"),
        async () => k.up("KeyI"),
        async () => press(page, "button", "mc.stop"),
        async () => press(page, "button", "mc.mouse.left"),
      ];
      await steps[i]();
      await page.waitForTimeout(i === 0 ? 0 : 160);
    },
  },
  {
    name: "screen",
    page: "screen",
    screenTab: "visuals",
    viewport: { height: 1200 },
    frames: 5,
    delay: [1100, 900, 900, 900, 1800],
    // The library, then the text card's steps in its place.
    clip: async (page) => {
      const studio = await page.locator("[data-studio]").boundingBox();
      return { x: studio.x - 12, y: studio.y - 8, width: studio.width + 24, height: 520 };
    },
    steps: async (page) => {
      await scrollTo(page, page.locator("[data-studio]"), 44);
    },
    // The library's bar, Text card, Mountains, Sunset colors, the preview.
    frame: async (page, i) => {
      if (i === 1) {
        await page.locator('[data-library-bar] [data-tile="text"]').dispatchEvent("click");
        await page.waitForTimeout(700);
      }
      if (i === 2) await press(page, "radio", "sc.tx.t.mountain", { wait: 600 });
      if (i === 3) {
        const name = new RegExp(`${escape(await t(page, "sc.tx.pal.sunset"))}$`);
        await page.getByRole("radio", { name }).first().dispatchEvent("click");
        await page.waitForTimeout(600);
      }
      if (i === 4) await press(page, "button", "sc.step.preview", { wait: 1200 });
    },
  },
  {
    name: "live-screen",
    page: "screen",
    screenTab: "live",
    // Your picture on the GIF page; the song's card replaces it; a meeting's
    // card goes before the song once the calendar's cards are on.
    fps: 12,
    // The GIF page large, with the line under it that names the live card
    // (the clock page beside it and its notes stay out: they would make the
    // card too small at 480 px).
    clip: async (page) => {
      const screen = await page.locator("main [data-screen-pages] canvas").first().boundingBox();
      const label = await page.locator("main [data-screen-pages]").getByText(await t(page, "sc.page.gif"), { exact: true }).first().boundingBox();
      const pad = 20;
      return { x: screen.x - pad, y: screen.y - pad, width: screen.width + 2 * pad, height: label.y + label.height + 12 - (screen.y - pad) };
    },
    steps: async (page) => {
      await makeCard(page, "sc.tx.t.name", "upload");
      await addCalendar(page);
      await openScreen(page, "live");
      await (await find(page, "switch", "calendar.title")).dispatchEvent("click");
      await page.waitForTimeout(400);
      await click(page, "sc.live.mode", { role: "switch", wait: 100 });
      const ack = await button(page, "sc.live.ack.go");
      if (await ack.isVisible()) await ack.click();
      await page.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
      // The song's card comes about 3 s after the live screen starts.
      await page.waitForTimeout(2500);
    },
    record: async (page) => {
      const status = page.locator("main section.card").first();
      await status.getByText("Midnight Train").first().waitFor({ timeout: 10_000 });
      await page.waitForTimeout(400);
      await (await find(page, "switch", "calendar.title")).dispatchEvent("click");
      await status.getByText("Daily stand-up").first().waitFor({ timeout: 10_000 });
      await page.waitForTimeout(1600);
    },
  },
  {
    name: "focus",
    query: { pomofast: "" },
    page: "focus",
    // One-minute blocks (2 s each with ?pomofast): focus fills and empties,
    // then the break does.
    motion: true,
    fps: 12,
    clip: async (page) => {
      const card = await page.locator("main section.card").first().boundingBox();
      const kb = await page.locator("main [data-key=esc]").first().evaluate((el) => {
        const r = el.closest(".kb-stage, .kb")?.getBoundingClientRect() ?? el.getBoundingClientRect();
        return { bottom: r.bottom };
      });
      return { x: card.x, y: card.y, width: card.width, height: kb.bottom - card.y + 20 };
    },
    steps: async (page) => {
      for (const [n, phase] of [
        [1, "pomo.phase.focus"],
        [2, "pomo.phase.break"],
      ]) {
        const label = `${await t(page, "pomo.seq.block", { n })} · ${await t(page, phase)}`;
        const input = page.getByLabel(label, { exact: true });
        await input.fill("1");
        await input.press("Tab");
      }
      await click(page, "pomo.transition.wave", { role: "radio" });
      await page.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
      await page.waitForTimeout(400);
    },
    record: async (page) => {
      await press(page, "button", "pomo.start");
      // The break ends 4 s later; stop as the next focus block starts.
      await page.waitForTimeout(4300);
    },
  },
  {
    // League of Legends lights' drawing: health recedes right to left and
    // turns amber, then red, while mana empties on F1–F12.
    name: "game-session",
    page: "games",
    viewport: { height: 1000 },
    frames: LOL_TOUR.length,
    delay: (i) => (i === 0 ? 1100 : 550),
    hold: 1500,
    clip: async (page) => boxOf(page.locator("[data-lol-preview]"), 12),
    steps: async (page) => {
      await gamesOn(page);
      await page.locator("[data-game='League of Legends'] [data-game-special]").click();
      await page.waitForTimeout(900);
      await scrollTo(page, page.locator("[data-lol-preview]"), 40);
    },
    frame: async (page, i) => {
      const [health, mana] = LOL_TOUR[i];
      await setRange(page, await t(page, "games.lol.preview.health"), health);
      await setRange(page, await t(page, "games.lol.preview.mana"), mana);
      await page.waitForTimeout(450);
    },
  },
  {
    // Music mode: the keys follow the made-up song, a second of each mode
    // (Bars, Beat, Level, Wave). Stepped frame by frame, so every run is the
    // same. A narrower window, so the keys stay readable at 480 px.
    name: "music",
    page: "music",
    viewport: { width: 980, height: 900 },
    frames: MUSIC_MODES.length * MUSIC_MODE_FRAMES,
    delay: 70,
    quality: 75,
    clip: async (page) => {
      const card = await page.locator("[data-music] section.card").first().boundingBox();
      const modes = await page.locator("[data-music-settings] [role=radiogroup]").first().boundingBox();
      return { x: card.x, y: card.y, width: card.width, height: modes.y + modes.height + 16 - card.y };
    },
    steps: async (page) => {
      await musicOn(page);
      await page.evaluate(() => document.querySelector("main").scrollTo({ top: 0, behavior: "instant" }));
      await page.evaluate(() => window.__ksMockMusicStep(20, true));
      await page.waitForTimeout(300);
    },
    frame: async (page, i) => {
      if (i % MUSIC_MODE_FRAMES === 0) {
        await press(page, "radio", MUSIC_MODES[i / MUSIC_MODE_FRAMES]);
      }
      await page.evaluate(() => window.__ksMockMusicStep(1));
      await page.mouse.move(1, 1);
      await page.waitForTimeout(80);
    },
  },
  {
    // Profiles › By app: every game gets "Gaming", then the code editor
    // "Work". A narrower window, so the card's text stays readable at 480 px.
    name: "profiles",
    page: "profiles",
    viewport: { width: 980, height: 1000 },
    frames: 6,
    delay: [1000, 800, 900, 1000, 800, 900],
    hold: 2200,
    clip: async (page) => {
      const card = await page.locator("[data-auto-profiles]").boundingBox();
      return { x: card.x - 8, y: card.y - 8, width: card.width + 16, height: 560 };
    },
    steps: async (page, { lang }) => {
      // The name the fourth frame picks.
      tours.set(page, L(lang, "Gaming", "Oyun"));
      await gamesOn(page);
      await perKeyProfile(page, "light.preset.home", L(lang, "Work", "İş"));
      await perKeyProfile(page, "light.preset.wasd", L(lang, "Gaming", "Oyun"));
      await go(page, "profiles");
      await click(page, "auto.title", { role: "switch", wait: 600 });
      await scrollTo(page, page.locator("[data-auto-profiles]"), 30);
    },
    frame: async (page, i) => {
      const program = page.getByRole("combobox", { name: await t(page, "auto.add"), exact: true });
      const profile = page.getByRole("combobox", { name: await t(page, "auto.profile"), exact: true });
      if (i === 1) await program.selectOption({ label: await t(page, "auto.allGames") });
      if (i === 2) await press(page, "button", "auto.add");
      if (i === 3) await profile.last().selectOption({ label: tours.get(page) });
      if (i === 4) await program.selectOption({ label: "Code" });
      if (i === 5) await press(page, "button", "auto.add");
      await page.mouse.move(1, 1);
      await page.waitForTimeout(450);
    },
  },
];

/** Set a range input (found by its label) as if dragged. */
async function setRange(page, label, value) {
  await page.getByLabel(label, { exact: true }).evaluate((el, v) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, String(v));
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, value);
}
