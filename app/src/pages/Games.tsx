// "Oyun oturumu" page (Araçlar): Keymotion notices a game, puts a card on
// the keyboard's screen when it starts and a summary when it ends, lights
// the keys the game needs, and keeps a history on this computer. The
// backend does the work (app/src-tauri/src/games.rs); this page sets it up
// and shows what it found. Design: keymotion-notes,
// oyun-oturumu-ve-otomatik-profil.md.

import {
  Gamepad2,
  History,
  Monitor,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Swords,
  Usb,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useEffectEvent, useMemo, useState, type ReactNode } from "react";
import type { GameCard } from "../bindings/GameCard";
import type { GameEntry } from "../bindings/GameEntry";
import type { GameLights } from "../bindings/GameLights";
import type { GameRest } from "../bindings/GameRest";
import type { GamesSettings } from "../bindings/GamesSettings";
import type { GamesState } from "../bindings/GamesState";
import type { GameSource } from "../bindings/GameSource";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import { ColorPicker, Segmented, Slider, Toggle } from "../components/controls";
import { KeyboardView } from "../components/KeyboardView";
import { GAME_KEY_SETS, SUMMARY_MIN_MS, playTime, rescanGames, tryGameLights, useGames } from "../lib/games";
import { percent, useT, type MessageKey } from "../lib/i18n";
import { useLive } from "../lib/live";
import { gameStartCard, gameSummaryCard, gameWords } from "../lib/liveCards";
import { fromHex, toHex } from "../lib/pomodoroLights";
import { useApp } from "../lib/store";
import { FramePlayer } from "./screen/common";

const SOURCE_KEY: Record<GameSource, MessageKey> = {
  steam: "games.source.steam",
  epic: "games.source.epic",
  gog: "games.source.gog",
  xbox: "games.source.xbox",
  riot: "games.source.riot",
  windows: "games.source.windows",
  mine: "games.source.mine",
};

export function Games({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const s = useGames((x) => x.state);
  const configure = useGames((x) => x.configure);
  const mark = useGames((x) => x.mark);
  const drawError = useGames((x) => x.drawError);
  const liveOn = useLive((x) => x.state?.enabled ?? false);
  const setPage = useApp((x) => x.setPage);
  const setScreenTab = useApp((x) => x.setScreenTab);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 15_000);
    return () => window.clearInterval(id);
  }, []);
  if (!s) return null;

  const set = (patch: Partial<GamesSettings>) => configure(patch).catch(console.error);
  const words = { hours: t("games.time.hours"), minutes: t("games.time.minutes"), less: t("games.time.less") };
  const time = (ms: number) => playTime(ms, words);
  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  const session = s.session;
  const status: MessageKey = !s.settings.enabled
    ? "games.status.off"
    : session
      ? "games.status.playing"
      : "games.status.waiting";

  return (
    <div className="flex flex-col gap-6" data-games>
      <header className="flex flex-col gap-2">
        <h1 className="page-title">{t("nav.games")}</h1>
        <p className="page-desc">{t("games.body")}</p>
      </header>

      <section className="card flex flex-col">
        <div className="flex flex-col gap-5 p-5">
          <div className="flex items-start justify-between gap-4">
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-semibold">{t("games.on")}</span>
              <span className="max-w-[80ch] text-[12px] text-ink-3">{t("games.on.body")}</span>
            </span>
            <Toggle checked={s.settings.enabled} onChange={(enabled) => set({ enabled })} label={t("games.on")} />
          </div>
          <div className="flex items-center gap-2 text-[12px]" data-games-status={status}>
            <span className={`h-2 w-2 shrink-0 rounded-full ${session ? "bg-ok" : s.settings.enabled ? "bg-signal" : "bg-ink-3"}`} aria-hidden />
            <span className={s.settings.enabled ? "text-ink-2" : "text-ink-3"}>
              {session ? t(status, { name: session.name }) : t(status)}
            </span>
          </div>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-3" data-games-numbers>
            <Number label={t("games.now")} value={session ? `${session.name} · ${time(now - session.startedMs)}` : "—"} />
            <Number label={t("games.today")} value={time(s.todayMs)} />
            <Number label={t("games.week")} value={time(s.weekMs)} />
          </div>
          {s.suggestions.map((g) => (
            <div
              key={g.path}
              className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]"
              data-games-suggestion
            >
              <span className="flex items-center gap-2">
                <Gamepad2 size={16} className="shrink-0 text-signal" />
                {t("games.suggest", { name: g.name })}
              </span>
              <span className="flex gap-2">
                <button type="button" className="btn-secondary" onClick={() => mark(g.path, g.name, true).catch(console.error)}>
                  {t("games.suggest.yes")}
                </button>
                <button type="button" className="btn-secondary" onClick={() => mark(g.path, g.name, false).catch(console.error)}>
                  {t("games.suggest.no")}
                </button>
              </span>
            </div>
          ))}
        </div>

        <div className="flex flex-col divide-y divide-line border-t border-line">
          <GameList games={s.games} time={time} onMark={(g, game) => mark(g.key, g.name, game).catch(console.error)} launchers={s.settings.launchers} />
          <AddGame state={s} mark={(path, name, game) => mark(path, name, game).catch(console.error)} />
          <Row icon={RefreshCw} title={t("games.launchers")} body={t("games.launchers.body")} checked={s.settings.launchers} onChange={(launchers) => set({ launchers })}>
            {s.settings.launchers && (
              <div>
                <button type="button" className="btn-secondary" onClick={() => rescanGames().catch(console.error)}>
                  {t("games.rescan")}
                </button>
              </div>
            )}
          </Row>

          <Row icon={Monitor} title={t("games.screen")} body={t("games.screen.body")}>
            {!liveOn && (
              <p className="text-[12px] text-ink-3">
                {t("games.screen.liveOff")}{" "}
                <button
                  type="button"
                  className="text-signal underline-offset-2 hover:underline"
                  onClick={() => {
                    setScreenTab("live");
                    setPage("screen");
                  }}
                >
                  {t("games.screen.liveOpen")}
                </button>
              </p>
            )}
            <Switch label={t("games.startCard")} body={t("games.startCard.body")} on={s.settings.startCard} onChange={(startCard) => set({ startCard })} />
            <Switch
              label={t("games.summaryCard")}
              body={t("games.summaryCard.body", { min: SUMMARY_MIN_MS / 60_000 })}
              on={s.settings.summaryCard}
              onChange={(summaryCard) => set({ summaryCard })}
            />
            <CardStrip temperature={s.temperature} />
            {drawError && <p className="text-[12px] text-danger">{t("games.drawError")}</p>}
          </Row>

          <Lights kb={kb} usb={usb} settings={s.settings} set={set} />

          <Row
            icon={Swords}
            title={t("games.lol")}
            body={t("games.lol.body")}
            checked={s.settings.lol.enabled}
            onChange={(enabled) => set({ lol: { ...s.settings.lol, enabled } })}
          >
            {s.settings.lol.enabled && (
              <>
                {!usb && <p className="text-[12px] text-ink-3">{t("games.noUsb")}</p>}
                <Switch label={t("games.lol.bars")} body={t("games.lol.bars.body")} on={s.settings.lol.bars} onChange={(bars) => set({ lol: { ...s.settings.lol, bars } })} />
                <Switch label={t("games.lol.death")} body={t("games.lol.death.body")} on={s.settings.lol.death} onChange={(death) => set({ lol: { ...s.settings.lol, death } })} />
                <Switch label={t("games.lol.events")} body={t("games.lol.events.body")} on={s.settings.lol.events} onChange={(events) => set({ lol: { ...s.settings.lol, events } })} />
                <LolCard />
                <p className="text-[12px] text-ink-3">{t("games.lol.never")}</p>
              </>
            )}
            <p className="text-[11px] text-ink-3" data-games-riot-notice>
              {t("games.lol.legal")}
            </p>
          </Row>

          <Row icon={ShieldCheck} title={t("games.quiet")} body={t("games.quiet.body")} checked={s.settings.quiet} onChange={(quiet) => set({ quiet })}>
            {s.settings.quiet && (
              <Switch
                label={t("games.quiet.reminders")}
                body={t("games.quiet.reminders.body")}
                on={s.settings.quietReminders}
                onChange={(quietReminders) => set({ quietReminders })}
              />
            )}
          </Row>

          <HistoryRow time={time} />
        </div>
      </section>
      <p className="text-[12px] text-ink-3">{t("games.privacy")}</p>
    </div>
  );
}

function Number({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-1 rounded-lg bg-surface-2 px-3 py-2.5">
      <span className="text-[12px] text-ink-3">{label}</span>
      <span className="num truncate text-[16px] font-semibold">{value}</span>
    </div>
  );
}

/** The games found or played; launchers' games never played fold away. */
function GameList({
  games,
  time,
  onMark,
  launchers,
}: {
  games: GameEntry[];
  time: (ms: number) => string;
  onMark: (g: GameEntry, game: boolean | null) => void;
  launchers: boolean;
}) {
  const t = useT();
  const [all, setAll] = useState(false);
  const played = games.filter((g) => g.played || g.source === "mine");
  const rest = games.length - played.length;
  const shown = all ? games : played;
  return (
    <div className="flex flex-col gap-3 px-5 py-4" data-games-list>
      <span className="flex items-start gap-2.5">
        <Gamepad2 size={16} className="mt-0.5 shrink-0 text-signal" />
        <span className="flex flex-col">
          <span className="text-[13px] font-semibold">{t("games.list")}</span>
          <span className="text-[12px] text-ink-3">{t(launchers ? "games.list.body" : "games.list.bodyOwn")}</span>
        </span>
      </span>
      {shown.length === 0 ? (
        <p className="text-[12px] text-ink-3">{t("games.list.empty")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line rounded-lg border border-line">
          {shown.map((g) => (
            <li key={g.key} className="flex flex-wrap items-center gap-3 px-3 py-2 text-[13px]" data-game={g.name}>
              <span className="min-w-0 flex-1 truncate font-medium">{g.name}</span>
              <span className="rounded-md bg-surface-2 px-2 py-0.5 text-[11px] text-ink-2">{t(SOURCE_KEY[g.source])}</span>
              <span className="num w-24 text-right text-[12px] text-ink-3">{g.totalMs > 0 ? time(g.totalMs) : "—"}</span>
              <button
                type="button"
                className="rounded-md px-2 py-1 text-[12px] text-ink-3 hover:bg-surface-2 hover:text-ink"
                aria-label={`${t(g.source === "mine" ? "games.forget" : "games.notGame")}, ${g.name}`}
                onClick={() => onMark(g, g.source === "mine" ? null : false)}
              >
                {t(g.source === "mine" ? "games.forget" : "games.notGame")}
              </button>
            </li>
          ))}
        </ul>
      )}
      {rest > 0 && (
        <button type="button" className="self-start text-[12px] text-signal hover:underline" onClick={() => setAll((a) => !a)}>
          {all ? t("games.list.less") : t("games.list.all").replace("{n}", String(rest))}
        </button>
      )}
    </div>
  );
}

/** A game by hand, from the programs lately in front, and the programs
 *  said not to be games, which can be taken back. */
function AddGame({ state, mark }: { state: GamesState; mark: (path: string, name: string, game: boolean | null) => void }) {
  const t = useT();
  const [pick, setPick] = useState("");
  const notGames = state.settings.marks.filter((m) => !m.game);
  const chosen = state.programs.find((p) => p.path === pick);
  return (
    <div className="flex flex-col gap-3 px-5 py-4" data-games-add>
      <span className="flex flex-col">
        <span className="text-[13px] font-semibold">{t("games.add")}</span>
        <span className="text-[12px] text-ink-3">{t("games.add.body")}</span>
      </span>
      <div className="flex flex-wrap items-center gap-2">
        <select
          className="h-8 min-w-[220px] max-w-full rounded-lg border border-line bg-bg-raise px-2 text-[13px] outline-none focus:border-signal"
          value={pick}
          aria-label={t("games.add")}
          onChange={(e) => setPick(e.target.value)}
        >
          <option value="">{t("games.add.pick")}</option>
          {state.programs.map((p) => (
            <option key={p.path} value={p.path}>
              {p.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="btn-secondary"
          disabled={!chosen}
          onClick={() => {
            if (!chosen) return;
            mark(chosen.path, chosen.name, true);
            setPick("");
          }}
        >
          {t("games.add.go")}
        </button>
      </div>
      {notGames.length > 0 && (
        <div className="flex flex-col gap-1.5" data-games-not>
          <span className="text-[12px] text-ink-3">{t("games.notGames")}</span>
          <ul className="flex flex-col divide-y divide-line rounded-lg border border-line">
            {notGames.map((m) => (
              <li key={m.path} className="flex items-center gap-3 px-3 py-1.5 text-[13px]">
                <span className="min-w-0 flex-1 truncate">{m.name}</span>
                <button
                  type="button"
                  className="rounded-md px-2 py-1 text-[12px] text-ink-3 hover:bg-surface-2 hover:text-ink"
                  aria-label={`${t("games.undo")}, ${m.name}`}
                  onClick={() => mark(m.path, m.name, null)}
                >
                  {t("games.undo")}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** The two cards a session puts on the screen, drawn by the real card code. */
function CardStrip({ temperature }: { temperature: boolean }) {
  const t = useT();
  const locale = useApp((x) => x.locale);
  const [frames, setFrames] = useState<Uint16Array[] | null>(null);
  const draw = useEffectEvent(() => {
    const w = gameWords(t);
    const started = Date.now() - (2 * 60 + 14) * 60_000;
    const base: GameCard = {
      kind: "start",
      name: t("games.example"),
      steamApp: null,
      startedMs: started,
      endedMs: null,
      gpuAvg: null,
      gpuPeak: null,
      cpuAvg: null,
      tempPeak: null,
      todayMs: 80 * 60_000,
      lol: null,
    };
    return Promise.all([
      gameStartCard(base, null, w),
      gameSummaryCard(
        { ...base, kind: "summary", endedMs: Date.now(), gpuAvg: 78, gpuPeak: 97, cpuAvg: 34, tempPeak: temperature ? 71 : null, todayMs: 214 * 60_000 },
        w,
      ),
    ]);
  });
  useEffect(() => {
    let live = true;
    draw().then((p) => live && setFrames(p.map((c) => c.frames[0]!)), console.error);
    return () => {
      live = false;
    };
  }, [locale, temperature]);
  const box = "h-[76px] w-[135px] shrink-0 rounded-md bg-black";
  return (
    <div className="flex flex-col gap-1.5" data-games-strip>
      <span className="text-[12px] text-ink-3">{t("games.screen.example")}</span>
      <div className="flex flex-wrap gap-3">
        {[t("games.startCard"), t("games.summaryCard")].map((label, i) => (
          <figure key={label} className="flex flex-col gap-1">
            {frames?.[i] ? <FramePlayer frames={[frames[i]!]} delaysMs={[100]} className={box} /> : <div className={box} />}
            <figcaption className="text-[11px] text-ink-3">{label}</figcaption>
          </figure>
        ))}
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
        startedMs: Date.now() - 32 * 60_000,
        endedMs: Date.now(),
        gpuAvg: 61,
        gpuPeak: 88,
        cpuAvg: 27,
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

/** How the keys look while a game runs. */
function Lights({
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
  const g = kb.geometry;
  const [draft, setDraft] = useState<Partial<GamesSettings>>({});
  useEffect(() => {
    if (!Object.keys(draft).length) return;
    const id = window.setTimeout(() => {
      set(draft);
      setDraft({});
    }, 150);
    return () => window.clearTimeout(id);
  }, [draft, set]);
  const shown = { ...s, ...draft };
  const colors = useMemo(() => {
    if (!g) return null;
    const lit = new Set(shown.keys);
    const [r, gg, b] = shown.color;
    const k = shown.rest === "dim" ? 0.12 : 0;
    const rest = `rgb(${Math.round(r * k)},${Math.round(gg * k)},${Math.round(b * k)})`;
    const out: Record<number, string> = {};
    for (const key of g.keys) out[key.led] = lit.has(key.id) ? `rgb(${r},${gg},${b})` : rest;
    return out;
  }, [g, shown.keys, shown.color, shown.rest]);
  const byLed = useMemo(() => new Map(g?.keys.map((k) => [k.led, k.id]) ?? []), [g]);
  // The chosen keys, for the drawing's ring and for screen readers.
  const selected = useMemo(() => new Set(g?.keys.filter((k) => shown.keys.includes(k.id)).map((k) => k.led) ?? []), [g, shown.keys]);
  const toggleKey = (id: string) => {
    const keys = shown.keys.includes(id) ? shown.keys.filter((k) => k !== id) : [...shown.keys, id];
    set({ keys });
  };
  return (
    <Row icon={Sparkles} title={t("games.lights")} body={t("games.lights.body")}>
      <Segmented<GameLights>
        label={t("games.lights")}
        value={shown.lights}
        onChange={(lights) => set({ lights })}
        options={[
          { value: "own", label: t("games.lights.own") },
          { value: "keys", label: t("games.lights.keys") },
        ]}
      />
      {shown.lights === "keys" && (
        <>
          <div className="flex flex-wrap items-center gap-2 text-[13px]">
            <span className="text-ink-2">{t("games.keys.sets")}</span>
            {(Object.keys(GAME_KEY_SETS) as (keyof typeof GAME_KEY_SETS)[]).map((id) => (
              <button key={id} type="button" className="btn-secondary" onClick={() => set({ keys: GAME_KEY_SETS[id] })}>
                {t(`games.keys.${id}` as MessageKey)}
              </button>
            ))}
          </div>
          {g && colors && (
            <div className="flex flex-col gap-1.5">
              <span className="text-[12px] text-ink-3">{t("games.keys.pick")}</span>
              <div className="w-full max-w-[760px]" data-games-keys>
                <KeyboardView
                  geometry={g}
                  connection={kb.connection}
                  colors={colors}
                  compact
                  label={t("games.lights")}
                  tool="select"
                  selected={selected}
                  onKeyAction={(led, action) => {
                    const id = byLed.get(led);
                    if (action === "press" && id) toggleKey(id);
                  }}
                />
              </div>
            </div>
          )}
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-6">
            <div className="flex min-w-0 flex-col gap-3">
              <ColorPicker label={t("games.color")} value={toHex(shown.color)} onChange={(hex) => setDraft((d) => ({ ...d, color: fromHex(hex) }))} />
              <Slider label={t("games.brightness")} value={shown.brightness} min={1} max={5} step={1} format={(v) => `${v}/5`} onChange={(brightness) => set({ brightness })} />
            </div>
            <div className="flex min-w-0 flex-col gap-2">
              <span className="text-[13px] font-medium">{t("games.rest")}</span>
              <Segmented<GameRest>
                label={t("games.rest")}
                value={shown.rest}
                onChange={(rest) => set({ rest })}
                options={[
                  { value: "dim", label: t("games.rest.dim") },
                  { value: "dark", label: t("games.rest.dark") },
                ]}
              />
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <button type="button" className="btn-secondary" disabled={!usb} onClick={() => tryGameLights().catch(console.error)}>
              {t("games.try")}
            </button>
            {!usb && (
              <span className="flex items-center gap-1.5 text-[12px] text-ink-3">
                <Usb size={13} />
                {t("games.noUsb")}
              </span>
            )}
          </div>
          <p className="text-[12px] text-ink-3">{t("games.lights.note")}</p>
        </>
      )}
    </Row>
  );
}

/** The latest sessions, and a way to forget them. */
function HistoryRow({ time }: { time: (ms: number) => string }) {
  const t = useT();
  const s = useGames((x) => x.state);
  const clear = useGames((x) => x.clearHistory);
  const locale = useApp((x) => x.locale);
  const [asking, setAsking] = useState(false);
  if (!s) return null;
  const when = (ms: number) =>
    new Date(ms).toLocaleString(locale, { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  return (
    <Row icon={History} title={t("games.history")} body={t("games.history.body")}>
      {s.recent.length === 0 ? (
        <p className="text-[12px] text-ink-3">{t("games.history.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-1.5 text-[13px]" data-games-recent>
          {s.recent.map((r) => (
            <li key={`${r.key}-${r.startMs}`} className="flex flex-wrap items-center gap-x-3 gap-y-0.5">
              <span className="min-w-0 flex-1 truncate font-medium">{r.name}</span>
              <span className="text-[12px] text-ink-3">{when(r.startMs)}</span>
              <span className="num w-24 text-right">{time(r.endMs - r.startMs)}</span>
              <span className="num w-44 text-right text-[12px] text-ink-3">
                {[r.gpuAvg != null ? `${t("games.card.gpu")} ${percent(Math.round(r.gpuAvg))}` : null, r.tempPeak != null ? `${Math.round(r.tempPeak)} °C` : null]
                  .filter(Boolean)
                  .join(" · ") || "—"}
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap items-center gap-3">
        {asking ? (
          <>
            <span className="text-[12px] text-ink-2">{t("games.history.ask")}</span>
            <button
              type="button"
              className="btn-secondary text-danger"
              onClick={() => {
                setAsking(false);
                clear().catch(console.error);
              }}
            >
              {t("games.history.clear")}
            </button>
            <button type="button" className="btn-secondary" onClick={() => setAsking(false)}>
              {t("games.cancel")}
            </button>
          </>
        ) : (
          <button type="button" className="btn-secondary" disabled={s.recent.length === 0} onClick={() => setAsking(true)}>
            {t("games.history.clear")}
          </button>
        )}
      </div>
    </Row>
  );
}

function Switch({ label, body, on, onChange }: { label: string; body: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-start justify-between gap-3">
      <span className="flex flex-col">
        <span className="text-[13px] font-medium">{label}</span>
        <span className="text-[12px] text-ink-3">{body}</span>
      </span>
      <Toggle checked={on} onChange={onChange} label={label} />
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
