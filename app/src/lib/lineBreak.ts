// Where text drawn on the keyboard screen may break into lines. Latin and
// Cyrillic text breaks at spaces. Chinese (and Japanese) has no spaces, so it
// may break between words, which Intl.Segmenter finds; punctuation stays with
// its word, so no line starts with "，" or "。" and none ends with "“" or "（".

/** A piece of text that stays on one line, and whether a space comes before it. */
export interface Piece {
  text: string;
  space: boolean;
}

const CJK = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\u3000-\u303F\uFF00-\uFFEF]/u;
/** Characters a line does not start with (closing punctuation, iteration marks). */
const NO_START = /^[，。、；：？！）」』】〕〉》〗”’…‥·・％‰%,.;:!?)\]}～〜ー々〻ゝゞヽヾ]/u;
/** Characters a line does not end with (opening punctuation). */
const NO_END = /[（「『【〔〈《〖“‘([{]$/u;

let segmenter: Intl.Segmenter | null | undefined;

function words(token: string): string[] {
  if (segmenter === undefined) {
    segmenter = typeof Intl.Segmenter === "function" ? new Intl.Segmenter("zh", { granularity: "word" }) : null;
  }
  if (!segmenter) return Array.from(token);
  return Array.from(segmenter.segment(token), (s) => s.segment);
}

/** `text` (one paragraph) in pieces that may each start a new line. */
export function pieces(text: string): Piece[] {
  const out: Piece[] = [];
  for (const token of text.split(/\s+/).filter(Boolean)) {
    const space = out.length > 0;
    if (!CJK.test(token)) {
      out.push({ text: token, space });
      continue;
    }
    let current: Piece | null = null;
    for (const w of words(token)) {
      const prev = current?.text ?? "";
      const canBreak = current !== null && (CJK.test(prev.slice(-1)) || CJK.test(w[0] ?? "")) && !NO_START.test(w) && !NO_END.test(prev);
      if (current && !canBreak) {
        current.text += w;
        continue;
      }
      current = { text: w, space: current === null && space };
      out.push(current);
    }
  }
  return out;
}

/** `line` with `piece` added after it. */
export function append(line: string, piece: Piece): string {
  return line ? `${line}${piece.space ? " " : ""}${piece.text}` : piece.text;
}

/** Whether `ch` should not start a line (it hangs at the end of the line before). */
export function noLineStart(ch: string): boolean {
  return NO_START.test(ch);
}

/** `text` without the spaces and punctuation an ellipsis replaces ("word, …" → "word…"). */
export function trimBeforeEllipsis(text: string): string {
  return text.replace(/[\s,.;:–\-，。、；：！？…—·]+$/u, "");
}
