// The interface's Simplified Chinese text. Empty until it is translated: a
// key that is missing here shows the English text (see i18n.ts), so the app
// works while the translation is in progress. `node tools/check-i18n.mjs`
// lists the missing keys and checks the ones that are here (placeholders,
// Chinese script, punctuation); with `--release` a missing key is an error.
//
// Chinese has one form after a number: no `.one` keys and no plural syntax
// ("{count} 帧").

import type { Messages } from "./en";

export const zhCN: Partial<Messages> = {};
