// Messages with placeholders and ICU plural syntax: the small part of ICU
// MessageFormat the interface uses, without a library.
//
//   "{name}"                     the value of `name`
//   "{count, plural, one {# frame} other {# frames}}"
//                                a form chosen by the language's plural rules
//                                (Intl.PluralRules): zero, one, two, few, many,
//                                other, or an exact "=0"; "#" is the number.
//                                "other" is required; a missing form uses it.
//
// Everything else is literal text. Unlike full ICU, apostrophes are plain
// text (English and Turkish use them in words), and a brace that does not
// start a placeholder ("{ }", "{1}") stays as it is. Russian needs one, few,
// many and other: 1 кадр, 2 кадра, 5 кадров, 1,5 кадра.
//
// No imports: tools/check-i18n.mjs loads this file in Node to check the
// dictionaries with the same parser the app uses.

export type MessagePart =
  | string
  | { kind: "arg"; name: string }
  | { kind: "plural"; name: string; forms: Record<string, MessagePart[]> }
  | { kind: "number" };

/** Values for a message's placeholders. */
export type MessageVars = Record<string, string | number>;

export class MessageSyntaxError extends Error {}

export const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"] as const;
const SELECTOR = /^(?:zero|one|two|few|many|other|=\d+(?:\.\d+)?)$/;
// "{name}" exactly, as the interface has always written it.
const SIMPLE = /^\{(\w+)\}/;
// "{name, type," with spaces allowed around the parts.
const COMPLEX = /^\{\s*(\w+)\s*,\s*(\w+)\s*,/;

/** The parts of `text`; throws MessageSyntaxError on broken plural syntax. */
export function parseMessage(text: string): MessagePart[] {
  let i = 0;

  const fail = (why: string): never => {
    throw new MessageSyntaxError(`${why} at character ${i + 1}`);
  };
  const skipSpace = () => {
    while (i < text.length && /\s/.test(text[i]!)) i++;
  };

  // Text up to the end, or up to the "}" that closes a plural form.
  function body(inForm: boolean): MessagePart[] {
    const out: MessagePart[] = [];
    let literal = "";
    const flush = () => {
      if (literal) out.push(literal);
      literal = "";
    };
    while (i < text.length) {
      const ch = text[i]!;
      if (ch === "}" && inForm) {
        flush();
        return out;
      }
      if (ch === "#" && inForm) {
        flush();
        out.push({ kind: "number" });
        i++;
        continue;
      }
      if (ch === "{") {
        const rest = text.slice(i);
        const simple = SIMPLE.exec(rest);
        if (simple) {
          flush();
          out.push({ kind: "arg", name: simple[1]! });
          i += simple[0].length;
          continue;
        }
        const complex = COMPLEX.exec(rest);
        if (complex) {
          flush();
          if (complex[2] !== "plural") fail(`"{${complex[1]}, ${complex[2]}}": only plural is supported`);
          i += complex[0].length;
          out.push(plural(complex[1]!));
          continue;
        }
      }
      literal += ch;
      i++;
    }
    if (inForm) fail("a plural form is not closed with }");
    flush();
    return out;
  }

  // After "{name, plural,": the forms, then the closing "}".
  function plural(name: string): MessagePart {
    const forms: Record<string, MessagePart[]> = {};
    for (;;) {
      skipSpace();
      if (i >= text.length) fail(`"{${name}, plural, …" is not closed with }`);
      if (text[i] === "}") {
        i++;
        break;
      }
      const m = /^[^\s{}]+/.exec(text.slice(i));
      const selector = m?.[0] ?? "";
      if (!SELECTOR.test(selector)) fail(`"${selector || text[i]}" is not a plural form (zero, one, two, few, many, other or =N)`);
      if (selector in forms) fail(`the plural form "${selector}" appears twice`);
      i += selector.length;
      skipSpace();
      if (text[i] !== "{") fail(`the plural form "${selector}" needs its text in {…}`);
      i++;
      forms[selector] = body(true);
      i++; // the form's "}"
    }
    if (!forms.other) fail(`"{${name}, plural, …}" needs an "other" form`);
    return { kind: "plural", name, forms };
  }

  return body(false);
}

const parsed = new Map<string, MessagePart[]>();
const rules = new Map<string, Intl.PluralRules>();
const numbers = new Map<string, Intl.NumberFormat>();

function cached<T>(map: Map<string, T>, key: string, make: () => T): T {
  let v = map.get(key);
  if (v === undefined) {
    v = make();
    map.set(key, v);
  }
  return v;
}

/** The plural category of `n` in `lang` ("one", "few", …). */
export function pluralCategory(n: number, lang: string): string {
  return cached(rules, lang, () => new Intl.PluralRules(lang)).select(n);
}

/**
 * `text` with its placeholders filled in, plural forms chosen with `lang`'s
 * rules (a BCP 47 tag: the language the text is written in). A placeholder
 * without a value stays as it is ("{name}"). Text with broken plural syntax
 * is returned unchanged, so a bad translation never stops the interface
 * (tools/check-i18n.mjs reports it).
 */
export function formatMessage(text: string, vars: MessageVars, lang: string): string {
  if (!text.includes("{")) return text;
  let parts = parsed.get(text);
  if (!parts) {
    try {
      parts = parseMessage(text);
    } catch {
      return text;
    }
    parsed.set(text, parts);
  }
  return render(parts, vars, lang, null);
}

function render(parts: MessagePart[], vars: MessageVars, lang: string, count: string | null): string {
  let out = "";
  for (const p of parts) {
    if (typeof p === "string") out += p;
    else if (p.kind === "arg") out += p.name in vars ? String(vars[p.name]) : `{${p.name}}`;
    else if (p.kind === "number") out += count ?? "#";
    else {
      const value = vars[p.name];
      const n = typeof value === "number" ? value : Number(value);
      let form = p.forms.other!;
      let shown = `{${p.name}}`;
      if (value !== undefined) {
        shown =
          typeof value === "number"
            ? cached(numbers, lang, () => new Intl.NumberFormat(lang, { useGrouping: false, maximumFractionDigits: 3 })).format(value)
            : value;
        if (Number.isFinite(n)) form = p.forms[`=${n}`] ?? p.forms[pluralCategory(n, lang)] ?? form;
      }
      out += render(form, vars, lang, shown);
    }
  }
  return out;
}

/**
 * The text for `key`: from `own` (the interface language's dictionary), or
 * from `fallback` (English) while `own` has none; `fromOwn` says which, so the
 * plural rules match the text's language. With `count` 1, the key's `.one`
 * form from the same dictionary when it has one (English and Turkish write
 * "1 frame" that way; Russian and Chinese use plural syntax instead).
 */
export function pickText(
  own: Partial<Record<string, string>>,
  fallback: Record<string, string>,
  key: string,
  count?: unknown,
): { text: string; fromOwn: boolean } {
  const fromOwn = own[key] !== undefined;
  const table = fromOwn ? own : fallback;
  const one = count === 1 ? table[`${key}.one`] : undefined;
  return { text: one ?? table[key] ?? key, fromOwn };
}

/** The placeholder names in `parts` (plural ones included, "#" not). */
export function placeholderNames(parts: MessagePart[]): Set<string> {
  const names = new Set<string>();
  const walk = (ps: MessagePart[]) => {
    for (const p of ps) {
      if (typeof p === "string" || p.kind === "number") continue;
      names.add(p.name);
      if (p.kind === "plural") Object.values(p.forms).forEach(walk);
    }
  };
  walk(parts);
  return names;
}

/** Every plural in `parts`, nested ones too: its variable and its forms. */
export function plurals(parts: MessagePart[]): { name: string; forms: string[] }[] {
  const out: { name: string; forms: string[] }[] = [];
  const walk = (ps: MessagePart[]) => {
    for (const p of ps) {
      if (typeof p === "string" || p.kind !== "plural") continue;
      out.push({ name: p.name, forms: Object.keys(p.forms) });
      Object.values(p.forms).forEach(walk);
    }
  };
  walk(parts);
  return out;
}
