// KeyboardEvent.code (the key's position) → HID keyboard usage (page 0x07).
// Recording by position keeps a macro correct on any Windows layout: on a
// Turkish Q keyboard "Semicolon" is the Ş key and plays back as Ş.

const MAP: Record<string, number> = {
  Enter: 0x28,
  Escape: 0x29,
  Backspace: 0x2a,
  Tab: 0x2b,
  Space: 0x2c,
  Minus: 0x2d,
  Equal: 0x2e,
  BracketLeft: 0x2f,
  BracketRight: 0x30,
  Backslash: 0x31,
  IntlHash: 0x32,
  Semicolon: 0x33,
  Quote: 0x34,
  Backquote: 0x35,
  Comma: 0x36,
  Period: 0x37,
  Slash: 0x38,
  CapsLock: 0x39,
  PrintScreen: 0x46,
  ScrollLock: 0x47,
  Pause: 0x48,
  Insert: 0x49,
  Home: 0x4a,
  PageUp: 0x4b,
  Delete: 0x4c,
  End: 0x4d,
  PageDown: 0x4e,
  ArrowRight: 0x4f,
  ArrowLeft: 0x50,
  ArrowDown: 0x51,
  ArrowUp: 0x52,
  NumLock: 0x53,
  NumpadDivide: 0x54,
  NumpadMultiply: 0x55,
  NumpadSubtract: 0x56,
  NumpadAdd: 0x57,
  NumpadEnter: 0x58,
  Numpad0: 0x62,
  NumpadDecimal: 0x63,
  IntlBackslash: 0x64,
  ContextMenu: 0x65,
  ControlLeft: 0xe0,
  ShiftLeft: 0xe1,
  AltLeft: 0xe2,
  MetaLeft: 0xe3,
  ControlRight: 0xe4,
  ShiftRight: 0xe5,
  AltRight: 0xe6,
  MetaRight: 0xe7,
};

for (let i = 0; i < 26; i++) MAP[`Key${String.fromCharCode(65 + i)}`] = 0x04 + i;
for (let i = 1; i <= 9; i++) MAP[`Digit${i}`] = 0x1d + i;
MAP.Digit0 = 0x27;
for (let i = 1; i <= 12; i++) MAP[`F${i}`] = 0x39 + i;
for (let i = 13; i <= 24; i++) MAP[`F${i}`] = 0x67 + (i - 12);
for (let i = 1; i <= 9; i++) MAP[`Numpad${i}`] = 0x58 + i;

/** HID usage for a `KeyboardEvent.code`, or `undefined` when the key has none. */
export function hidFromCode(code: string): number | undefined {
  return MAP[code];
}

const BY_HID = new Map<number, string>(Object.entries(MAP).map(([code, hid]) => [hid, code]));

/** `KeyboardEvent.code` of the key at HID usage `hid`. */
export function codeFromHid(hid: number): string | undefined {
  return BY_HID.get(hid);
}
