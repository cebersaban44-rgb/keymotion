import { AlertTriangle, Check, ChevronDown, Clock3, FileImage, Images, Info, Loader2, Palette, Pencil, Radio, Search, Trash2, Type, Upload, Usb, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
import type { ScreenItem } from "../bindings/ScreenItem";
import { percent, useT, type MessageKey } from "../lib/i18n";
import { builtIn, useScreen } from "../lib/screen";
import { openSource, uploadSeconds, type Source } from "../lib/screenImport";
import { InfoBar, Tabs } from "../components/controls";
import { useApp, type ScreenTab } from "../lib/store";
import {
  DriverBackup,
  FramePlayer,
  GifPicture,
  Hint,
  errorKey,
  loadFrames,
  pickThumb,
  LibraryConfirm,
  RetryUpload,
  fmtSeconds,
  remaining,
  useGifPage,
  useHasFactory,
  useItemName,
  useNow,
  type Done,
} from "./screen/common";
import { ACCEPT, FileFlow } from "./screen/FileFlow";
import { LivePanel } from "./screen/LivePanel";
import { MatchLightsDialog } from "./screen/MatchLights";
import { SlideFlow } from "./screen/SlideFlow";
import { TextFlow } from "./screen/TextFlow";
import { friendlyError } from "../lib/errors";

// -------------------------------------------------------------- the page

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
    <div className="flex flex-col gap-5">
      <header>
        <h1 className="page-title">{t("sc.title")}</h1>
        <p className="page-desc">{t("sc.desc")}</p>
        <Tabs<ScreenTab>
          className="mt-3.5"
          label={t("sc.title")}
          value={tab}
          onChange={setTab}
          data="screen-tab"
          options={[
            { value: "visuals", label: t("sc.tab.visuals"), icon: Images, data: "screen-tab" },
            { value: "live", label: t("sc.tab.live"), icon: Radio, data: "screen-tab" },
          ]}
        />
      </header>

      {!usb && <InfoBar tone="warn" icon={<Usb size={20} strokeWidth={1.6} />} title={t("sc.needsUsb")} data="usb" />}

      <KeyboardScreens usb={usb} />

      {/* Kept mounted on other tabs, so a visual being made is not lost. */}
      <div className={tab === "visuals" ? "flex flex-col gap-5" : "hidden"}>
        <Library kb={kb} usb={usb} loaded={loaded} />
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
export function ClockPage() {
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

/** A picture in the keyboard's black frame, with its name and a note under it. */
function Panel({
  label,
  sub,
  empty,
  className = "",
  children,
}: {
  label: string;
  sub: string;
  empty?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <figure className={`flex min-w-0 flex-col ${className}`}>
      <div className={`rounded-lg p-2 ${empty ? "border border-dashed border-line-strong" : "bg-[#040507] shadow-[0_0_0_1px_rgb(0_0_0/0.6),inset_0_1px_0_rgb(255_255_255/0.08)]"}`}>
        <div className={`relative aspect-[240/135] w-full overflow-hidden rounded-[2px] ${empty ? "" : "bg-black"}`}>{children}</div>
      </div>
      <figcaption className="mt-2.5 flex items-baseline justify-between gap-3">
        <span className="text-[13px] font-semibold">{label}</span>
        <span className="truncate text-[12px] text-ink-2" title={sub}>
          {sub}
        </span>
      </figcaption>
    </figure>
  );
}

function KeyboardScreens({ usb }: { usb: boolean }) {
  const t = useT();
  const state = useScreen((s) => s.state);
  const gif = useGifPage();
  const { waiting } = gif;

  return (
    <section className="card grid grid-cols-1 gap-6 p-5 @3xl:grid-cols-[minmax(0,496px)_minmax(0,1fr)]" data-screen-pages>
      <Panel label={t("sc.page.gif")} empty={!gif.item && !gif.showsLive} sub={gif.caption}>
        <GifPicture gif={gif} />
      </Panel>
      <div className="flex min-w-0 flex-col gap-4">
        <Panel label={t("sc.page.home")} sub={t("sc.page.home.sub")} className="max-w-[256px]">
          <ClockPage />
        </Panel>
        <div className="flex flex-col gap-2.5 text-[13px] text-ink-2">
          <IncompleteLine usb={usb} />
          {waiting && (
            <p className="flex gap-2" data-gif-waiting>
              <Clock3 size={14} className="mt-0.5 shrink-0 text-warn" />
              <span>{t("sc.page.gif.waiting", { name: waiting.name })}</span>
            </p>
          )}
          {state && (
            <p className="num text-[13px] font-semibold text-ink" title={t("sc.wear.hint")} data-upload-counts>
              {t("sc.uploads.counts", { count: state.uploadsToday, total: state.uploadsTotal })}
            </p>
          )}
          <p className="flex gap-2">
            <Info size={14} className="mt-0.5 shrink-0 text-signal" />
            <span>{t("sc.pages.note")}</span>
          </p>
          <p className="flex gap-2">
            <Check size={14} className="mt-0.5 shrink-0 text-ok" />
            <span>{t("sc.pages.safe")}</span>
          </p>
          <FactoryLine />
        </div>
      </div>
    </section>
  );
}

/** The last upload stopped partway: say so, and offer to upload it again. */
function IncompleteLine({ usb }: { usb: boolean }) {
  const t = useT();
  const name = useItemName();
  const incomplete = useScreen((s) => s.state?.incomplete ?? null);
  const items = useScreen((s) => s.state?.items);
  if (!incomplete) return null;
  const item = items?.find((i) => i.id === incomplete.id) ?? null;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-warn/40 bg-warn/10 px-3 py-2.5 text-ink" role="status" data-incomplete>
      <p className="flex gap-2">
        <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
        <span>
          <span className="font-semibold">{t("sc.incomplete.title")}</span>
          <span className="block text-ink-2">
            {t("sc.incomplete.body")} {item ? t("sc.incomplete.retry", { name: name(item) }) : t("sc.incomplete.pick")}
          </span>
        </span>
      </p>
      {item && (
        <div className="pl-[22px]">
          <RetryUpload item={item} usb={usb} />
        </div>
      )}
    </div>
  );
}

/** No factory backup: what that means, and the official driver's copy to
 *  back it up from when it is on this PC. */
function FactoryLine() {
  const t = useT();
  const hasFactory = useHasFactory();
  const driver = useScreen((s) => s.driver);
  const keymotion = useScreen((s) => s.state?.items.find((i) => i.kind === "keymotion") ?? null);
  /** Bring Keymotion's animation into view in the library, its upload
   *  button focused (uploading still asks first). */
  const showKeymotion = () => {
    const row = keymotion ? document.querySelector<HTMLElement>(`[data-library-item="${CSS.escape(keymotion.id)}"]`) : null;
    const target = row ?? document.querySelector<HTMLElement>("[data-library]");
    target?.scrollIntoView({ block: "center", behavior: "smooth" });
    row?.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus({ preventScroll: true });
  };
  const [backedUp, setBackedUp] = useState(false);
  useEffect(() => {
    if (!hasFactory) useScreen.getState().findDriver();
  }, [hasFactory]);
  if (hasFactory) {
    return backedUp ? (
      <p className="flex gap-2" role="status">
        <Check size={14} className="mt-0.5 shrink-0 text-ok" />
        <span>{t("sc.factory.backedUp")}</span>
      </p>
    ) : null;
  }
  return (
    <div className="flex flex-col gap-2" data-factory-missing>
      <p className="flex gap-2">
        <AlertTriangle size={14} className="mt-0.5 shrink-0 text-warn" />
        <span>
          {t("sc.factory.missing")} {t(driver === "found" ? "sc.factory.driverFound" : "sc.factory.driverRestores")}
        </span>
      </p>
      <div className="pl-[22px]">
        <DriverBackup onDone={() => setBackedUp(true)} />
      </div>
      {keymotion && (
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 pl-[22px]" data-keymotion-offer>
          <span>{t("sc.keymotion.offer")}</span>
          <button type="button" onClick={showKeymotion} className="text-signal underline-offset-2 hover:underline">
            {t("sc.keymotion.show")}
          </button>
        </p>
      )}
    </div>
  );
}

// ------------------------------------------------------ studio + library

type Flow = { kind: "file"; source: Source } | { kind: "text" } | { kind: "slides" };

/** Shown at first; more come in steps, so a large library stays light
 *  (every tile loads its picture). */
const LIBRARY_PAGE = 6;
const LIBRARY_MORE = 12;
/** One empty list, so the library memo stays put before the library is known. */
const NO_ITEMS: readonly ScreenItem[] = [];
type LibraryFilter = "all" | "image" | "animation";
const LIBRARY_FILTERS: { id: LibraryFilter; label: MessageKey }[] = [
  { id: "all", label: "sc.lib.filter.all" },
  { id: "image", label: "sc.lib.filter.image" },
  { id: "animation", label: "sc.lib.filter.animation" },
];

/** A library item's representative frame, as large as its tile. */
function LibraryThumb({ item }: { item: ScreenItem }) {
  const [f, setF] = useState<Uint16Array | null>(null);
  useEffect(() => {
    let live = true;
    loadFrames(item.id, item.frames === 1).then((d) => live && setF(pickThumb(d.frames)), console.error);
    return () => {
      live = false;
    };
  }, [item.id, item.frames]);
  return f ? <FramePlayer frames={[f]} delaysMs={[100]} className="h-full w-full bg-black" /> : <div className="h-full w-full bg-black" />;
}

/**
 * The library in the Photos app's way: a picture grid, a filter and a
 * search box in its heading, and one command bar. The bar makes a new
 * visual (a picture or video, a text card, a slide show: each runs steps
 * 2–4 in its own flow right here and comes back with what it did) and acts
 * on the picture chosen in the grid (upload, match the lights, rename,
 * remove). A file dropped on the library opens like "Picture or video".
 */
function Library({ kb, usb, loaded }: { kb: KeyboardInfo; usb: boolean; loaded: boolean }) {
  const t = useT();
  const name = useItemName();
  const state = useScreen((s) => s.state);
  const upload = useScreen((s) => s.upload);
  const [flow, setFlow] = useState<Flow | null>(null);
  const [done, setDone] = useState<Done | null>(null);
  const [matching, setMatching] = useState<ScreenItem | null>(null);
  const [confirmItem, setConfirmItem] = useState<ScreenItem | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [notice, setNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const [kind, setKind] = useState<LibraryFilter>("all");
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(LIBRARY_PAGE);
  const [over, setOver] = useState(false);
  const [opening, setOpening] = useState<number | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const doneItem = useScreen((s) => (done?.id ? (s.state?.items.find((i) => i.id === done.id) ?? null) : null));
  const fileSource = flow?.kind === "file" ? flow.source : null;
  const all = state?.items ?? NO_ITEMS;
  const shownId = state?.shown;
  // What the keyboard shows first, then the factory animation, then newest.
  // `name` is stable until the factory item or the language changes.
  const items = useMemo(() => {
    const q = query.trim().toLocaleLowerCase();
    return all
      .filter((it) => kind === "all" || (kind === "image" ? it.kind === "image" : it.kind !== "image"))
      .filter((it) => !q || name(it).toLocaleLowerCase().includes(q))
      .sort((a, b) => Number(b.id === shownId) - Number(a.id === shownId));
  }, [all, kind, query, shownId, name]);
  const visible = items.slice(0, limit);
  const rest = items.length - visible.length;
  const selected = all.find((i) => i.id === selectedId) ?? null;
  useEffect(() => setLimit(LIBRARY_PAGE), [kind, query]);
  useEffect(() => setDeleting(false), [selectedId]);
  // The decoded file holds bitmaps and decoders; free them when it goes.
  useEffect(() => () => fileSource?.dispose(), [fileSource]);

  const start = (f: Flow) => {
    setDone(null);
    setNotice(null);
    setFlow(f);
  };
  const finish = (d: Done) => {
    setFlow(null);
    setDone(d);
    if (d.id) setSelectedId(d.id);
  };
  const openFile = async (file: File) => {
    setOpenError(null);
    setOpening(0);
    try {
      const src = await openSource(file, (p) => setOpening(p));
      setOpening(null);
      start({ kind: "file", source: src });
    } catch (e) {
      console.error(e);
      setOpening(null);
      setOpenError(t(errorKey(e)));
    }
  };
  const runUpload = async (it: ScreenItem) => {
    setConfirmItem(null);
    setNotice(null);
    try {
      await useScreen.getState().uploadToKeyboard(it, "library");
      setNotice({ ok: true, text: `${t("sc.done.uploaded", { name: name(it) })}. ${t("sc.done.fnHome")}` });
    } catch (e) {
      setNotice({ ok: false, text: friendlyError(e, t) });
    }
  };
  const common = { usb, loaded, onCancel: () => setFlow(null), onFinished: finish };

  if (flow?.kind === "file") return <FileFlow key={flow.source.name + flow.source.durationMs} source={flow.source} {...common} />;
  if (flow?.kind === "text") return <TextFlow {...common} />;
  if (flow?.kind === "slides") return <SlideFlow {...common} />;

  const cut = !!selected && state?.incomplete?.id === selected.id;
  const busy = !!selected && upload?.id === selected.id;
  const own = !!selected && !builtIn(selected.kind);

  return (
    <section
      className={`flex flex-col rounded-lg transition-shadow ${over ? "shadow-[0_0_0_2px_var(--signal)]" : ""}`}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes("Files")) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOver(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const f = e.dataTransfer.files[0];
        if (f) void openFile(f);
      }}
      data-studio
    >
      <h2 className="section-title flex-wrap">
        <span>
          {t("sc.library")}
          <span className="count num">{all.length}</span>
        </span>
        <span className="flex flex-wrap items-center gap-2.5 font-normal">
          <span role="radiogroup" aria-label={t("sc.library")} className="seg">
            {LIBRARY_FILTERS.map((f) => (
              <button key={f.id} type="button" role="radio" aria-checked={kind === f.id} onClick={() => setKind(f.id)}>
                {t(f.label)}
              </button>
            ))}
          </span>
          <label className="relative flex h-8 w-[220px] items-center">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("sc.lib.search")}
              aria-label={t("sc.lib.search")}
              className="nav-search h-8 w-full rounded-md border border-line bg-bg-raise pr-8 pl-3 text-[13px] outline-none placeholder:text-ink-3"
            />
            {query ? (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label={t("sc.close")}
                className="absolute right-1 grid h-6 w-6 place-items-center rounded-[3px] text-ink-2 hover:bg-[var(--subtle-hover)]"
              >
                <X size={13} />
              </button>
            ) : (
              <Search size={13} className="pointer-events-none absolute right-2.5 text-ink-2" aria-hidden />
            )}
          </label>
        </span>
      </h2>

      {/* One command bar: make a new visual, or act on the chosen one. */}
      <div role="toolbar" aria-label={t("sc.library")} className="cmdbar" data-library-bar>
        <button type="button" className="cb" disabled={opening !== null} onClick={() => fileInput.current?.click()} data-tile="file">
          {opening !== null ? <Loader2 size={16} className="animate-spin" /> : <FileImage size={16} strokeWidth={1.5} />}
          {opening !== null ? t("sc.drop.opening", { percent: percent(Math.round(opening * 100)) }) : t("sc.src.file")}
        </button>
        <input
          ref={fileInput}
          type="file"
          hidden
          accept={ACCEPT}
          onChange={(e) => {
            const f = e.target.files?.[0];
            e.target.value = "";
            if (f) void openFile(f);
          }}
        />
        <button type="button" className="cb" onClick={() => start({ kind: "text" })} data-tile="text">
          <Type size={16} strokeWidth={1.5} />
          {t("sc.src.text")}
        </button>
        <button type="button" className="cb" onClick={() => start({ kind: "slides" })} data-tile="slides">
          <Images size={16} strokeWidth={1.5} />
          {t("sc.src.slides")}
        </button>
        <span className="sep" aria-hidden />
        <button
          type="button"
          className="cb acc"
          disabled={!selected || !usb || !!upload}
          title={!usb ? t("sc.needsUsbShort") : selected?.kind === "factory" ? t("sc.factory.restoreHint") : undefined}
          onClick={() => selected && setConfirmItem(selected)}
          data-library-upload
        >
          {busy ? <Loader2 size={16} className="animate-spin" /> : <Upload size={16} strokeWidth={1.5} />}
          {cut ? t("sc.incomplete.action") : selected?.kind === "factory" ? t("sc.factory.restore") : t("sc.upload.short")}
        </button>
        {kb.geometry && (
          <button type="button" className="cb" disabled={!selected} onClick={() => selected && setMatching(selected)}>
            <Palette size={16} strokeWidth={1.5} />
            {t("sc.lm.action")}
          </button>
        )}
        <button type="button" className="cb" disabled={!own} onClick={() => selected && setEditing(selected.id)}>
          <Pencil size={16} strokeWidth={1.5} />
          {t("sc.rename")}
        </button>
        <button
          type="button"
          className={`cb ${deleting ? "danger" : ""}`}
          disabled={!own || busy}
          onBlur={() => setDeleting(false)}
          onClick={() => {
            if (!selected) return;
            if (!deleting) return setDeleting(true);
            setDeleting(false);
            const id = selected.id;
            setSelectedId(null);
            useScreen.getState().remove(id).catch((x) => setNotice({ ok: false, text: friendlyError(x, t) }));
          }}
        >
          <Trash2 size={16} strokeWidth={1.5} />
          {deleting ? t("sc.remove.confirm") : t("sc.remove")}
        </button>
        {selected && (
          <span className="r num" aria-live="polite">
            {t("sc.lib.selected", { count: 1 })}
          </span>
        )}
      </div>
      <p className="mt-2.5 mb-3.5 text-[12px] text-ink-2">
        {t("sc.library.note")} {t("sc.src.body")}
      </p>

      {openError && (
        <div className="mb-3">
          <Hint tone="error">{openError}</Hint>
        </div>
      )}
      <AnimatePresence>
        {done && (
          <motion.div initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="mb-3">
            <InfoBar
              tone="ok"
              live
              title={
                <>
                  {t(done.kind === "uploaded" ? "sc.done.uploaded" : "sc.done.saved", { name: done.name })}
                  {done.seconds != null && <span className="num ml-2 font-normal text-ink-2">{fmtSeconds(done.seconds)}</span>}
                </>
              }
              actions={
                <>
                  {doneItem && kb.geometry && (
                    <button type="button" onClick={() => setMatching(doneItem)} className="btn-secondary btn-sm">
                      <Palette size={13} /> {t("sc.lm.action")}
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => setDone(null)}
                    aria-label={t("sc.close")}
                    className="btn-subtle btn-sm !px-2"
                  >
                    <X size={14} />
                  </button>
                </>
              }
            >
              {done.kind === "uploaded" ? t("sc.done.uploadedHint") : t("sc.done.savedHint")}
            </InfoBar>
          </motion.div>
        )}
      </AnimatePresence>
      {notice && (
        <div className="mb-3">
          <InfoBar tone={notice.ok ? "ok" : "error"} live title={notice.text} />
        </div>
      )}

      {all.length === 0 && <p className="pb-1 text-[13px] text-ink-2">{t("sc.library.empty")}</p>}
      {all.length > 0 && items.length === 0 && <p className="pb-1 text-[13px] text-ink-2">{t("sc.lib.none")}</p>}
      <ul className="grid grid-cols-2 gap-x-3 gap-y-[18px] @3xl:grid-cols-3" data-library>
        {visible.map((it) => {
          const onKeyboard = state?.shown === it.id;
          const isCut = state?.incomplete?.id === it.id;
          const chosen = it.id === selectedId;
          return (
            <li key={it.id} data-library-item={it.id} className="flex min-w-0 flex-col gap-1.5">
              <button
                type="button"
                aria-pressed={chosen}
                aria-label={name(it)}
                onClick={() => setSelectedId(chosen ? null : it.id)}
                className={`relative aspect-[240/135] overflow-hidden rounded-md transition-shadow ${
                  chosen ? "shadow-[0_0_0_2px_var(--signal)]" : "shadow-[0_0_0_1px_var(--line)] hover:shadow-[0_0_0_1px_var(--line-strong)]"
                }`}
              >
                <LibraryThumb item={it} />
                {chosen && (
                  <span className="absolute top-2 right-2 grid h-5 w-5 place-items-center rounded bg-signal text-signal-ink shadow" aria-hidden>
                    <Check size={13} strokeWidth={2.4} />
                  </span>
                )}
              </button>
              {editing === it.id ? (
                <input
                  autoFocus
                  defaultValue={it.name}
                  maxLength={60}
                  aria-label={t("sc.rename")}
                  onBlur={(e) => {
                    setEditing(null);
                    const v = e.target.value.trim();
                    if (v && v !== it.name) useScreen.getState().rename(it.id, v).catch((x) => setNotice({ ok: false, text: friendlyError(x, t) }));
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                    if (e.key === "Escape") setEditing(null);
                  }}
                  className="mt-1 h-8 rounded-md border border-line bg-bg-raise px-2 text-[14px] outline-none"
                />
              ) : (
                <span className="mt-1 truncate text-[14px]" title={name(it)}>
                  {name(it)}
                </span>
              )}
              <span className="num text-[12px] text-ink-2">
                {t("sc.frames", { count: it.frames })}
                {it.frames > 1 ? ` · ${t("sc.loop")} ${fmtSeconds(it.loopMs / 1000)}` : ""} · {t("sc.uploadTime")} ~
                {fmtSeconds(uploadSeconds(it.frames))}
              </span>
              <span className="flex flex-wrap gap-1.5">
                {onKeyboard && (
                  <span className="flex h-[22px] items-center gap-1.5 rounded-full bg-ok/15 px-2 text-[12px]">
                    <span className="h-1.5 w-1.5 rounded-full bg-ok" aria-hidden />
                    {t("sc.badge.onKeyboard")}
                  </span>
                )}
                {it.kind === "factory" && (
                  <span className="flex h-[22px] items-center rounded-full bg-[var(--subtle-sel)] px-2 text-[12px]">{t("sc.badge.factory")}</span>
                )}
                {it.kind === "keymotion" && (
                  <span className="flex h-[22px] items-center rounded-full bg-[var(--subtle-sel)] px-2 text-[12px]">{t("sc.badge.keymotion")}</span>
                )}
                {isCut && (
                  <span className="flex h-[22px] items-center rounded-full bg-warn/15 px-2 text-[12px] text-warn">{t("sc.incomplete.badge")}</span>
                )}
              </span>
            </li>
          );
        })}
      </ul>
      {(rest > 0 || limit > LIBRARY_PAGE) && (
        <div className="flex flex-wrap items-center justify-center gap-2 pt-4">
          {rest > 0 && (
            <button type="button" onClick={() => setLimit((n) => n + LIBRARY_MORE)} className="btn-secondary" data-library-more>
              <ChevronDown size={14} /> {t("sc.lib.more")}
              <span className="num text-ink-2">· {t("sc.lib.left", { count: rest })}</span>
            </button>
          )}
          {limit > LIBRARY_PAGE && (
            <button type="button" onClick={() => setLimit(LIBRARY_PAGE)} className="btn-subtle">
              {t("sc.lib.less")}
            </button>
          )}
        </div>
      )}
      <AnimatePresence>
        {confirmItem && (
          <LibraryConfirm item={confirmItem} onCancel={() => setConfirmItem(null)} onConfirm={() => runUpload(confirmItem)} />
        )}
        {matching && <MatchLightsDialog kb={kb} item={matching} onClose={() => setMatching(null)} />}
      </AnimatePresence>
    </section>
  );
}

// ----------------------------------------------------- upload progress

/** Floating progress while an upload runs. Hidden on the Screen page when
 *  the studio itself shows the progress. */
export function ScreenUploadToast() {
  const t = useT();
  const name = useItemName();
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
  const left = upload ? remaining(upload, now) : null;
  return (
    <AnimatePresence>
      {visible && upload && (
        <motion.div
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          className="flyout fixed right-6 bottom-6 z-40 flex w-[340px] flex-col gap-2 p-4"
          role="status"
        >
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <Loader2 size={14} className="animate-spin text-signal" />
            <span className="truncate">{t("sc.uploading.name", { name: name(upload) })}</span>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full bg-signal transition-[width]"
              style={{ width: `${Math.round((upload.done / Math.max(1, upload.total)) * 100)}%` }}
            />
          </div>
          <div className="num flex justify-between text-[12px] text-ink-3">
            <span>{t("sc.uploaded.pct", { pct: percent(Math.round((upload.done / Math.max(1, upload.total)) * 100)) })}</span>
            <span>
              {left ? t("sc.left", { time: left }) : ""}
            </span>
          </div>
          <p className="text-[12px] text-ink-2">{t("sc.uploading.note")}</p>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
