//! The UI's words on the backend side: the tray menu and the notifications
//! get templates from the UI (`TrayTexts`) and fill them in here, with the
//! UI's language ("en", "tr", "ru" or "zh-CN") for plural forms, dates and
//! the documentation site's folder.
//!
//! Templates use the UI's syntax (app/src/lib/i18n/plural.ts): `{name}` is a
//! value, and `{name, plural, one {# block} few {# блока} other {# blocks}}`
//! picks a form by the number's plural category, `#` being the number. Only
//! whole numbers reach the tray, so the rules below are CLDR's for integers.

/// The documentation site; each UI language but English has a folder.
pub const DOCS_SITE: &str = "https://cebersaban44-rgb.github.io/keymotion/";

/// The site's folder for `locale`: "" (English), "tr/", "ru/", "zh-cn/".
pub fn docs_folder(locale: &str) -> &'static str {
    match locale {
        "tr" => "tr/",
        "ru" => "ru/",
        "zh-CN" => "zh-cn/",
        _ => "",
    }
}

/// A page of the documentation site in the UI language.
pub fn docs_page(locale: &str, path: &str) -> String {
    format!("{DOCS_SITE}{}{path}", docs_folder(locale))
}

/// CLDR plural category of a whole number `n` in `locale`.
fn plural_category(locale: &str, n: u64) -> &'static str {
    match locale {
        // one: 1, 21, 31… but not 11; few: 2–4, 22–24… but not 12–14; many: the rest.
        "ru" => match (n % 10, n % 100) {
            (1, r) if r != 11 => "one",
            (2..=4, r) if !(12..=14).contains(&r) => "few",
            _ => "many",
        },
        // Chinese has one form.
        "zh-CN" => "other",
        // English, Turkish.
        _ if n == 1 => "one",
        _ => "other",
    }
}

/// `template` with each `{name}` replaced by its value from `pairs`, and each
/// plural resolved with `locale`'s rules. A name without a value stays as it
/// is. A template with broken plural syntax gets its `{name}` places filled
/// and nothing else, so a bad translation still shows something readable.
pub fn fill(locale: &str, template: &str, pairs: &[(&str, &str)]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    match render(locale, &chars, &mut i, pairs, None) {
        Ok(text) => text,
        Err(()) => pairs.iter().fold(template.to_string(), |s, (k, v)| s.replace(&format!("{{{k}}}"), v)),
    }
}

fn value<'a>(pairs: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn skip_space(chars: &[char], i: &mut usize) {
    while chars.get(*i).is_some_and(|c| c.is_whitespace()) {
        *i += 1;
    }
}

fn word(chars: &[char], i: &mut usize) -> String {
    let start = *i;
    while chars.get(*i).copied().is_some_and(is_word) {
        *i += 1;
    }
    chars[start..*i].iter().collect()
}

/// Text from `i` to the end, or (inside a plural form, `count` set) to the
/// `}` that closes the form, which is left for the caller.
fn render(
    locale: &str,
    chars: &[char],
    i: &mut usize,
    pairs: &[(&str, &str)],
    count: Option<&str>,
) -> Result<String, ()> {
    let mut out = String::new();
    while let Some(&c) = chars.get(*i) {
        match c {
            '}' if count.is_some() => return Ok(out),
            '#' if count.is_some() => {
                out.push_str(count.unwrap_or_default());
                *i += 1;
            }
            '{' => {
                // "{name}" exactly.
                let mut j = *i + 1;
                let name = word(chars, &mut j);
                if !name.is_empty() && chars.get(j) == Some(&'}') {
                    match value(pairs, &name) {
                        Some(v) => out.push_str(v),
                        None => out.push_str(&format!("{{{name}}}")),
                    }
                    *i = j + 1;
                    continue;
                }
                // "{name, plural, …}", spaces allowed around the parts.
                let mut j = *i + 1;
                skip_space(chars, &mut j);
                let name = word(chars, &mut j);
                skip_space(chars, &mut j);
                if name.is_empty() || chars.get(j) != Some(&',') {
                    out.push(c);
                    *i += 1;
                    continue;
                }
                j += 1;
                skip_space(chars, &mut j);
                let kind = word(chars, &mut j);
                skip_space(chars, &mut j);
                if kind.is_empty() || chars.get(j) != Some(&',') {
                    out.push(c);
                    *i += 1;
                    continue;
                }
                if kind != "plural" {
                    return Err(());
                }
                *i = j + 1;
                out.push_str(&plural(locale, chars, i, pairs, &name)?);
            }
            _ => {
                out.push(c);
                *i += 1;
            }
        }
    }
    if count.is_some() {
        Err(())
    } else {
        Ok(out)
    }
}

/// After "{name, plural,": the forms up to the closing "}", resolved.
fn plural(locale: &str, chars: &[char], i: &mut usize, pairs: &[(&str, &str)], name: &str) -> Result<String, ()> {
    let given = value(pairs, name);
    let shown = given.map(str::to_string).unwrap_or_else(|| format!("{{{name}}}"));
    let mut forms: Vec<(String, String)> = Vec::new();
    loop {
        skip_space(chars, i);
        match chars.get(*i) {
            None => return Err(()),
            Some('}') => {
                *i += 1;
                break;
            }
            Some(_) => {}
        }
        let start = *i;
        while chars.get(*i).is_some_and(|c| !c.is_whitespace() && *c != '{' && *c != '}') {
            *i += 1;
        }
        let selector: String = chars[start..*i].iter().collect();
        let known = matches!(selector.as_str(), "zero" | "one" | "two" | "few" | "many" | "other")
            || selector.strip_prefix('=').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if !known || forms.iter().any(|(s, _)| *s == selector) {
            return Err(());
        }
        skip_space(chars, i);
        if chars.get(*i) != Some(&'{') {
            return Err(());
        }
        *i += 1;
        let text = render(locale, chars, i, pairs, Some(&shown))?;
        *i += 1; // the form's "}"
        forms.push((selector, text));
    }
    let form = |s: &str| forms.iter().find(|(k, _)| k == s).map(|(_, t)| t.clone());
    let other = form("other").ok_or(())?;
    let Some(n) = given.and_then(|v| v.trim().parse::<u64>().ok()) else { return Ok(other) };
    Ok(form(&format!("={n}")).or_else(|| form(plural_category(locale, n))).unwrap_or(other))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RU_BLOCKS: &str = "{n, plural, one {# блок} few {# блока} many {# блоков} other {# блока}}";

    #[test]
    fn russian_plural_forms() {
        let f = |n: &str| fill("ru", RU_BLOCKS, &[("n", n)]);
        assert_eq!(f("1"), "1 блок");
        assert_eq!(f("2"), "2 блока");
        assert_eq!(f("4"), "4 блока");
        assert_eq!(f("5"), "5 блоков");
        assert_eq!(f("0"), "0 блоков");
        assert_eq!(f("11"), "11 блоков");
        assert_eq!(f("12"), "12 блоков");
        assert_eq!(f("14"), "14 блоков");
        assert_eq!(f("21"), "21 блок");
        assert_eq!(f("22"), "22 блока");
        assert_eq!(f("25"), "25 блоков");
        assert_eq!(f("111"), "111 блоков");
        assert_eq!(f("101"), "101 блок");
        // Not a whole number: "other".
        assert_eq!(f("1,5"), "1,5 блока");
    }

    #[test]
    fn english_turkish_and_chinese() {
        let en = "Ready: {blocks, plural, one {# block} other {# blocks}}, {min} min";
        assert_eq!(fill("en", en, &[("blocks", "1"), ("min", "25")]), "Ready: 1 block, 25 min");
        assert_eq!(fill("en", en, &[("blocks", "6"), ("min", "100")]), "Ready: 6 blocks, 100 min");
        assert_eq!(
            fill("tr", "Hazır: {blocks} blok, {min} dk", &[("blocks", "6"), ("min", "100")]),
            "Hazır: 6 blok, 100 dk"
        );
        assert_eq!(fill("zh-CN", "{n, plural, other {# 个}}", &[("n", "1")]), "1 个");
        // An exact form wins over the category.
        assert_eq!(fill("en", "{n, plural, =0 {none} one {# left} other {# left}}", &[("n", "0")]), "none");
    }

    #[test]
    fn plain_text_and_missing_values() {
        assert_eq!(fill("en", "{a} and {b}", &[("a", "x")]), "x and {b}");
        assert_eq!(fill("en", "{ } {1} {", &[]), "{ } {1} {");
        assert_eq!(fill("en", "# {n}", &[("n", "3")]), "# 3");
        // Spaces around the parts; a plural form inside another one.
        let nested = "{ h , plural , one {# hour{m, plural, one { # min} other { # mins}}} other {# hours}}";
        assert_eq!(fill("en", nested, &[("h", "1"), ("m", "2")]), "1 hour 2 mins");
        // No value for the plural: its "other" form.
        assert_eq!(fill("en", "{n, plural, one {# x} other {# xs}}", &[]), "{n} xs");
    }

    #[test]
    fn broken_plural_syntax_falls_back_to_plain_places() {
        assert_eq!(fill("en", "{n, plural, one {# x}", &[("n", "1")]), "{n, plural, one {# x}");
        assert_eq!(fill("en", "{n, plural, one {x}} {m}", &[("m", "2")]), "{n, plural, one {x}} 2");
        assert_eq!(fill("en", "{n, select, a {x} other {y}} {m}", &[("m", "2")]), "{n, select, a {x} other {y}} 2");
        assert_eq!(fill("en", "{n, plural, lots {x} other {y}}", &[("n", "2")]), "{n, plural, lots {x} other {y}}");
    }

    #[test]
    fn docs_pages_by_language() {
        assert_eq!(
            docs_page("en", "about/release-notes/"),
            "https://cebersaban44-rgb.github.io/keymotion/about/release-notes/"
        );
        assert_eq!(docs_page("tr", "x/"), "https://cebersaban44-rgb.github.io/keymotion/tr/x/");
        assert_eq!(docs_page("ru", "x/"), "https://cebersaban44-rgb.github.io/keymotion/ru/x/");
        assert_eq!(docs_page("zh-CN", "x/"), "https://cebersaban44-rgb.github.io/keymotion/zh-cn/x/");
        assert_eq!(docs_page("??", ""), DOCS_SITE);
    }
}
