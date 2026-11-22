// Updates (roadmap stage 6), as the owner approved them in the previews of
// 2026-11-12 and 2026-11-16: the rows under Settings › About with the choice
// of what the app does by itself, the strip above the page when a new
// version is found, the card at the bottom of the sidebar, and the install
// dialog.

import { ArrowUpRight, Download, Loader2, RefreshCw, TriangleAlert } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, type SyntheticEvent } from "react";
import type { UpdateMode } from "../bindings/UpdateMode";
import type { UpdateState } from "../bindings/UpdateState";
import { intlTag, useAgo, useT, type Locale, type MessageKey, type Translate } from "../lib/i18n";
import { openLink, openWindowsPage } from "../lib/ipc";
import { useApp } from "../lib/store";
import { useUnsaved } from "../lib/unsaved";
import { megabytes, notesFor, releasePage, useUpdates } from "../lib/updates";
import { Row, secondaryButton } from "./BackupCard";

const primaryButton =
  "flex h-9 shrink-0 items-center gap-2 rounded-lg bg-signal px-3.5 text-[13px] font-semibold text-signal-ink transition-[filter] hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-40";

function Progress({ done, total }: { done: number; total: number | null }) {
  const share = total ? Math.min(1, done / total) : null;
  return (
    <span
      className="block h-1.5 w-full overflow-hidden rounded-full bg-line-strong"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={share == null ? undefined : Math.round(share * 100)}
    >
      <span
        className={`block h-full rounded-full bg-signal transition-[width] duration-200 ${share == null ? "w-1/3 animate-pulse" : ""}`}
        style={share == null ? undefined : { width: `${share * 100}%` }}
      />
    </span>
  );
}

/** One line on where the update stands, for the settings row. */
function statusLine(s: UpdateState, t: Translate, since: (ms: number) => string, locale: Locale): string {
  const current = s.current;
  const st = s.status;
  const ago = (ms: number) => (Date.now() - ms < 60_000 ? t("update.justNow") : since(ms));
  switch (st.kind) {
    case "idle":
      return s.lastCheckMs ? t("update.state.upToDate", { current, ago: ago(s.lastCheckMs) }) : t("update.state.idle", { current });
    case "checking":
      return t("update.state.checking", { current });
    case "upToDate":
      return t("update.state.upToDate", { current, ago: ago(s.lastCheckMs ?? Date.now()) });
    case "available":
      return t("update.state.available", { current, version: st.version });
    case "ready":
      return t("update.state.ready", { current, version: st.version });
    case "downloading":
      if (st.total != null && st.done >= st.total) return t("update.state.downloaded", { version: st.version });
      return t("update.state.downloading", {
        version: st.version,
        done: megabytes(st.done, locale),
        total: st.total != null ? megabytes(st.total, locale) : "?",
      });
    case "installing":
      return t("update.state.installing", { version: st.version });
    case "failed":
      return `${current} · ${t(`update.fail.${st.reason}` as MessageKey)}`;
  }
}

const MODES: UpdateMode[] = ["auto", "notify", "off"];

/** What the app does about new versions by itself: three choices, one line each. */
function ModeChoice({ mode, onChange }: { mode: UpdateMode; onChange: (m: UpdateMode) => void }) {
  const t = useT();
  return (
    <div className="border-t border-line py-4" data-update-mode={mode}>
      <div className="text-[13.5px]" id="update-mode-label">
        {t("update.mode")}
      </div>
      <div role="radiogroup" aria-labelledby="update-mode-label" className="mt-2.5 flex flex-col gap-2">
        {MODES.map((m) => {
          const on = m === mode;
          return (
            <label
              key={m}
              className={`flex cursor-pointer items-start gap-3 rounded-lg border px-3.5 py-2.5 transition-colors has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-signal/60 ${
                on ? "border-signal bg-surface" : "border-line hover:bg-surface/60"
              }`}
              data-update-mode-option={m}
            >
              <input
                type="radio"
                name="update-mode"
                value={m}
                checked={on}
                onChange={() => onChange(m)}
                className="sr-only"
              />
              <span
                aria-hidden
                className={`mt-[3px] flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border-2 ${
                  on ? "border-signal" : "border-ink-3"
                }`}
              >
                {on && <span className="h-1.5 w-1.5 rounded-full bg-signal" />}
              </span>
              <span className="flex min-w-0 flex-col gap-0.5">
                <span className="text-[13.5px]">
                  {t(`update.mode.${m}` as MessageKey)}
                  {m === "notify" && (
                    <span className="ml-2 rounded-md bg-signal/15 px-1.5 py-px text-[11px] text-signal">
                      {t("update.mode.recommended")}
                    </span>
                  )}
                </span>
                <span className="text-[12.5px] text-ink-2">{t(`update.mode.${m}.hint` as MessageKey)}</span>
              </span>
            </label>
          );
        })}
      </div>
    </div>
  );
}

/** Settings › About: the version with the update's state, and what the app does by itself. */
export function UpdateRows() {
  const t = useT();
  const ago = useAgo();
  const locale = useApp((s) => s.locale);
  const { state, check, setMode, openDialog, error } = useUpdates();
  if (!state) return null;
  // The Microsoft Store build: the Store updates it. The GitHub copy, when
  // it is installed too, now starts this one instead and can go.
  if (state.store) {
    return (
      <>
        <Row label={t("settings.version")} hint={t("update.store.hint", { current: state.current })}>
          <button type="button" className={secondaryButton} onClick={() => void openWindowsPage("store-updates")}>
            {t("update.store.open")}
            <ArrowUpRight size={14} aria-hidden />
          </button>
        </Row>
        {state.githubCopy && (
          <Row label={t("update.githubCopy.label")} hint={t("update.githubCopy.hint")}>
            <button
              type="button"
              className={secondaryButton}
              onClick={() => void openWindowsPage("installed-apps")}
              data-github-copy
            >
              {t("update.githubCopy.open")}
              <ArrowUpRight size={14} aria-hidden />
            </button>
          </Row>
        )}
      </>
    );
  }
  const st = state.status;
  return (
    <>
      <Row label={t("settings.version")} hint={statusLine(state, t, ago, locale)}>
        <div className="flex shrink-0 items-center gap-2" data-update-row={st.kind}>
          {st.kind === "available" || st.kind === "ready" ? (
            <>
              <button type="button" className={secondaryButton} onClick={() => openDialog(true)}>
                {t("update.whatsNew")}
              </button>
              <button type="button" className={primaryButton} onClick={() => openDialog(true)}>
                <Download size={14} aria-hidden />
                {t("update.installNow")}
              </button>
            </>
          ) : st.kind === "downloading" ? (
            <span className="w-40">
              <Progress done={st.done} total={st.total} />
            </span>
          ) : st.kind === "installing" ? (
            <Loader2 size={16} className="animate-spin text-ink-2" aria-hidden />
          ) : (
            <button type="button" className={secondaryButton} onClick={() => void check()} disabled={st.kind === "checking"}>
              {st.kind === "checking" ? <Loader2 size={14} className="animate-spin" aria-hidden /> : <RefreshCw size={14} aria-hidden />}
              {t(st.kind === "failed" ? "update.retry" : "update.check")}
            </button>
          )}
        </div>
      </Row>
      {error && <p className="-mt-2 pb-3 text-[12.5px] text-danger">{t("update.fail.other")}</p>}
      <ModeChoice mode={state.mode} onChange={(m) => void setMode(m)} />
    </>
  );
}

/** Above the page: a new version was found (or is downloading). */
export function UpdateStrip() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const { state, laterFor, later, openDialog } = useUpdates();
  const st = state?.status;
  const show =
    ((st?.kind === "available" || st?.kind === "ready") && laterFor !== st.version) ||
    st?.kind === "downloading" ||
    st?.kind === "installing";
  return (
    <AnimatePresence initial={false}>
      {show && st && (
        <motion.div
          key="update-strip"
          initial={{ opacity: 0, height: 0 }}
          animate={{ opacity: 1, height: "auto" }}
          exit={{ opacity: 0, height: 0 }}
          transition={{ duration: 0.2 }}
          className="overflow-hidden"
          data-update-strip={st.kind}
        >
          <div className="card mb-5 flex flex-wrap items-center gap-x-3 gap-y-2 border-signal/35 px-4 py-2.5">
            {st.kind === "available" || st.kind === "ready" ? (
              <>
                <span className="h-2 w-2 shrink-0 rounded-full bg-signal" aria-hidden />
                <p className="min-w-0 flex-1 text-[13px]">
                  {t(st.kind === "ready" ? "update.strip.ready" : "update.strip", { version: st.version })}{" "}
                  <span className="text-ink-2">{t(st.kind === "ready" ? "update.strip.ready.hint" : "update.strip.hint")}</span>
                </p>
                <button type="button" className={secondaryButton} onClick={later}>
                  {t("update.later")}
                </button>
                <button type="button" className={primaryButton} onClick={() => openDialog(true)}>
                  {t("update.installNow")}
                </button>
              </>
            ) : (
              <>
                <p className="min-w-0 flex-1 text-[13px]">
                  {t("update.strip.downloading", { version: st.version })}{" "}
                  <span className="text-ink-2">{t("update.strip.downloading.hint")}</span>
                </p>
                <span className="flex w-44 items-center gap-2">
                  {st.kind === "downloading" ? (
                    <Progress done={st.done} total={st.total} />
                  ) : (
                    <Loader2 size={15} className="animate-spin text-ink-2" aria-hidden />
                  )}
                </span>
                {st.kind === "downloading" && st.total != null && (
                  <span className="num text-[12px] text-ink-2">
                    {megabytes(st.done, locale)} / {megabytes(st.total, locale)} MB
                  </span>
                )}
              </>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/**
 * The bottom of the sidebar: a new version waits (owner's wish: "sol alt
 * menüde bir bildirim"). Opens the install dialog. Collapsed, only the icon.
 */
export function UpdateCard({ collapsed, onTip, onTipEnd }: {
  collapsed: boolean;
  onTip: (text: string) => (e: SyntheticEvent<HTMLElement>) => void;
  onTipEnd: () => void;
}) {
  const t = useT();
  const { state, openDialog } = useUpdates();
  const st = state?.status;
  if (!st || (st.kind !== "available" && st.kind !== "ready")) return null;
  const title = t(st.kind === "ready" ? "update.card.ready" : "update.card", { version: st.version });
  const sub = t(st.kind === "ready" ? "update.card.ready.sub" : "update.card.sub", { version: st.version });
  return (
    <button
      type="button"
      onClick={() => openDialog(true)}
      aria-label={collapsed ? `${title}. ${sub}` : undefined}
      onMouseEnter={onTip(title)}
      onFocus={onTip(title)}
      onMouseLeave={onTipEnd}
      onBlur={onTipEnd}
      className={`flex items-center rounded-lg border border-signal/40 text-left hover:bg-surface/60 ${
        collapsed ? "h-10 justify-center" : "gap-3 px-3.5 py-2.5"
      }`}
      data-update-card={st.kind}
    >
      <Download size={16} className="shrink-0 text-signal" aria-hidden />
      {!collapsed && (
        <span className="min-w-0 flex-1 leading-tight">
          <span className="block truncate text-[13px] font-medium">{title}</span>
          <span className="block text-[12px] leading-snug text-ink-3">{sub}</span>
        </span>
      )}
    </button>
  );
}

/** "Install now": what is new, then download and install. */
export function UpdateDialog() {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const { state, dialog, openDialog, install, cancel, check } = useUpdates();
  const unsaved = useUnsaved();
  const primary = useRef<HTMLButtonElement>(null);
  const st = state?.status;
  const open = dialog && !!st && st.kind !== "idle" && st.kind !== "upToDate" && st.kind !== "checking";
  const version = st && "version" in st ? st.version : null;
  // The notes come with the version found; keep them while it downloads.
  const found = useRef<{ version: string; notes: string | null; dateMs: number | null } | null>(null);
  if (st?.kind === "available" || st?.kind === "ready") found.current = { version: st.version, notes: st.notes, dateMs: st.dateMs };
  const notes = found.current && found.current.version === version ? notesFor(found.current.notes, [locale]) : null;
  const dateMs = found.current?.version === version ? found.current?.dateMs : null;

  useEffect(() => {
    if (open) primary.current?.focus();
  }, [open]);
  const close = () => {
    if (st?.kind === "downloading") void cancel();
    openDialog(false);
  };
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && st?.kind !== "installing") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <AnimatePresence>
      {open && st && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-[2px]"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
        >
          <motion.div
            role="dialog"
            aria-modal="true"
            aria-labelledby="update-title"
            className="card flex w-full max-w-[480px] flex-col gap-4 p-6"
            initial={{ opacity: 0, y: 8, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 4, scale: 0.98 }}
            transition={{ duration: 0.18, ease: "easeOut" }}
            data-update-dialog={st.kind}
          >
            <div className="flex flex-col gap-1">
              <h2 id="update-title" className="font-display text-[20px] leading-tight font-semibold">
                {t("update.dialog.title", { version: version ?? "" })}
              </h2>
              <p className="num text-[12.5px] text-ink-2">
                {t("update.dialog.current", { current: state.current })}
                {dateMs != null &&
                  ` · ${t("update.dialog.date", {
                    date: new Date(dateMs).toLocaleDateString(intlTag(locale), {
                      day: "numeric",
                      month: "long",
                      year: "numeric",
                    }),
                  })}`}
              </p>
            </div>

            <div className="flex flex-col gap-1.5">
              <h3 className="text-[13px] font-semibold">{t("update.dialog.notes")}</h3>
              {notes ? (
                <ul className="flex max-h-48 list-disc flex-col gap-1 overflow-y-auto pl-5 text-[13px] text-ink-2">
                  {notes.map((n, i) => (
                    <li key={i}>{n}</li>
                  ))}
                </ul>
              ) : (
                version && (
                  <button
                    type="button"
                    onClick={() => void openLink(releasePage(version)).catch(console.error)}
                    className="flex items-center gap-1 self-start text-[13px] text-signal hover:underline"
                  >
                    {t("update.dialog.notesLink")}
                    <ArrowUpRight size={13} aria-hidden />
                  </button>
                )
              )}
            </div>

            <p className="text-[12.5px] leading-relaxed text-ink-2">
              {t("update.dialog.restart")} {t("update.dialog.keyboard")}
            </p>
            {unsaved.length > 0 && (st.kind === "available" || st.kind === "ready") && (
              <p className="flex items-start gap-2 text-[12.5px] text-warn" data-update-unsaved>
                <TriangleAlert size={14} className="mt-0.5 shrink-0" aria-hidden />
                {t("update.dialog.unsaved")}
              </p>
            )}
            {st.kind === "failed" && (
              <p role="alert" className="flex items-start gap-2 text-[13px] text-danger">
                <TriangleAlert size={14} className="mt-0.5 shrink-0" aria-hidden />
                {t(`update.fail.${st.reason}` as MessageKey)}
              </p>
            )}

            {st.kind === "downloading" || st.kind === "installing" ? (
              <div className="flex flex-col gap-3">
                <div className="flex items-center gap-3">
                  {st.kind === "downloading" ? (
                    <Progress done={st.done} total={st.total} />
                  ) : (
                    <Loader2 size={16} className="animate-spin text-ink-2" aria-hidden />
                  )}
                  <span className="num shrink-0 text-[12px] text-ink-2">
                    {st.kind === "downloading"
                      ? st.total != null && st.done >= st.total
                        ? t("update.state.downloaded", { version: st.version })
                        : `${megabytes(st.done, locale)}${st.total != null ? ` / ${megabytes(st.total, locale)}` : ""} MB`
                      : t("update.state.installing", { version: st.version })}
                  </span>
                </div>
                {st.kind === "downloading" && (
                  <div className="flex justify-end">
                    <button ref={primary} type="button" className={secondaryButton} onClick={close}>
                      {t("update.cancel")}
                    </button>
                  </div>
                )}
              </div>
            ) : (
              <div className="flex items-center justify-end gap-2">
                <button
                  type="button"
                  onClick={close}
                  className="h-9 rounded-lg px-3.5 text-[13px] text-ink-2 transition-colors hover:text-ink"
                >
                  {t("update.cancel")}
                </button>
                {st.kind === "failed" ? (
                  <button ref={primary} type="button" className={primaryButton} onClick={() => void check()}>
                    <RefreshCw size={14} aria-hidden />
                    {t("update.retry")}
                  </button>
                ) : (
                  <button ref={primary} type="button" className={primaryButton} onClick={() => void install()}>
                    <Download size={14} aria-hidden />
                    {t("update.install")}
                  </button>
                )}
              </div>
            )}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
