// Profiles (profiles.rs): the list for the keyboard's model, loading one
// part by part, and the words the Profiles page and the Overview card use.

import { create } from "zustand";
import type { ProfileInfo } from "../bindings/ProfileInfo";
import type { ProfilePart } from "../bindings/ProfilePart";
import type { ProfileParts } from "../bindings/ProfileParts";
import type { ProfilesState } from "../bindings/ProfilesState";
import type { Locale, MessageKey, Translate } from "./i18n";
import { getProfiles, loadProfilePart } from "./ipc";
import { adoptKeymapRecord } from "./keymap";
import { initLighting } from "./lighting";
import { adoptMacrosRecord } from "./macros";
import { useScreen } from "./screen";

interface ProfilesStore {
  /** Model the list below was read for. */
  modelId: string | null;
  state: ProfilesState | null;
  /** Read the list (and what the keyboard has) for `modelId`. */
  load: (modelId: string) => Promise<void>;
}

export const useProfiles = create<ProfilesStore>((set) => ({
  modelId: null,
  state: null,
  load: async (modelId) => {
    const state = await getProfiles(modelId);
    set({ modelId, state });
  },
}));

export const PART_LABEL: Record<ProfilePart, MessageKey> = {
  lighting: "prof.part.lighting",
  keymap: "prof.part.keymap",
  macros: "prof.part.macros",
  screen: "prof.part.screen",
};

/** The part inside a sentence ("ışık, tuş atamaları ve makrolar"). */
export const PART_NOUN: Record<ProfilePart, MessageKey> = {
  lighting: "prof.noun.lighting",
  keymap: "prof.noun.keymap",
  macros: "prof.noun.macros",
  screen: "prof.noun.screen",
};

export const PARTS: readonly ProfilePart[] = ["lighting", "keymap", "macros", "screen"];

const intl = (locale: Locale) => (locale === "tr" ? "tr-TR" : "en-US");

/** "a, b ve c" / "a, b, and c". */
export function listOf(items: string[], locale: Locale): string {
  return new Intl.ListFormat(intl(locale), { type: "conjunction" }).format(items);
}

function capitalize(text: string, locale: Locale): string {
  return text.charAt(0).toLocaleUpperCase(intl(locale)) + text.slice(1);
}

export function profileName(p: ProfileInfo, t: Translate): string {
  return p.builtIn ? t("prof.default") : p.name;
}

/** The parts a profile holds, in loading order. */
export function partsOf(p: ProfileInfo): ProfilePart[] {
  return PARTS.filter((part) => p[part] !== null);
}

/** One sentence: what the profile puts on the keyboard. */
export function profileSummary(p: ProfileInfo, t: Translate, locale: Locale): string {
  if (p.builtIn) return t("prof.default.sum");
  const out: string[] = [];
  const l = p.lighting;
  if (l?.kind === "effect") {
    out.push(
      l.mode === 0 ? t("prof.sum.off") : t("prof.sum.effect", { effect: t(`fx.${l.mode}` as MessageKey), level: l.brightness }),
    );
  } else if (l?.kind === "per-key") {
    out.push(l.keys > 0 ? t("prof.sum.perKey", { count: l.keys }) : t("prof.sum.perKey.none"));
  }
  if (p.keymap !== null) out.push(p.keymap === 0 ? t("prof.sum.keymap.factory") : t("prof.sum.keymap", { count: p.keymap }));
  if (p.macros !== null) out.push(p.macros === 0 ? t("prof.sum.macros.none") : t("prof.sum.macros", { count: p.macros }));
  if (p.screen) out.push(p.screen.inLibrary ? t("prof.sum.screen", { name: p.screen.name }) : t("prof.sum.screen.missing"));
  return capitalize(t("prof.sum", { list: listOf(out, locale) }), locale);
}

/** The profile on the keyboard: the one with the most parts when several match. */
export function profileOnKeyboard(s: ProfilesState): ProfileInfo | null {
  const size = (p: ProfileInfo) => partsOf(p).length;
  return [...s.profiles, s.factory].filter((p) => p.onKeyboard).sort((a, b) => size(b) - size(a))[0] ?? null;
}

// Measured write times (docs/donanim-dogrulama.md): an effect 0.3 s, a
// per-key design 0.9 s, a keymap layer 0.6 s; macros about as long.
const SECONDS: Record<Exclude<ProfilePart, "screen">, number> = { lighting: 0.3, keymap: 1.3, macros: 0.6 };

/** About how long loading `p` takes, in whole seconds. */
export function loadSeconds(p: ProfileInfo, pageMs: number): number {
  let s = 0;
  for (const part of p.toWrite) {
    if (part === "screen") s += ((p.screen?.pages ?? 0) * pageMs) / 1000;
    else if (part === "lighting" && p.lighting?.kind === "per-key") s += 0.9;
    else s += SECONDS[part];
  }
  return Math.max(1, Math.ceil(s));
}

/** "3 sn", "4 dk". */
export function fmtDuration(seconds: number, t: Translate): string {
  return seconds < 60 ? `${seconds} ${t("time.s")}` : `${Math.ceil(seconds / 60)} ${t("time.m")}`;
}

// What goes into a new profile: a per-machine choice.
const PARTS_KEY = "ks.profileParts";
const DEFAULT_PARTS: ProfileParts = { lighting: true, keymap: true, macros: true, screen: false };

export function loadPartsChoice(): ProfileParts {
  try {
    const raw = JSON.parse(localStorage.getItem(PARTS_KEY) ?? "null") as Partial<ProfileParts> | null;
    if (!raw || typeof raw !== "object") return DEFAULT_PARTS;
    const pick = (k: keyof ProfileParts) => (typeof raw[k] === "boolean" ? (raw[k] as boolean) : DEFAULT_PARTS[k]);
    return { lighting: pick("lighting"), keymap: pick("keymap"), macros: pick("macros"), screen: pick("screen") };
  } catch {
    return DEFAULT_PARTS;
  }
}

export function savePartsChoice(parts: ProfileParts) {
  try {
    localStorage.setItem(PARTS_KEY, JSON.stringify(parts));
  } catch {
    /* per-machine convenience only */
  }
}

/** A part of a load failed; `cause` is the backend's (or the upload's) error. */
export class ProfileLoadError extends Error {
  part: ProfilePart;
  constructor(part: ProfilePart, cause: unknown) {
    super(cause instanceof Error ? cause.message : String(cause), { cause });
    this.part = part;
  }
}

export type StepState = "waiting" | "running" | "done";

/**
 * Put `p` on the keyboard, one part at a time: only the parts that differ
 * (`toWrite`), the screen picture last as an ordinary library upload.
 * `stop()` is asked before each part. Throws a [`ProfileLoadError`] at the
 * first part that fails. Afterwards the editors and the list read the
 * records again; the parts written replace the editors' drafts.
 */
export async function loadProfile(
  modelId: string,
  p: ProfileInfo,
  onStep: (part: ProfilePart, state: StepState) => void,
  stop: () => boolean,
): Promise<"done" | "stopped"> {
  const tried = new Set<ProfilePart>();
  try {
    for (const part of p.toWrite) {
      if (stop()) return "stopped";
      onStep(part, "running");
      tried.add(part);
      try {
        if (part === "screen") await uploadPicture(p);
        else await loadProfilePart(modelId, p.id, part);
      } catch (e) {
        throw new ProfileLoadError(part, e);
      }
      onStep(part, "done");
    }
    return "done";
  } finally {
    const jobs: Promise<unknown>[] = [useProfiles.getState().load(modelId)];
    if (tried.has("lighting")) jobs.push(initLighting());
    if (tried.has("keymap")) jobs.push(adoptKeymapRecord(modelId));
    if (tried.has("macros")) jobs.push(adoptMacrosRecord(modelId));
    await Promise.all(jobs.map((j) => j.catch(console.error)));
  }
}

async function uploadPicture(p: ProfileInfo) {
  const screen = useScreen.getState();
  const item = screen.state?.items.find((i) => i.id === p.screen?.id);
  if (!item) throw new Error("unknown library item");
  // `uploadToKeyboard` returns at once while another upload runs.
  if (screen.upload) throw new Error("screen upload stopped: another upload is running");
  await screen.uploadToKeyboard(item, "library");
}
