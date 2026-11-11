import { Check, ChevronDown, Images, Info, Library, Loader2, Palette, Pencil, Radio, Search, Trash2, Upload, Usb, X, type LucideIcon } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ScreenItem } from "../bindings/ScreenItem";
import { useT, type MessageKey } from "../lib/i18n";
import { useLive } from "../lib/live";
import { itemData, useScreen } from "../lib/screen";
import { fromBlob, uploadSeconds, type Source } from "../lib/screenImport";
import { useApp, type ScreenTab } from "../lib/store";
import {
  ConfirmUpload,
  FramePlayer,
  Stepper,
  Thumb,
  fmtSeconds,
  loadFrames,
  pickThumb,
  remaining,
  useNow,
  type Done,
} from "./screen/common";
import { FileFlow, FileTile } from "./screen/FileFlow";
import { LivePanel } from "./screen/LivePanel";
import { MatchLightsDialog } from "./screen/MatchLights";
import { SlideFlow, SlideTile } from "./screen/SlideFlow";
import { TextFlow, TextTile } from "./screen/TextFlow";
import { friendlyError } from "../lib/errors";

// -------------------------------------------------------------- the page

const TABS: { id: ScreenTab; label: MessageKey; icon: LucideIcon }[] = [
  { id: "visuals", label: "sc.tab.visuals", icon: Images },
  { id: "live", label: "sc.tab.live", icon: Radio },
];

/**
 * The Screen page in two tabs: "Görseller" (make a visual, and the library
 * of visuals made before, right under it) and "Canlı ekran" (cards that
 * update the screen by themselves). What the keyboard's screen shows sits
 * above both. The focus timer has its own page (it is one of the live
 * screen's sources, but mostly lights the keys).
 */
export function Screen({ kb }: { kb: KeyboardInfo }) {
  const t = useT();
  const usb = kb.connection === "usb" && kb.hasCommandChannel && kb.features.includes("screen");
  const state = useScreen((s) => s.state);
  const modelId = useScreen((s) => s.modelId);
  const loaded = state !== null && modelId === kb.modelId;
  const tab = useApp((s) => s.screenTab);
  const setTab = useApp((s) => s.setScreenTab);

  useEffect(() => {
    if (!loaded) useScreen.getState().load(kb.modelId).catch(console.error);
  }, [kb.modelId, loaded]);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-end justify-between gap-6">
        <div>
          <h1 className="page-title">{t("sc.title")}</h1>
        </div>
        <div role="tablist" aria-label={t("sc.title")} className="inline-flex rounded-lg border border-line bg-bg-raise p-1">
          {TABS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              role="tab"
              type="button"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={`flex items-center gap-2 rounded-lg px-4 py-1.5 text-[13px] transition-colors ${
                tab === id ? "bg-surface-2 text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:text-ink"
              }`}
              data-screen-tab={id}
            >
              <Icon size={14} className={tab === id ? "text-signal" : "text-ink-3"} />
              {t(label)}
            </button>
          ))}
        </div>
      </header>

      {!usb && (
        <div className="flex items-center gap-3 rounded-lg border border-signal/30 bg-signal/10 px-4 py-3 text-[13px]">
          <Usb size={16} className="text-signal" />
          {t("sc.needsUsb")}
        </div>
      )}

      <KeyboardScreens />

      {/* Kept mounted on other tabs, so a visual being made is not lost. */}
      <div className={tab === "visuals" ? "flex flex-col gap-6" : "hidden"}>
        <Studio kb={kb} usb={usb} loaded={loaded} />
        <LibraryCard kb={kb} usb={usb} />
      </div>
      {tab === "live" && <LivePanel usb={usb} />}
    </div>
  );
}

// ------------------------------------------------------ keyboard screens

function pad(n: number) {
  return String(n).padStart(2, "0");
}

/** The firmware's home page (clock), as on the owner's keyboard photo. */
function ClockPage() {
  const t = useT();
  const now = useNow();
  const mono = "JetBrains Mono Variable, monospace";
  const chips = [
    { l: "WIN", c: "#5fe3ff", on: true },
    { l: "USB", c: "#8a6bff", on: true },
    { l: "2.4", c: "#7ee08a", on: false },
    { l: "BT1", c: "#3aa0ff", on: false },
    { l: "lock", c: "#e8d24a", on: false },
    { l: "A", c: "#e8d24a", on: false },
    { l: "1", c: "#e8d24a", on: false },
  ];
  return (
    <svg viewBox="0 0 240 135" className="block h-full w-full" role="img" aria-label={t("sc.page.home")}>
      <defs>
        <linearGradient id="sc-home" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#0d2a5a" />
          <stop offset="1" stopColor="#071a3c" />
        </linearGradient>
      </defs>
      <rect width="240" height="135" fill="url(#sc-home)" />
      <text x="10" y="20" fill="#5fd4ff" fontFamily={mono} fontSize="11">
        {`${now.getFullYear()}/${pad(now.getMonth() + 1)}/${pad(now.getDate())}`}
      </text>
      <path d="M190 10 l-4 7 h3 l-2 6 l5 -8 h-3 l2 -5 z" fill="#c6f04a" />
      <rect x="199" y="10" width="26" height="12" rx="2" fill="none" stroke="#7ee08a" strokeWidth="1.4" />
      <rect x="225.5" y="13.5" width="2" height="5" fill="#7ee08a" />
      {[0, 1, 2, 3, 4].map((i) => (
        <rect key={i} x={201.5 + i * 4.6} y="12.5" width="3.4" height="7" fill="#7ee08a" />
      ))}
      <text
        x="120"
        y="76"
        textAnchor="middle"
        fill="#f4f8ff"
        fontFamily="'Times New Roman', Times, serif"
        fontSize="34"
      >
        {`${pad(now.getHours())}: ${pad(now.getMinutes())}: ${pad(now.getSeconds())}`}
      </text>
      {chips.map((c, i) => (
        <g key={c.l} transform={`translate(${9 + i * 32} 104)`}>
          <rect width="26" height="22" rx="4" fill={c.on ? c.c : "none"} fillOpacity={0.85} stroke={c.c} strokeWidth="1.5" />
          {c.l === "lock" ? (
            <g fill="none" stroke={c.c} strokeWidth="1.6">
              <path d="M9.5 10.5 V7.8 a3.5 3.5 0 0 1 7 0 V10.5" />
              <rect x="7.5" y="10.5" width="11" height="7.5" rx="1.2" />
            </g>
          ) : (
            <text
              x="13"
              y="15.5"
              textAnchor="middle"
              fill={c.on ? "#071426" : c.c}
              fontFamily={mono}
              fontSize={c.l.length > 2 ? 8 : 12}
              fontWeight="700"
            >
              {c.l}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}

function Panel({ label, sub, children }: { label: string; sub: string; children: ReactNode }) {
  return (
    <figure className="flex flex-col gap-2">
      <div className="relative aspect-[240/135] w-full overflow-hidden rounded-lg bg-black shadow-[0_0_0_6px_#0b0c0e,0_0_0_7px_var(--line)]">
        {children}
      </div>
      <figcaption className="flex items-baseline justify-between gap-2 px-1">
        <span className="text-[13px] font-medium">{label}</span>
        <span className="text-[11px] text-ink-3">{sub}</span>
      </figcaption>
    </figure>
  );
}

function KeyboardScreens() {
  const t = useT();
  const state = useScreen((s) => s.state);
  const shownId = state?.shown ?? null;
  const factory = state?.items.find((i) => i.kind === "factory") ?? null;
  const liveLast = useLive((s) => s.state?.last ?? null);
  const showsLive = shownId === "live";
  const shown = state?.items.find((i) => i.id === shownId) ?? null;
  const item = shown ?? factory;
  const [frames, setFrames] = useState<{ frames: Uint16Array[]; delaysMs: number[] } | null>(null);

  useEffect(() => {
    setFrames(null);
    let live = true;
    // A live card changes under the same id: read it fresh each time.
    const load = showsLive ? itemData("live").then(fromBlob) : item ? loadFrames(item.id) : null;
    load?.then((f) => live && setFrames(f), console.error);
    return () => {
      live = false;
    };
  }, [item?.id, showsLive, liveLast?.atMs]);

  return (
    <section className="card grid grid-cols-[1fr_1fr_minmax(220px,0.8fr)] items-start gap-6 p-5">
      <Panel label={t("sc.page.home")} sub={t("sc.page.home.sub")}>
        <ClockPage />
      </Panel>
      <Panel
        label={t("sc.page.gif")}
        sub={
          showsLive
            ? `${t("sc.page.gif.live")}: ${liveLast?.name ?? ""}`
            : shown
              ? shown.name
              : factory
                ? t("sc.page.gif.maybeFactory")
                : t("sc.page.gif.unknown")
        }
      >
        {frames ? (
          <FramePlayer
            frames={frames.frames}
            delaysMs={frames.delaysMs}
            className={`h-full w-full ${shown || showsLive ? "" : "opacity-60"}`}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-[12px] text-ink-3">
            {item ? <Loader2 size={16} className="animate-spin" /> : t("sc.page.gif.unknown")}
          </div>
        )}
      </Panel>
      <div className="flex flex-col gap-3 text-[13px] text-ink-2">
        {state && (
          <div className="num flex flex-col gap-0.5 text-[12px] text-ink-3" title={t("sc.wear.hint")}>
            <span>
              {t("sc.uploads.today")}: <span className="text-ink-2">{state.uploadsToday}</span> · {t("sc.uploads.total")}:{" "}
              <span className="text-ink-2">{state.uploadsTotal}</span>
            </span>
            <span>{t("sc.wear.short")}</span>
          </div>
        )}
        <p className="flex gap-2">
          <Info size={14} className="mt-0.5 shrink-0 text-signal" />
          <span>{t("sc.pages.note")}</span>
        </p>
        <p className="flex gap-2">
          <Check size={14} className="mt-0.5 shrink-0 text-ok" />
          <span>{t("sc.pages.safe")}</span>
        </p>
      </div>
    </section>
  );
}

// ------------------------------------------------------------- the studio

type Flow = { kind: "file"; source: Source } | { kind: "text" } | { kind: "slides" };

/** Step 1 picks a source; each source runs steps 2–4 in its own flow and
 *  comes back here with what it did. */
function Studio({ kb, usb, loaded }: { kb: KeyboardInfo; usb: boolean; loaded: boolean }) {
  const t = useT();
  const [flow, setFlow] = useState<Flow | null>(null);
  const [done, setDone] = useState<Done | null>(null);
  const [matching, setMatching] = useState(false);
  const doneItem = useScreen((s) => (done?.id ? (s.state?.items.find((i) => i.id === done.id) ?? null) : null));
  const fileSource = flow?.kind === "file" ? flow.source : null;

  // The decoded file holds bitmaps and decoders; free them when it goes.
  useEffect(() => () => fileSource?.dispose(), [fileSource]);

  const start = (f: Flow) => {
    setDone(null);
    setFlow(f);
  };
  const finish = (d: Done) => {
    setFlow(null);
    setDone(d);
  };
  const common = { usb, loaded, onCancel: () => setFlow(null), onFinished: finish };

  if (flow?.kind === "file") return <FileFlow key={flow.source.name + flow.source.durationMs} source={flow.source} {...common} />;
  if (flow?.kind === "text") return <TextFlow {...common} />;
  if (flow?.kind === "slides") return <SlideFlow {...common} />;

  return (
    <section className="card flex flex-col gap-4 p-5">
      <Stepper step={1} />
      <AnimatePresence>
        {done && (
          <motion.div
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            role="status"
            className="flex items-start gap-3 rounded-lg border border-ok/30 bg-ok/10 px-4 py-3"
          >
            <Check size={18} className="mt-0.5 shrink-0 text-ok" />
            <div className="flex-1 text-[13px]">
              <div className="font-semibold">
                “{done.name}” {done.kind === "uploaded" ? t("sc.done.uploaded") : t("sc.done.saved")}
                {done.seconds != null && <span className="num ml-2 font-normal text-ink-3">{fmtSeconds(done.seconds)}</span>}
              </div>
              <div className="text-ink-2">{done.kind === "uploaded" ? t("sc.done.uploadedHint") : t("sc.done.savedHint")}</div>
              {doneItem && kb.geometry && (
                <button
                  type="button"
                  onClick={() => setMatching(true)}
                  className="mt-2 flex h-8 items-center gap-1.5 rounded-lg border border-line bg-surface px-3 text-[12px] font-medium hover:bg-surface-2"
                >
                  <Palette size={13} /> {t("sc.lm.action")}
                </button>
              )}
            </div>
            <button
              type="button"
              onClick={() => setDone(null)}
              aria-label={t("sc.close")}
              className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
            >
              <X size={14} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>
      <div>
        <h2 className="font-display text-[22px] font-semibold">
          {done ? t("sc.src.titleNext") : t("sc.src.title")}
        </h2>
        <p className="mt-1 max-w-[70ch] text-[13px] text-ink-2">{t("sc.src.body")}</p>
      </div>
      <div className="grid grid-cols-3 gap-4">
        <FileTile onOpened={(source) => start({ kind: "file", source })} />
        <TextTile onStart={() => start({ kind: "text" })} />
        <SlideTile onStart={() => start({ kind: "slides" })} />
      </div>
      <AnimatePresence>
        {matching && doneItem && <MatchLightsDialog kb={kb} item={doneItem} onClose={() => setMatching(false)} />}
      </AnimatePresence>
    </section>
  );
}

// --------------------------------------------------------------- library

/** Shown at first; more come in steps, so a large library stays light
 *  (every card loads its picture). */
const LIBRARY_PAGE = 6;
const LIBRARY_MORE = 12;
/** A search box once the library is this big. */
const LIBRARY_SEARCH_FROM = 12;
type LibraryFilter = "all" | "image" | "animation";
const LIBRARY_FILTERS: { id: LibraryFilter; label: MessageKey }[] = [
  { id: "all", label: "sc.lib.filter.all" },
  { id: "image", label: "sc.lib.filter.image" },
  { id: "animation", label: "sc.lib.filter.animation" },
];

function LibraryCard({ kb, usb }: { kb: KeyboardInfo; usb: boolean }) {
  const t = useT();
  const state = useScreen((s) => s.state);
  const upload = useScreen((s) => s.upload);
  const [confirmItem, setConfirmItem] = useState<ScreenItem | null>(null);
  const [matchItem, setMatchItem] = useState<ScreenItem | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const [kind, setKind] = useState<LibraryFilter>("all");
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(LIBRARY_PAGE);
  const all = state?.items ?? [];
  // What the keyboard shows first, then the factory animation, then newest.
  const items = useMemo(() => {
    const q = query.trim().toLocaleLowerCase();
    const shownId = state?.shown;
    return all
      .filter((it) => kind === "all" || (kind === "image" ? it.kind === "image" : it.kind !== "image"))
      .filter((it) => !q || it.name.toLocaleLowerCase().includes(q))
      .sort((a, b) => Number(b.id === shownId) - Number(a.id === shownId));
  }, [all, kind, query, state?.shown]);
  const visible = items.slice(0, limit);
  const rest = items.length - visible.length;
  useEffect(() => setLimit(LIBRARY_PAGE), [kind, query]);

  const runUpload = async (it: ScreenItem) => {
    setConfirmItem(null);
    setNotice(null);
    try {
      await useScreen.getState().uploadToKeyboard(it, "library");
      setNotice({ ok: true, text: `“${it.name}” ${t("sc.done.uploaded")}. ${t("sc.done.fnHome")}` });
    } catch (e) {
      setNotice({ ok: false, text: friendlyError(e, t) });
    }
  };

  return (
    <section className="card flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2 px-1">
        <div className="flex items-center gap-2">
          <Library size={14} className="text-ink-3" />
          <div className="eyebrow">{t("sc.library")}</div>
          <span className="num text-[11px] text-ink-3">{all.length}</span>
        </div>
        <span className="text-[12px] text-ink-3">{t("sc.library.note")}</span>
      </div>
      {all.length > LIBRARY_PAGE && (
        <div className="flex flex-wrap items-center gap-2 px-1">
          <div role="radiogroup" aria-label={t("sc.library")} className="inline-flex rounded-lg border border-line bg-bg-raise p-0.5">
            {LIBRARY_FILTERS.map((f) => (
              <button
                key={f.id}
                type="button"
                role="radio"
                aria-checked={kind === f.id}
                onClick={() => setKind(f.id)}
                className={`rounded-md px-3 py-1 text-[12px] ${
                  kind === f.id ? "bg-surface-2 text-ink shadow-[inset_0_0_0_1px_var(--line)]" : "text-ink-2 hover:text-ink"
                }`}
              >
                {t(f.label)}
              </button>
            ))}
          </div>
          {all.length > LIBRARY_SEARCH_FROM && (
            <label className="flex h-8 min-w-[220px] flex-1 items-center gap-2 rounded-lg border border-line bg-bg-raise px-2.5 sm:max-w-[320px]">
              <Search size={13} className="text-ink-3" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("sc.lib.search")}
                aria-label={t("sc.lib.search")}
                className="min-w-0 flex-1 bg-transparent text-[13px] outline-none"
              />
              {query && (
                <button type="button" onClick={() => setQuery("")} aria-label={t("sc.close")} className="text-ink-3 hover:text-ink">
                  <X size={13} />
                </button>
              )}
            </label>
          )}
        </div>
      )}
      {notice && (
        <p className={`flex items-start gap-2 px-1 text-[13px] ${notice.ok ? "text-ok" : "text-danger"}`} role="status">
          {notice.ok ? <Check size={14} className="mt-0.5 shrink-0" /> : <X size={14} className="mt-0.5 shrink-0" />}
          {notice.text}
        </p>
      )}
      {all.length === 0 && <p className="px-1 pb-1 text-[13px] text-ink-3">{t("sc.library.empty")}</p>}
      {all.length > 0 && items.length === 0 && <p className="px-1 pb-1 text-[13px] text-ink-3">{t("sc.lib.none")}</p>}
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-2" data-library>
        {visible.map((it) => {
          const onKeyboard = state?.shown === it.id;
          const busy = upload?.id === it.id;
          return (
            <li
              key={it.id}
              className={`flex gap-3 rounded-lg border p-3 transition-colors ${
                onKeyboard ? "border-ok/40 bg-ok/5" : "border-line hover:bg-surface-2/60"
              }`}
            >
              <Thumb item={it} />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                {editing === it.id ? (
                  <input
                    autoFocus
                    defaultValue={it.name}
                    maxLength={60}
                    aria-label={t("sc.rename")}
                    onBlur={(e) => {
                      setEditing(null);
                      const v = e.target.value.trim();
                      if (v && v !== it.name) useScreen.getState().rename(it.id, v).catch((x) => setNotice({ ok: false, text: String(x) }));
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                      if (e.key === "Escape") setEditing(null);
                    }}
                    className="h-7 rounded-md border border-signal bg-bg-raise px-2 text-[13px] outline-none"
                  />
                ) : (
                  <span className="truncate text-[13px] font-semibold" title={it.name}>
                    {it.name}
                  </span>
                )}
                <span className="num text-[11px] text-ink-3">
                  {it.frames} {t("sc.frames")}
                  {it.frames > 1 ? ` · ${t("sc.loop")} ${fmtSeconds(it.loopMs / 1000)}` : ""}
                </span>
                <span className="num text-[11px] text-ink-3">
                  {t("sc.uploadTime")} ~{fmtSeconds(uploadSeconds(it.frames))}
                </span>
                <span className="flex gap-1.5 pt-1">
                  {it.kind === "factory" && (
                    <span className="rounded-full bg-surface-2 px-2 py-px text-[11px] text-ink-2">{t("sc.badge.factory")}</span>
                  )}
                  {onKeyboard && (
                    <span className="rounded-full bg-ok/15 px-2 py-px text-[11px] text-ok">{t("sc.badge.onKeyboard")}</span>
                  )}
                </span>
              </div>
              <div className="flex flex-col items-end justify-between gap-2">
                <button
                  type="button"
                  disabled={!usb || !!upload}
                  title={!usb ? t("sc.needsUsbShort") : it.kind === "factory" ? t("sc.factory.restoreHint") : undefined}
                  onClick={() => setConfirmItem(it)}
                  className="flex h-8 items-center gap-1.5 rounded-lg border border-signal/40 px-2.5 text-[12px] font-medium text-signal hover:bg-signal/10 disabled:opacity-35"
                >
                  {busy ? <Loader2 size={13} className="animate-spin" /> : <Upload size={13} />}
                  {it.kind === "factory" ? t("sc.factory.restore") : t("sc.upload.short")}
                </button>
                <div className="flex gap-1">
                  {kb.geometry && (
                    <button
                      type="button"
                      title={t("sc.lm.action")}
                      aria-label={t("sc.lm.action")}
                      onClick={() => setMatchItem(it)}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
                    >
                      <Palette size={13} />
                    </button>
                  )}
                  {it.kind !== "factory" && (
                    <button
                      type="button"
                      title={t("sc.rename")}
                      aria-label={t("sc.rename")}
                      onClick={() => setEditing(it.id)}
                      className="flex h-7 w-7 items-center justify-center rounded-md text-ink-3 hover:bg-surface hover:text-ink"
                    >
                      <Pencil size={13} />
                    </button>
                  )}
                  {it.kind !== "factory" && (
                    <button
                      type="button"
                      title={deleting === it.id ? t("sc.remove.confirm") : t("sc.remove")}
                      aria-label={deleting === it.id ? t("sc.remove.confirm") : t("sc.remove")}
                      disabled={busy}
                      onBlur={() => setDeleting(null)}
                      onClick={() => {
                        if (deleting !== it.id) return setDeleting(it.id);
                        setDeleting(null);
                        useScreen.getState().remove(it.id).catch((x) => setNotice({ ok: false, text: String(x) }));
                      }}
                      className={`flex h-7 items-center justify-center gap-1 rounded-md px-1.5 text-[12px] ${
                        deleting === it.id ? "bg-danger/15 text-danger" : "text-ink-3 hover:bg-surface hover:text-danger"
                      }`}
                    >
                      <Trash2 size={13} />
                      {deleting === it.id && t("sc.remove.confirmShort")}
                    </button>
                  )}
                </div>
              </div>
            </li>
          );
        })}
      </ul>
      {(rest > 0 || limit > LIBRARY_PAGE) && (
        <div className="flex flex-wrap items-center justify-center gap-2 pt-1">
          {rest > 0 && (
            <button
              type="button"
              onClick={() => setLimit((n) => n + LIBRARY_MORE)}
              className="flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-[13px] text-ink-2 hover:bg-surface-2 hover:text-ink"
              data-library-more
            >
              <ChevronDown size={14} /> {t("sc.lib.more")}
              <span className="num text-ink-3">
                · {rest} {t("sc.lib.left")}
              </span>
            </button>
          )}
          {limit > LIBRARY_PAGE && (
            <button
              type="button"
              onClick={() => setLimit(LIBRARY_PAGE)}
              className="h-8 rounded-lg px-3 text-[13px] text-ink-3 hover:bg-surface-2 hover:text-ink"
            >
              {t("sc.lib.less")}
            </button>
          )}
        </div>
      )}
      <AnimatePresence>
        {confirmItem && (
          <LibraryConfirm item={confirmItem} onCancel={() => setConfirmItem(null)} onConfirm={() => runUpload(confirmItem)} />
        )}
        {matchItem && <MatchLightsDialog kb={kb} item={matchItem} onClose={() => setMatchItem(null)} />}
      </AnimatePresence>
    </section>
  );
}

function LibraryConfirm({ item, onCancel, onConfirm }: { item: ScreenItem; onCancel: () => void; onConfirm: () => void }) {
  const [first, setFirst] = useState<Uint16Array | null>(null);
  useEffect(() => {
    loadFrames(item.id, item.frames === 1).then((d) => setFirst(pickThumb(d.frames)), console.error);
  }, [item.id, item.frames]);
  if (!first) return null;
  return <ConfirmUpload frames={item.frames} loopMs={item.loopMs} preview={first} onCancel={onCancel} onConfirm={onConfirm} />;
}

// ----------------------------------------------------- upload progress

/** Floating progress while an upload runs. Hidden on the Screen page when
 *  the studio itself shows the progress. */
export function ScreenUploadToast() {
  const t = useT();
  const upload = useScreen((s) => s.upload);
  const page = useApp((s) => s.page);
  const [now, setNow] = useState(Date.now());
  const running = upload !== null;
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, [running]);
  const visible = !!upload && !(page === "screen" && upload.origin === "editor");
  return (
    <AnimatePresence>
      {visible && upload && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          className="card fixed right-6 bottom-6 z-40 flex w-[340px] flex-col gap-2 p-4 shadow-xl"
          role="status"
        >
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <Loader2 size={14} className="animate-spin text-signal" />
            <span className="truncate">
              {t("sc.uploading")}: {upload.name}
            </span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full bg-signal transition-[width]"
              style={{ width: `${Math.round((upload.done / Math.max(1, upload.total)) * 100)}%` }}
            />
          </div>
          <div className="num flex justify-between text-[12px] text-ink-3">
            <span>
              {upload.done}/{upload.total} {t("sc.pages")}
            </span>
            <span>
              {remaining(upload, now) ? `${remaining(upload, now)} ${t("sc.left")}` : ""}
            </span>
          </div>
          <p className="text-[12px] text-ink-2">{t("sc.uploading.note")}</p>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
