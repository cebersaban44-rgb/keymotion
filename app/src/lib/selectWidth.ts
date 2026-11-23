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

/** Keep every box's width up to date as pages, languages and choices
 *  change. Returns the function that stops. */
export function followSelectWidths(): () => void {
  if (!CSS.supports("appearance: base-select")) return () => {};
  let frame = 0;
  const update = () => {
    frame = 0;
    for (const select of document.querySelectorAll<HTMLSelectElement>(SELECTOR)) {
      const width = `${widthOf(select)}px`;
      if (select.style.getPropertyValue("--select-w") !== width) select.style.setProperty("--select-w", width);
    }
  };
  const later = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  // Pages and texts change the document's nodes; the widths are set as a
  // style, which this doesn't watch.
  const watch = new MutationObserver(later);
  watch.observe(document.body, { childList: true, subtree: true, characterData: true });
  void document.fonts.ready.then(later);
  later();
  return () => {
    watch.disconnect();
    cancelAnimationFrame(frame);
  };
}
