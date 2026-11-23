// "Müzik modu" page (Araçlar): the key lights move with the sound the PC
// plays, in one of four modes. The backend listens and lights the keys
// (app/src-tauri/src/music.rs); this page sets it up and shows what the keys
// show. The focus timer and a game's own lights go over the music; the
// music goes over the system gauges (owner's decision, 2026-11-22). The pill
// pickers sit in a <div> so they keep their own width in the column.

import { AudioLines, Gamepad2, Palette, ShieldCheck, Sparkles, Usb, Volume2, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { MusicMode } from "../bindings/MusicMode";
import type { MusicPalette } from "../bindings/MusicPalette";
import type { MusicRest } from "../bindings/MusicRest";
import type { MusicSettings } from "../bindings/MusicSettings";
import type { MusicSilence } from "../bindings/MusicSilence";
import { ColorPicker, InfoBar, Segmented, Slider, Toggle } from "../components/controls";
import { KeyboardView } from "../components/KeyboardView";
import { percent, useT, type MessageKey } from "../lib/i18n";
import { SENSITIVITY_MAX, SENSITIVITY_MIN, useMusic, watchMusic } from "../lib/music";
import { usePomodoro } from "../lib/pomodoro";
import { fromHex, toHex } from "../lib/pomodoroLights";
import { useApp } from "../lib/store";

const MODE_HINT: Record<MusicMode, MessageKey> = {
  bars: "music.mode.bars.hint",
  beat: "music.mode.beat.hint",
  level: "music.mode.level.hint",
  wave: "music.mode.wave.hint",
};

export function Music({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  const s = useMusic((x) => x.state);
  const configure = useMusic((x) => x.configure);
  const timer = usePomodoro((x) => x.state);
  // Colour drags and the slider fire many changes: show them at once, save
  // the latest shortly after.
  const [draft, setDraft] = useState<Partial<MusicSettings>>({});
  const configureRef = useRef(configure);
  configureRef.current = configure;
  const draftRef = useRef(draft);
  draftRef.current = draft;
  useEffect(() => {
    if (!Object.keys(draft).length) return;
    const id = window.setTimeout(() => {
      configureRef.current(draft).catch(console.error);
      setDraft({});
    }, 150);
    return () => window.clearTimeout(id);
  }, [draft]);
  // Leaving the page with a change still waiting saves it.
  useEffect(
    () => () => {
      if (Object.keys(draftRef.current).length) configureRef.current(draftRef.current).catch(console.error);
    },
    [],
  );
  // The sound is read for the preview only while the window is on screen:
  // closed to the tray or minimized, the page stays mounted but nobody looks.
  const windowShown = useApp((x) => x.windowShown);
  useEffect(() => (windowShown ? watchMusic(kb.geometry ?? null) : undefined), [kb.geometry, windowShown]);
  const geometry = kb.geometry;
  const colors = useMemo(() => {
    if (!s) return null;
    const out: Record<number, string> = {};
    for (const c of s.frame) out[c.led] = `rgb(${c.r},${c.g},${c.b})`;
    return out;
  }, [s]);
  if (!s) return null;

  const shown = { ...s.settings, ...draft };
  const set = (patch: Partial<MusicSettings>) => configure(patch).catch(console.error);
  const timerFront = !!timer && (timer.trying || (timer.status !== "idle" && timer.settings.lights));
  const status: MessageKey = !shown.enabled
    ? "music.status.off"
    : !usb
      ? "music.status.noUsb"
      : timerFront && s.status === "playing"
        ? "music.status.timer"
        : (`music.status.${s.status}` as MessageKey);
  const lit = shown.enabled && usb && s.status === "playing" && !timerFront;
  const twoColors = shown.palette === "two";

  return (
    <div className="flex flex-col gap-6" data-music>
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.music")}</h1>
        <p className="page-desc">{t("music.body")}</p>
      </header>
      {!usb && (
        <InfoBar tone="warn" icon={<Usb size={20} strokeWidth={1.6} />} title={t("music.noUsb")} data="usb" />
      )}

      <section className="card flex flex-col">
        {/* On or off, and what the keys show. */}
        <div className="flex flex-col gap-5 p-5">
          <div className="flex items-start justify-between gap-4">
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-semibold">{t("music.on")}</span>
              <span className="max-w-[80ch] text-[12px] text-ink-3">{t("music.on.body")}</span>
            </span>
            <Toggle checked={shown.enabled} onChange={(enabled) => set({ enabled })} label={t("music.on")} />
          </div>
          <div className="flex items-center gap-2 text-[12px]" data-music-status={status}>
            <span className={`h-2 w-2 shrink-0 rounded-full ${lit ? "bg-ok" : "bg-ink-3"}`} aria-hidden />
            <span className={shown.enabled ? "text-ink-2" : "text-ink-3"}>{t(status)}</span>
          </div>
          {geometry && colors && (
            <div className="flex flex-col gap-2">
              <span className="text-[12px] text-ink-3">{t("music.preview")}</span>
              <div className="w-full max-w-[760px]" data-music-preview>
                <KeyboardView geometry={geometry} connection={kb.connection} colors={colors} compact label={t("music.preview")} />
              </div>
            </div>
          )}
        </div>

        {/* Settings. */}
        <div className="flex flex-col divide-y divide-line border-t border-line" data-music-settings>
          <Row icon={Sparkles} title={t("music.mode")} body={t(MODE_HINT[shown.mode])}>
            <div>
              <Segmented<MusicMode>
                label={t("music.mode")}
                value={shown.mode}
                onChange={(mode) => set({ mode })}
                options={[
                  { value: "bars", label: t("music.mode.bars") },
                  { value: "beat", label: t("music.mode.beat") },
                  { value: "level", label: t("music.mode.level") },
                  { value: "wave", label: t("music.mode.wave") },
                ]}
              />
            </div>
            {(shown.mode === "bars" || shown.mode === "wave") && (
              <div className="flex items-start justify-between gap-3">
                <span className="flex flex-col">
                  <span className="text-[13px] font-medium">{t("music.center")}</span>
                  <span className="text-[12px] text-ink-3">{t("music.center.body")}</span>
                </span>
                <Toggle checked={shown.center} onChange={(center) => set({ center })} label={t("music.center")} />
              </div>
            )}
          </Row>
          <Row icon={Palette} title={t("music.look")} body={t("music.look.body")}>
            <div className="flex flex-col gap-2">
              <span className="text-[13px] font-medium">{t("music.palette")}</span>
              <div>
                <Segmented<MusicPalette>
                  label={t("music.palette")}
                  value={shown.palette}
                  onChange={(palette) => set({ palette })}
                  options={[
                    { value: "heat", label: t("music.palette.heat") },
                    { value: "rainbow", label: t("music.palette.rainbow") },
                    { value: "one", label: t("music.palette.single") },
                    { value: "two", label: t("music.palette.two") },
                  ]}
                />
              </div>
            </div>
            {(shown.palette === "one" || twoColors) && (
              <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-6">
                <ColorPicker
                  label={t(twoColors ? "music.color.first" : "music.color")}
                  value={toHex(shown.color)}
                  onChange={(hex) => setDraft((d) => ({ ...d, color: fromHex(hex) }))}
                />
                {twoColors && (
                  <ColorPicker
                    label={t("music.color.second")}
                    value={toHex(shown.color2)}
                    onChange={(hex) => setDraft((d) => ({ ...d, color2: fromHex(hex) }))}
                  />
                )}
              </div>
            )}
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-6">
              <Slider
                label={t("music.sensitivity")}
                value={shown.sensitivity}
                min={SENSITIVITY_MIN}
                max={SENSITIVITY_MAX}
                step={5}
                format={(v) => percent(v)}
                onChange={(sensitivity) => setDraft((d) => ({ ...d, sensitivity }))}
              />
              <Slider
                label={t("music.brightness")}
                value={shown.brightness}
                min={1}
                max={5}
                step={1}
                format={(v) => `${v}/5`}
                onChange={(brightness) => set({ brightness })}
              />
            </div>
            <div className="flex flex-col gap-2">
              <span className="text-[13px] font-medium">{t("music.rest")}</span>
              <div>
                <Segmented<MusicRest>
                  label={t("music.rest")}
                  value={shown.rest}
                  onChange={(rest) => set({ rest })}
                  options={[
                    { value: "dark", label: t("music.rest.dark") },
                    { value: "dim", label: t("music.rest.dim") },
                  ]}
                />
              </div>
            </div>
          </Row>
          <Row icon={Volume2} title={t("music.silence")} body={t("music.silence.body")}>
            <div>
              <Segmented<MusicSilence>
                label={t("music.silence")}
                value={shown.silence}
                onChange={(silence) => set({ silence })}
                options={[
                  { value: "saved", label: t("music.silence.saved") },
                  { value: "dim", label: t("music.silence.dim") },
                  { value: "dark", label: t("music.silence.dark") },
                ]}
              />
            </div>
          </Row>
          <Row
            icon={Gamepad2}
            title={t("music.games")}
            body={t("music.games.body")}
            checked={shown.pauseInGames}
            onChange={(pauseInGames) => set({ pauseInGames })}
          />
        </div>

        <div className="flex flex-col gap-2 border-t border-line px-5 py-4" data-music-notes>
          <p className="flex items-start gap-2 text-[12px] text-ink-3">
            <ShieldCheck size={14} className="mt-px shrink-0" />
            {t("music.privacy")}
          </p>
          <p className="flex items-start gap-2 text-[12px] text-ink-3">
            <AudioLines size={14} className="mt-px shrink-0" />
            {t("music.end")}
          </p>
        </div>
      </section>
    </div>
  );
}

/** One group of settings: a title and what it does, its switch, and what
 *  belongs to it, indented under it (as on the gauges page). */
function Row({
  icon: Icon,
  title,
  body,
  checked,
  onChange,
  children,
}: {
  icon: LucideIcon;
  title: string;
  body: string;
  checked?: boolean;
  onChange?: (v: boolean) => void;
  children?: ReactNode;
}) {
  const nested = Array.isArray(children) ? children.some(Boolean) : !!children;
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-start justify-between gap-3">
        <span className="flex items-start gap-2.5">
          <Icon size={16} className="mt-0.5 shrink-0 text-signal" />
          <span className="flex max-w-[80ch] flex-col">
            <span className="text-[13px] font-semibold">{title}</span>
            <span className="text-[12px] text-ink-3">{body}</span>
          </span>
        </span>
        {onChange && checked !== undefined && <Toggle checked={checked} onChange={onChange} label={title} />}
      </div>
      {nested && <div className="ml-[7px] flex flex-col gap-3 border-l-2 border-line pl-5">{children}</div>}
    </div>
  );
}
