// "Hatırlatıcılar" page (Araçlar): reminders that go off once, every day at
// a time, or every few minutes in working hours. The backend decides when
// and sweeps a wave of the reminder's colour over the keys
// (app/src-tauri/src/reminders.rs); a card goes on the screen too when the
// reminder asks for one and the live mode is on. Owner's choices
// (2026-11-11): card "saat önde", the wave, under the calendar in the
// screen order, repeating ones wait while nobody is at the computer, the
// card stays 2 minutes.

import {
  Bell,
  BellRing,
  Check,
  Droplet,
  Eye,
  Loader2,
  Monitor,
  Pencil,
  PersonStanding,
  Pill,
  Plus,
  Trash2,
  Volume2,
  Waves,
  X,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { Reminder } from "../bindings/Reminder";
import type { ReminderIcon } from "../bindings/ReminderIcon";
import type { ReminderInfo } from "../bindings/ReminderInfo";
import type { ReminderWhen } from "../bindings/ReminderWhen";
import { Segmented, Toggle } from "../components/controls";
import { tNow, useT, type MessageKey } from "../lib/i18n";
import { submitLive, useLive } from "../lib/live";
import { reminderCard, reminderWords } from "../lib/liveCards";
import {
  EVERY_CHOICES,
  ICONS,
  REMINDER_COLORS,
  REMINDER_SOURCE,
  STAY_CHOICES,
  TEMPLATES,
  WEEKDAYS,
  dayWord,
  fromTemplate,
  hhmm,
  minuteText,
  parseMinute,
  perDay,
  previewCard,
  reminderError,
  rgbCss,
  tryReminderCardBack,
  tryReminderLight,
  useReminders,
  whenText,
  type TemplateId,
} from "../lib/reminders";
import type { Prepared } from "../lib/screenImport";
import { useApp } from "../lib/store";
import { FramePlayer, Hint } from "./screen/common";

export const ICON_OF: Record<ReminderIcon, LucideIcon> = {
  bell: Bell,
  drop: Droplet,
  stand: PersonStanding,
  eye: Eye,
  pill: Pill,
};

/** Above this many uploads a day, a card reminder is warned about. */
const MANY_UPLOADS = 16;

export function Reminders({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const setPage = useApp((s) => s.setPage);
  const s = useReminders((x) => x.state);
  const drawError = useReminders((x) => x.drawError);
  const configure = useReminders((x) => x.configure);
  const liveOn = useLive((x) => !!x.state?.enabled);
  const [editing, setEditing] = useState<{ draft: Reminder | null; step: Step } | null>(null);
  const [done, setDone] = useState<string | null>(null);
  if (!s) return null;

  const usb = kb.connection === "usb" && kb.hasCommandChannel;
  const set = (patch: Parameters<typeof configure>[0]) => configure(patch).catch(console.error);
  const list = s.reminders;
  const anyCard = list.some((i) => i.reminder.enabled && i.reminder.card);
  const next = list
    .filter((i) => i.nextMs != null)
    .sort((a, b) => a.nextMs! - b.nextMs!)[0];
  const status = !s.settings.enabled
    ? t("reminder.status.off")
    : s.away && s.settings.awayWait && list.some((i) => i.reminder.enabled && i.reminder.when.kind === "every")
      ? t("reminder.status.away")
      : next
        ? `${t("reminder.status.next")} ${when(next.nextMs!, t, locale)} · ${next.reminder.title}`
        : list.length
          ? t("reminder.status.none")
          : t("reminder.status.empty");

  return (
    <div className="flex flex-col gap-6" data-reminders>
      <header className="flex flex-col gap-2">
        <div className="eyebrow">{t("reminder.eyebrow")}</div>
        <h1 className="font-display text-[40px] leading-none font-semibold tracking-[-0.035em]">{t("nav.reminders")}</h1>
        <p className="max-w-[78ch] text-[13px] text-ink-2">{t("reminder.body")}</p>
      </header>

      <section className="card flex flex-col gap-6 p-5">
        <div className="flex flex-col gap-4 rounded-2xl border border-line bg-bg-raise/50 p-5">
          <div className="flex items-start justify-between gap-4">
            <span className="flex flex-col gap-1">
              <span className="text-[14px] font-semibold">{t("reminder.on")}</span>
              <span className="max-w-[80ch] text-[11.5px] text-ink-3">{t("reminder.on.body")}</span>
            </span>
            <Toggle checked={s.settings.enabled} onChange={(enabled) => set({ enabled })} label={t("reminder.on")} />
          </div>
          <div className="flex items-center gap-2 text-[12px]" data-reminders-status>
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${s.settings.enabled && next ? "bg-ok shadow-[0_0_8px_var(--ok)]" : "bg-ink-3"}`}
              aria-hidden
            />
            <span className={s.settings.enabled ? "text-ink-2" : "text-ink-3"}>{status}</span>
          </div>
          {!usb && <Hint tone="usb">{t("reminder.noUsb")}</Hint>}
          {anyCard && !liveOn && (
            <div className="flex flex-wrap items-center gap-3">
              <Hint tone="note">{t("reminder.liveOff")}</Hint>
              <button
                type="button"
                onClick={() => setPage("screen")}
                className="h-8 rounded-lg border border-line px-3 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              >
                {t("reminder.liveOff.go")}
              </button>
            </div>
          )}
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-[12.5px] font-medium">{t("reminder.list")}</span>
          {list.length === 0 && !editing && <p className="text-[12px] text-ink-3">{t("reminder.list.empty")}</p>}
          {list.length > 0 && (
            <ul className="flex flex-col divide-y divide-line rounded-xl border border-line" data-reminder-list>
              {list.map((i) => (
                <Row
                  key={i.reminder.id}
                  info={i}
                  showing={s.showing === i.reminder.id}
                  away={s.away && s.settings.awayWait}
                  onEdit={() => {
                    setDone(null);
                    setEditing({ draft: { ...i.reminder }, step: 1 });
                  }}
                />
              ))}
            </ul>
          )}
          {done && !editing && (
            <p className="flex items-center gap-2 text-[12px] text-ok" data-reminder-done>
              <Check size={14} /> {done}
            </p>
          )}
          {editing ? (
            <Editor
              initial={editing.draft}
              startStep={editing.step}
              onCancel={() => setEditing(null)}
              onDone={(msg) => {
                setEditing(null);
                setDone(msg);
              }}
            />
          ) : (
            <div>
              <button
                type="button"
                onClick={() => {
                  setDone(null);
                  setEditing({ draft: null, step: 0 });
                }}
                className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[12.5px] font-semibold text-signal hover:bg-signal/10"
                data-reminder-add
              >
                <Plus size={14} /> {t("reminder.add")}
              </button>
            </div>
          )}
        </div>

        <div className="flex flex-col gap-3" data-reminder-settings>
          <span className="text-[12.5px] font-medium">{t("reminder.settings")}</span>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span className="text-[12px] text-ink-2">{t("reminder.stay")}</span>
            <Segmented<string>
              label={t("reminder.stay")}
              value={String(s.settings.stayMin)}
              onChange={(v) => set({ stayMin: Number(v) })}
              options={STAY_CHOICES.map((m) => ({ value: String(m), label: `${m} ${t("sc.live.m")}` }))}
            />
          </div>
          <SettingRow title={t("reminder.screen")} body={t("reminder.screen.body")}>
            <Toggle checked={s.settings.screen} onChange={(screen) => set({ screen })} label={t("reminder.screen")} />
          </SettingRow>
          <SettingRow title={t("reminder.away")} body={t("reminder.away.body")}>
            <Toggle checked={s.settings.awayWait} onChange={(awayWait) => set({ awayWait })} label={t("reminder.away")} />
          </SettingRow>
        </div>
        {drawError && <Hint tone="error">{drawError}</Hint>}
      </section>
    </div>
  );
}

/** "14:00", or "yarın 14:00" when not today. */
function when(ms: number, t: (k: MessageKey) => string, locale: string) {
  const today = new Date(ms).toDateString() === new Date().toDateString();
  return today ? hhmm(ms) : `${dayWord(ms, t, locale).toLocaleLowerCase(locale)} ${hhmm(ms)}`;
}

function SettingRow({ title, body, children }: { title: string; body: string; children: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-3 rounded-xl border border-line px-4 py-3">
      <span className="flex min-w-0 max-w-[70ch] flex-1 flex-col">
        <span className="text-[12.5px] font-medium">{title}</span>
        <span className="text-[11.5px] text-ink-3">{body}</span>
      </span>
      {children}
    </div>
  );
}

function IconDot({ icon, color, size = 32 }: { icon: ReminderIcon; color: readonly number[]; size?: number }) {
  const Icon = ICON_OF[icon];
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-full"
      style={{ width: size, height: size, background: `rgba(${color[0]},${color[1]},${color[2]},0.16)`, color: rgbCss(color) }}
    >
      <Icon size={Math.round(size / 2)} />
    </span>
  );
}

function Row({ info, showing, away, onEdit }: { info: ReminderInfo; showing: boolean; away: boolean; onEdit: () => void }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const save = useReminders((x) => x.save);
  const remove = useReminders((x) => x.remove);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const r = info.reminder;
  const past = r.when.kind === "once" && r.when.at <= Date.now();
  const nextText = showing
    ? t("reminder.next.showing")
    : !r.enabled
      ? past
        ? t("reminder.next.past")
        : t("reminder.next.off")
      : info.nextMs != null
        ? when(info.nextMs, t, locale)
        : away && r.when.kind === "every"
          ? t("reminder.next.away")
          : "—";

  return (
    <li className="flex flex-col gap-2 px-4 py-3" data-reminder={r.id}>
      <div className="flex flex-wrap items-center gap-3">
        <IconDot icon={r.icon} color={r.color} />
        <span className="flex min-w-[160px] flex-1 flex-col">
          <span className={`truncate text-[13px] font-semibold ${r.enabled ? "" : "text-ink-3"}`}>{r.title}</span>
          <span className="text-[11.5px] text-ink-3">{whenText(r.when, t, locale)}</span>
        </span>
        <span className="flex items-center gap-1.5 text-ink-3" aria-label={t("reminder.how")}>
          <Waves size={14} aria-label={t("reminder.badge.light")} />
          {r.card && <Monitor size={14} aria-label={t("reminder.badge.card")} />}
          {r.sound && <Volume2 size={14} aria-label={t("reminder.badge.sound")} />}
        </span>
        <span className={`num min-w-[84px] text-right text-[12px] ${showing ? "text-signal" : "text-ink-2"}`} data-reminder-next>
          {nextText}
        </span>
        <span className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => tryReminderLight(r.color, r.sound).catch(console.error)}
            className="h-8 rounded-lg border border-line px-2.5 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
            data-reminder-try
          >
            {t("reminder.try")}
          </button>
          <button
            type="button"
            onClick={onEdit}
            aria-label={t("reminder.edit")}
            title={t("reminder.edit")}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-ink-3 hover:bg-surface-2 hover:text-ink"
            data-reminder-edit
          >
            <Pencil size={14} />
          </button>
          <button
            type="button"
            onClick={() => setAsking(true)}
            aria-label={t("reminder.remove")}
            title={t("reminder.remove")}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-ink-3 hover:bg-danger/10 hover:text-danger"
            data-reminder-remove
          >
            <Trash2 size={14} />
          </button>
        </span>
        <Toggle
          checked={r.enabled}
          onChange={(enabled) => {
            setError(null);
            save({ ...r, enabled }).catch((e) => setError(reminderError(e, t)));
          }}
          label={r.title}
        />
      </div>
      {asking && (
        <div className="flex flex-wrap items-center gap-2 text-[12px]" data-reminder-remove-ask>
          <span className="text-ink-2">{t("reminder.remove.ask")}</span>
          <button
            type="button"
            onClick={() => remove(r.id).catch(console.error)}
            className="h-8 rounded-lg border border-danger/50 px-3 text-danger hover:bg-danger/10"
            data-reminder-remove-yes
          >
            {t("reminder.remove")}
          </button>
          <button type="button" onClick={() => setAsking(false)} className="h-8 rounded-lg border border-line px-3 text-ink-2 hover:bg-surface-2">
            {t("gauges.cancel")}
          </button>
        </div>
      )}
      {error && <p className="text-[12px] text-danger">{error}</p>}
    </li>
  );
}

// ------------------------------------------------------------------ editor

type Step = 0 | 1 | 2;

/** Adding (from a template) or changing a reminder, in steps: which one,
 *  what and when, how it reminds. Cancel on every step; the page says what
 *  was done at the end. */
function Editor({
  initial,
  startStep,
  onCancel,
  onDone,
}: {
  initial: Reminder | null;
  startStep: Step;
  onCancel: () => void;
  onDone: (msg: string) => void;
}) {
  const t = useT();
  const save = useReminders((x) => x.save);
  const [step, setStep] = useState<Step>(startStep);
  const [r, setR] = useState<Reminder | null>(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const isNew = !initial?.id;
  const steps: MessageKey[] = ["reminder.step.pick", "reminder.step.when", "reminder.step.how"];
  const patch = (p: Partial<Reminder>) => {
    setError(null);
    setR((x) => (x ? { ...x, ...p } : x));
  };

  const checkWhen = (): string | null => {
    if (!r) return null;
    if (!r.title.trim()) return t("reminder.err.title");
    const w = r.when;
    if (w.kind === "once" && w.at <= Date.now()) return t("reminder.err.passed");
    if (w.kind !== "once" && (w.days & 0x7f) === 0) return t("reminder.err.days");
    if (w.kind === "every" && w.from >= w.to) return t("reminder.err.range");
    return null;
  };

  const submit = async () => {
    if (!r) return;
    const bad = checkWhen();
    if (bad) {
      setError(bad);
      setStep(1);
      return;
    }
    setBusy(true);
    try {
      await save(r);
      onDone(`${r.title.trim()} ${isNew ? t("reminder.added") : t("reminder.saved")}`);
    } catch (e) {
      setError(reminderError(e, t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 rounded-xl border border-signal/40 bg-signal/[0.04] p-4" data-reminder-editing={step}>
      <div className="flex items-center justify-between gap-3">
        <ol className="flex flex-wrap items-center gap-2 text-[11.5px]">
          {steps.map((k, i) => (
            <li key={k} className={`flex items-center gap-1.5 ${i === step ? "font-semibold text-ink" : "text-ink-3"}`}>
              <span
                className={`num flex h-5 w-5 items-center justify-center rounded-full text-[10.5px] ${
                  i < step ? "bg-ok text-bg" : i === step ? "bg-signal text-signal-ink" : "bg-surface-2"
                }`}
              >
                {i < step ? <Check size={11} /> : i + 1}
              </span>
              {t(k)}
            </li>
          ))}
        </ol>
        <button
          type="button"
          onClick={onCancel}
          className="flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1 text-[12px] text-ink-2 hover:bg-surface-2"
          data-reminder-cancel
        >
          <X size={13} /> {t("gauges.cancel")}
        </button>
      </div>

      {step === 0 && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-2">
          {TEMPLATES.map((tpl) => (
            <button
              key={tpl.id}
              type="button"
              onClick={() => {
                setR(fromTemplate(tpl.id as TemplateId, t));
                setError(null);
                setStep(1);
              }}
              className="flex items-center gap-2.5 rounded-xl border border-line bg-bg-raise/60 px-3 py-3 text-left hover:border-signal/60 hover:bg-surface-2"
              data-reminder-template={tpl.id}
            >
              <IconDot icon={tpl.icon} color={tpl.color} size={28} />
              <span className="flex flex-col">
                <span className="text-[13px] font-medium">{t(tpl.label)}</span>
                <span className="text-[11px] text-ink-3">{tpl.card ? t("reminder.tpl.withCard") : t("reminder.tpl.lightOnly")}</span>
              </span>
            </button>
          ))}
        </div>
      )}

      {step === 1 && r && <WhenForm r={r} patch={patch} />}
      {step === 2 && r && <HowForm r={r} patch={patch} />}

      {error && (
        <p className="text-[12px] text-danger" data-reminder-error>
          {error}
        </p>
      )}

      {step > 0 && r && (
        <div className="flex flex-wrap items-center gap-2">
          {(step === 2 || (step === 1 && isNew)) && (
            <button
              type="button"
              onClick={() => {
                setError(null);
                setStep((step - 1) as Step);
              }}
              className="h-9 rounded-lg border border-line px-3 text-[12.5px] text-ink-2 hover:bg-surface-2"
              data-reminder-back
            >
              {t("reminder.back")}
            </button>
          )}
          {step === 1 ? (
            <button
              type="button"
              onClick={() => {
                const bad = checkWhen();
                if (bad) setError(bad);
                else setStep(2);
              }}
              className="h-9 rounded-lg bg-signal px-4 text-[12.5px] font-semibold text-signal-ink hover:brightness-110"
              data-reminder-forward
            >
              {t("reminder.nextStep")}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void submit()}
              disabled={busy}
              className="flex h-9 items-center gap-2 rounded-lg bg-signal px-4 text-[12.5px] font-semibold text-signal-ink hover:brightness-110"
              data-reminder-save
            >
              {busy && <Loader2 size={14} className="animate-spin" />} {t("reminder.save")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

const pad = (n: number) => String(n).padStart(2, "0");
const dateValue = (ms: number) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
const timeValue = (ms: number) => {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
/** Local "2026-11-11" and "19:30" to ms. */
function joinLocal(date: string, time: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  const minute = parseMinute(time);
  if (!m || minute == null) return null;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Math.floor(minute / 60), minute % 60).getTime();
}

const input = "num h-9 rounded-lg border border-line bg-bg-raise px-3 text-[12.5px] outline-none focus:border-signal";

function Labeled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[12px] font-medium">{label}</span>
      {children}
    </label>
  );
}

function TimeInput({ value, onChange, label, data }: { value: number; onChange: (m: number) => void; label: string; data: string }) {
  return (
    <Labeled label={label}>
      <input
        type="time"
        value={minuteText(value)}
        onChange={(e) => {
          const m = parseMinute(e.target.value);
          if (m != null) onChange(m);
        }}
        className={`${input} w-[120px]`}
        data-reminder-time={data}
      />
    </Labeled>
  );
}

function DayChips({ days, onChange }: { days: number; onChange: (d: number) => void }) {
  const t = useT();
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[12px] font-medium">{t("reminder.days")}</span>
      <div className="flex flex-wrap gap-1" role="group" aria-label={t("reminder.days")}>
        {[0, 1, 2, 3, 4, 5, 6].map((i) => {
          const on = (days & (1 << i)) !== 0;
          return (
            <button
              key={i}
              type="button"
              aria-pressed={on}
              onClick={() => onChange(days ^ (1 << i))}
              className={`h-8 min-w-[44px] rounded-lg border px-2 text-[12px] ${
                on ? "border-signal/60 bg-signal/15 text-ink" : "border-line text-ink-3 hover:bg-surface-2"
              }`}
              data-reminder-day={i}
            >
              {t(`reminder.day.${i}` as MessageKey)}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** What to remind of, and when. */
function WhenForm({ r, patch }: { r: Reminder; patch: (p: Partial<Reminder>) => void }) {
  const t = useT();
  const w = r.when;
  const setWhen = (when: ReminderWhen) => patch({ when });
  const switchKind = (kind: ReminderWhen["kind"]) => {
    if (kind === w.kind) return;
    const minute = w.kind === "daily" ? w.minute : w.kind === "every" ? w.from : (() => {
      const d = new Date(w.at);
      return d.getHours() * 60 + d.getMinutes();
    })();
    const days = w.kind === "once" ? WEEKDAYS : w.days;
    if (kind === "once") {
      const d = new Date();
      d.setHours(0, 0, 0, 0);
      let at = d.getTime() + minute * 60_000;
      if (at <= Date.now()) at += 86_400_000;
      setWhen({ kind: "once", at });
    } else if (kind === "daily") setWhen({ kind: "daily", minute, days });
    else setWhen({ kind: "every", every: 60, from: 540, to: 1080, days });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
        <Labeled label={t("reminder.title")}>
          <input
            type="text"
            value={r.title}
            maxLength={40}
            placeholder={t("reminder.title.placeholder")}
            onChange={(e) => patch({ title: e.target.value })}
            className={input.replace("num ", "")}
            data-reminder-title
          />
        </Labeled>
        <Labeled label={t("reminder.note")}>
          <input
            type="text"
            value={r.note}
            maxLength={60}
            placeholder={t("reminder.note.placeholder")}
            onChange={(e) => patch({ note: e.target.value })}
            className={input.replace("num ", "")}
            data-reminder-note
          />
        </Labeled>
      </div>
      <div className="flex flex-col items-start gap-1.5">
        <span className="text-[12px] font-medium">{t("reminder.kind")}</span>
        <Segmented<ReminderWhen["kind"]>
          label={t("reminder.kind")}
          value={w.kind}
          onChange={switchKind}
          options={[
            { value: "once", label: t("reminder.kind.once") },
            { value: "daily", label: t("reminder.kind.daily") },
            { value: "every", label: t("reminder.kind.every") },
          ]}
        />
      </div>
      {w.kind === "once" && (
        <div className="flex flex-wrap gap-3">
          <Labeled label={t("reminder.date")}>
            <input
              type="date"
              value={dateValue(w.at)}
              onChange={(e) => {
                const at = joinLocal(e.target.value, timeValue(w.at));
                if (at != null) setWhen({ kind: "once", at });
              }}
              className={`${input} w-[160px]`}
              data-reminder-date
            />
          </Labeled>
          <Labeled label={t("reminder.time")}>
            <input
              type="time"
              value={timeValue(w.at)}
              onChange={(e) => {
                const at = joinLocal(dateValue(w.at), e.target.value);
                if (at != null) setWhen({ kind: "once", at });
              }}
              className={`${input} w-[120px]`}
              data-reminder-time="once"
            />
          </Labeled>
        </div>
      )}
      {w.kind === "daily" && (
        <div className="flex flex-wrap items-end gap-4">
          <TimeInput value={w.minute} onChange={(minute) => setWhen({ ...w, minute })} label={t("reminder.time")} data="daily" />
          <DayChips days={w.days} onChange={(days) => setWhen({ ...w, days })} />
        </div>
      )}
      {w.kind === "every" && (
        <div className="flex flex-col gap-3">
          <div className="flex flex-col items-start gap-1.5">
            <span className="text-[12px] font-medium">{t("reminder.every")}</span>
            <Segmented<string>
              label={t("reminder.every")}
              value={String(w.every)}
              onChange={(v) => setWhen({ ...w, every: Number(v) })}
              options={EVERY_CHOICES.map((m) => ({ value: String(m), label: `${m} ${t("sc.live.m")}` }))}
            />
          </div>
          <div className="flex flex-wrap items-end gap-4">
            <TimeInput value={w.from} onChange={(from) => setWhen({ ...w, from })} label={t("reminder.from")} data="from" />
            <TimeInput value={w.to} onChange={(to) => setWhen({ ...w, to })} label={t("reminder.to")} data="to" />
            <DayChips days={w.days} onChange={(days) => setWhen({ ...w, days })} />
          </div>
          <p className="max-w-[80ch] text-[11.5px] text-ink-3">{t("reminder.every.hint")}</p>
        </div>
      )}
    </div>
  );
}

/** How it reminds: picture and colour, the wave (always), the card, the sound. */
function HowForm({ r, patch }: { r: Reminder; patch: (p: Partial<Reminder>) => void }) {
  const t = useT();
  const stay = useReminders((x) => x.state?.settings.stayMin ?? 2);
  const uploads = perDay(r.when) * 2;
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-6">
        <div className="flex flex-col gap-1.5">
          <span className="text-[12px] font-medium">{t("reminder.icon")}</span>
          <div className="flex gap-1" role="radiogroup" aria-label={t("reminder.icon")}>
            {ICONS.map((icon) => {
              const Icon = ICON_OF[icon];
              const on = icon === r.icon;
              return (
                <button
                  key={icon}
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={icon}
                  onClick={() => patch({ icon })}
                  className={`flex h-9 w-9 items-center justify-center rounded-lg border ${
                    on ? "border-signal/60 bg-signal/15 text-ink" : "border-line text-ink-3 hover:bg-surface-2"
                  }`}
                  data-reminder-icon={icon}
                >
                  <Icon size={16} />
                </button>
              );
            })}
          </div>
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-[12px] font-medium">{t("reminder.color")}</span>
          <div className="flex gap-1.5" role="radiogroup" aria-label={t("reminder.color")}>
            {REMINDER_COLORS.map((c) => {
              const on = c.every((v, i) => v === r.color[i]);
              return (
                <button
                  key={c.join()}
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={rgbCss(c)}
                  onClick={() => patch({ color: [...c] })}
                  className="h-9 w-9 rounded-lg border border-black/20"
                  style={{ background: rgbCss(c), boxShadow: on ? `0 0 0 2px var(--surface), 0 0 0 4px ${rgbCss(c)}` : undefined }}
                />
              );
            })}
          </div>
        </div>
      </div>

      <SettingRow title={t("reminder.light")} body={t("reminder.light.body")}>
        <button
          type="button"
          onClick={() => tryReminderLight(r.color, r.sound).catch(console.error)}
          className="h-8 rounded-lg border border-line px-3 text-[12px] text-ink-2 hover:bg-surface-2 hover:text-ink"
          data-reminder-try-light
        >
          {t("reminder.try")}
        </button>
      </SettingRow>
      <SettingRow title={t("reminder.card")} body={`${t("reminder.card.body.a")} ${stay} ${t("sc.live.m")} ${t("reminder.card.body.b")}`}>
        <Toggle checked={r.card} onChange={(card) => patch({ card })} label={t("reminder.card")} />
      </SettingRow>
      {r.card && (
        <p className={`text-[12px] ${uploads > MANY_UPLOADS ? "text-warn" : "text-ink-3"}`} data-reminder-cost>
          {t("reminder.card.cost.a")} {uploads} {t("reminder.card.cost.b")}
          {uploads > MANY_UPLOADS ? ` ${t("reminder.card.many")}` : ""}
        </p>
      )}
      <SettingRow title={t("reminder.sound")} body={t("reminder.sound.body")}>
        <Toggle checked={r.sound} onChange={(sound) => patch({ sound })} label={t("reminder.sound")} />
      </SettingRow>
      {r.card && <CardPreview r={r} />}
    </div>
  );
}

/** The card as the screen will show it (no upload), and a try on the
 *  keyboard: the card goes now and the own picture comes back after a
 *  minute (as for the calendar card). */
function CardPreview({ r }: { r: Reminder }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const liveOn = useLive((s) => !!s.state?.enabled);
  const [card, setCard] = useState<Prepared | null>(null);
  const [sent, setSent] = useState<"idle" | "sending" | "sent" | "failed">("idle");
  // Draw again only when what the card says changes.
  const key = JSON.stringify([r.title, r.note, r.icon, r.color, r.when, locale]);
  const target = useMemo(() => previewCard(r), [key]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    let off = false;
    reminderCard(target, reminderWords(tNow, locale))
      .then((p) => !off && setCard(p))
      .catch(console.error);
    return () => {
      off = true;
    };
  }, [target, locale]);

  const tryOnKeyboard = async () => {
    if (!card) return;
    setSent("sending");
    try {
      useLive.setState({ state: await submitLive(card, REMINDER_SOURCE) });
      await tryReminderCardBack();
      setSent("sent");
    } catch (e) {
      console.error(e);
      setSent("failed");
    }
  };

  return (
    <div className="flex flex-col gap-2" data-reminder-preview>
      <span className="text-[12.5px] font-medium">{t("reminder.preview")}</span>
      <div className="flex flex-wrap items-start gap-4">
        <div className="overflow-hidden rounded-lg border border-line bg-black">
          {card ? <FramePlayer frames={card.frames} delaysMs={card.delaysMs} className="block h-[135px] w-[240px]" /> : <div className="h-[135px] w-[240px]" />}
        </div>
        <div className="flex min-w-[220px] flex-1 flex-col gap-2">
          <span className="text-[11.5px] text-ink-3">{t("reminder.preview.tryBody")}</span>
          {liveOn ? (
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => void tryOnKeyboard()}
                className="flex h-9 items-center gap-2 rounded-lg border border-signal/50 px-3 text-[12.5px] font-semibold text-signal hover:bg-signal/10"
                data-reminder-try-card
              >
                {sent === "sending" ? <Loader2 size={14} className="animate-spin" /> : <BellRing size={14} />} {t("reminder.preview.try")}
              </button>
              {sent === "sent" && <span className="text-[12px] text-ok">{t("reminder.preview.sent")}</span>}
              {sent === "failed" && <span className="text-[12px] text-danger">{t("reminder.preview.failed")}</span>}
            </div>
          ) : (
            <span className="text-[12px] text-ink-3">{t("reminder.preview.needsLive")}</span>
          )}
        </div>
      </div>
    </div>
  );
}
