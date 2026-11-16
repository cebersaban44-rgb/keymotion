// League of Legends lights, under League of Legends in the Game session
// page's list ("Special lights"): where health shows and how it recedes,
// the colours, and each event's animation. The drawing plays
// frames the backend draws with the same code a match uses (lol.rs), so what
// the page shows is what the keys will do.

import { Play, Skull, Usb } from "lucide-react";
import { useEffect, useEffectEvent, useMemo, useRef, useState, type ReactNode } from "react";
import type { GamesSettings } from "../bindings/GamesSettings";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { LolAnim } from "../bindings/LolAnim";
import type { LolEvent } from "../bindings/LolEvent";
import type { LolEventLight } from "../bindings/LolEventLight";
import type { LolLayout } from "../bindings/LolLayout";
import type { LolScene } from "../bindings/LolScene";
import type { LolSettings } from "../bindings/LolSettings";
import type { PomodoroMotion } from "../bindings/PomodoroMotion";
import { ColorPicker, Segmented, Toggle } from "../components/controls";
import { KeyboardView } from "../components/KeyboardView";
import { LOL_STEP_MS, lolPreview, tryLol, type LolFrame } from "../lib/games";
import { useT, type MessageKey } from "../lib/i18n";
import { gameSummaryCard, gameWords } from "../lib/liveCards";
import { fromHex, toHex } from "../lib/pomodoroLights";
import { useApp } from "../lib/store";
import { Switch } from "./gamesRows";
import { FramePlayer } from "./screen/common";

/** The events in the order a match brings them, in three groups. */
const GROUPS: { title: MessageKey; events: LolEvent[] }[] = [
  { title: "games.lol.group.mine", events: ["levelUp", "kill"] },
  { title: "games.lol.group.map", events: ["ace", "dragon", "herald", "baron"] },
  { title: "games.lol.group.end", events: ["win", "lose"] },
];

/** A game that has lights of its own: League of Legends (games.rs `is_lol`). */
export function hasSpecialLights(key: string): boolean {
  return key.toLowerCase().endsWith("league of legends.exe");
}
const ANIMS: LolAnim[] = ["wave", "flash", "pulse", "sparkle", "ring", "drain"];
const MOTIONS: PomodoroMotion[] = ["rightToLeft", "leftToRight", "topToBottom", "toCenter", "fade"];
const MOTION_KEY: Record<PomodoroMotion, MessageKey> = {
  rightToLeft: "pomo.motion.rightToLeft",
  leftToRight: "pomo.motion.leftToRight",
  topToBottom: "pomo.motion.topToBottom",
  toCenter: "pomo.motion.toCenter",
  fade: "pomo.motion.fade",
};
const select = "h-8 rounded-lg border border-line bg-bg-raise px-2 text-[13px] outline-none focus:border-signal";

export function LolLights({
  kb,
  usb,
  settings: s,
  set,
}: {
  kb: KeyboardInfo;
  usb: boolean;
  settings: GamesSettings;
  set: (patch: Partial<GamesSettings>) => void;
}) {
  const t = useT();
  const l = s.lol;
  const setLol = (patch: Partial<LolSettings>) => set({ lol: { ...l, ...patch } });
  const setLight = (e: LolEvent, patch: Partial<LolEventLight>) =>
    setLol({ lights: { ...l.lights, [e]: { ...l.lights[e], ...patch } } });
  return (
    <div className="flex flex-col gap-3 rounded-lg border border-line bg-surface-2/40 p-4" data-lol>
      <div className="flex items-start justify-between gap-3">
        <span className="flex max-w-[80ch] flex-col">
          <span className="text-[13px] font-semibold">{t("games.lol.panel")}</span>
          <span className="text-[12px] text-ink-3">{t("games.lol.body")}</span>
        </span>
        <Toggle checked={l.enabled} onChange={(enabled) => setLol({ enabled })} label={t("games.lol.panel")} />
      </div>
      {l.enabled && (
        <>
          {!usb && <p className="text-[12px] text-ink-3">{t("games.noUsb")}</p>}
          <Preview kb={kb} usb={usb} settings={s} />
          <Switch label={t("games.lol.bars")} body={t("games.lol.bars.body")} on={l.bars} onChange={(bars) => setLol({ bars })} />
          {l.bars && (
            <div className="flex flex-col gap-3" data-lol-bars>
              <Field label={t("games.lol.layout")}>
                <Segmented<LolLayout>
                  label={t("games.lol.layout")}
                  value={l.layout}
                  onChange={(layout) => setLol({ layout })}
                  options={[
                    { value: "keyboard", label: t("games.lol.layout.keyboard") },
                    { value: "row", label: t("games.lol.layout.row") },
                  ]}
                />
              </Field>
              {l.layout === "keyboard" && (
                <Field label={t("games.lol.motion")}>
                  <select className={select} value={l.motion} aria-label={t("games.lol.motion")} onChange={(e) => setLol({ motion: e.target.value as PomodoroMotion })}>
                    {MOTIONS.map((m) => (
                      <option key={m} value={m}>
                        {t(MOTION_KEY[m])}
                      </option>
                    ))}
                  </select>
                </Field>
              )}
              <Field label={t("games.lol.healthColor")}>
                <select
                  className={select}
                  value={l.healthColor ? "one" : "auto"}
                  aria-label={t("games.lol.healthColor")}
                  onChange={(e) => setLol({ healthColor: e.target.value === "one" ? [52, 199, 89] : null })}
                >
                  <option value="auto">{t("games.lol.healthColor.auto")}</option>
                  <option value="one">{t("games.lol.healthColor.one")}</option>
                </select>
              </Field>
              {l.healthColor && (
                <ColorPicker inline label={t("games.lol.healthColor")} value={toHex(l.healthColor)} onChange={(hex) => setLol({ healthColor: fromHex(hex) })} />
              )}
              <div className="flex flex-wrap items-center gap-3">
                <label className="flex items-center gap-2 text-[13px]">
                  <input type="checkbox" checked={l.mana} onChange={(e) => setLol({ mana: e.target.checked })} />
                  {t("games.lol.mana")}
                </label>
                {l.mana && <Swatch label={t("games.lol.manaColor")} value={l.manaColor} onChange={(manaColor) => setLol({ manaColor })} />}
                <span className="text-[12px] text-ink-3">{t("games.lol.mana.body")}</span>
              </div>
            </div>
          )}
          <Switch label={t("games.lol.death")} body={t("games.lol.death.body")} on={l.death} onChange={(death) => setLol({ death })} />
          <Switch label={t("games.lol.events")} body={t("games.lol.events.body")} on={l.events} onChange={(events) => setLol({ events })} />
          {l.events && (
            <div className="flex flex-col gap-3" data-lol-events>
              {GROUPS.map((group) => (
                <div key={group.title} className="flex flex-col gap-1.5">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-[12px] font-medium text-ink-2">{t(group.title)}</span>
                    {group.title === "games.lol.group.map" && (
                      <label className="flex items-center gap-1.5 text-[12px] text-ink-2">
                        <input type="checkbox" checked={l.teamOnly} onChange={(x) => setLol({ teamOnly: x.target.checked })} data-lol-team-only />
                        {t("games.lol.teamOnly")}
                      </label>
                    )}
                  </div>
                  <ul className="flex flex-col divide-y divide-line rounded-lg border border-line">
              {group.events.map((e) => {
                const light = l.lights[e];
                const name = t(`games.lol.event.${e}`);
                return (
                  <li key={e} className="flex flex-wrap items-center gap-3 px-3 py-2 text-[13px]" data-lol-event={e}>
                    <input type="checkbox" checked={light.on} aria-label={name} onChange={(x) => setLight(e, { on: x.target.checked })} />
                    <span className={`min-w-[120px] flex-1 ${light.on ? "" : "text-ink-3"}`}>{name}</span>
                    <select
                      className={select}
                      value={light.anim}
                      aria-label={`${t("games.lol.anim")}, ${name}`}
                      onChange={(x) => {
                        setLight(e, { anim: x.target.value as LolAnim });
                        playEvent(e);
                      }}
                    >
                      {ANIMS.map((a) => (
                        <option key={a} value={a}>
                          {t(`games.lol.anim.${a}`)}
                        </option>
                      ))}
                    </select>
                    {e === "dragon" && (
                      <label className="flex items-center gap-1.5 text-[12px] text-ink-2">
                        <input type="checkbox" checked={l.dragonKindColor} onChange={(x) => setLol({ dragonKindColor: x.target.checked })} />
                        {t("games.lol.dragonKind")}
                      </label>
                    )}
                    {(e !== "dragon" || !l.dragonKindColor) && (
                      <Swatch label={`${t("games.lol.color")}, ${name}`} value={light.color} onChange={(color) => setLight(e, { color })} />
                    )}
                    <button
                      type="button"
                      className="rounded-md p-1.5 text-ink-3 hover:bg-surface-2 hover:text-ink"
                      aria-label={`${t("games.lol.play")}, ${name}`}
                      title={t("games.lol.play")}
                      onClick={() => playEvent(e)}
                    >
                      <Play size={14} />
                    </button>
                  </li>
                );
              })}
                  </ul>
                </div>
              ))}
            </div>
          )}
          <LolCard />
          <p className="text-[12px] text-ink-3">{t("games.lol.never")}</p>
        </>
      )}
      <p className="text-[11px] text-ink-3" data-games-riot-notice>
        {t("games.lol.legal")}
      </p>
    </div>
  );
}

// The preview below listens for these: an event row's play button.
const PLAY = "keymotion:lol-play";
function playEvent(event: LolEvent) {
  window.dispatchEvent(new CustomEvent<LolEvent>(PLAY, { detail: event }));
}

/** A label and its control on one line. */
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <span className="min-w-[110px] text-[13px] text-ink-2">{label}</span>
      {children}
    </div>
  );
}

/** A small colour well. */
function Swatch({ label, value, onChange }: { label: string; value: [number, number, number]; onChange: (c: [number, number, number]) => void }) {
  const hex = toHex(value);
  return (
    <span className="relative h-7 w-9 shrink-0 overflow-hidden rounded-md border border-line-strong" style={{ background: hex }} title={label}>
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(fromHex(e.target.value))}
        className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        aria-label={label}
      />
    </span>
  );
}

/** The keyboard drawing: health and mana from the sliders, or an event's
 *  animation or death played once, then back to health. */
function Preview({ kb, usb, settings }: { kb: KeyboardInfo; usb: boolean; settings: GamesSettings }) {
  const t = useT();
  const g = kb.geometry;
  const [health, setHealth] = useState(70);
  const [mana, setMana] = useState(55);
  const [playing, setPlaying] = useState<LolScene | null>(null);
  const [frame, setFrame] = useState<LolFrame | null>(null);
  const timer = useRef<number | null>(null);
  const bars: LolScene = useMemo(() => ({ kind: "bars", health: health / 100, mana: mana / 100 }), [health, mana]);
  const scene = playing ?? bars;

  // The frames of the scene; a moving one is played, then health returns.
  const show = useEffectEvent(async (sc: LolScene) => {
    const frames = await lolPreview(kb.modelId, settings, sc);
    if (timer.current) window.clearInterval(timer.current);
    timer.current = null;
    if (frames.length <= 1) {
      setFrame(frames[0] ?? null);
      return;
    }
    let i = 0;
    setFrame(frames[0]!);
    timer.current = window.setInterval(() => {
      i += 1;
      if (i >= frames.length) {
        if (timer.current) window.clearInterval(timer.current);
        timer.current = null;
        setPlaying(null);
        return;
      }
      setFrame(frames[i]!);
    }, LOL_STEP_MS);
  });
  useEffect(() => {
    const id = window.setTimeout(() => void show(scene).catch(console.error), playing ? 0 : 120);
    return () => window.clearTimeout(id);
  }, [scene, playing, settings, kb.modelId]);
  useEffect(() => () => {
    if (timer.current) window.clearInterval(timer.current);
  }, []);
  useEffect(() => {
    const on = (e: Event) => setPlaying({ kind: "event", event: (e as CustomEvent<LolEvent>).detail });
    window.addEventListener(PLAY, on);
    return () => window.removeEventListener(PLAY, on);
  }, []);

  const colors = useMemo(() => {
    const out: Record<number, string> = {};
    for (const [led, [r, gg, b]] of frame ?? []) out[led] = `rgb(${r},${gg},${b})`;
    return out;
  }, [frame]);

  return (
    <div className="flex flex-col gap-2" data-lol-preview>
      {g && (
        <div className="w-full max-w-[760px]">
          <KeyboardView geometry={g} connection={kb.connection} colors={colors} compact label={t("games.lol.preview")} />
        </div>
      )}
      <div className="flex flex-wrap items-center gap-3 text-[12px] text-ink-2">
        <label className="flex items-center gap-2">
          {t("games.lol.preview.health")}
          <input type="range" min={0} max={100} step={1} value={health} onChange={(e) => setHealth(Number(e.target.value))} aria-label={t("games.lol.preview.health")} />
        </label>
        <label className="flex items-center gap-2">
          {t("games.lol.preview.mana")}
          <input type="range" min={0} max={100} step={1} value={mana} onChange={(e) => setMana(Number(e.target.value))} aria-label={t("games.lol.preview.mana")} />
        </label>
        <button type="button" className="btn-secondary" onClick={() => setPlaying({ kind: "death" })}>
          <Skull size={14} />
          {t("games.lol.preview.death")}
        </button>
        <button type="button" className="btn-secondary" disabled={!usb} onClick={() => tryLol(scene).catch(console.error)} data-lol-try>
          {t("games.try")}
        </button>
        {!usb && (
          <span className="flex items-center gap-1.5 text-ink-3">
            <Usb size={13} />
            {t("games.noUsb")}
          </span>
        )}
      </div>
    </div>
  );
}

/** The summary card of a League of Legends match, drawn by the real code. */
function LolCard() {
  const t = useT();
  const locale = useApp((x) => x.locale);
  const [frame, setFrame] = useState<Uint16Array | null>(null);
  const draw = useEffectEvent(() =>
    gameSummaryCard(
      {
        kind: "summary",
        name: "League of Legends",
        steamApp: null,
        exe: null,
        startedMs: Date.now() - 32 * 60_000,
        endedMs: Date.now(),
        gpuAvg: 61,
        gpuPeak: 88,
        cpuAvg: 27,
        cpuPeak: 64,
        tempPeak: 66,
        todayMs: 125 * 60_000,
        lol: { win: true, kills: 8, deaths: 3, assists: 12, cs: 214, champion: "Ahri", mode: "CLASSIC" },
      },
      gameWords(t),
    ),
  );
  useEffect(() => {
    let live = true;
    draw().then((p) => live && setFrame(p.frames[0]!), console.error);
    return () => {
      live = false;
    };
  }, [locale]);
  return (
    <figure className="flex flex-col gap-1">
      {frame ? <FramePlayer frames={[frame]} delaysMs={[100]} className="h-[76px] w-[135px] rounded-md bg-black" /> : <div className="h-[76px] w-[135px] rounded-md bg-black" />}
      <figcaption className="text-[11px] text-ink-3">{t("games.lol.card")}</figcaption>
    </figure>
  );
}

