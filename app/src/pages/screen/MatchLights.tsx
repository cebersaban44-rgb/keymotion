// "Işıkları ekrana uydur": key lighting from a screen picture (phase B3).
// Nothing is written here: the chosen design opens on the Lighting page
// as an unsaved per-key draft, where the keys preview it live (RAM) and
// the user saves it, or not.

import { Loader2, Palette, X } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import type { KeyboardInfo } from "../../bindings/KeyboardInfo";
import { KeyboardView } from "../../components/KeyboardView";
import { useT, type MessageKey } from "../../lib/i18n";
import { useLighting } from "../../lib/lighting";
import { extractPalette, lightDesign, type LightMatch } from "../../lib/screenPalette";
import { useApp } from "../../lib/store";
import { FramePlayer, loadFrames, pickThumb } from "./common";

const OPTIONS: { id: LightMatch; title: MessageKey; body: MessageKey }[] = [
  { id: "gradient", title: "sc.lm.gradient", body: "sc.lm.gradientBody" },
  { id: "single", title: "sc.lm.single", body: "sc.lm.singleBody" },
  { id: "image", title: "sc.lm.image", body: "sc.lm.imageBody" },
];

export function MatchLightsDialog({
  kb,
  item,
  onClose,
}: {
  kb: KeyboardInfo;
  item: { id: string; name: string; frames: number };
  onClose: () => void;
}) {
  const t = useT();
  const [frames, setFrames] = useState<Uint16Array[] | null>(null);
  const [delays, setDelays] = useState<number[]>([]);
  const [kind, setKind] = useState<LightMatch>("gradient");
  const geometry = kb.geometry;

  useEffect(() => {
    let live = true;
    loadFrames(item.id, item.frames === 1).then((d) => {
      if (!live) return;
      setDelays(d.delaysMs);
      setFrames(d.frames);
    }, console.error);
    return () => {
      live = false;
    };
  }, [item.id, item.frames]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const palette = useMemo(() => (frames ? extractPalette(frames, delays) : []), [frames, delays]);
  const design = useMemo(
    () => (frames && geometry ? lightDesign(kind, geometry, palette, frames) : null),
    [frames, geometry, kind, palette],
  );
  const thumb = useMemo(() => (frames ? pickThumb(frames) : null), [frames]);

  const open = () => {
    if (!design) return;
    const l = useLighting.getState();
    l.setPerKey(design);
    l.setTab("per-key");
    useApp.getState().setPage("lighting");
    onClose();
  };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      role="dialog"
      aria-modal
      aria-labelledby="lm-title"
    >
      <motion.div initial={{ scale: 0.97, y: 8 }} animate={{ scale: 1, y: 0 }} className="card flex w-[720px] max-w-[94vw] flex-col gap-4 p-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="lm-title" className="font-display text-[22px] font-semibold">
              {t("sc.lm.title")}
            </h2>
            <p className="mt-1 text-[13px] text-ink-2">{t("sc.lm.body")}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("sc.close")}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-ink-3 hover:bg-surface-2 hover:text-ink"
          >
            <X size={16} />
          </button>
        </div>

        {!frames ? (
          <div className="flex h-[200px] items-center justify-center text-ink-3">
            <Loader2 size={18} className="animate-spin" />
          </div>
        ) : (
          <>
            <div className="flex items-center gap-4">
              {thumb && <FramePlayer frames={[thumb]} delaysMs={[100]} className="h-[68px] w-[120px] shrink-0 rounded-md bg-black" />}
              <div className="flex min-w-0 flex-col gap-1.5">
                <span className="truncate text-[13px] font-semibold">{item.name}</span>
                <div className="flex gap-1.5" aria-label={t("sc.lm.palette")}>
                  {palette.map((s) => (
                    <span
                      key={s.hex}
                      title={`${s.hex.toUpperCase()} → ${s.led.toUpperCase()}`}
                      className="h-7 w-10 rounded-md border border-black/20"
                      style={{ background: `linear-gradient(90deg, ${s.hex} 50%, ${s.led} 50%)` }}
                    />
                  ))}
                </div>
                <span className="text-[11px] text-ink-3">{t("sc.lm.paletteNote")}</span>
              </div>
            </div>

            <div role="radiogroup" aria-label={t("sc.lm.title")} className="grid grid-cols-3 gap-2">
              {OPTIONS.map((o) => (
                <button
                  key={o.id}
                  type="button"
                  role="radio"
                  aria-checked={kind === o.id}
                  data-match={o.id}
                  onClick={() => setKind(o.id)}
                  className={`flex flex-col items-start gap-0.5 rounded-lg border p-3 text-left transition-colors ${
                    kind === o.id ? "border-signal bg-signal/10" : "border-line hover:bg-surface-2"
                  }`}
                >
                  <span className="text-[13px] font-semibold">{t(o.title)}</span>
                  <span className="text-[12px] text-ink-2">{t(o.body)}</span>
                </button>
              ))}
            </div>

            {geometry && design && (
              <div
                className="overflow-hidden rounded-lg border border-line p-4"
                style={{ background: "linear-gradient(180deg, var(--plate), color-mix(in oklch, var(--plate) 80%, black))" }}
              >
                <KeyboardView geometry={geometry} connection={kb.connection} colors={design} />
              </div>
            )}

            <p className="text-[12px] text-ink-3">{t("sc.lm.note")}</p>
          </>
        )}

        <div className="flex justify-end gap-2">
          <button
            type="button"
            autoFocus
            onClick={onClose}
            className="h-10 rounded-lg border border-line px-4 text-[13px] text-ink-2 hover:bg-surface-2"
          >
            {t("sc.cancel")}
          </button>
          <button
            type="button"
            disabled={!design}
            onClick={open}
            className="flex h-10 items-center gap-2 rounded-lg bg-signal px-4 text-[13px] font-semibold text-signal-ink disabled:opacity-40"
          >
            <Palette size={15} /> {t("sc.lm.open")}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
