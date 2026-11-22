// The interface's Russian text. Empty until it is translated: a key that is
// missing here shows the English text (see i18n.ts), so the app works while
// the translation is in progress. `node tools/check-i18n.mjs` lists the
// missing keys and checks the ones that are here (placeholders, plural
// forms, Cyrillic script); with `--release` a missing key is an error.
//
// Counts are written with ICU plural syntax and all four Russian forms, not
// with `.one` keys:
//   "{count, plural, one {# кадр} few {# кадра} many {# кадров} other {# кадра}}"

import type { Messages } from "./en";

export const ru: Partial<Messages> = {};
