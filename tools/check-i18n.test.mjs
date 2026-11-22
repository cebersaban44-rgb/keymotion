// Tests for the interface's message engine (app/src/lib/i18n/plural.ts),
// language detection (locales.ts), line breaking (lineBreak.ts) and the
// dictionary checks (check-i18n.mjs). Node runs the app's TypeScript as it is.
// Run: node --test tools/check-i18n.test.mjs

import assert from "node:assert/strict";
import { test } from "node:test";

import { pieces, trimBeforeEllipsis } from "../app/src/lib/lineBreak.ts";
import { intlTag, localeForWindows } from "../app/src/lib/i18n/locales.ts";
import { formatMessage, parseMessage, pickText, placeholderNames, plurals } from "../app/src/lib/i18n/plural.ts";
import { checkDictionaries, hashOf, loadDictionaries } from "./check-i18n.mjs";

const RU = "{count, plural, one {# кадр} few {# кадра} many {# кадров} other {# кадра}}";

test("plural: Russian one, few, many and other", () => {
  const f = (count) => formatMessage(RU, { count }, "ru-RU");
  assert.equal(f(1), "1 кадр");
  assert.equal(f(2), "2 кадра");
  assert.equal(f(4), "4 кадра");
  assert.equal(f(5), "5 кадров");
  assert.equal(f(11), "11 кадров");
  assert.equal(f(14), "14 кадров");
  assert.equal(f(21), "21 кадр");
  assert.equal(f(22), "22 кадра");
  assert.equal(f(0), "0 кадров");
  // A fraction: "other", written with the Russian decimal comma.
  assert.equal(f(1.5), "1,5 кадра");
});

test("plural: English, Turkish, Chinese, exact forms, other variables", () => {
  const en = "{count, plural, =0 {No frames} one {# frame} other {# frames}} of {max}";
  assert.equal(formatMessage(en, { count: 0, max: 5 }, "en-US"), "No frames of 5");
  assert.equal(formatMessage(en, { count: 1, max: 5 }, "en-US"), "1 frame of 5");
  assert.equal(formatMessage(en, { count: 3, max: 5 }, "en-US"), "3 frames of 5");
  assert.equal(formatMessage("{n} kare", { n: 3 }, "tr-TR"), "3 kare");
  assert.equal(formatMessage("{n, plural, other {# 帧}}", { n: 1 }, "zh-CN"), "1 帧");
  // Two counts in one text; the plural on a variable other than count.
  const two = "{n, plural, one {# switch} other {# switches}} of {max, plural, one {# key} other {# keys}}";
  assert.equal(formatMessage(two, { n: 1, max: 3 }, "en-US"), "1 switch of 3 keys");
  // A value given as text is shown as it is and still picks its form.
  assert.equal(formatMessage("{n, plural, one {# item} other {# items}}", { n: "1" }, "en-US"), "1 item");
});

test("plural: plain text stays plain", () => {
  // Apostrophes are text (unlike full ICU), braces that are not placeholders stay.
  assert.equal(formatMessage("Riot's {name} isn't {x}", { name: "LoL" }, "en-US"), "Riot's LoL isn't {x}");
  assert.equal(formatMessage("{ } {1} # {", { 1: "one" }, "en-US"), "{ } one # {");
  assert.equal(formatMessage("No places.", {}, "en-US"), "No places.");
  // Broken plural syntax: the text unchanged, never an exception.
  assert.equal(formatMessage("{n, plural, one {x}", { n: 1 }, "en-US"), "{n, plural, one {x}");
  // No value for the plural: its "other" form, with the place kept.
  assert.equal(formatMessage("{n, plural, one {# x} other {# xs}}", {}, "en-US"), "{n} xs");
});

test("plural: syntax errors name the problem", () => {
  assert.throws(() => parseMessage("{n, plural, one {x}}"), /needs an "other" form/);
  assert.throws(() => parseMessage("{n, plural, lots {x} other {y}}"), /not a plural form/);
  assert.throws(() => parseMessage("{n, plural, one {x} one {y} other {z}}"), /appears twice/);
  assert.throws(() => parseMessage("{n, select, a {x} other {y}}"), /only plural/);
  assert.throws(() => parseMessage("{n, plural, one {x} other {y}"), /not closed/);
  assert.throws(() => parseMessage("{n, plural, one x other {y}}"), /needs its text/);
  const parts = parseMessage(`${RU} из {max}`);
  assert.deepEqual([...placeholderNames(parts)].sort(), ["count", "max"]);
  assert.deepEqual(plurals(parts), [{ name: "count", forms: ["one", "few", "many", "other"] }]);
});

test("pickText: the language's own text, else English, with .one forms from the same dictionary", () => {
  const en = { a: "{count} frames", "a.one": "{count} frame", b: "Save" };
  assert.deepEqual(pickText({}, en, "a", 1), { text: "{count} frame", fromOwn: false });
  assert.deepEqual(pickText({}, en, "a", 2), { text: "{count} frames", fromOwn: false });
  // Russian has the key (plural syntax, no .one): its own text, also for 1.
  assert.deepEqual(pickText({ a: RU }, en, "a", 1), { text: RU, fromOwn: true });
  assert.deepEqual(pickText({ a: "x" }, en, "b"), { text: "Save", fromOwn: false });
});

test("localeForWindows: the language to start in", () => {
  const cases = {
    "tr-TR": "tr",
    tr: "tr",
    "ru-RU": "ru",
    "ru-KZ": "ru",
    ru: "ru",
    "zh-CN": "zh-CN",
    "zh-Hans": "zh-CN",
    "zh-Hans-CN": "zh-CN",
    "zh-SG": "zh-CN",
    zh: "zh-CN",
    "zh-TW": "en",
    "zh-HK": "en",
    "zh-MO": "en",
    "zh-Hant": "en",
    "zh-Hant-TW": "en",
    "uk-UA": "en",
    "be-BY": "en",
    "kk-KZ": "en",
    "en-US": "en",
    "de-DE": "en",
    "": "en",
    "not a tag": "en",
  };
  for (const [tag, want] of Object.entries(cases)) assert.equal(localeForWindows(tag), want, tag);
  assert.equal(intlTag("zh-CN"), "zh-CN");
  assert.equal(intlTag("ru"), "ru-RU");
});

test("lineBreak: Chinese breaks between words, punctuation stays with its word", () => {
  const ps = pieces("上传到键盘，完成。“保存”");
  const texts = ps.map((p) => p.text);
  assert.ok(texts.length > 2, JSON.stringify(texts));
  assert.equal(texts.join(""), "上传到键盘，完成。“保存”");
  for (const t of texts.slice(1)) assert.doesNotMatch(t, /^[，。”]/u, JSON.stringify(texts));
  for (const t of texts) assert.doesNotMatch(t, /“$/u, JSON.stringify(texts));
  assert.ok(ps.every((p) => !p.space));
  // Spaces still separate words, in every language.
  assert.deepEqual(pieces("Сохранить в клавиатуру"), [
    { text: "Сохранить", space: false },
    { text: "в", space: true },
    { text: "клавиатуру", space: true },
  ]);
  const brand = pieces("黑爵键盘 AK980");
  assert.equal(brand.map((p) => p.text).join(""), "黑爵键盘AK980");
  assert.deepEqual(brand.at(-1), { text: "AK980", space: true });
  assert.equal(trimBeforeEllipsis("完成，"), "完成");
  assert.equal(trimBeforeEllipsis("word, "), "word");
});

const EN = {
  "a.save": "Save",
  "a.frames": "{count} frames",
  "a.frames.one": "{count} frame",
  "a.left": "{min} min left",
  "a.done": "Done.",
  "games.lol.legal": "Not endorsed by Riot Games.",
  "app.name": "Keymotion",
};
const TR = {
  "a.save": "Kaydet",
  "a.frames": "{count} kare",
  "a.frames.one": "{count} kare",
  "a.left": "{min} dk kaldı",
  "a.done": "Tamam.",
  "games.lol.legal": "Not endorsed by Riot Games.",
  "app.name": "Keymotion",
};
// The rules a key broke (length warnings only when asked: most translations are longer).
const rules = (findings, lang, key, length = false) =>
  findings
    .filter((f) => f.lang === lang && f.key === key && (length || f.rule !== "length"))
    .map((f) => `${f.level}:${f.rule}`);

test("check: complete dictionaries pass; missing ones warn, or fail with --release", () => {
  const ok = checkDictionaries({ en: EN, tr: TR, ru: {}, "zh-CN": {} });
  assert.equal(ok.filter((f) => f.level === "error").length, 0);
  // The .one key is not expected in Russian or Chinese.
  assert.equal(ok.filter((f) => f.rule === "missing").length, 2 * 6);
  assert.ok(ok.every((f) => f.key !== "a.frames.one"));
  const release = checkDictionaries({ en: EN, tr: TR, ru: {}, "zh-CN": {} }, { release: true });
  assert.equal(release.filter((f) => f.level === "error").length, 2 * 6);
  // Turkish must be complete.
  const { "a.save": _, ...partialTr } = TR;
  assert.deepEqual(rules(checkDictionaries({ en: EN, tr: partialTr, ru: {}, "zh-CN": {} }), "tr", "a.save"), ["error:missing"]);
});

test("check: keys, placeholders and plurals", () => {
  const ru = {
    "a.extra": "Лишний",
    "a.frames.one": "{count} кадр",
    "a.frames": "{count, plural, one {# кадр} other {# кадра}}",
    "a.left": "Осталось {mins} мин",
    "a.save": "{x, plural, one {",
  };
  const zh = { "a.frames": "{count, plural, other {# 帧}}", "a.left": "剩余 {min} 分钟" };
  const f = checkDictionaries({ en: EN, tr: TR, ru, "zh-CN": zh });
  assert.deepEqual(rules(f, "ru", "a.extra"), ["error:keys"]);
  assert.deepEqual(rules(f, "ru", "a.frames.one"), ["error:keys"]);
  assert.deepEqual(rules(f, "ru", "a.frames"), ["error:plural"]);
  assert.deepEqual(rules(f, "ru", "a.left"), ["error:placeholders"]);
  assert.deepEqual(rules(f, "ru", "a.save"), ["error:plural"]);
  assert.deepEqual(rules(f, "zh-CN", "a.frames"), ["error:plural"]);
  assert.deepEqual(rules(f, "zh-CN", "a.left"), []);
  // A misspelled plural key in English.
  const bad = checkDictionaries({ en: { ...EN, "a.fram.one": "x" }, tr: { ...TR, "a.fram.one": "x" }, ru: {}, "zh-CN": {} });
  assert.deepEqual(rules(bad, "en", "a.fram.one"), ["error:keys"]);
  // Russian without a plural where English has a .one form: a warning.
  const noPlural = checkDictionaries({ en: EN, tr: TR, ru: { "a.frames": "Кадров: {count}" }, "zh-CN": {} });
  assert.deepEqual(rules(noPlural, "ru", "a.frames"), ["warning:plural"]);
  // An option named .one beside other options is not a plural form.
  const options = { "a.color": "Color", "a.color.auto": "Automatic", "a.color.one": "One color" };
  const opt = checkDictionaries({ en: { ...EN, ...options }, tr: { ...TR, ...options }, ru: {}, "zh-CN": {} });
  assert.deepEqual(rules(opt, "en", "a.color.one"), ["error:keys"]);
  // A count English shows only through its .one form may pick the Russian form.
  const times = { "a.times": "times", "a.times.one": "time" };
  const ruTimes = { "a.times": "{count, plural, one {раз} few {раза} many {раз} other {раза}}" };
  const t = checkDictionaries({ en: { ...EN, ...times }, tr: { ...TR, ...times }, ru: ruTimes, "zh-CN": {} });
  assert.deepEqual(rules(t, "ru", "a.times"), []);
});

test("check: untranslated text, scripts, punctuation, forbidden words, the Riot line", () => {
  const ru = {
    "a.save": "Save",
    "a.left": "Осталось {min} мин!",
    "a.done": "Готово",
    "app.name": "Keymotion",
    "games.lol.legal": "Не одобрено Riot Games.",
  };
  const zh = {
    "a.save": "Сохранить",
    "a.left": "剩余{min}分钟.",
    "a.done": "完成...",
  };
  const f = checkDictionaries({ en: EN, tr: TR, ru, "zh-CN": zh });
  assert.deepEqual(rules(f, "ru", "a.save"), ["error:untranslated", "error:script"]);
  assert.deepEqual(rules(f, "ru", "a.left"), ["error:tone"]);
  assert.deepEqual(rules(f, "ru", "a.done"), []);
  assert.deepEqual(rules(f, "ru", "app.name"), []);
  assert.deepEqual(rules(f, "ru", "games.lol.legal"), ["error:riot"]);
  assert.deepEqual(rules(f, "zh-CN", "a.save"), ["error:script", "error:script"]);
  assert.deepEqual(rules(f, "zh-CN", "a.left"), ["error:punctuation", "error:punctuation"]);
  // "..." and an ASCII full stop after Chinese.
  assert.deepEqual(rules(f, "zh-CN", "a.done"), ["error:punctuation", "error:punctuation"]);
  const g = checkDictionaries({
    en: EN,
    tr: TR,
    ru: { "a.save": "Сохранить в трей", "a.done": "Готово - да." },
    "zh-CN": { "a.save": "保存到托盘。", "a.done": "完成。请您重试。" },
  });
  assert.deepEqual(rules(g, "ru", "a.save", true), ["error:glossary", "warning:length"]);
  assert.deepEqual(rules(g, "ru", "a.done"), ["warning:punctuation"]);
  assert.deepEqual(rules(g, "zh-CN", "a.save", true), ["error:punctuation", "error:glossary", "warning:length"]);
  assert.deepEqual(rules(g, "zh-CN", "a.done"), ["error:glossary"]);
});

test("check: stale translations, from meta.json hashes", () => {
  const ru = { "a.save": "Сохранить", "a.done": "Готово." };
  const meta = { ru: { "a.save": hashOf("Save"), "a.done": hashOf("Finished.") } };
  const f = checkDictionaries({ en: EN, tr: TR, ru, "zh-CN": {} }, { meta });
  assert.deepEqual(rules(f, "ru", "a.save"), []);
  assert.deepEqual(rules(f, "ru", "a.done"), ["warning:stale"]);
  const release = checkDictionaries({ en: EN, tr: TR, ru, "zh-CN": {} }, { meta, release: true });
  assert.ok(rules(release, "ru", "a.done").includes("error:stale"));
});

test("check: the app's dictionaries have no errors", async () => {
  const { dicts, where } = await loadDictionaries();
  assert.ok(Object.keys(dicts.en).length > 1000);
  assert.ok(where.en.lines["app.name"] > 0);
  const errors = checkDictionaries(dicts).filter((f) => f.level === "error");
  assert.deepEqual(errors, []);
});
