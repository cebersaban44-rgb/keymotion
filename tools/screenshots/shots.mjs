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

const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** An element by its role and the app's text for `key`. `start` matches
 *  names that begin with the text (a card with a second line, say). */
async function find(page, role, key, { vars, start = false, within } = {}) {
  const text = await t(page, key, vars);
  const name = start ? new RegExp(`^\\s*${escape(text)}`) : text;
  return (within ?? page).getByRole(role, { name, exact: !start }).first();
}
const button = (page, key, opts) => find(page, "button", key, opts);
async function click(page, key, opts = {}) {
  const el = await find(page, opts.role ?? "button", key, opts);
  await el.click();
  await page.waitForTimeout(opts.wait ?? 350);
}

/** Open a page of the app as the sidebar does. */
async function go(page, id, wait = 800) {
  await page.evaluate((p) => {
    window.__keymotionStore.getState().setPage(p);
    document.querySelector("main").scrollTo({ top: 0, behavior: "instant" });
  }, id);
  await page.waitForTimeout(wait);
}
async function openScreen(page, tab, wait = 900) {
  await page.evaluate((p) => {
    window.__keymotionStore.getState().openScreen(p);
    document.querySelector("main").scrollTo({ top: 0, behavior: "instant" });
  }, tab);
  await page.waitForTimeout(wait);
}

/** Scroll the page so `locator` sits `offset` px below the top of the window. */
async function scrollTo(page, locator, offset = 28) {
  await locator.evaluate((el, off) => {
    const main = document.querySelector("main");
    const top = el.getBoundingClientRect().top - main.getBoundingClientRect().top + main.scrollTop - off;
    main.scrollTo({ top, behavior: "instant" });
  }, offset);
  await page.waitForTimeout(350);
}
async function scrollToBottom(page) {
  await page.evaluate(() => {
    const main = document.querySelector("main");
    main.scrollTo({ top: main.scrollHeight, behavior: "instant" });
  });
  await page.waitForTimeout(350);
}

/** The open dialog with a margin of the dimmed page around it. */
const dialogClip =
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
async function waitText(page, key, opts = {}) {
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
async function addReminder(page, template, title) {
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
async function gaugesOn(page) {
  await go(page, "gauges");
  await click(page, "gauges.on", { role: "switch", wait: 600 });
}

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
      await scrollTo(page, await find(page, "button", "update.check"), 120);
      await scrollToBottom(page);
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
      await click(page, "sc.src.textGo", { wait: 900 });
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
    clip: dialogClip(28, "[role=dialog] > .card"),
    steps: async (page) => {
      await makeCard(page, "sc.tx.t.mountain", "save", "sc.tx.pal.sunset");
      const row = page.locator("[data-library-item]", { hasText: await t(page, "sc.tx.sample.mountain.t") }).first();
      await click(page, "sc.lm.action", { within: row, wait: 900 });
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
    query: { pomofast: "" },
    page: "focus",
    steps: async (page) => {
      await click(page, "pomo.start", { wait: 600 });
      // Some of the first block gone: the ring and the keys part empty.
      await page.waitForTimeout(9000);
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
const libraryCard = (page) => page.locator("section", { has: page.locator("[data-library]") }).last();

/** The Screen page's studio, by its first step's name ("Source"). */
const studioSteps = async (page) => page.locator("main").getByText(await t(page, "sc.step.source"), { exact: true }).first();

/** Make the window as tall as the page, so all of it shows (up to `max`). */
async function fitHeight(page, max = 1600) {
  const height = await page.evaluate(() => {
    const main = document.querySelector("main");
    return main.scrollHeight;
  });
  await page.setViewportSize({ width: page.viewportSize().width, height: Math.min(max, height) });
  await page.waitForTimeout(500);
}

/** Screen › Live screen › on (and its one-time warning about the factory animation). */
async function liveOn(page) {
  await openScreen(page, "live");
  await click(page, "sc.live.mode", { role: "switch", wait: 700 });
  const ack = await button(page, "sc.live.ack.go");
  if (await ack.isVisible()) await ack.click();
  await page.waitForTimeout(800);
}

/** Screen › Images › Make a card, `template`, Preview, then save it to the
 *  library or upload it. */
async function makeCard(page, template, then, palette) {
  await openScreen(page, "visuals");
  await click(page, "sc.src.textGo", { wait: 900 });
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
const keyboardCard = (page) => page.locator("section", { has: page.locator('[data-key="esc"]') }).last();

/** Grow a rectangle by `pad` on every side. */
const padded = (box, pad) => ({ x: box.x - pad, y: box.y - pad, width: box.width + 2 * pad, height: box.height + 2 * pad });

/** Settings › Color calibration, the card. */
const calibrationCard = async (page) =>
  page.locator("section").filter({ has: page.getByText(await t(page, "cal.title"), { exact: true }) }).first();

/** Key mapping › Base layer: the number pad's "/" becomes Volume up. */
async function volumeUpOnSlash(page) {
  await page.locator('[data-key="kp-slash"]').first().click();
  await page.waitForTimeout(300);
  await click(page, "km.cat.media", { role: "radio", wait: 400 });
  await click(page, "km.media.volume-up", { wait: 500 });
}

/** Macros › Add, renamed, recorded: Shift+H, I, then a left click. */
async function recordGreeting(page, { lang }) {
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
    // The list down to the row with Flow (the last one chosen).
    clip: async (page) => {
      const card = await page.locator("main section.card").filter({ has: page.getByText(await t(page, "light.effects"), { exact: true }) }).first().boundingBox();
      const flow = await (await find(page, "button", "fx.16", { start: true })).boundingBox();
      return { x: card.x - 10, y: card.y - 10, width: card.width + 20, height: flow.y + flow.height - card.y + 26 };
    },
    steps: async (page) => {
      await scrollTo(page, page.locator("main").getByText(await t(page, "light.effects"), { exact: true }).first(), 60);
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
    frames: 5,
    delay: [900, 900, 900, 900, 1800],
    clip: async (page) => {
      const source = page.getByText(await t(page, "sc.step.source"), { exact: true });
      const card = await page.locator("main section.card").filter({ has: source }).last().boundingBox();
      return { x: card.x, y: card.y, width: card.width, height: 520 };
    },
    steps: async (page) => {
      await scrollTo(page, await studioSteps(page), 44);
    },
    // Source, Name card, Mountains, Sunset colors, then the preview.
    frame: async (page, i) => {
      if (i === 1) await press(page, "button", "sc.src.textGo", { wait: 700 });
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
    clip: async (page) => {
      const card = await page.locator("main section.card").first().boundingBox();
      const gif = await page.locator("main").getByText(await t(page, "sc.page.gif"), { exact: true }).first().boundingBox();
      // Both screen pages; the notes on the right don't change.
      return { x: card.x, y: card.y, width: gif.x - card.x + 330, height: gif.y + gif.height - card.y + 16 };
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
];
