// How key actions read on keycaps (short) and in the panel (long).

import {
  ArrowLeft,
  ArrowRight,
  Ban,
  Calculator,
  House,
  Mail,
  Monitor,
  Mouse,
  MousePointerClick,
  Music,
  OctagonX,
  Play,
  RotateCw,
  Search,
  SkipBack,
  SkipForward,
  Square,
  Star,
  Volume1,
  Volume2,
  VolumeX,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";
import type { KeyActionInfo } from "../bindings/KeyActionInfo";
import type { KeymapCatalog } from "../bindings/KeymapCatalog";
import type { KeyUsageInfo } from "../bindings/KeyUsageInfo";
import type { MouseButtonName } from "../bindings/MouseButtonName";
import type { MessageKey } from "./i18n";

type T = (key: MessageKey) => string;

/** Modifier bits: long name and keycap symbol. Right-hand ones are marked "R". */
const MODS: [bit: number, name: string, symbol: string][] = [
  [0x01, "Ctrl", "⌃"],
  [0x02, "Shift", "⇧"],
  [0x04, "Alt", "⎇"],
  [0x08, "Win", "⊞"],
  [0x10, "R Ctrl", "⌃"],
  [0x20, "R Shift", "⇧"],
  [0x40, "R Alt", "⎇"],
  [0x80, "R Win", "⊞"],
];

/** Left-hand modifiers offered as "held with" toggles. */
export const MOD_TOGGLES = MODS.slice(0, 4).map(([bit, name]) => ({ bit, name }));

export const MEDIA_ICONS: Record<string, LucideIcon> = {
  "play-pause": Play,
  next: SkipForward,
  previous: SkipBack,
  stop: Square,
  "volume-up": Volume2,
  "volume-down": Volume1,
  mute: VolumeX,
  "media-player": Music,
  mail: Mail,
  calculator: Calculator,
  "my-computer": Monitor,
  "browser-home": House,
  "browser-back": ArrowLeft,
  "browser-forward": ArrowRight,
  "browser-stop": OctagonX,
  "browser-refresh": RotateCw,
  "browser-favorites": Star,
  "browser-search": Search,
};

const MOUSE_SHORT: Record<MouseButtonName, string> = {
  left: "L",
  right: "R",
  middle: "M",
  back: "4",
  forward: "5",
  "double-click": "2×",
};

/** Keycap-sized names for long legends. */
const SHORT: Record<string, string> = {
  "print-screen": "PrtSc",
  "scroll-lock": "ScrLk",
  caps: "Caps",
  backspace: "Bksp",
  insert: "Ins",
  delete: "Del",
  pgup: "PgUp",
  pgdn: "PgDn",
  numlock: "Num",
  "iso-hash": "ISO#",
  "iso-backslash": "ISO\\",
  "kp-enter": "N↵",
};

export function usageShort(u: KeyUsageInfo): string {
  const short = SHORT[u.id];
  if (short) return short;
  if (u.group === "keypad") return `N${u.label.replace("Num ", "")}`;
  return u.label;
}

function usageOf(catalog: KeymapCatalog | null, code: number) {
  return catalog?.keys.find((k) => k.code === code);
}

function modNames(mods: number): string[] {
  return MODS.filter(([bit]) => mods & bit).map(([, name]) => name);
}

function modSymbols(mods: number): string {
  return MODS.filter(([bit]) => mods & bit)
    .map(([, , sym]) => sym)
    .join("");
}

function mediaId(catalog: KeymapCatalog | null, usage: number): string | undefined {
  return catalog?.media.find((m) => m.usage === usage)?.id;
}

/** Full description, e.g. "Ctrl + Shift + A" or "Volume up". */
export function actionText(a: KeyActionInfo, catalog: KeymapCatalog | null, t: T): string {
  switch (a.kind) {
    case "default":
      return t("km.default");
    case "disabled":
      return t("km.disabled");
    case "key": {
      const key = a.code ? (usageOf(catalog, a.code)?.label ?? `0x${a.code.toString(16)}`) : null;
      return [...modNames(a.modifiers), ...(key ? [key] : [])].join(" + ");
    }
    case "media": {
      const id = mediaId(catalog, a.usage);
      return id ? t(`km.media.${id}` as MessageKey) : `Media 0x${a.usage.toString(16)}`;
    }
    case "mouse":
      return t(`km.mouse.${a.button}` as MessageKey);
    case "wheel":
      return t(a.up ? "km.wheel.up" : "km.wheel.down");
    case "macro":
      return `${t("km.macro")} ${a.slot + 1}`;
  }
}

/** Keycap legend: symbols and icons that fit a 1u key. */
export function actionShort(a: KeyActionInfo, catalog: KeymapCatalog | null): ReactNode {
  switch (a.kind) {
    case "default":
      return null;
    case "disabled":
      return <Ban aria-hidden />;
    case "key": {
      const u = a.code ? usageOf(catalog, a.code) : undefined;
      if (!a.code) {
        // A modifier on its own: name it like its key.
        const names = modNames(a.modifiers);
        return names.length === 1 ? names[0] : modSymbols(a.modifiers);
      }
      const key = u ? usageShort(u) : `0x${a.code.toString(16)}`;
      return modSymbols(a.modifiers) + key;
    }
    case "media": {
      const Icon = MEDIA_ICONS[mediaId(catalog, a.usage) ?? ""];
      return Icon ? <Icon aria-hidden /> : "Media";
    }
    case "mouse":
      return (
        <>
          <MousePointerClick aria-hidden />
          {MOUSE_SHORT[a.button]}
        </>
      );
    case "wheel":
      return (
        <>
          <Mouse aria-hidden />
          {a.up ? "↑" : "↓"}
        </>
      );
    case "macro":
      return `M${a.slot + 1}`;
  }
}
