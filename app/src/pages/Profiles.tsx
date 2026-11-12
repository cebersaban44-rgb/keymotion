// Profiller: named sets of lighting, key mapping and macros (and, when
// asked, a screen picture) the user puts on the keyboard in one go
// (profiles.rs has the rules). Each action is a short inline flow under its
// row: what will happen, "Vazgeç" and the action; when it ends the list is
// back as it was with a one-line result at the top.

import { AlertTriangle, Check, Circle, CircleCheck, Copy, Ellipsis, FileDown, FolderOpen, Loader2, Pencil, Trash2 } from "lucide-react";
import { useEffect, useEffectEvent, useId, useRef, useState, type ChangeEvent, type FormEvent, type KeyboardEvent, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ProfileInfo } from "../bindings/ProfileInfo";
import type { ProfilePart } from "../bindings/ProfilePart";
import type { ProfileParts } from "../bindings/ProfileParts";
import type { ProfilesState } from "../bindings/ProfilesState";
import { friendlyError } from "../lib/errors";
import { percent, useT, type Locale, type Translate } from "../lib/i18n";
import {
  createProfile,
  deleteProfile,
  duplicateProfile,
  exportProfile,
  importProfile,
  renameProfile,
  revealExport,
  updateProfile,
} from "../lib/ipc";
import { dirtyLayers, useKeymap } from "../lib/keymap";
import { lightingUnsaved, useLighting } from "../lib/lighting";
import { dirtySlots, useMacros } from "../lib/macros";
import {
  fmtDuration,
  listOf,
  loadPartsChoice,
  loadProfile,
  loadSeconds,
  PART_LABEL,
  PART_NOUN,
  PARTS,
  partsOf,
  ProfileLoadError,
  profileName,
  profileSummary,
  savePartsChoice,
  useProfiles,
  type StepState,
} from "../lib/profiles";
import { useScreen } from "../lib/screen";
import { useApp } from "../lib/store";

/** Larger files are not read (profiles.rs MAX_FILE_BYTES). */
const MAX_FILE_BYTES = 32 * 1024 * 1024;

type Flow =
  | { kind: "create" }
  | { kind: "load"; id: string }
  | { kind: "update"; id: string }
  | { kind: "rename"; id: string }
  | { kind: "delete"; id: string };

type Notice = { ok: boolean; text: string; reveal?: boolean };

/** A load in progress: each part it writes and how far it is. */
type Run = { id: string; steps: Partial<Record<ProfilePart, StepState>> };

/** Editors holding changes not saved to the keyboard, by part. */
function useDrafts(): (parts: readonly ProfilePart[]) => boolean {
  const lighting = useLighting((s) => lightingUnsaved(s) !== null);
  const keymap = useKeymap((s) => dirtyLayers(s).length > 0);
  const macros = useMacros((s) => dirtySlots(s).length > 0);
  return (parts) =>
    parts.some((p) => (p === "lighting" && lighting) || (p === "keymap" && keymap) || (p === "macros" && macros));
}

/** "Profil 3": the first number not taken. */
function freeName(state: ProfilesState | null, t: Translate): string {
  const taken = new Set((state?.profiles ?? []).map((p) => p.name.toLowerCase()));
  for (let n = (state?.profiles.length ?? 0) + 1; ; n++) {
    const name = t("prof.create.defaultName", { n });
    if (!taken.has(name.toLowerCase())) return name;
  }
}

export function Profiles({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const locale = useApp((s) => s.locale);
  const modelId = kb.modelId;
  const canWrite = kb.connection === "usb" && kb.hasCommandChannel;
  const state = useProfiles((s) => (s.modelId === modelId ? s.state : null));
  const uploading = useScreen((s) => s.upload !== null);
  const pageMs = useScreen((s) => s.state?.limits.pageMs ?? 145);
  const hasDrafts = useDrafts();
  const [flow, setFlow] = useState<Flow | null>(null);
  const [flowError, setFlowError] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busy, setBusy] = useState(false);
  const [run, setRun] = useState<Run | null>(null);
  const [parts, setPartsState] = useState<ProfileParts>(loadPartsChoice);
  const stopRef = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  /** Where the focus goes back when a flow ends. */
  const returnTo = useRef<HTMLElement | null>(null);
  const returnRow = useRef<string | null>(null);
  const reasonId = useId();

  useEffect(() => {
    useProfiles
      .getState()
      .load(modelId)
      .catch((e) => setNotice({ ok: false, text: friendlyError(e, t) }));
    // The text of a failure is fixed when it happens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modelId]);

  const setParts = (next: ProfileParts) => {
    setPartsState(next);
    savePartsChoice(next);
  };

  const start = (f: Flow) => {
    if (document.activeElement instanceof HTMLElement) returnTo.current = document.activeElement;
    // A row's buttons are hidden during its flow: back to its row then.
    returnRow.current = "id" in f ? (f.id === state?.factory.id ? "default" : f.id) : null;
    setNotice(null);
    setFlowError(null);
    setFlow(f);
  };

  /** End the flow: back to the list, the focus where it was. */
  const finish = (n: Notice | null) => {
    setFlow(null);
    setFlowError(null);
    if (n) setNotice(n);
    setFocusBack((k) => k + 1);
  };

  // Once the list is drawn again and its buttons are usable (a button
  // disabled while busy loses the focus), the focus goes back; to the
  // title when that button is gone (a profile that is now on the keyboard).
  const [focusBack, setFocusBack] = useState(0);
  useEffect(() => {
    if (!focusBack || busy || run) return;
    const usable = (el: HTMLElement | null | undefined): el is HTMLElement =>
      !!el?.isConnected && el !== document.body && !(el as HTMLButtonElement).disabled;
    const row = returnRow.current ? document.querySelector(`[data-profile="${CSS.escape(returnRow.current)}"]`) : null;
    const inRow = [row?.querySelector<HTMLElement>("[data-profile-menu]"), row?.querySelector<HTMLElement>("button")];
    const target = [returnTo.current, ...inRow].find(usable);
    (target ?? titleRef.current)?.focus();
  }, [focusBack, busy, run]);

  const reload = () => useProfiles.getState().load(modelId);

  /** Run an action that changes the list; its error stays in the flow. */
  const act = async (f: () => Promise<Notice | null>, keepFlowOnError = false) => {
    setBusy(true);
    try {
      const n = await f();
      await reload().catch(console.error);
      finish(n);
    } catch (e) {
      const text = friendlyError(e, t);
      if (keepFlowOnError) setFlowError(text);
      else finish({ ok: false, text });
    } finally {
      setBusy(false);
    }
  };

  const profiles = state?.profiles ?? [];
  const locked = busy || run !== null;

  // A profile chosen in the tray menu: its load confirmation, as its button
  // would open. When it cannot be loaded now, its row shows why.
  const profileRequest = useApp((s) => s.profileRequest);
  const openRequested = useEffectEvent((id: string, s: ProfilesState) => {
    useApp.getState().clearProfileRequest();
    const p = [s.factory, ...s.profiles].find((x) => x.id === id);
    if (!p) return;
    const loadable = !p.onKeyboard && canWrite && !uploading && !locked && !p.otherModel && p.toWrite.length > 0;
    if (loadable) start({ kind: "load", id: p.id });
    const key = p.builtIn ? "default" : p.id;
    window.setTimeout(() => {
      document.querySelector(`[data-profile="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "nearest" });
    }, 50);
  });
  useEffect(() => {
    if (profileRequest && state) openRequested(profileRequest, state);
  }, [profileRequest, state]);

  // ---------------------------------------------------------------- actions

  const doLoad = async (p: ProfileInfo) => {
    stopRef.current = false;
    setRun({ id: p.id, steps: Object.fromEntries(p.toWrite.map((part) => [part, "waiting"])) });
    const done: ProfilePart[] = [];
    let n: Notice;
    try {
      const result = await loadProfile(
        modelId,
        p,
        (part, s) => {
          if (s === "done") done.push(part);
          setRun((r) => (r ? { ...r, steps: { ...r.steps, [part]: s } } : r));
        },
        () => stopRef.current,
      );
      if (result === "stopped") n = { ok: true, text: t("prof.load.stopped") };
      else {
        const fnHome = done.includes("screen") ? ` ${t("sc.done.fnHome")}` : "";
        n = { ok: true, text: `${t("prof.load.done", { name: profileName(p, t) })}${fnHome}` };
      }
    } catch (e) {
      const cause = e instanceof ProfileLoadError ? e.cause : e;
      const why = friendlyError(cause, t);
      // Nothing written yet: the cause alone says it.
      n =
        e instanceof ProfileLoadError && done.length > 0
          ? { ok: false, text: `${t("prof.load.failed", { part: t(PART_LABEL[e.part]) })} ${why}` }
          : { ok: false, text: why };
    }
    setRun(null);
    finish(n);
  };

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // The same file can be chosen again.
    e.target.value = "";
    if (!file) return;
    setNotice(null);
    if (file.size > MAX_FILE_BYTES) {
      setNotice({ ok: false, text: t("err.profile.tooLarge") });
      return;
    }
    returnTo.current = fileInput.current?.previousElementSibling as HTMLElement | null;
    returnRow.current = null;
    await act(async () => {
      const saved = await importProfile(await file.arrayBuffer());
      // The profile's picture went into the library.
      useScreen.getState().load(modelId).catch(console.error);
      return { ok: true, text: t("prof.import.done", { name: saved.name }) };
    });
  };

  const duplicate = (p: ProfileInfo) => {
    if (document.activeElement instanceof HTMLElement) returnTo.current = document.activeElement;
    returnRow.current = p.id;
    setNotice(null);
    void act(async () => {
      const saved = await duplicateProfile(p.id, t("prof.duplicate.name", { name: profileName(p, t) }));
      return { ok: true, text: t("prof.create.done", { name: saved.name }) };
    });
  };

  const exportOne = (p: ProfileInfo) => {
    if (document.activeElement instanceof HTMLElement) returnTo.current = document.activeElement;
    returnRow.current = p.id;
    setNotice(null);
    void act(async () => {
      const r = await exportProfile(p.id, p.name);
      return {
        ok: true,
        text: t(r.inDownloads ? "data.export.done" : "data.export.doneElsewhere", { file: r.file }),
        reveal: true,
      };
    });
  };

  // ---------------------------------------------------------------- view

  const usbReason = !canWrite ? t("sc.needsUsbShort") : uploading ? t("prof.load.uploadBusy") : null;

  const rowProps = (p: ProfileInfo) => ({
    p,
    t,
    locale,
    flow: flow && "id" in flow && flow.id === p.id ? flow : null,
    run: run?.id === p.id ? run : null,
    locked,
    loadBlocked: usbReason,
    reasonId: usbReason ? reasonId : undefined,
    onStart: start,
    onDuplicate: () => duplicate(p),
    onExport: () => exportOne(p),
  });

  const flowBox = (p: ProfileInfo): ReactNode => {
    if (!flow || !("id" in flow) || flow.id !== p.id) return null;
    const name = profileName(p, t);
    const cancel = () => finish(null);
    switch (flow.kind) {
      case "load": {
        if (run?.id === p.id) {
          const current = p.toWrite.find((part) => run.steps[part] === "running");
          return (
            <LoadProgress
              p={p}
              run={run}
              t={t}
              canStop={current !== "screen"}
              onStop={() => {
                stopRef.current = true;
              }}
            />
          );
        }
        const sentences = [
          t("prof.load.confirm", { list: listOf(p.toWrite.map((w) => t(PART_NOUN[w])), locale) }),
          !p.screen ? t("prof.load.noScreen") : !p.screen.inLibrary ? t("prof.load.missingScreen") : null,
          hasDrafts(p.toWrite) ? t("prof.load.drafts") : null,
          t("prof.load.time", { time: fmtDuration(loadSeconds(p, pageMs), t) }),
        ];
        return (
          <ConfirmBox
            text={sentences.filter(Boolean).join(" ")}
            t={t}
            tone="signal"
            action={t("prof.load")}
            disabled={!!usbReason}
            onCancel={cancel}
            onConfirm={() => void doLoad(p)}
          />
        );
      }
      case "update": {
        const own = partsOf(p);
        const text = [
          t("prof.update.confirm", { name, list: listOf(own.map((w) => t(PART_NOUN[w])), locale) }),
          hasDrafts(own) ? t("prof.unsaved") : null,
        ];
        return (
          <ConfirmBox
            text={text.filter(Boolean).join(" ")}
            t={t}
            tone="plain"
            action={t("prof.update.button")}
            busy={busy}
            error={flowError}
            onCancel={cancel}
            onConfirm={() =>
              void act(async () => {
                await updateProfile(modelId, p.id);
                return { ok: true, text: t("prof.update.done", { name }) };
              })
            }
          />
        );
      }
      case "delete":
        return (
          <ConfirmBox
            text={t("prof.delete.confirm", { name })}
            t={t}
            tone="danger"
            action={t("prof.delete")}
            busy={busy}
            error={flowError}
            onCancel={cancel}
            onConfirm={() =>
              void act(async () => {
                await deleteProfile(p.id);
                return { ok: true, text: t("prof.delete.done", { name }) };
              })
            }
          />
        );
      case "rename":
        return (
          <NameForm
            t={t}
            initial={p.name}
            action={t("prof.rename.save")}
            busy={busy}
            error={flowError}
            onCancel={cancel}
            onSubmit={(next) =>
              act(async () => {
                if (next === p.name) return null;
                await renameProfile(p.id, next);
                return { ok: true, text: t("prof.rename.done", { name: next.trim() }) };
              }, true)
            }
          />
        );
    }
  };

  return (
    <div className="flex flex-col gap-6" data-profiles>
      <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <div className="min-w-0 flex-1 basis-[320px]">
          <h1 ref={titleRef} tabIndex={-1} className="page-title outline-none">
            {t("nav.profiles")}
          </h1>
          <p className="page-desc">{t("prof.desc")}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            className="btn-secondary"
            disabled={locked}
            onClick={() => fileInput.current?.click()}
            data-profiles-import
          >
            {t("prof.import")}
          </button>
          <input ref={fileInput} type="file" accept=".json,application/json" className="hidden" onChange={onFile} />
          <button
            type="button"
            className="btn-primary"
            disabled={locked || !state || state.full || flow?.kind === "create"}
            title={state?.full ? t("prof.full") : undefined}
            onClick={() => start({ kind: "create" })}
            data-profiles-create
          >
            {t("prof.create")}
          </button>
        </div>
      </header>

      {notice && (
        <div
          role={notice.ok ? "status" : "alert"}
          className={`flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-[13px] ${notice.ok ? "text-ink-2" : "text-danger"}`}
          data-profiles-notice={notice.ok ? "ok" : "error"}
        >
          <span className="flex items-start gap-2">
            {notice.ok ? (
              <Check size={14} className="mt-0.5 shrink-0 text-ok" aria-hidden />
            ) : (
              <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden />
            )}
            {notice.text}
          </span>
          {notice.reveal && (
            <button type="button" className="btn-secondary" onClick={() => revealExport().catch(console.error)}>
              <FolderOpen size={14} aria-hidden />
              {t("data.export.show")}
            </button>
          )}
        </div>
      )}

      {state?.full && <p className="text-[13px] text-ink-3">{t("prof.full")}</p>}
      {usbReason && (
        <p id={reasonId} className="text-[13px] text-ink-3" data-profiles-reason>
          {usbReason}
        </p>
      )}

      <section aria-label={t("nav.profiles")} className="card p-0">
        <ul className="flex flex-col">
          {flow?.kind === "create" && state && (
            <CreateRow
              t={t}
              locale={locale}
              state={state}
              parts={parts}
              initial={freeName(state, t)}
              hasDrafts={hasDrafts}
              busy={busy}
              error={flowError}
              onCancel={() => finish(null)}
              onCreate={(name, chosen) =>
                act(async () => {
                  const saved = await createProfile(modelId, name, chosen);
                  return { ok: true, text: t("prof.create.done", { name: saved.name }) };
                }, true)
              }
            />
          )}
          {state && profiles.length === 0 && flow?.kind !== "create" && (
            <li className="px-5 py-4 text-[13px] text-ink-3">{t("prof.empty")}</li>
          )}
          {profiles.map((p) => (
            <ProfileRow key={p.id} {...rowProps(p)}>
              {flowBox(p)}
            </ProfileRow>
          ))}
          {state && (
            <ProfileRow {...rowProps(state.factory)}>{flowBox(state.factory)}</ProfileRow>
          )}
          {!state && (
            <li className="flex items-center gap-2 px-5 py-4 text-[13px] text-ink-3">
              <Loader2 size={14} className="animate-spin" aria-hidden />
            </li>
          )}
        </ul>
      </section>

      <PartsChoice t={t} parts={parts} onChange={setParts} />
    </div>
  );
}

// ------------------------------------------------------------------ rows

function ProfileRow({
  p,
  t,
  locale,
  flow,
  run,
  locked,
  loadBlocked,
  reasonId,
  onStart,
  onDuplicate,
  onExport,
  children,
}: {
  p: ProfileInfo;
  t: Translate;
  locale: Locale;
  flow: Flow | null;
  run: Run | null;
  locked: boolean;
  /** Why loading cannot start now (no cable, an upload running). */
  loadBlocked: string | null;
  reasonId?: string;
  onStart: (f: Flow) => void;
  onDuplicate: () => void;
  onExport: () => void;
  children: ReactNode;
}) {
  const name = profileName(p, t);
  const active = flow !== null || run !== null;
  // Nothing loading could change: only a missing picture differs.
  const nothingToWrite = p.toWrite.length === 0;
  const loadTitle = p.otherModel
    ? t("prof.load.otherModel")
    : nothingToWrite
      ? t("prof.load.missingScreen")
      : (loadBlocked ?? undefined);
  return (
    <li
      className={`border-t border-line first:border-t-0 first:rounded-t-lg last:rounded-b-lg ${active ? "bg-surface-2/60" : ""}`}
      data-profile={p.builtIn ? "default" : p.id}
      data-on-keyboard={p.onKeyboard || undefined}
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4">
        <div className="flex min-w-[220px] flex-1 flex-col gap-1">
          <span className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[15px] font-semibold">
            <span className="min-w-0 break-words">{name}</span>
            {p.onKeyboard && (
              <span
                className="flex items-center gap-1.5 rounded border border-ok/50 px-1.5 py-px text-[12px] font-semibold text-ink"
                data-profile-badge
              >
                <span className="h-1.5 w-1.5 rounded-full bg-ok" aria-hidden />
                {t("prof.onKeyboard")}
              </span>
            )}
          </span>
          <span className="text-[13px] text-ink-3">{profileSummary(p, t, locale)}</span>
          {p.otherModel && <span className="text-[13px] text-warn">{t("prof.load.otherModel")}</span>}
        </div>
        {!active && (
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            {!p.builtIn && !p.onKeyboard && (
              <button
                type="button"
                className="btn-secondary"
                disabled={locked}
                onClick={() => onStart({ kind: "update", id: p.id })}
              >
                {t("prof.update")}
              </button>
            )}
            {!p.onKeyboard && (
              <button
                type="button"
                className="btn-secondary"
                disabled={locked || !!loadBlocked || p.otherModel || nothingToWrite}
                title={loadTitle}
                aria-describedby={loadBlocked ? reasonId : undefined}
                onClick={() => onStart({ kind: "load", id: p.id })}
                data-profile-load
              >
                {t("prof.load")}
              </button>
            )}
            {!p.builtIn && (
              <MoreMenu
                label={t("prof.more", { name })}
                disabled={locked}
                items={[
                  { label: t("prof.rename"), icon: <Pencil size={14} />, onSelect: () => onStart({ kind: "rename", id: p.id }) },
                  { label: t("prof.duplicate"), icon: <Copy size={14} />, onSelect: onDuplicate },
                  { label: t("prof.export"), icon: <FileDown size={14} />, onSelect: onExport },
                  {
                    label: t("prof.delete"),
                    icon: <Trash2 size={14} />,
                    danger: true,
                    onSelect: () => onStart({ kind: "delete", id: p.id }),
                  },
                ]}
              />
            )}
          </div>
        )}
      </div>
      {children && <div className="px-5 pb-4">{children}</div>}
    </li>
  );
}

/** What will happen, "Vazgeç" and the action, inside the row. */
function ConfirmBox({
  text,
  t,
  tone,
  action,
  busy = false,
  disabled = false,
  error,
  onCancel,
  onConfirm,
}: {
  text: string;
  t: Translate;
  tone: "signal" | "plain" | "danger";
  action: string;
  busy?: boolean;
  disabled?: boolean;
  error?: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    // The safe answer has the focus.
    cancelRef.current?.focus();
  }, []);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape" && !busy) {
      e.preventDefault();
      onCancel();
    }
  };
  const box =
    tone === "signal"
      ? "border-signal/40 bg-signal/10"
      : tone === "danger"
        ? "border-danger/40 bg-danger/10"
        : "border-line-strong bg-surface";
  return (
    <div
      role="group"
      aria-label={action}
      onKeyDown={onKey}
      className={`flex flex-wrap items-center gap-3 rounded-lg border px-3.5 py-3 ${box}`}
      data-profile-confirm
    >
      <div className="flex min-w-[240px] flex-1 flex-col gap-1">
        <p className="text-[13px] text-ink">{text}</p>
        {error && (
          <p role="alert" className="flex items-start gap-2 text-[13px] text-danger">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden />
            {error}
          </p>
        )}
      </div>
      <div className="flex shrink-0 gap-2">
        <button ref={cancelRef} type="button" className="btn-secondary" disabled={busy} onClick={onCancel}>
          {t("unsaved.cancel")}
        </button>
        <button
          type="button"
          className={
            tone === "danger"
              ? "inline-flex h-9 items-center gap-2 rounded-lg bg-danger/15 px-4 text-[13.5px] font-semibold text-danger shadow-[inset_0_0_0_1px_var(--danger)] hover:bg-danger/25 disabled:opacity-40"
              : "btn-primary"
          }
          disabled={busy || disabled}
          onClick={onConfirm}
        >
          {busy && <Loader2 size={14} className="animate-spin" aria-hidden />}
          {action}
        </button>
      </div>
    </div>
  );
}

/** The load running: each part with its state; "Vazgeç" stops before the next one. */
function LoadProgress({
  p,
  run,
  t,
  canStop,
  onStop,
}: {
  p: ProfileInfo;
  run: Run;
  t: Translate;
  canStop: boolean;
  onStop: () => void;
}) {
  const upload = useScreen((s) => s.upload);
  const [stopping, setStopping] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // The button that started it is gone; the focus stays in the row.
    box.current?.focus();
  }, []);
  return (
    <div
      ref={box}
      tabIndex={-1}
      className="flex flex-wrap items-start gap-3 rounded-lg border border-signal/40 bg-signal/10 px-3.5 py-3 outline-none"
      data-profile-progress
    >
      <div className="flex min-w-[240px] flex-1 flex-col gap-2">
        <p className="flex items-center gap-2 text-[13px] text-ink" role="status">
          <Loader2 size={14} className="animate-spin" aria-hidden />
          {t("prof.load.running")}
        </p>
        <ol className="flex flex-col gap-1.5">
          {p.toWrite.map((part) => {
            const s = run.steps[part] ?? "waiting";
            const pct = part === "screen" && s === "running" && upload && upload.total > 0
              ? percent(Math.floor((upload.done / upload.total) * 100))
              : null;
            return (
              <li key={part} className="flex items-center gap-2 text-[13px]" data-step={part} data-state={s}>
                {s === "done" ? (
                  <CircleCheck size={15} className="shrink-0 text-ok" aria-hidden />
                ) : s === "running" ? (
                  <Loader2 size={15} className="shrink-0 animate-spin text-signal" aria-hidden />
                ) : (
                  <Circle size={15} className="shrink-0 text-ink-3" aria-hidden />
                )}
                <span className={s === "waiting" ? "text-ink-3" : "text-ink"}>{t(PART_LABEL[part])}</span>
                <span className="sr-only">, {t(`prof.step.${s}`)}</span>
                {pct && <span className="num text-ink-3">{pct}</span>}
              </li>
            );
          })}
        </ol>
      </div>
      <button
        type="button"
        className="btn-secondary"
        disabled={!canStop || stopping}
        onClick={() => {
          setStopping(true);
          onStop();
        }}
      >
        {t("unsaved.cancel")}
      </button>
    </div>
  );
}

/** A name field with "Vazgeç" and the action (creating, renaming). */
function NameForm({
  t,
  initial,
  action,
  busy,
  error,
  canSubmit = true,
  onCancel,
  onSubmit,
  children,
}: {
  t: Translate;
  initial: string;
  action: string;
  busy: boolean;
  error: string | null;
  /** Something else (the parts) allows it; the name must not be empty either way. */
  canSubmit?: boolean;
  onCancel: () => void;
  onSubmit: (name: string) => Promise<void>;
  /** Notes under the field. */
  children?: ReactNode;
}) {
  const [name, setName] = useState(initial);
  const inputId = useId();
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.select();
  }, []);
  const ready = canSubmit && !!name.trim() && !busy;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (ready) void onSubmit(name.trim());
  };
  return (
    <form
      onSubmit={submit}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !busy) {
          e.preventDefault();
          onCancel();
        }
      }}
      className="flex flex-col gap-2"
      data-profile-name-form
    >
      <label htmlFor={inputId} className="text-[13px] font-medium">
        {t("prof.create.name")}
      </label>
      <div className="flex flex-wrap items-center gap-2">
        <input
          ref={input}
          id={inputId}
          value={name}
          maxLength={40}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={!!error || undefined}
          onChange={(e) => setName(e.target.value)}
          className="h-9 min-w-[200px] flex-1 rounded-lg border border-line-strong bg-bg-raise px-3 text-[14px] text-ink outline-none focus:border-signal"
        />
        <button type="button" className="btn-secondary" disabled={busy} onClick={onCancel}>
          {t("unsaved.cancel")}
        </button>
        <button type="submit" className="btn-primary" disabled={!ready}>
          {busy && <Loader2 size={14} className="animate-spin" aria-hidden />}
          {action}
        </button>
      </div>
      {children}
      {error && (
        <p role="alert" className="flex items-start gap-2 text-[13px] text-danger">
          <AlertTriangle size={14} className="mt-0.5 shrink-0" aria-hidden />
          {error}
        </p>
      )}
    </form>
  );
}

/** The new profile's name, at the top of the list; its parts come from the choice below. */
function CreateRow({
  t,
  locale,
  state,
  parts,
  initial,
  hasDrafts,
  busy,
  error,
  onCancel,
  onCreate,
}: {
  t: Translate;
  locale: Locale;
  state: ProfilesState;
  parts: ProfileParts;
  initial: string;
  hasDrafts: (parts: readonly ProfilePart[]) => boolean;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onCreate: (name: string, parts: ProfileParts) => Promise<void>;
}) {
  // What actually goes in: unknown lighting and a screen without a library
  // picture stay out.
  const chosen: ProfileParts = {
    lighting: parts.lighting && state.current.lighting,
    keymap: parts.keymap,
    macros: parts.macros,
    screen: parts.screen && state.current.screen !== null,
  };
  const included = PARTS.filter((p) => chosen[p]);
  const notes = [
    included.length > 0
      ? t("prof.create.takes", { list: listOf(included.map((p) => t(PART_NOUN[p])), locale) })
      : t("prof.create.none"),
    parts.lighting && !state.current.lighting ? t("prof.create.noLight") : null,
    parts.screen && state.current.screen === null ? t("prof.create.noScreen") : null,
    hasDrafts(included) ? t("prof.unsaved") : null,
  ].filter((n): n is string => !!n);
  return (
    <li className="rounded-t-lg bg-surface-2/60 px-5 py-4" aria-label={t("prof.create.title")} data-profile-create>
      <NameForm
        t={t}
        initial={initial}
        action={t("prof.create.button")}
        busy={busy}
        error={error}
        canSubmit={included.length > 0}
        onCancel={onCancel}
        onSubmit={(name) => onCreate(name, chosen)}
      >
        <p className="text-[13px] text-ink-2" data-profile-create-notes>
          {notes.join(" ")}
        </p>
      </NameForm>
    </li>
  );
}

/** "Yeni profile neler girsin": the parts a new profile takes. */
function PartsChoice({ t, parts, onChange }: { t: Translate; parts: ProfileParts; onChange: (p: ProfileParts) => void }) {
  return (
    <section className="flex flex-col gap-2.5" aria-labelledby="profile-parts-title" data-profile-parts>
      <h2 id="profile-parts-title" className="eyebrow">
        {t("prof.parts.title")}
      </h2>
      <div className="flex flex-wrap gap-x-6 gap-y-2 text-[14px] text-ink-2">
        {PARTS.map((part) => (
          <label key={part} className="flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={parts[part]}
              onChange={(e) => onChange({ ...parts, [part]: e.target.checked })}
              className="h-4 w-4 accent-[var(--signal)]"
            />
            {t(PART_LABEL[part])}
          </label>
        ))}
      </div>
      <p className="text-[13px] text-ink-3">{t("prof.parts.screenNote")}</p>
    </section>
  );
}

// ------------------------------------------------------------------ menu

interface MenuItem {
  label: string;
  icon: ReactNode;
  danger?: boolean;
  onSelect: () => void;
}

/** "⋯": a small menu, by mouse or keyboard (arrows, Home/End, Escape). */
function MoreMenu({ label, items, disabled }: { label: string; items: MenuItem[]; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const away = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!menu.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    window.addEventListener("pointerdown", away);
    return () => window.removeEventListener("pointerdown", away);
  }, [open]);

  const close = () => {
    setOpen(false);
    button.current?.focus();
  };

  const onKey = (e: KeyboardEvent) => {
    const list = [...(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const i = list.indexOf(document.activeElement as HTMLElement);
    const go = (n: number) => {
      e.preventDefault();
      list[(n + list.length) % list.length]?.focus();
    };
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") go(i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(list.length - 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Tab") setOpen(false);
  };

  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            setOpen(true);
          }
        }}
        className="flex h-9 w-9 items-center justify-center rounded-lg text-ink shadow-[inset_0_0_0_1px_var(--line-strong)] transition-colors hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-ink-3 disabled:shadow-[inset_0_0_0_1px_var(--line)]"
        data-profile-menu
      >
        <Ellipsis size={16} aria-hidden />
      </button>
      {open && (
        <div
          ref={menu}
          id={menuId}
          role="menu"
          aria-label={label}
          onKeyDown={onKey}
          className="absolute top-[calc(100%+4px)] right-0 z-30 flex min-w-[190px] flex-col rounded-lg border border-line-strong bg-surface p-1 shadow-[0_10px_30px_-14px_rgb(0_0_0/0.55)]"
        >
          {items.map((it) => (
            <button
              key={it.label}
              type="button"
              role="menuitem"
              tabIndex={-1}
              onClick={() => {
                close();
                it.onSelect();
              }}
              className={`flex h-8 items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] hover:bg-surface-2 focus:bg-surface-2 focus:outline-none ${
                it.danger ? "text-danger" : "text-ink"
              }`}
            >
              <span aria-hidden className={it.danger ? "" : "text-ink-3"}>
                {it.icon}
              </span>
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
