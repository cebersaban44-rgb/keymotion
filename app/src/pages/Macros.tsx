import { ArrowDown, ArrowUp, Circle, Info, Keyboard, MousePointerClick, Plus, Square, Trash2, Usb, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeymapCatalog } from "../bindings/KeymapCatalog";
import type { KeymapLayer } from "../bindings/KeymapLayer";
import type { MacroInfo } from "../bindings/MacroInfo";
import type { MacroStepInfo } from "../bindings/MacroStepInfo";
import { ActionBar } from "../components/ActionBar";
import { Segmented } from "../components/controls";
import { hidFromCode } from "../lib/hidCodes";
import { getKeymapCatalog } from "../lib/ipc";
import { useAgo, useT, type MessageKey } from "../lib/i18n";
import { useKeymap } from "../lib/keymap";
import { budgetUsed, dirtySlots, duration, useMacros } from "../lib/macros";
import { useLayoutLabel } from "../lib/layout";
import { useApp } from "../lib/store";

type Timing = "real" | "fixed";

const MOUSE: { mask: number; key: MessageKey }[] = [
  { mask: 1, key: "mc.mouse.left" },
  { mask: 2, key: "mc.mouse.right" },
  { mask: 4, key: "mc.mouse.middle" },
];

let catalogCache: Promise<KeymapCatalog> | null = null;
function useCatalog(): KeymapCatalog | null {
  const [catalog, setCatalog] = useState<KeymapCatalog | null>(null);
  useEffect(() => {
    catalogCache ??= getKeymapCatalog();
    let live = true;
    catalogCache.then((c) => live && setCatalog(c)).catch(console.error);
    return () => {
      live = false;
    };
  }, []);
  return catalog;
}

/** Keys (by label) that play macro `slot`, per layer. */
function useBoundKeys(kb: KeyboardInfo, slot: number | null): { layer: KeymapLayer; led: number; label: string }[] {
  const draft = useKeymap((s) => s.draft);
  return useMemo(() => {
    if (slot === null) return [];
    const keys = kb.geometry?.keys ?? [];
    const out: { layer: KeymapLayer; led: number; label: string }[] = [];
    for (const layer of ["base", "fn"] as const) {
      for (const [led, a] of Object.entries(draft[layer])) {
        if (a.kind === "macro" && a.slot === slot) {
          const k = keys.find((x) => x.led === Number(led));
          out.push({ layer, led: Number(led), label: k?.label || k?.id || led });
        }
      }
    }
    return out;
  }, [draft, kb.geometry, slot]);
}

export function Macros({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel && kb.features.includes("macros");
  const catalog = useCatalog();
  const { draft, limits, selected, select, create, modelId, saved, save, discard } = useMacros();
  const loaded = saved !== null && modelId === kb.modelId;
  const used = budgetUsed(draft);
  const full = draft.filter(Boolean).length >= limits.maxSlots;
  const changed = useMacros((s) => dirtySlots(s).length);

  return (
    <div className="@container flex flex-col gap-6">
      <header className="flex items-end justify-between gap-6">
        <div>
          <h1 className="page-title">{t("nav.macros")}</h1>
          <p className="page-desc">{t("mc.eyebrow")}</p>
        </div>
        <Memory used={used} budget={limits.budget} />
      </header>

      {!usb && (
        <div className="flex items-center gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("mc.needsUsb")}
        </div>
      )}

      <div className="grid grid-cols-[240px_minmax(0,1fr)] items-start gap-5 @3xl:grid-cols-[280px_minmax(0,1fr)]">
        <div className="sticky top-4 flex flex-col gap-5">
          <section className="card flex flex-col gap-2 p-3">
            <div className="flex items-center justify-between px-2 pt-2 pb-1">
              <div className="eyebrow">{t("mc.list")}</div>
              <button
                type="button"
                disabled={!loaded || full}
                onClick={() => create(`${t("mc.newName")} ${draft.filter(Boolean).length + 1}`)}
                className="flex items-center gap-1.5 rounded-lg bg-signal px-2.5 py-1 text-[12px] font-semibold text-signal-ink disabled:opacity-40"
              >
                <Plus size={13} />
                {t("mc.new")}
              </button>
            </div>
            <MacroList onSelect={select} selected={selected} />
            <RecordNote />
          </section>
        </div>
        {selected !== null && draft[selected] ? (
          <Editor key={selected} kb={kb} slot={selected} macro={draft[selected]!} catalog={catalog} />
        ) : (
          <section className="card flex flex-col items-center gap-3 p-10 text-center">
            <Keyboard size={28} className="text-ink-3" />
            <p className="max-w-[46ch] text-[13px] text-ink-2">{t("mc.emptyEditor")}</p>
          </section>
        )}
      </div>

      <ActionBar
        changes={changed > 0 ? t("bar.macrosChanged", { count: changed }) : null}
        blocked={used > limits.budget ? t("mc.overBudget") : null}
        canSave={usb && loaded}
        onSave={save}
        onRevert={discard}
      />
    </div>
  );
}

function Memory({ used, budget }: { used: number; budget: number }) {
  const t = useT();
  const pct = Math.min(100, Math.round((used / budget) * 100));
  const over = used > budget;
  return (
    <div className="flex w-[220px] flex-col gap-1.5" title={`${used} / ${budget} B`}>
      <div className="flex items-baseline justify-between text-[12px]">
        <span className="text-ink-2">{t("mc.memory")}</span>
        <span className={`num ${over ? "text-danger" : "text-ink-3"}`}>
          {used} / {budget} B
        </span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
        <div
          className={`h-full rounded-full ${over ? "bg-danger" : "bg-signal"}`}
          style={{ width: `${Math.max(pct, used > 0 ? 2 : 0)}%` }}
        />
      </div>
    </div>
  );
}

function MacroList({ selected, onSelect }: { selected: number | null; onSelect: (slot: number) => void }) {
  const t = useT();
  const draft = useMacros((s) => s.draft);
  const minDelay = useMacros((s) => s.limits.minDelayMs);
  const dirtyKey = useMacros((s) => dirtySlots(s).join(","));
  const dirty = new Set(dirtyKey ? dirtyKey.split(",").map(Number) : []);
  const items = draft.map((m, slot) => ({ m, slot })).filter((x): x is { m: MacroInfo; slot: number } => !!x.m);
  if (items.length === 0) return <p className="px-2 pb-2 text-[13px] text-ink-3">{t("mc.none")}</p>;
  return (
    <ul className="flex flex-col gap-1">
      {items.map(({ m, slot }) => (
        <li key={slot}>
          <button
            type="button"
            aria-current={selected === slot}
            onClick={() => onSelect(slot)}
            className={`flex w-full items-center gap-3 rounded-lg border px-2.5 py-2 text-left transition-colors ${
              selected === slot ? "border-signal bg-signal/10" : "border-transparent hover:bg-surface-2"
            }`}
          >
            <span className="num flex h-8 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-[12px] font-semibold">
              M{slot + 1}
            </span>
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[13px] font-medium">{m.name || t("mc.unnamed")}</span>
              <span className="num text-[11px] text-ink-3">
                {t("mc.steps", { count: m.steps.length })} · {(duration(m, minDelay) / 1000).toFixed(2)} s
              </span>
            </span>
            {dirty.has(slot) && (
              <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-signal" title={t("km.legend.pending")} />
            )}
          </button>
        </li>
      ))}
    </ul>
  );
}

function Editor({
  kb,
  slot,
  macro,
  catalog,
}: {
  kb: KeyboardInfo;
  slot: number;
  macro: MacroInfo;
  catalog: KeymapCatalog | null;
}) {
  const t = useT();
  const { rename, setSteps, remove, limits } = useMacros();
  const assign = useKeymap((s) => s.assign);
  const setPage = useApp((s) => s.setPage);
  const bound = useBoundKeys(kb, slot);
  const layout = useLayoutLabel();
  const [recording, setRecording] = useState(false);
  const [timing, setTiming] = useState<Timing>("real");
  const [fixedMs, setFixedMs] = useState(20);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const steps = macro.steps;
  const stepsRef = useRef(steps);
  stepsRef.current = steps;

  const clampDelay = (ms: number) => Math.min(65535, Math.max(limits.minDelayMs, Math.round(ms)));

  // Recording: keys by position, until "Stop". Keys held at the end are
  // released so a played macro never leaves a key down.
  useEffect(() => {
    if (!recording) return;
    const held = new Set<number>();
    const startLength = stepsRef.current.length;
    let last = performance.now();
    const add = (kind: MacroStepInfo["kind"], code: number) => {
      const now = performance.now();
      const next = stepsRef.current.slice();
      const prev = next[next.length - 1];
      if (prev && timing === "real" && next.length > startLength) {
        next[next.length - 1] = { ...prev, delayMs: clampDelay(now - last) };
      }
      next.push({ kind, code, delayMs: clampDelay(timing === "fixed" ? fixedMs : limits.minDelayMs) });
      last = now;
      setSteps(slot, next);
    };
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const code = hidFromCode(e.code);
      if (e.repeat || code === undefined || held.has(code)) return;
      held.add(code);
      add("key-down", code);
    };
    const up = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const code = hidFromCode(e.code);
      if (code === undefined || !held.has(code)) return;
      held.delete(code);
      add("key-up", code);
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
      if (held.size) {
        const next = stepsRef.current.slice();
        for (const code of held) next.push({ kind: "key-up", code, delayMs: clampDelay(limits.minDelayMs) });
        setSteps(slot, next);
      }
    };
    // Settings are read when recording starts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording]);

  const label = (s: MacroStepInfo) => {
    if (s.kind === "mouse-down" || s.kind === "mouse-up") {
      const m = MOUSE.find((x) => x.mask === s.code);
      return m ? t(m.key) : `${t("mc.mouse")} ${s.code}`;
    }
    return layout(s.code) ?? catalog?.keys.find((k) => k.code === s.code)?.label ?? `0x${s.code.toString(16)}`;
  };
  const update = (i: number, patch: Partial<MacroStepInfo>) =>
    setSteps(
      slot,
      steps.map((s, j) => (j === i ? { ...s, ...patch } : s)),
    );
  const move = (i: number, by: number) => {
    const j = i + by;
    if (j < 0 || j >= steps.length) return;
    const next = steps.slice();
    [next[i], next[j]] = [next[j]!, next[i]!];
    setSteps(slot, next);
  };
  const addClick = (mask: number) =>
    setSteps(slot, [
      ...steps,
      { kind: "mouse-down", code: mask, delayMs: clampDelay(fixedMs) },
      { kind: "mouse-up", code: mask, delayMs: clampDelay(fixedMs) },
    ]);
  const unbalanced = useMemo(() => {
    const down = new Set<string>();
    for (const s of steps) {
      const id = `${s.kind.startsWith("mouse") ? "m" : "k"}${s.code}`;
      if (s.kind.endsWith("down")) down.add(id);
      else down.delete(id);
    }
    return down.size > 0;
  }, [steps]);

  const doDelete = () => {
    for (const b of bound) assign(b.led, { kind: "default" }, b.layer);
    remove(slot);
  };

  return (
    <section className="card flex flex-col gap-5 p-5">
      <div className="flex items-center gap-3">
        <span className="num flex h-10 w-11 shrink-0 items-center justify-center rounded-lg border border-line-strong bg-surface-2 text-[13px] font-semibold">
          M{slot + 1}
        </span>
        <input
          value={macro.name}
          maxLength={limits.maxName}
          onChange={(e) => rename(slot, e.target.value)}
          aria-label={t("mc.name")}
          placeholder={t("mc.unnamed")}
          disabled={recording}
          className="h-10 flex-1 rounded-lg border border-line bg-bg-raise px-3 text-[15px] font-medium outline-none focus:border-signal"
        />
        {confirmDelete ? (
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={doDelete}
              className="flex h-9 items-center gap-1.5 rounded-lg bg-danger px-3 text-[13px] font-semibold text-white"
            >
              <Trash2 size={13} />
              {t("mc.delete")}
            </button>
            <button
              type="button"
              onClick={() => setConfirmDelete(false)}
              className="h-9 rounded-lg px-2 text-[13px] text-ink-2 hover:text-ink"
            >
              {t("unsaved.cancel")}
            </button>
          </div>
        ) : (
          <button
            type="button"
            disabled={recording}
            onClick={() => setConfirmDelete(true)}
            aria-label={t("mc.delete")}
            className="flex h-9 w-9 items-center justify-center rounded-lg text-ink-3 hover:bg-surface-2 hover:text-danger disabled:opacity-40"
          >
            <Trash2 size={15} />
          </button>
        )}
      </div>
      {confirmDelete && (
        <p className="-mt-2 text-[12px] text-ink-2">
          {bound.length
            ? t("mc.deleteBound", { keys: bound.map((b) => b.label).join(", ") })
            : t("mc.deleteHint")}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-3 rounded-lg border border-line bg-bg-raise p-3">
        <button
          type="button"
          onClick={() => setRecording(!recording)}
          className={`flex h-9 items-center gap-2 rounded-lg px-3.5 text-[13px] font-semibold transition-colors ${
            recording ? "bg-danger text-white" : "bg-surface-2 text-ink hover:bg-surface"
          }`}
        >
          {recording ? <Square size={13} fill="currentColor" /> : <Circle size={13} fill="var(--danger)" stroke="none" />}
          {recording ? t("mc.stop") : t("mc.record")}
        </button>
        <Segmented
          label={t("mc.timing")}
          value={timing}
          onChange={setTiming}
          disabled={recording}
          options={[
            { value: "real", label: t("mc.timing.real") },
            { value: "fixed", label: t("mc.timing.fixed") },
          ]}
        />
        {timing === "fixed" && (
          <label className="flex items-center gap-1.5 text-[12px] text-ink-2">
            <input
              type="number"
              min={limits.minDelayMs}
              max={65535}
              value={fixedMs}
              disabled={recording}
              onChange={(e) => setFixedMs(Number(e.target.value) || limits.minDelayMs)}
              className="num h-8 w-20 rounded-lg border border-line bg-surface px-2 text-right outline-none focus:border-signal"
            />
            ms
          </label>
        )}
        <span className="ml-auto flex items-center gap-1.5">
          {MOUSE.map((m) => (
            <button
              key={m.mask}
              type="button"
              disabled={recording}
              onClick={() => addClick(m.mask)}
              className="flex h-8 items-center gap-1.5 rounded-lg border border-line bg-surface-2 px-2.5 text-[12px] text-ink-2 hover:text-ink disabled:opacity-40"
            >
              <MousePointerClick size={13} />+ {t(m.key)}
            </button>
          ))}
        </span>
      </div>
      {recording && (
        <p className="flex items-center gap-2 rounded-lg border border-danger/30 bg-danger/10 px-3 py-2 text-[13px]">
          <span className="h-2 w-2 animate-pulse rounded-full bg-danger" />
          {t("mc.recordingHint")}
        </p>
      )}

      {steps.length === 0 ? (
        <p className="text-[13px] text-ink-3">{t("mc.noSteps")}</p>
      ) : (
        <ol className="flex max-h-[460px] flex-col gap-1 overflow-y-auto pr-1">
          {steps.map((s, i) => {
            const down = s.kind.endsWith("down");
            return (
              <li
                key={i}
                className="group grid grid-cols-[28px_20px_1fr_auto_auto] items-center gap-2.5 rounded-lg px-2 py-1 hover:bg-surface-2"
              >
                <span className="num text-right text-[11px] text-ink-3">{i + 1}</span>
                <span className={down ? "text-signal" : "text-ink-3"} title={t(down ? "mc.press" : "mc.release")}>
                  {down ? <ArrowDown size={14} /> : <ArrowUp size={14} />}
                </span>
                <span className="flex items-center gap-2 text-[13px]">
                  <span className="num rounded-md border border-line bg-surface px-1.5 py-0.5 text-[12px]">
                    {label(s)}
                  </span>
                  <span className="text-[12px] text-ink-3">{t(down ? "mc.press" : "mc.release")}</span>
                </span>
                <label className="flex items-center gap-1 text-[12px] text-ink-3">
                  <input
                    type="number"
                    min={limits.minDelayMs}
                    max={65535}
                    value={s.delayMs}
                    disabled={recording}
                    aria-label={t("mc.delay")}
                    onChange={(e) => update(i, { delayMs: Number(e.target.value) || 0 })}
                    onBlur={(e) => update(i, { delayMs: clampDelay(Number(e.target.value) || 0) })}
                    className="num h-7 w-[72px] rounded-md border border-line bg-surface px-1.5 text-right text-[12px] text-ink outline-none focus:border-signal"
                  />
                  ms
                </label>
                <span className="flex opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                  <IconButton label={t("mc.up")} disabled={recording || i === 0} onClick={() => move(i, -1)}>
                    <ArrowUp size={13} />
                  </IconButton>
                  <IconButton
                    label={t("mc.down")}
                    disabled={recording || i === steps.length - 1}
                    onClick={() => move(i, 1)}
                  >
                    <ArrowDown size={13} />
                  </IconButton>
                  <IconButton
                    label={t("mc.removeStep")}
                    disabled={recording}
                    onClick={() =>
                      setSteps(
                        slot,
                        steps.filter((_, j) => j !== i),
                      )
                    }
                  >
                    <X size={13} />
                  </IconButton>
                </span>
              </li>
            );
          })}
        </ol>
      )}

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-line pt-4 text-[12px] text-ink-2">
        <span className="num">
          {t("mc.steps", { count: steps.length })} · {(duration(macro, limits.minDelayMs) / 1000).toFixed(2)} s
        </span>
        {steps.length > 0 && (
          <button
            type="button"
            disabled={recording}
            onClick={() => setSteps(slot, [])}
            className="flex items-center gap-1.5 text-ink-3 hover:text-ink disabled:opacity-40"
          >
            <X size={12} />
            {t("mc.clear")}
          </button>
        )}
        {unbalanced && !recording && <span className="text-warn">{t("mc.unbalanced")}</span>}
        <span className="ml-auto flex items-center gap-2">
          {bound.length ? (
            <>
              {t("mc.boundTo")}
              {bound.map((b) => (
                <span key={`${b.layer}-${b.led}`} className="num rounded-md border border-line px-1.5 py-0.5">
                  {b.layer === "fn" ? `Fn + ${b.label}` : b.label}
                </span>
              ))}
            </>
          ) : (
            t("mc.notBound")
          )}
          <button
            type="button"
            onClick={() => setPage("keymap")}
            className="rounded-lg border border-line px-2 py-1 text-ink-2 hover:text-ink"
          >
            {t("mc.bind")}
          </button>
        </span>
      </div>
    </section>
  );
}

function IconButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink disabled:opacity-30"
    >
      {children}
    </button>
  );
}

/** Macros cannot be read back: what the list shows and when it was saved. */
function RecordNote() {
  const t = useT();
  const ago = useAgo();
  const writtenMs = useMacros((s) => s.writtenMs);
  return (
    <p className="mx-2 mb-1 flex gap-2 border-t border-line pt-3 text-[12px] text-ink-2">
      <Info size={13} className="mt-0.5 shrink-0 text-ink-3" />
      <span>
        {t("mc.readNote")}{" "}
        <span className="text-ink-3">{writtenMs ? t("mc.record.at", { ago: ago(writtenMs) }) : t("mc.record.never")}</span>
      </span>
    </p>
  );
}
