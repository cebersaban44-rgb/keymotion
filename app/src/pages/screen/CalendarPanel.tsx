// The calendar's place in "Ekranda ne görünür?" (Canlı ekran): its status,
// the meetings ahead, the calendars added (Google, Outlook, Apple iCloud or
// any published address), when the card comes and goes, and the reminder
// key. The backend reads the calendars and decides; this sets it up.

import { AlertTriangle, CalendarDays, Check, Loader2, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { CalendarProvider } from "../../bindings/CalendarProvider";
import type { CalendarState } from "../../bindings/CalendarState";
import type { CalendarUntil } from "../../bindings/CalendarUntil";
import type { LiveState } from "../../bindings/LiveState";
import { Segmented, Toggle } from "../../components/controls";
import {
  CALENDAR_SOURCE,
  LEAD_CHOICES,
  addressOk,
  calendarColor,
  hhmm,
  linkProblem,
  previewCard,
  providerOf,
  tryCardBack,
  tryReminder,
  useCalendar,
} from "../../lib/calendar";
import { calendarCard, calendarWords } from "../../lib/liveCards";
import { submitLive, useLive } from "../../lib/live";
import type { Prepared } from "../../lib/screenImport";
import { useApp } from "../../lib/store";
import { tNow, useT, type MessageKey } from "../../lib/i18n";
import { FramePlayer, Hint } from "./common";
import { friendlyError } from "../../lib/errors";

const PROVIDERS: { id: CalendarProvider; label: MessageKey; steps: MessageKey[]; note?: MessageKey }[] = [
  { id: "google", label: "calendar.p.google", steps: ["calendar.help.google.1", "calendar.help.google.2", "calendar.help.google.3", "calendar.help.google.4"], note: "calendar.help.google.note" },
  { id: "outlook", label: "calendar.p.outlook", steps: ["calendar.help.outlook.1", "calendar.help.outlook.2", "calendar.help.outlook.3"] },
  { id: "apple", label: "calendar.p.apple", steps: ["calendar.help.apple.1", "calendar.help.apple.2", "calendar.help.apple.3"], note: "calendar.help.apple.note" },
  { id: "other", label: "calendar.p.other", steps: ["calendar.help.other.1"] },
];

const providerLabel = (p: CalendarProvider): MessageKey => PROVIDERS.find((x) => x.id === p)?.label ?? "calendar.p.other";

/** The backend's read errors in the UI language (Windows gives its own
 *  network messages in the system language already). */
function errText(e: string, t: (k: MessageKey) => string) {
  if (e === "not a calendar") return t("calendar.err.notCal");
  if (e === "web page") return t("calendar.err.webPage");
  const http = /^http (\d{3})$/.exec(e);
  if (http) {
    const code = Number(http[1]);
    if (code === 404) return t("calendar.err.notFound");
    if (code === 401 || code === 403) return t("calendar.err.denied");
    return t("calendar.err.http");
  }
  if (e === "calendar too large") return t("calendar.err.large");
  return friendlyError(e, t, true);
}

/** The calendar layer's status pill. */
export function calendarStatus(live: LiveState, c: CalendarState | null, onScreen: boolean): MessageKey {
  if (!live.enabled) return "sc.live.st.liveOff";
  if (!c || c.sources.length === 0) return "calendar.st.none";
  if (!c.settings.screen) return "sc.live.st.cardsOff";
  if (live.pending?.source === CALENDAR_SOURCE && live.nextTokenMs != null) return "sc.live.st.waitBudget";
  if (onScreen) return "sc.live.st.showing";
  if (c.showing) return "calendar.st.due";
  if (c.nextCardMs != null) return "calendar.st.next";
  return "calendar.st.quiet";
}

/** What goes under the calendar layer. */
export function CalendarBody() {
  const t = useT();
  const c = useCalendar((x) => x.state);
  const drawError = useCalendar((x) => x.drawError);
  const configure = useCalendar((x) => x.configure);
  const refresh = useCalendar((x) => x.refresh);
  const [adding, setAdding] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  if (!c) return null;
  const set = (patch: Parameters<typeof configure>[0]) => configure(patch).catch(console.error);
  const empty = c.sources.length === 0;

  return (
    <div className="flex flex-col gap-4" data-calendar>
      {!empty && <Upcoming c={c} />}
      {!empty && <CardPreview c={c} />}

      <div className="flex flex-col gap-2">
        <span className="text-[13px] font-medium">{t("calendar.sources")}</span>
        {empty && !adding && <p className="text-[12px] text-ink-3">{t("calendar.sources.empty")}</p>}
        {c.sources.length > 0 && (
          <ul className="flex flex-col divide-y divide-line rounded-lg border border-line" data-calendar-sources>
            {c.sources.map((s) => (
              <SourceRow key={s.id} s={s} />
            ))}
          </ul>
        )}
        {done && !adding && (
          <p className="flex items-center gap-2 text-[12px] text-ok" data-calendar-done>
            <Check size={14} /> {done}
          </p>
        )}
        {adding ? (
          <AddCalendar
            onCancel={() => setAdding(false)}
            onDone={(msg) => {
              setAdding(false);
              setDone(msg);
            }}
          />
        ) : (
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => {
                setDone(null);
                setAdding(true);
              }}
              className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
              data-calendar-add
            >
              <Plus size={14} /> {t("calendar.add")}
            </button>
            {!empty && (
              <button
                type="button"
                onClick={() => refresh().catch(console.error)}
                className="flex h-9 items-center gap-2 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
                data-calendar-refresh
              >
                {c.reading ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />} {t("calendar.refresh")}
              </button>
            )}
          </div>
        )}
      </div>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-x-6 gap-y-3">
        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-medium">{t("calendar.lead")}</span>
          <Segmented<string>
            label={t("calendar.lead")}
            value={String(c.settings.leadMin)}
            onChange={(v) => set({ leadMin: Number(v) })}
            options={LEAD_CHOICES.map((m) => ({ value: String(m), label: `${m} ${t("sc.live.m")}` }))}
          />
        </div>
        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-medium">{t("calendar.until")}</span>
          <Segmented<CalendarUntil>
            label={t("calendar.until")}
            value={c.settings.until}
            onChange={(until) => set({ until })}
            options={[
              { value: "start", label: t("calendar.until.start") },
              { value: "shortly", label: t("calendar.until.shortly") },
              { value: "end", label: t("calendar.until.end") },
            ]}
          />
        </div>
      </div>

      <div className="flex flex-wrap items-start justify-between gap-3 rounded-lg border border-line px-4 py-3" data-calendar-reminder>
        <span className="flex max-w-[70ch] flex-col">
          <span className="text-[13px] font-medium">{t("calendar.reminder")}</span>
          <span className="text-[12px] text-ink-3">{t("calendar.reminder.body")}</span>
        </span>
        <span className="flex items-center gap-2">
          {c.settings.reminder && (
            <button
              type="button"
              onClick={() => tryReminder().catch(console.error)}
              className="h-8 rounded-lg border border-line px-3 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              data-calendar-try
            >
              {t("calendar.reminder.try")}
            </button>
          )}
          <Toggle checked={c.settings.reminder} onChange={(reminder) => set({ reminder })} label={t("calendar.reminder")} />
        </span>
      </div>
      {drawError && <Hint tone="error">{t("err.draw")}</Hint>}
    </div>
  );
}

/** The meetings ahead, the one whose card is due marked. */
function Upcoming({ c }: { c: CalendarState }) {
  const t = useT();
  const list = c.upcoming.slice(0, 5);
  const showingKey = c.showing ? `${c.showing.source}@${c.showing.startMs}` : null;
  const next = c.upcoming.find((e) => !e.allDay && c.nextCardMs != null && e.startMs - c.settings.leadMin * 60_000 === c.nextCardMs);
  return (
    <div className="flex flex-col gap-2" data-calendar-upcoming>
      <span className="text-[13px] font-medium">{t("calendar.upcoming")}</span>
      {list.length === 0 ? (
        <p className="text-[12px] text-ink-3">{t("calendar.upcoming.none")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {list.map((e) => {
            const due = `${e.source}@${e.startMs}` === showingKey;
            return (
              <li
                key={`${e.source}@${e.startMs}@${e.title}`}
                className={`flex items-center gap-3 rounded-lg px-3 py-1.5 text-[13px] ${due ? "bg-signal/10" : ""}`}
              >
                <span className="h-2.5 w-2.5 shrink-0 rounded-full" style={{ background: calendarColor(e.color) }} aria-hidden />
                <span className="num w-[92px] shrink-0 text-ink-2">
                  {e.allDay ? t("calendar.allDay") : `${hhmm(e.startMs)}–${hhmm(e.endMs)}`}
                </span>
                <span className="min-w-0 flex-1 truncate">
                  {e.title || "—"}
                  {e.location && <span className="text-ink-3"> · {e.location}</span>}
                </span>
                {due && <span className="shrink-0 text-[11px] font-semibold text-signal">{t("calendar.cardTime")}</span>}
              </li>
            );
          })}
        </ul>
      )}
      {next && c.nextCardMs != null && (
        <p className="text-[12px] text-ink-3" data-calendar-next>
          {t("calendar.next", { time: hhmm(c.nextCardMs), start: hhmm(next.startMs), title: next.title })}
        </p>
      )}
      {c.upcoming.some((e) => e.allDay) && <p className="text-[12px] text-ink-3">{t("calendar.allDay.note")}</p>}
    </div>
  );
}

/**
 * The card the screen will show, drawn here (no upload), and a try on the
 * keyboard: the card goes now and the own picture comes back after a
 * minute (owner's request, 2026-11-11: the card only showed before a
 * meeting, so its look could not be checked).
 */
function CardPreview({ c }: { c: CalendarState }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const liveOn = useLive((s) => !!s.state?.enabled);
  const [card, setCard] = useState<Prepared | null>(null);
  const [sent, setSent] = useState<"idle" | "sending" | "sent" | "failed">("idle");
  const { card: target, sample } = previewCard(c, {
    title: t("calendar.preview.sampleTitle"),
    location: t("calendar.preview.sampleRoom"),
  });
  const name = c.sources.find((s) => s.id === target.event.source)?.name ?? "";
  // Draw again only when what the card says changes.
  const key = JSON.stringify([target.event.title, target.event.location, target.event.startMs, target.event.endMs, target.event.color, target.next?.title, target.next?.startMs, name, locale]);
  const stable = useMemo(() => target, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    let off = false;
    calendarCard(stable, name, calendarWords(tNow, locale))
      .then((p) => !off && setCard(p))
      .catch(console.error);
    return () => {
      off = true;
    };
  }, [stable, name, locale]);

  const tryOnKeyboard = async () => {
    if (!card) return;
    setSent("sending");
    try {
      useLive.setState({ state: await submitLive(card, CALENDAR_SOURCE) });
      await tryCardBack();
      setSent("sent");
    } catch (e) {
      console.error(e);
      setSent("failed");
    }
  };

  return (
    <div className="flex flex-col gap-2" data-calendar-preview>
      <span className="text-[13px] font-medium">{t("calendar.preview")}</span>
      <div className="flex flex-wrap items-start gap-4">
        <div className="overflow-hidden rounded-lg border border-line bg-black">
          {card ? <FramePlayer frames={card.frames} delaysMs={card.delaysMs} className="block h-[135px] w-[240px]" /> : <div className="h-[135px] w-[240px]" />}
        </div>
        <div className="flex min-w-[220px] flex-1 flex-col gap-2">
          <span className="text-[12px] text-ink-2">
            {sample ? t("calendar.preview.sample") : t("calendar.preview.next", { time: hhmm(target.event.startMs), title: target.event.title })}
          </span>
          <span className="text-[12px] text-ink-3">{t("calendar.preview.tryBody")}</span>
          {liveOn ? (
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => void tryOnKeyboard()}
                className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
                data-calendar-try-card
              >
                {sent === "sending" ? <Loader2 size={14} className="animate-spin" /> : <CalendarDays size={14} />} {t("calendar.preview.try")}
              </button>
              {sent === "sent" && <span className="text-[12px] text-ok">{t("calendar.preview.sent")}</span>}
              {sent === "failed" && <span className="text-[12px] text-danger">{t("calendar.preview.failed")}</span>}
            </div>
          ) : (
            <span className="text-[12px] text-ink-3">{t("calendar.preview.needsLive")}</span>
          )}
        </div>
      </div>
    </div>
  );
}

function SourceRow({ s }: { s: CalendarState["sources"][number] }) {
  const t = useT();
  const update = useCalendar((x) => x.update);
  const remove = useCalendar((x) => x.remove);
  const [asking, setAsking] = useState(false);
  return (
    <li className="flex flex-col gap-2 px-3 py-2.5" data-calendar-source={s.id}>
      <div className="flex items-center gap-3">
        <span className="h-3 w-3 shrink-0 rounded-full" style={{ background: calendarColor(s.color) }} aria-hidden />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-[13px] font-medium">{s.name}</span>
          <span className="truncate text-[12px] text-ink-3">
            {t(providerLabel(s.provider))} · {s.host}
          </span>
        </span>
        <span className="hidden shrink-0 text-[12px] sm:block">
          {s.error ? (
            <span className="flex items-center gap-1 text-warn">
              <AlertTriangle size={12} /> {t("calendar.read.failed")}
            </span>
          ) : s.lastOkMs != null ? (
            <span className="text-ink-3">
              {t("calendar.read.at", { time: hhmm(s.lastOkMs) })} · {t("calendar.read.events", { count: s.events })}
            </span>
          ) : (
            <span className="flex items-center gap-1 text-ink-3">
              <Loader2 size={12} className="animate-spin" /> {t("calendar.read.reading")}
            </span>
          )}
        </span>
        <Toggle checked={s.enabled} onChange={(enabled) => update(s.id, { enabled }).catch(console.error)} label={s.name} />
        <button
          type="button"
          onClick={() => setAsking(true)}
          title={t("calendar.remove")}
          aria-label={t("calendar.remove")}
          className="rounded-lg p-1.5 text-ink-3 hover:bg-surface-2 hover:text-danger"
        >
          <Trash2 size={14} />
        </button>
      </div>
      {s.error && <p className="text-[12px] text-warn">{errText(s.error, t)}</p>}
      {asking && (
        <div className="flex flex-wrap items-center gap-2 text-[12px]">
          <span className="text-ink-2">{t("calendar.remove.ask")}</span>
          <button
            type="button"
            onClick={() => remove(s.id).catch(console.error)}
            className="rounded-lg border border-danger/50 px-2.5 py-1 font-semibold text-danger hover:bg-danger/10"
          >
            {t("calendar.remove.yes")}
          </button>
          <button type="button" onClick={() => setAsking(false)} className="rounded-lg border border-line px-2.5 py-1 text-ink-2 hover:bg-surface-2">
            {t("gauges.cancel")}
          </button>
        </div>
      )}
    </li>
  );
}

type Step = { at: "pick" } | { at: "paste"; provider: CalendarProvider } | { at: "reading"; id: string; provider: CalendarProvider };

/** Adding a calendar in steps: which one, its address, reading it. */
function AddCalendar({ onCancel, onDone }: { onCancel: () => void; onDone: (msg: string) => void }) {
  const t = useT();
  const add = useCalendar((x) => x.add);
  const remove = useCalendar((x) => x.remove);
  const c = useCalendar((x) => x.state);
  const [step, setStep] = useState<Step>({ at: "pick" });
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const provider = step.at === "pick" ? null : PROVIDERS.find((p) => p.id === step.provider)!;
  const reading = step.at === "reading" ? c?.sources.find((s) => s.id === step.id) : undefined;

  // The read ended: done (back to the start) or failed (stay, say why).
  useEffect(() => {
    if (step.at !== "reading" || !reading) return;
    if (reading.lastOkMs != null && !reading.error) {
      onDone(t("calendar.added", { name: reading.name, events: t("calendar.read.events", { count: reading.events }) }));
    }
  }, [step, reading, onDone, t]);

  const steps: MessageKey[] = ["calendar.step.pick", "calendar.step.paste", "calendar.step.read"];
  const index = step.at === "pick" ? 0 : step.at === "paste" ? 1 : 2;

  const problem = url && addressOk(url) ? linkProblem(url) : null;

  const submit = async () => {
    if (!provider) return;
    if (!addressOk(url)) {
      setError(t("calendar.err.address"));
      return;
    }
    if (problem?.blocks) {
      setError(t(problem.key));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const before = new Set(c?.sources.map((s) => s.id));
      const state = await add(url.trim(), name.trim() || t(provider.label));
      const added = state.sources.find((s) => !before.has(s.id));
      if (added) setStep({ at: "reading", id: added.id, provider: provider.id });
    } catch (e) {
      const msg = String(e);
      setError(msg.includes("already") ? t("calendar.err.already") : msg.includes("too many") ? t("calendar.err.many") : t("calendar.err.address"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 rounded-lg border border-signal/40 bg-signal/[0.04] p-4" data-calendar-adding={step.at}>
      <div className="flex items-center justify-between gap-3">
        <ol className="flex flex-wrap items-center gap-2 text-[12px]">
          {steps.map((k, i) => (
            <li key={k} className={`flex items-center gap-1.5 ${i === index ? "font-semibold text-ink" : "text-ink-3"}`}>
              <span
                className={`num flex h-5 w-5 items-center justify-center rounded-full text-[11px] ${
                  i < index ? "bg-ok text-bg" : i === index ? "bg-signal text-signal-ink" : "bg-surface-2"
                }`}
              >
                {i < index ? <Check size={11} /> : i + 1}
              </span>
              {t(k)}
            </li>
          ))}
        </ol>
        <button
          type="button"
          onClick={onCancel}
          className="flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1 text-[12px] text-ink-2 hover:bg-surface-2"
          data-calendar-cancel
        >
          <X size={13} /> {t("gauges.cancel")}
        </button>
      </div>

      {step.at === "pick" && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-2">
          {PROVIDERS.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => {
                setStep({ at: "paste", provider: p.id });
                setName(t(p.label));
                setError(null);
              }}
              className="flex items-center gap-2 rounded-lg border border-line bg-bg-raise/60 px-3 py-3 text-left text-[13px] font-medium hover:border-signal/60 hover:bg-surface-2"
              data-calendar-provider={p.id}
            >
              <CalendarDays size={16} className="text-signal" /> {t(p.label)}
            </button>
          ))}
        </div>
      )}

      {step.at === "paste" && provider && (
        <div className="flex flex-col gap-3">
          <ol className="flex list-decimal flex-col gap-1 pl-5 text-[12px] text-ink-2">
            {provider.steps.map((k) => (
              <li key={k}>{t(k)}</li>
            ))}
          </ol>
          {provider.note && <Hint tone="warn">{t(provider.note)}</Hint>}
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px] font-medium">{t("calendar.address")}</span>
            <input
              type="url"
              value={url}
              onChange={(e) => {
                setUrl(e.target.value);
                setError(null);
              }}
              placeholder={t(`calendar.placeholder.${provider.id}` as MessageKey)}
              spellCheck={false}
              autoComplete="off"
              className="num h-9 rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal"
              data-calendar-url
            />
          </label>
          {problem && (
            <p className={`text-[12px] ${problem.blocks ? "text-danger" : "text-warn"}`} data-calendar-link-problem>
              {t(problem.key)}
            </p>
          )}
          {url && addressOk(url) && !problem && providerOf(url) !== provider.id && provider.id !== "other" && (
            <p className="text-[12px] text-ink-3">{t("calendar.otherHost")}</p>
          )}
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px] font-medium">{t("calendar.name")}</span>
            <input
              type="text"
              value={name}
              maxLength={40}
              onChange={(e) => setName(e.target.value)}
              className="h-9 rounded-lg border border-line bg-bg-raise px-3 text-[13px] outline-none focus:border-signal"
              data-calendar-name
            />
          </label>
          <p className="text-[12px] text-ink-3">{t("calendar.private")}</p>
          {error && <p className="text-[12px] text-danger" data-calendar-error>{error}</p>}
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void submit()}
              className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink"
              data-calendar-submit
            >
              {busy ? <Loader2 size={14} className="animate-spin" /> : <Plus size={14} />} {t("calendar.add.do")}
            </button>
            <button
              type="button"
              onClick={() => setStep({ at: "pick" })}
              className="h-9 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2"
            >
              {t("calendar.back")}
            </button>
          </div>
        </div>
      )}

      {step.at === "reading" && (
        <div className="flex flex-col gap-3" data-calendar-reading>
          {reading?.error ? (
            <>
              <p className="flex items-center gap-2 text-[13px] text-warn">
                <AlertTriangle size={14} /> {t("calendar.read.failedLong")}
              </p>
              <p className="text-[12px] text-ink-2">{errText(reading.error, t)}</p>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => {
                    remove(step.id).catch(console.error);
                    setStep({ at: "paste", provider: step.provider });
                  }}
                  className="h-9 rounded-lg border border-signal/50 px-3 text-[13px] font-semibold text-signal hover:bg-signal/10"
                  data-calendar-fix
                >
                  {t("calendar.fix")}
                </button>
                <button type="button" onClick={() => onDone(t("calendar.addedFailing", { name: reading.name }))} className="h-9 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2">
                  {t("calendar.keep")}
                </button>
              </div>
            </>
          ) : (
            <p className="flex items-center gap-2 text-[13px] text-ink-2">
              <Loader2 size={14} className="animate-spin" /> {t("calendar.read.reading")}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
