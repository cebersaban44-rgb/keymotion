import { AlertTriangle, Ban, Check, Info, Keyboard, Loader2, Lock, Mouse, RotateCcw, Undo2, Usb, X } from "lucide-react";
import { useEffect, useMemo, useState, type KeyboardEvent, type ReactNode } from "react";
import type { KeyActionInfo } from "../bindings/KeyActionInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { KeyGeo } from "../bindings/KeyGeo";
import type { KeymapCatalog } from "../bindings/KeymapCatalog";
import type { KeyUsageGroup } from "../bindings/KeyUsageGroup";
import type { KeymapLayer } from "../bindings/KeymapLayer";
import { Segmented } from "../components/controls";
import { KeyboardView, type Legend } from "../components/KeyboardView";
import { getKeymapCatalog } from "../lib/ipc";
import { useAgo, useT, type MessageKey } from "../lib/i18n";
import { dirtyLayers, FN_USAGE, normalize, sameAction, usageAction, useKeymap, type LayerMap } from "../lib/keymap";
import { actionShort, actionText, MEDIA_ICONS, MOD_TOGGLES, usageShort, type MacroNames } from "../lib/keymapLabels";
import { dirtySlots, useMacros } from "../lib/macros";
import { layoutLabel, useLayout, useLayoutLabel } from "../lib/layout";
import { useApp } from "../lib/store";
import type { MacroPlayMode } from "../bindings/MacroPlayMode";
import { friendlyError } from "../lib/errors";

type Status = { kind: "idle" } | { kind: "busy" } | { kind: "ok"; ms: number } | { kind: "error"; message: string };
type Category = "keys" | "media" | "mouse" | "macro" | "special";

/** Names of the macros being edited, for key legends and the panel. */
function useMacroNames(): MacroNames {
  const draft = useMacros((s) => s.draft);
  return useMemo(() => (slot: number) => draft[slot]?.name || undefined, [draft]);
}

const GROUPS: KeyUsageGroup[] = ["letter", "digit", "function", "editing", "navigation", "symbol", "keypad", "modifier"];

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

export function Keymap({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel && kb.features.includes("keymap");
  const catalog = useCatalog();
  const { layer, setLayer, draft, saved, selected, select, modelId } = useKeymap();
  const loaded = saved !== null && modelId === kb.modelId;
  const keys = kb.geometry?.keys ?? [];
  const selectedKey = keys.find((k) => k.led === selected) ?? null;

  const counts = { base: Object.keys(draft.base).length, fn: Object.keys(draft.fn).length };

  return (
    <div className="flex flex-col gap-6">
      <header className="flex items-end justify-between gap-6">
        <div>
          <h1 className="page-title">{t("nav.keymap")}</h1>
        </div>
        <div role="tablist" className="inline-flex rounded-lg border border-line bg-bg-raise p-1">
          {(["base", "fn"] as const).map((id) => (
            <button
              key={id}
              role="tab"
              type="button"
              aria-selected={layer === id}
              onClick={() => setLayer(id)}
              className={`flex items-center gap-2 rounded-lg px-4 py-1.5 text-[13px] transition-colors ${
                layer === id ? "bg-surface-2 text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:text-ink"
              }`}
            >
              {t(id === "base" ? "km.layer.base" : "km.layer.fn")}
              {counts[id] > 0 && (
                <span className="num rounded-full bg-signal/15 px-1.5 text-[11px] text-signal">{counts[id]}</span>
              )}
            </button>
          ))}
        </div>
      </header>

      {!usb && (
        <div className="flex items-center gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("km.needsUsb")}
        </div>
      )}

      <Plate kb={kb} layer={layer} catalog={catalog} onSelect={select} selected={selected} />

      <div className="grid grid-cols-[1fr_360px] items-start gap-5">
        <Picker layer={layer} catalog={catalog} keyGeo={selectedKey} disabled={!loaded} />
        <div className="sticky top-4 flex flex-col gap-5">
          <SelectedCard layer={layer} catalog={catalog} keyGeo={selectedKey} />
          <SaveCard usb={usb} loaded={loaded} layer={layer} />
        </div>
      </div>

      <KeyTester />
    </div>
  );
}

function Plate({
  kb,
  layer,
  catalog,
  selected,
  onSelect,
}: {
  kb: KeyboardInfo;
  layer: KeymapLayer;
  catalog: KeymapCatalog | null;
  selected: number | null;
  onSelect: (led: number | null) => void;
}) {
  const t = useT();
  const draft = useKeymap((s) => s.draft[layer]);
  const saved = useKeymap((s) => s.saved?.[layer] ?? null);
  const keys = kb.geometry?.keys ?? [];
  const names = useMacroNames();
  const layoutMap = useLayout((s) => s.map);

  const { legends, colors, classes } = useMemo(() => {
    const layout = (hid: number) => layoutLabel(layoutMap, hid);
    const legends: Record<number, Legend> = {};
    const colors: Record<number, string> = {};
    const classes: Record<number, string> = {};
    for (const k of keys) {
      const a = draft[k.led];
      if (a) {
        legends[k.led] = {
          main: actionShort(a, catalog, layout),
          text: actionText(a, catalog, t, names, layout),
          sub: k.label,
        };
        colors[k.led] = a.kind === "disabled" ? "var(--danger)" : "var(--signal)";
      } else {
        colors[k.led] = "transparent";
      }
      const pending = saved && !sameAction(a, saved[k.led]);
      if (k.hid === FN_USAGE) classes[k.led] = "is-locked";
      else if (pending) classes[k.led] = "is-pending";
    }
    return { legends, colors, classes };
  }, [keys, draft, saved, catalog, t, names, layoutMap]);

  const pendingCount = Object.values(classes).filter((c) => c === "is-pending").length;
  const selectedSet = useMemo(() => new Set(selected === null ? [] : [selected]), [selected]);

  return (
    <section
      onPointerDown={(e) => {
        if (!(e.target as HTMLElement).closest(".kb-key")) onSelect(null);
      }}
      className="kb-keymap relative overflow-hidden rounded-lg border border-line p-6 pb-4"
      style={{ background: "linear-gradient(180deg, var(--plate), color-mix(in oklch, var(--plate) 80%, black))" }}
    >
      {kb.geometry && (
        <KeyboardView
          geometry={kb.geometry}
          battery={kb.battery?.percent ?? null}
          connection={kb.connection}
          colors={colors}
          legends={legends}
          keyClass={classes}
          selected={selectedSet}
          tool="select"
          onKeyAction={(led, action) => {
            if (action === "press") onSelect(led === selected ? null : led);
          }}
        />
      )}
      <div className="mt-4 flex items-center gap-4 text-[12px] text-[#8b8d92]">
        <span className="flex items-center gap-2">
          <Keyboard size={13} />
          {t(layer === "base" ? "km.layer.base.caption" : "km.layer.fn.caption")}
        </span>
        {pendingCount > 0 && (
          <span className="flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-signal" />
            <span>
              <span className="num">{pendingCount}</span> {t("km.legend.pending")}
            </span>
          </span>
        )}
      </div>
    </section>
  );
}

/** Assign the selected key, normalising "a key sends itself" to the default. */
function useAssign(layer: KeymapLayer, keyGeo: KeyGeo | null) {
  const assign = useKeymap((s) => s.assign);
  return (action: KeyActionInfo) => {
    if (!keyGeo || keyGeo.hid === FN_USAGE) return;
    assign(keyGeo.led, normalize(layer, keyGeo.hid, action));
  };
}

function Picker({
  layer,
  catalog,
  keyGeo,
  disabled,
}: {
  layer: KeymapLayer;
  catalog: KeymapCatalog | null;
  keyGeo: KeyGeo | null;
  disabled: boolean;
}) {
  const t = useT();
  const [category, setCategory] = useState<Category>("keys");
  const layout = useLayoutLabel();
  const [mods, setMods] = useState(0);
  const current = useKeymap((s) => (keyGeo ? s.draft[layer][keyGeo.led] : undefined));
  const assignRaw = useAssign(layer, keyGeo);
  const locked = keyGeo?.hid === FN_USAGE;
  const off = disabled || !keyGeo || locked || !catalog;

  // A new key starts without "held with" modifiers.
  useEffect(() => setMods(0), [keyGeo?.led]);

  const assign = (a: KeyActionInfo) => {
    assignRaw(a);
    setMods(0);
  };
  const isCurrent = (a: KeyActionInfo) => !off && !!keyGeo && sameAction(normalize(layer, keyGeo.hid, a), current);

  const chip = (active: boolean) =>
    `flex h-8 min-w-9 items-center justify-center gap-1.5 rounded-lg border px-2.5 text-[13px] transition-colors disabled:cursor-not-allowed ${
      active
        ? "border-signal bg-signal/15 text-ink"
        : "border-line bg-surface-2 text-ink-2 hover:border-line-strong hover:text-ink"
    }`;

  return (
    <section className={`card flex flex-col gap-4 p-5 ${off ? "opacity-60" : ""}`} aria-disabled={off}>
      <div className="flex items-center justify-between gap-4">
        <div className="eyebrow">{t("km.target")}</div>
        <Segmented
          label={t("km.target")}
          value={category}
          onChange={setCategory}
          disabled={!catalog}
          options={[
            { value: "keys", label: t("km.cat.keys") },
            { value: "media", label: t("km.cat.media") },
            { value: "mouse", label: t("km.cat.mouse") },
            { value: "macro", label: t("km.cat.macro") },
            { value: "special", label: t("km.cat.special") },
          ]}
        />
      </div>

      {!keyGeo && (
        <p className="flex items-center gap-2 text-[13px] text-ink-2">
          <Info size={14} className="shrink-0 text-ink-3" />
          {t("km.selectHint")}
        </p>
      )}

      {category === "keys" && catalog && (
        <div className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-2 rounded-lg border border-line bg-bg-raise px-3 py-2.5">
            <span className="mr-1 text-[12px] font-medium">{t("km.with")}</span>
            {MOD_TOGGLES.map((m) => (
              <button
                key={m.bit}
                type="button"
                aria-pressed={(mods & m.bit) !== 0}
                disabled={off}
                onClick={() => setMods(mods ^ m.bit)}
                className={chip((mods & m.bit) !== 0)}
              >
                {m.name}
              </button>
            ))}
            <span className="ml-auto text-[11px] text-ink-3">{t("km.with.hint")}</span>
          </div>
          {GROUPS.map((g) => {
            const items = catalog.keys.filter((k) => k.group === g);
            return (
              <div key={g} className="flex flex-col gap-2">
                <div className="text-[11px] font-medium tracking-wide text-ink-3 uppercase">
                  {t(`km.group.${g}` as MessageKey)}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {items.map((u) => {
                    const a = usageAction(u.code, mods);
                    return (
                      <button
                        key={u.code}
                        type="button"
                        disabled={off}
                        title={u.label}
                        aria-pressed={isCurrent(a)}
                        onClick={() => assign(a)}
                        className={`${chip(isCurrent(a))} num`}
                      >
                        {g === "keypad" ? usageShort(u) : (layout(u.code) ?? u.label)}
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {category === "media" && catalog && (
        <div className="grid grid-cols-3 gap-2">
          {catalog.media.map((m) => {
            const a: KeyActionInfo = { kind: "media", usage: m.usage };
            const Icon = MEDIA_ICONS[m.id];
            return (
              <button
                key={m.usage}
                type="button"
                disabled={off}
                aria-pressed={isCurrent(a)}
                onClick={() => assign(a)}
                className={`${chip(isCurrent(a))} h-10 justify-start`}
              >
                {Icon && <Icon size={15} />}
                <span className="truncate">{t(`km.media.${m.id}` as MessageKey)}</span>
              </button>
            );
          })}
        </div>
      )}

      {category === "mouse" && catalog && (
        <div className="grid grid-cols-3 gap-2">
          {[
            ...catalog.mouse.map((b) => ({ a: { kind: "mouse", button: b } as KeyActionInfo, key: `mouse.${b}` })),
            { a: { kind: "wheel", up: true } as KeyActionInfo, key: "wheel.up" },
            { a: { kind: "wheel", up: false } as KeyActionInfo, key: "wheel.down" },
          ].map(({ a, key }) => (
            <button
              key={key}
              type="button"
              disabled={off}
              aria-pressed={isCurrent(a)}
              onClick={() => assign(a)}
              className={`${chip(isCurrent(a))} h-10 justify-start`}
            >
              <Mouse size={15} />
              <span className="truncate">{t(`km.${key}` as MessageKey)}</span>
            </button>
          ))}
        </div>
      )}

      {category === "macro" && (
        <MacroPicker off={off} current={current} assign={assign} isCurrent={isCurrent} chip={chip} keyLed={keyGeo?.led} />
      )}

      {category === "special" && (
        <div className="grid grid-cols-2 gap-2">
          {(
            [
              { a: { kind: "default" }, label: "km.default", hint: "km.default.hint", icon: <RotateCcw size={15} /> },
              { a: { kind: "disabled" }, label: "km.disabled", hint: "km.disabled.hint", icon: <Ban size={15} /> },
            ] as { a: KeyActionInfo; label: MessageKey; hint: MessageKey; icon: ReactNode }[]
          ).map(({ a, label, hint, icon }) => (
            <button
              key={label}
              type="button"
              disabled={off}
              aria-pressed={isCurrent(a)}
              onClick={() => assign(a)}
              className={`${chip(isCurrent(a))} h-auto flex-col items-start gap-1 py-2.5 text-left`}
            >
              <span className="flex items-center gap-1.5 font-medium text-ink">
                {icon}
                {t(label)}
              </span>
              <span className="text-[12px] text-ink-3">{t(hint)}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

const PLAY_MODES: { value: MacroPlayMode; label: MessageKey }[] = [
  { value: "once", label: "km.play.once" },
  { value: "times", label: "km.play.times" },
  { value: "toggle", label: "km.play.toggle" },
  { value: "hold", label: "km.play.hold" },
];

/** Pick a macro and how the key plays it. */
function MacroPicker({
  off,
  current,
  assign,
  isCurrent,
  chip,
  keyLed,
}: {
  off: boolean;
  current: KeyActionInfo | undefined;
  assign: (a: KeyActionInfo) => void;
  isCurrent: (a: KeyActionInfo) => boolean;
  chip: (active: boolean) => string;
  keyLed: number | undefined;
}) {
  const t = useT();
  const draft = useMacros((s) => s.draft);
  const dirtyKey = useMacros((s) => dirtySlots(s).join(","));
  const dirty = new Set(dirtyKey ? dirtyKey.split(",").map(Number) : []);
  const setPage = useApp((s) => s.setPage);
  const [mode, setMode] = useState<MacroPlayMode>("once");
  const [count, setCount] = useState(2);

  // Show how the selected key plays its macro now.
  useEffect(() => {
    if (current?.kind === "macro") {
      setMode(current.mode);
      if (current.mode === "times") setCount(current.count);
    } else {
      setMode("once");
    }
    // Only when another key is selected.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [keyLed]);

  const action = (slot: number): KeyActionInfo => ({
    kind: "macro",
    slot,
    mode,
    count: mode === "times" ? Math.min(255, Math.max(1, count)) : 1,
  });
  const macros = draft.map((m, slot) => ({ m, slot })).filter((x) => x.m !== null);

  if (macros.length === 0) {
    return (
      <div className="flex items-center justify-between gap-3 rounded-lg border border-line bg-bg-raise px-4 py-3 text-[13px] text-ink-2">
        {t("km.macro.none")}
        <button
          type="button"
          onClick={() => setPage("macros")}
          className="shrink-0 rounded-lg border border-line px-2.5 py-1 text-[12px] text-ink-2 hover:text-ink"
        >
          {t("km.macro.go")}
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-[12px] font-medium">{t("km.play")}</span>
        <Segmented
          label={t("km.play")}
          value={mode}
          onChange={(m) => {
            setMode(m);
            // Re-assign with the new mode when the key already plays a macro.
            if (current?.kind === "macro" && !off)
              assign({ ...current, mode: m, count: m === "times" ? Math.min(255, Math.max(1, count)) : 1 });
          }}
          disabled={off}
          options={PLAY_MODES.map((p) => ({ value: p.value, label: t(p.label) }))}
        />
        {mode === "times" && (
          <label className="flex items-center gap-1.5 text-[12px] text-ink-2">
            <input
              type="number"
              min={1}
              max={255}
              value={count}
              disabled={off}
              onChange={(e) => {
                const n = Math.min(255, Math.max(1, Number(e.target.value) || 1));
                setCount(n);
                if (current?.kind === "macro" && current.mode === "times" && !off) assign({ ...current, count: n });
              }}
              className="num h-8 w-16 rounded-lg border border-line bg-surface px-2 text-right outline-none focus:border-signal"
            />
            {t("km.play.count")}
          </label>
        )}
      </div>
      <div className="grid grid-cols-2 gap-2">
        {macros.map(({ m, slot }) => {
          const a = action(slot);
          return (
            <button
              key={slot}
              type="button"
              disabled={off}
              aria-pressed={isCurrent(a)}
              onClick={() => assign(a)}
              className={`${chip(isCurrent(a))} h-10 justify-start`}
            >
              <span className="num rounded-md bg-bg-raise px-1.5 text-[11px] text-ink-3">M{slot + 1}</span>
              <span className="truncate">{m?.name || t("mc.unnamed")}</span>
              {dirty.has(slot) && <span className="ml-auto text-[11px] text-signal">{t("km.macro.unsaved")}</span>}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function SelectedCard({
  layer,
  catalog,
  keyGeo,
}: {
  layer: KeymapLayer;
  catalog: KeymapCatalog | null;
  keyGeo: KeyGeo | null;
}) {
  const t = useT();
  const action = useKeymap((s) => (keyGeo ? s.draft[layer][keyGeo.led] : undefined));
  const assign = useAssign(layer, keyGeo);
  const names = useMacroNames();
  const layout = useLayoutLabel();
  if (!keyGeo) return null;
  const locked = keyGeo.hid === FN_USAGE;
  const own = layer === "base" ? t("km.ownFunction") : t("km.fnDefault");
  return (
    <section className="card flex flex-col gap-3 p-5">
      <div className="eyebrow">{t("km.selected")}</div>
      <div className="flex items-center gap-3">
        <span className="num flex h-11 min-w-11 items-center justify-center rounded-lg border border-line-strong bg-surface-2 px-2.5 text-[14px] font-semibold">
          {keyGeo.label || "␣"}
        </span>
        <span className="text-ink-3">→</span>
        <span className="font-display text-[17px] font-semibold">
          {locked ? "Fn" : action ? actionText(action, catalog, t, names, layout) : own}
        </span>
      </div>
      {locked ? (
        <p className="flex gap-2 text-[12px] text-ink-2">
          <Lock size={14} className="mt-0.5 shrink-0" />
          {t("km.fnLocked")}
        </p>
      ) : (
        action && (
          <button
            type="button"
            onClick={() => assign({ kind: "default" })}
            className="flex items-center gap-1.5 self-start text-[12px] text-ink-3 hover:text-ink"
          >
            <RotateCcw size={12} />
            {t("km.resetKey")}
          </button>
        )
      )}
    </section>
  );
}

function SaveCard({ usb, loaded, layer }: { usb: boolean; loaded: boolean; layer: KeymapLayer }) {
  const t = useT();
  const ago = useAgo();
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const dirty = useKeymap((s) => dirtyLayers(s).length > 0);
  const changed = useKeymap((s) => countChanges(s.draft, s.saved));
  const layerEmpty = useKeymap((s) => Object.keys(s.draft[layer]).length === 0);
  const writtenMs = useKeymap((s) => s.writtenMs[layer]);
  const { save, discard, resetLayer } = useKeymap();

  const run = async () => {
    setStatus({ kind: "busy" });
    const started = performance.now();
    try {
      await save();
      setStatus({ kind: "ok", ms: Math.round(performance.now() - started) });
    } catch (e) {
      setStatus({ kind: "error", message: friendlyError(e, t) });
    }
  };

  return (
    <section className="card flex flex-col gap-4 p-5">
      <button
        type="button"
        disabled={!usb || !loaded || !dirty || status.kind === "busy"}
        onClick={run}
        className="flex h-10 items-center justify-center gap-2 rounded-lg bg-signal text-[13.5px] font-semibold text-signal-ink transition-transform active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-40"
      >
        {status.kind === "busy" && <Loader2 size={15} className="animate-spin" />}
        {status.kind === "busy" ? t("km.saving") : t("km.save")}
        {dirty && status.kind !== "busy" && (
          <span className="num rounded-full bg-signal-ink/15 px-1.5 text-[11px]">{changed}</span>
        )}
      </button>
      <StatusLine status={status} dirty={dirty} changed={changed} />
      <div className="flex flex-wrap gap-x-4 gap-y-2">
        <button
          type="button"
          disabled={!dirty || status.kind === "busy"}
          onClick={discard}
          className="flex items-center gap-1.5 text-[12px] text-ink-3 hover:text-ink disabled:opacity-40"
        >
          <Undo2 size={12} />
          {t("km.revert")}
        </button>
        <button
          type="button"
          disabled={layerEmpty || status.kind === "busy"}
          onClick={() => resetLayer(layer)}
          className="flex items-center gap-1.5 text-[12px] text-ink-3 hover:text-ink disabled:opacity-40"
        >
          <RotateCcw size={12} />
          {t("km.resetLayer")}
        </button>
      </div>
      <div className="flex flex-col gap-2.5 border-t border-line pt-4 text-[12px] text-ink-2">
        {layer === "fn" && (
          <p className="flex gap-2 rounded-lg border border-warn/30 bg-warn/10 p-3">
            <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
            {t("km.fnNote")}
          </p>
        )}
        <p className="flex gap-2">
          <Info size={14} className="mt-0.5 shrink-0 text-ink-3" />
          <span>
            {t("km.readNote")}{" "}
            <span className="text-ink-3">
              {writtenMs ? `${t("km.record.at")}: ${ago(writtenMs)} ${t("km.record.ago")}.` : t("km.record.never")}
            </span>
          </span>
        </p>
      </div>
    </section>
  );
}

function countChanges(draft: Record<KeymapLayer, LayerMap>, saved: Record<KeymapLayer, LayerMap> | null): number {
  if (!saved) return 0;
  let n = 0;
  for (const l of ["base", "fn"] as const) {
    const leds = new Set([...Object.keys(draft[l]), ...Object.keys(saved[l])].map(Number));
    for (const led of leds) if (!sameAction(draft[l][led], saved[l][led])) n += 1;
  }
  return n;
}

function StatusLine({ status, dirty, changed }: { status: Status; dirty: boolean; changed: number }) {
  const t = useT();
  if (status.kind === "busy")
    return (
      <span className="flex items-center gap-2 text-[12px] text-ink-2">
        <Loader2 size={13} className="animate-spin" />
        {t("km.saving")}
      </span>
    );
  if (status.kind === "error")
    return (
      <span className="flex items-start gap-2 text-[12px] text-danger">
        <X size={13} className="mt-0.5 shrink-0" />
        {status.message}
      </span>
    );
  if (dirty)
    return (
      <span className="text-[12px] text-ink-2">
        <span className="num">{changed}</span> {t("km.changed")} ·{" "}
        <span className="text-ink-3">{t("km.persistNote")}</span>
      </span>
    );
  if (status.kind === "ok")
    return (
      <span className="flex items-center gap-2 text-[12px] text-ok">
        <Check size={13} />
        {t("km.saved")} <span className="num text-ink-3">· {status.ms} ms</span>
      </span>
    );
  return <span className="text-[12px] text-ink-3">{t("km.persistNote")}</span>;
}

/** Media keys as the browser names them (KeyboardEvent.key). */
const BROWSER_MEDIA: Record<string, string> = {
  MediaPlayPause: "play-pause",
  MediaTrackNext: "next",
  MediaTrackPrevious: "previous",
  MediaStop: "stop",
  AudioVolumeUp: "volume-up",
  AudioVolumeDown: "volume-down",
  AudioVolumeMute: "mute",
  LaunchMediaPlayer: "media-player",
  LaunchMail: "mail",
  LaunchApplication2: "calculator",
  LaunchApplication1: "my-computer",
  BrowserHome: "browser-home",
  BrowserBack: "browser-back",
  BrowserForward: "browser-forward",
  BrowserStop: "browser-stop",
  BrowserRefresh: "browser-refresh",
  BrowserFavorites: "browser-favorites",
  BrowserSearch: "browser-search",
};

const MOUSE_BUTTONS = ["left", "middle", "right", "back", "forward"] as const;

/** Shows what reaches Windows when keys are pressed, to check a new keymap. */
function KeyTester() {
  const t = useT();
  const [events, setEvents] = useState<{ id: number; text: string }[]>([]);
  const push = (text: string) => setEvents((list) => [{ id: performance.now(), text }, ...list].slice(0, 24));
  const keyText = (e: KeyboardEvent) => {
    const media = BROWSER_MEDIA[e.key];
    if (media) return t(`km.media.${media}` as MessageKey);
    const mods = [e.ctrlKey && "Ctrl", e.shiftKey && "Shift", e.altKey && "Alt", e.metaKey && "Win"].filter(Boolean);
    const isMod = ["Control", "Shift", "Alt", "Meta"].includes(e.key);
    const name = e.key.length === 1 ? e.key.toUpperCase() : e.key === " " ? "Space" : e.key;
    return isMod ? (e.code.endsWith("Right") ? `R ${name}` : name) : [...mods, name].join(" + ");
  };
  return (
    <section className="card flex flex-col gap-3 p-5">
      <div className="flex items-baseline justify-between">
        <div className="eyebrow">{t("km.test.title")}</div>
        <span className="text-[12px] text-ink-3">{t("km.test.hint")}</span>
      </div>
      <div
        tabIndex={0}
        role="textbox"
        aria-label={t("km.test.title")}
        onKeyDown={(e) => {
          if (e.key === "Tab") return;
          e.preventDefault();
          if (!e.repeat) push(keyText(e));
        }}
        onMouseDown={(e) => {
          if (e.button !== 0) e.preventDefault();
          push(`${t("km.test.mouse")} ${t(`km.mouse.${MOUSE_BUTTONS[e.button] ?? "left"}` as MessageKey)}`);
        }}
        onAuxClick={(e) => e.preventDefault()}
        onContextMenu={(e) => e.preventDefault()}
        onWheel={(e) => push(`${t("km.test.wheel")} ${e.deltaY < 0 ? "↑" : "↓"}`)}
        className="flex min-h-14 flex-wrap items-center gap-1.5 rounded-lg border border-dashed border-line-strong bg-bg-raise px-3 py-2.5 outline-none focus:border-signal"
      >
        {events.length === 0 ? (
          <span className="text-[13px] text-ink-3">{t("km.test.empty")}</span>
        ) : (
          events.map((ev, i) => (
            <span
              key={ev.id}
              className={`num rounded-lg border px-2 py-1 text-[12px] ${
                i === 0 ? "border-signal bg-signal/15 text-ink" : "border-line text-ink-2"
              }`}
            >
              {ev.text}
            </span>
          ))
        )}
      </div>
    </section>
  );
}
