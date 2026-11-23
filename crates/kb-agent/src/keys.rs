//! Which keys a light covers, as typed: `esc`, `f1-f12`, `wasd`,
//! `f1-f4,numpad`.
//!
//! This only reads the text. Which keys the names mean is the keyboard
//! model's business: the app resolves a [`KeySpec`] against the connected
//! model and answers `unknown_key` for a name the model does not have, so
//! the same names work on every model.

use std::fmt;
use std::str::FromStr;

use crate::error::{AgentError, ErrorCode};
use crate::values::shown;

/// The most comma-separated items in one key list.
pub const MAX_KEY_ITEMS: usize = 64;
/// The longest key list, in bytes.
pub const MAX_KEY_SPEC_LEN: usize = 1024;
/// The longest key id.
pub const MAX_KEY_ID_LEN: usize = 32;
/// The most keys one numbered range (`f1-f12`) may cover.
pub const MAX_RANGE_KEYS: u32 = 64;
/// The highest number in a numbered range.
const MAX_RANGE_NUMBER: u32 = 999;

/// Named groups of keys. The model file says which keys belong to each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyGroup {
    /// The whole keyboard.
    All,
    /// F1 to F12.
    FRow,
    /// The number row.
    Numbers,
    /// A to Z.
    Letters,
    /// W, A, S, D.
    Wasd,
    /// The arrow keys.
    Arrows,
    /// The number pad.
    Numpad,
}

impl KeyGroup {
    pub const ALL: [KeyGroup; 7] =
        [Self::All, Self::FRow, Self::Numbers, Self::Letters, Self::Wasd, Self::Arrows, Self::Numpad];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::FRow => "f-row",
            Self::Numbers => "numbers",
            Self::Letters => "letters",
            Self::Wasd => "wasd",
            Self::Arrows => "arrows",
            Self::Numpad => "numpad",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.as_str() == name)
    }
}

/// One comma-separated item of a key list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyItem {
    /// A named group.
    Group(KeyGroup),
    /// One key by its id in the model file: `esc`, `w`, `kp-enter`.
    Key(String),
    /// Numbered keys from `from` to `to` (`from <= to`), all with the same
    /// prefix: `f1-f12`, `kp1-kp3`, `1-5`.
    Range { prefix: String, from: u32, to: u32 },
}

impl KeyItem {
    /// The key ids this item names, without the model; `None` for a group.
    pub fn ids(&self) -> Option<Vec<String>> {
        match self {
            Self::Group(_) => None,
            Self::Key(id) => Some(vec![id.clone()]),
            Self::Range { prefix, from, to } => Some((*from..=*to).map(|n| format!("{prefix}{n}")).collect()),
        }
    }
}

impl fmt::Display for KeyItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Group(g) => f.write_str(g.as_str()),
            Self::Key(id) => f.write_str(id),
            Self::Range { prefix, from, to } => write!(f, "{prefix}{from}-{prefix}{to}"),
        }
    }
}

/// A key list as typed, read but not yet resolved against a model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeySpec {
    items: Vec<KeyItem>,
}

impl KeySpec {
    /// The whole keyboard.
    pub fn all() -> Self {
        Self { items: vec![KeyItem::Group(KeyGroup::All)] }
    }

    /// Comma-separated items, any case, spaces around commas allowed. An
    /// item is a group name, a numbered range or a key id.
    pub fn parse(input: &str) -> Result<Self, AgentError> {
        let bad = |why: String| {
            AgentError::new(ErrorCode::BadKeys, why)
                .with_hint("Use key ids (esc, f1, w), ranges (f1-f12) or groups (all, f-row, numbers, letters, wasd, arrows, numpad), separated by commas.")
        };
        if input.len() > MAX_KEY_SPEC_LEN {
            return Err(bad(format!("The key list is longer than {MAX_KEY_SPEC_LEN} characters.")));
        }
        let lower = input.to_ascii_lowercase();
        let mut items = Vec::new();
        for raw in lower.split(',') {
            let item = raw.trim();
            if item.is_empty() {
                return Err(bad(if input.trim().is_empty() {
                    "The key list is empty.".to_string()
                } else {
                    format!("'{}' has an empty item.", shown(input))
                }));
            }
            if items.len() == MAX_KEY_ITEMS {
                return Err(bad(format!("The key list has more than {MAX_KEY_ITEMS} items.")));
            }
            items.push(parse_item(item).map_err(bad)?);
        }
        Ok(Self { items })
    }

    pub fn items(&self) -> &[KeyItem] {
        &self.items
    }

    /// Whether the list covers the whole keyboard.
    pub fn is_all(&self) -> bool {
        self.items.contains(&KeyItem::Group(KeyGroup::All))
    }
}

impl Default for KeySpec {
    fn default() -> Self {
        Self::all()
    }
}

/// The canonical text: items in order, lower case, comma-separated. It
/// reads back to the same spec.
impl fmt::Display for KeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

impl FromStr for KeySpec {
    type Err = AgentError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// `f12` → (`f`, 12); `1` → (``, 1). Letters then digits only.
fn numbered(s: &str) -> Option<(&str, u32)> {
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (prefix, digits) = s.split_at(split);
    if !prefix.bytes().all(|b| b.is_ascii_lowercase()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 3 {
        return Some((prefix, u32::MAX));
    }
    Some((prefix, digits.parse().ok()?))
}

/// One trimmed, lower-case item.
fn parse_item(item: &str) -> Result<KeyItem, String> {
    if let Some(group) = KeyGroup::from_name(item) {
        return Ok(KeyItem::Group(group));
    }
    // Two numbered ids around one hyphen are a range. Ids like `kp-enter`
    // have a hyphen too, but no number on both sides.
    if let Some((a, b)) = item.split_once('-') {
        if let (Some((pa, na)), Some((pb, nb))) = (numbered(a), numbered(b)) {
            if pa != pb {
                return Err(format!("'{}' is not a range: both ends need the same name, like f1-f12.", shown(item)));
            }
            let (from, to) = (na.min(nb), na.max(nb));
            if to > MAX_RANGE_NUMBER {
                return Err(format!("'{}' is not a key range.", shown(item)));
            }
            if to - from + 1 > MAX_RANGE_KEYS {
                return Err(format!("'{}' covers more than {MAX_RANGE_KEYS} keys.", shown(item)));
            }
            return Ok(KeyItem::Range { prefix: pa.to_string(), from, to });
        }
    }
    // Letters, digits, `-` and `_`, with a letter or digit at both ends
    // (`f1-` is an unfinished range, not an id).
    let bytes = item.as_bytes();
    let id_ok = item.len() <= MAX_KEY_ID_LEN
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && item.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if id_ok {
        Ok(KeyItem::Key(item.to_string()))
    } else {
        Err(format!("'{}' is not a key name.", shown(item)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(s: &str) -> Vec<KeyItem> {
        KeySpec::parse(s).unwrap().items().to_vec()
    }

    fn key(id: &str) -> KeyItem {
        KeyItem::Key(id.to_string())
    }

    fn range(prefix: &str, from: u32, to: u32) -> KeyItem {
        KeyItem::Range { prefix: prefix.to_string(), from, to }
    }

    #[test]
    fn single_keys_groups_and_ranges() {
        assert_eq!(items("esc"), [key("esc")]);
        assert_eq!(items("F1"), [key("f1")]);
        assert_eq!(items("w"), [key("w")]);
        assert_eq!(items("f1-f12"), [range("f", 1, 12)]);
        assert_eq!(items("kp1-kp3"), [range("kp", 1, 3)]);
        assert_eq!(items("1-5"), [range("", 1, 5)]);
        assert_eq!(items("f12-f1"), [range("f", 1, 12)], "either order");
        assert_eq!(items("f-row"), [KeyItem::Group(KeyGroup::FRow)]);
        for group in KeyGroup::ALL {
            assert_eq!(items(group.as_str()), [KeyItem::Group(group)]);
        }
        assert_eq!(items(" f1-f4 , NUMPAD,esc "), [range("f", 1, 4), KeyItem::Group(KeyGroup::Numpad), key("esc")]);
    }

    #[test]
    fn hyphenated_key_ids_are_not_ranges() {
        // Ids from the AK980 PRO model file.
        for id in ["kp-slash", "kp-star", "kp-minus", "kp-plus", "kp-enter", "kp-dot"] {
            assert_eq!(items(id), [key(id)]);
        }
        assert_eq!(items("kp-1"), [key("kp-1")], "not numbered on both sides: an id the model may not have");
    }

    #[test]
    fn ranges_expand_to_ids() {
        assert_eq!(range("f", 1, 3).ids().unwrap(), ["f1", "f2", "f3"].map(String::from).to_vec());
        assert_eq!(key("esc").ids().unwrap(), ["esc".to_string()]);
        assert_eq!(KeyItem::Group(KeyGroup::Wasd).ids(), None);
    }

    #[test]
    fn bad_key_lists_are_refused() {
        let long_list = vec!["a"; MAX_KEY_ITEMS + 1].join(",");
        let long_spec = "a,".repeat(MAX_KEY_SPEC_LEN);
        let long_id = "k".repeat(MAX_KEY_ID_LEN + 1);
        for bad in [
            "",
            " ",
            ",",
            "esc,",
            "esc,,f1",
            "f1-kp3",
            "f1-f1000",
            "1-100",
            "f 1",
            "-esc",
            "_x",
            "ü",
            "a;b",
            "a/b",
            "f1-",
            "esc_",
            long_list.as_str(),
            long_spec.as_str(),
            long_id.as_str(),
        ] {
            assert_eq!(KeySpec::parse(bad).unwrap_err().code, ErrorCode::BadKeys, "{bad:?}");
        }
        assert!(KeySpec::parse(&vec!["a"; MAX_KEY_ITEMS].join(",")).is_ok());
        assert!(KeySpec::parse("1-64").is_ok());
    }

    #[test]
    fn the_text_form_reads_back() {
        for s in ["esc", "f1-f12", "all", "f-row,wasd,kp-enter", "kp1-kp9,1-5"] {
            let spec = KeySpec::parse(s).unwrap();
            assert_eq!(spec.to_string(), s);
            assert_eq!(KeySpec::parse(&spec.to_string()).unwrap(), spec);
        }
        assert_eq!(KeySpec::parse("F12-F1 , ESC").unwrap().to_string(), "f1-f12,esc");
        assert!(KeySpec::all().is_all());
        assert!(KeySpec::parse("esc,all").unwrap().is_all());
        assert!(!KeySpec::parse("esc").unwrap().is_all());
    }
}
