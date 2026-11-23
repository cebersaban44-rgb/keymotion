// List boxes keep their width. Windows 11's open list (styles.css) uses
// Chromium's customizable select, which makes a box as wide as the choice it
// shows, so a box would grow and shrink with each choice. The system's list
// made it as wide as its longest choice; this measures that width for every
// box and styles.css uses it where the box has no width of its own.

const SELECTOR = "select:not(.bare)";

let canvas: CanvasRenderingContext2D | null = null;

/** The longest choice's width plus the box's padding and edges. */
function widthOf(select: HTMLSelectElement): number {
  canvas ??= document.createElement("canvas").getContext("2d");
  if (!canvas) return 0;
  const cs = getComputedStyle(select);
  canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
  let widest = 0;
  for (const option of select.options) widest = Math.max(widest, canvas.measureText(option.label).width);
  const px = (v: string) => parseFloat(v) || 0;
  return Math.ceil(widest + px(cs.paddingLeft) + px(cs.paddingRight) + px(cs.borderLeftWidth) + px(cs.borderRightWidth));
}

/** What a box's width depends on, cheap to read: its choices, the
 *  language (the font) and its classes (size, padding). */
function signature(select: HTMLSelectElement): string {
  let labels = "";
  for (const option of select.options) labels += `${option.label}\n`;
  return `${document.documentElement.lang}|${select.className}|${labels}`;
}

/** Keep every box's width up to date as pages, languages and choices
 *  change. Returns the function that stops. */
export function followSelectWidths(): () => void {
  if (!CSS.supports("appearance: base-select")) return () => {};
  // Measured only when a box's signature changes: the document changes
  // often (clocks, live readings), the boxes seldom.
  let measured = new WeakMap<HTMLSelectElement, string>();
  let frame = 0;
  const update = () => {
    frame = 0;
    for (const select of document.querySelectorAll<HTMLSelectElement>(SELECTOR)) {
      const sig = signature(select);
      if (measured.get(select) === sig) continue;
      measured.set(select, sig);
      const width = `${widthOf(select)}px`;
      if (select.style.getPropertyValue("--select-w") !== width) select.style.setProperty("--select-w", width);
    }
  };
  const later = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  /** Everything again: the fonts have loaded or the theme or language changed. */
  const again = () => {
    measured = new WeakMap();
    later();
  };
  // Pages and texts change the document's nodes; the widths are set as a
  // style, which this doesn't watch.
  const watch = new MutationObserver(later);
  watch.observe(document.body, { childList: true, subtree: true, characterData: true });
  const root = new MutationObserver(again);
  root.observe(document.documentElement, { attributes: true, attributeFilter: ["lang", "data-theme"] });
  void document.fonts.ready.then(again);
  later();
  return () => {
    watch.disconnect();
    root.disconnect();
    cancelAnimationFrame(frame);
  };
}
