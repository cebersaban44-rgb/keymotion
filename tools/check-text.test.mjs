// Tests for check-text.mjs: mojibake is found in Turkish, Russian and Chinese
// text read with the wrong Windows code page, and real text is left alone.
// Run: node --test tools/check-text.test.mjs

import assert from "node:assert/strict";
import { test } from "node:test";

import { findMojibake, withoutSamples } from "./check-text.mjs";

// Real text in the app's languages, with the punctuation each one uses.
const REAL = [
  "Klavyeye kaydet · Işıklar, Tuş atama ve Makrolar; ŞİMDİ ÇALIŞIYOR — “tamam” …",
  "DAĞ’IN ÖTESİ: ağaç\u00A0·\u00A0dağ, İstanbul’da 40 °C, %40",
  "Сохранить в клавиатуру — всё «готово», ёлка, Рёв, Сёмга, 10 °С, 40\u00A0%, №5",
  "Рік, Ґанок, Їжак, Ўладзімір: «Подключите клавиатуру…» „вложенные“",
  "上传到键盘，完成。正在保存…“保存”（2.4G 接收器）：5 分钟前；黑爵 AK980 V2（带屏版）",
  "Español: ESPAÑA, Ñandú; Português: não, São Paulo; naïve café…”",
  "Keymotion isn't endorsed by Riot Games — K / D / A, 1–6, 25 min",
];

for (const line of REAL) {
  test(`real text is not mojibake: ${line.slice(0, 24)}…`, () => {
    assert.deepEqual(findMojibake(line), []);
  });
}

// Code page 1254 as Windows maps it (Node's decoder knows it); 1252 differs
// from it in six letters.
const cp1254 = (s) => new TextDecoder("windows-1254").decode(Buffer.from(s, "utf8"));
const cp1252 = (s) =>
  cp1254(s).replace(/[ĞİŞğış]/g, (c) => ({ Ğ: "Ð", İ: "Ý", Ş: "Þ", ğ: "ð", ı: "ý", ş: "þ" })[c]);
const cp1251 = (s) => new TextDecoder("windows-1251").decode(Buffer.from(s, "utf8"));

const SAMPLES = {
  turkish: ["ışık", "Şimdi", "çalış", "—"],
  russian: ["Сохранить", "клавиатура", "тест", "Привет", "ёлка"],
  chinese: ["上传", "键盘", "，", "。", "保存", "灯光"],
};

for (const [name, words] of Object.entries(SAMPLES)) {
  for (const [cp, decode] of [["1252", cp1252], ["1254", cp1254], ["1251", cp1251]]) {
    if (name === "turkish" && cp === "1251") continue; // Not a case the check covers.
    test(`${name} read as code page ${cp} is found`, () => {
      for (const w of words) {
        const broken = decode(w);
        assert.notEqual(broken, w);
        assert.ok(findMojibake(broken).length > 0, `${JSON.stringify(w)} → ${JSON.stringify(broken)} was not found`);
      }
    });
  }
}

test("a Markdown code span that only shows a sample is allowed", () => {
  const sample = cp1254("т");
  assert.equal(withoutSamples(`The pattern \`${sample}\` is mojibake.`), "The pattern  is mojibake.");
  assert.ok(findMojibake(withoutSamples(`Broken: ${sample}`)).length > 0);
});
