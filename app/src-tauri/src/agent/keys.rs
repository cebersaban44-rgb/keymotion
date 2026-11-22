//! Which LEDs an agent's key list means on the keyboard in use.
//!
//! `kb_agent::KeySpec` only reads the text (`esc`, `f1-f12`, `wasd`); the
//! model file (`kb-devices`) says which keys a keyboard has and where they
//! sit. The groups go by the model file's key ids, the same on every
//! model: `f1`…`f24` are the F row, `1`…`0` the number row, `a`…`z` the
//! letters, `kp…` and `numlock` the number pad.

use crate::agent::effects::Place;
use kb_agent::{AgentError, ErrorCode, KeyGroup, KeyItem, KeySpec};
use kb_devices::Model;
use serde::Serialize;
use std::collections::HashMap;

/// One key of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub id: String,
    pub label: String,
    pub led: u8,
    /// Top left corner and size, in key units (the model file's).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Key {
    fn centre(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Whether key `id` belongs to `group`.
pub fn in_group(group: KeyGroup, id: &str) -> bool {
    let numbered = |prefix: &str, max: u32| {
        id.strip_prefix(prefix)
            .filter(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| (1..=max).contains(&n))
    };
    match group {
        KeyGroup::All => true,
        KeyGroup::FRow => numbered("f", 24),
        KeyGroup::Numbers => id.len() == 1 && id.as_bytes()[0].is_ascii_digit(),
        KeyGroup::Letters => id.len() == 1 && id.as_bytes()[0].is_ascii_lowercase(),
        KeyGroup::Wasd => matches!(id, "w" | "a" | "s" | "d"),
        KeyGroup::Arrows => matches!(id, "up" | "down" | "left" | "right"),
        KeyGroup::Numpad => id == "numlock" || id.starts_with("kp"),
    }
}

/// A model's keys, ready to resolve key lists against.
#[derive(Debug, Clone)]
pub struct KeyIndex {
    pub model_id: String,
    pub model_name: String,
    keys: Vec<Key>,
    by_id: HashMap<String, usize>,
}

impl KeyIndex {
    /// `None` for a model without a key layout.
    pub fn of(model: &Model) -> Option<Self> {
        let g = model.geometry.as_ref()?;
        let keys: Vec<Key> = g
            .keys
            .iter()
            .map(|k| Key { id: k.id.clone(), label: k.label.clone(), led: k.led, x: k.x, y: k.y, w: k.w, h: k.h })
            .collect();
        Some(Self::new(&model.id, &model.name, keys))
    }

    pub fn new(model_id: &str, model_name: &str, keys: Vec<Key>) -> Self {
        let by_id = keys.iter().enumerate().map(|(i, k)| (k.id.clone(), i)).collect();
        Self { model_id: model_id.to_string(), model_name: model_name.to_string(), keys, by_id }
    }

    /// The model with id `model_id` in the catalogue.
    pub fn for_model(model_id: &str) -> Option<Self> {
        kb_devices::catalogue().ok()?.iter().find(|m| m.id == model_id).and_then(Self::of)
    }

    #[cfg(test)]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// The keys `spec` names, each once, in reading order (top to bottom,
    /// then left to right, by their centres), with where each sits among
    /// them. `unknown_key` for a key, or a whole group, the model does not
    /// have.
    pub fn resolve(&self, spec: &KeySpec) -> Result<Vec<(u8, Place)>, AgentError> {
        let mut picked: Vec<usize> = Vec::new();
        let mut add = |i: usize| {
            if !picked.contains(&i) {
                picked.push(i);
            }
        };
        for item in spec.items() {
            match item {
                KeyItem::Group(group) => {
                    let members: Vec<usize> =
                        (0..self.keys.len()).filter(|&i| in_group(*group, &self.keys[i].id)).collect();
                    if members.is_empty() {
                        return Err(AgentError::new(
                            ErrorCode::UnknownKey,
                            format!("This keyboard has no keys in the '{}' group.", group.as_str()),
                        )
                        .with_hint("keymotion-cli keys lists the keys and groups this keyboard has."));
                    }
                    members.into_iter().for_each(&mut add);
                }
                _ => {
                    for id in item.ids().unwrap_or_default() {
                        let Some(&i) = self.by_id.get(&id) else {
                            return Err(AgentError::new(
                                ErrorCode::UnknownKey,
                                format!("This keyboard has no key '{id}'."),
                            )
                            .with_hint("keymotion-cli keys lists the keys and groups this keyboard has."));
                        };
                        add(i);
                    }
                }
            }
        }
        Ok(self.placed(&picked))
    }

    /// The same as [`resolve`](Self::resolve), leaving out what the model
    /// does not have (a light kept from another keyboard).
    pub fn resolve_lenient(&self, spec: &KeySpec) -> Vec<(u8, Place)> {
        if let Ok(keys) = self.resolve(spec) {
            return keys;
        }
        let mut picked: Vec<usize> = Vec::new();
        for item in spec.items() {
            let found: Vec<usize> = match item {
                KeyItem::Group(group) => (0..self.keys.len()).filter(|&i| in_group(*group, &self.keys[i].id)).collect(),
                _ => item.ids().unwrap_or_default().iter().filter_map(|id| self.by_id.get(id).copied()).collect(),
            };
            for i in found {
                if !picked.contains(&i) {
                    picked.push(i);
                }
            }
        }
        self.placed(&picked)
    }

    /// LEDs with their places among `picked`.
    fn placed(&self, picked: &[usize]) -> Vec<(u8, Place)> {
        let mut order: Vec<usize> = picked.to_vec();
        // Reading order for a bar: by row (rounded to half a key), then
        // left to right.
        order.sort_by(|&a, &b| {
            let (ax, ay) = self.keys[a].centre();
            let (bx, by) = self.keys[b].centre();
            ((ay * 2.0).round()).total_cmp(&(by * 2.0).round()).then(ax.total_cmp(&bx))
        });
        let span = |pick: fn((f32, f32)) -> f32| {
            let values = order.iter().map(|&i| pick(self.keys[i].centre()));
            let lo = values.clone().fold(f32::INFINITY, f32::min);
            let hi = values.fold(f32::NEG_INFINITY, f32::max);
            (lo, hi - lo)
        };
        let (x0, xs) = span(|c| c.0);
        let (y0, ys) = span(|c| c.1);
        let share = |v: f32, lo: f32, span: f32| if span > f32::EPSILON { (v - lo) / span } else { 0.5 };
        let count = order.len();
        order
            .iter()
            .enumerate()
            .map(|(rank, &i)| {
                let (cx, cy) = self.keys[i].centre();
                (self.keys[i].led, Place { x: share(cx, x0, xs), y: share(cy, y0, ys), rank, count })
            })
            .collect()
    }

    /// What `keys` answers: every key with its groups, and each group's keys.
    pub fn describe(&self) -> KeysData {
        let groups_of = |id: &str| {
            KeyGroup::ALL.into_iter().filter(|g| *g != KeyGroup::All && in_group(*g, id)).map(|g| g.as_str()).collect()
        };
        KeysData {
            model: self.model_id.clone(),
            name: self.model_name.clone(),
            keys: self
                .keys
                .iter()
                .map(|k| KeyData {
                    id: k.id.clone(),
                    label: k.label.clone(),
                    x: k.x,
                    y: k.y,
                    w: k.w,
                    h: k.h,
                    groups: groups_of(&k.id),
                })
                .collect(),
            groups: KeyGroup::ALL
                .into_iter()
                .map(|g| GroupData {
                    name: g.as_str(),
                    keys: if g == KeyGroup::All {
                        Vec::new()
                    } else {
                        self.keys.iter().filter(|k| in_group(g, &k.id)).map(|k| k.id.clone()).collect()
                    },
                })
                .filter(|g| g.name == KeyGroup::All.as_str() || !g.keys.is_empty())
                .collect(),
        }
    }
}

/// The answer to `keys`.
#[derive(Debug, Clone, Serialize)]
pub struct KeysData {
    pub model: String,
    pub name: String,
    pub keys: Vec<KeyData>,
    /// The groups this keyboard has (`all` lists no keys: it is every one).
    pub groups: Vec<GroupData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyData {
    pub id: String,
    pub label: String,
    /// Top left corner and size, in key units.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub groups: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupData {
    pub name: &'static str,
    pub keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ak980() -> KeyIndex {
        let model = kb_devices::catalogue().unwrap().iter().find(|m| m.id == "ajazz-ak980-pro").unwrap();
        KeyIndex::of(model).unwrap()
    }

    fn ids(index: &KeyIndex, spec: &str) -> Vec<String> {
        let leds = index.resolve(&KeySpec::parse(spec).unwrap()).unwrap();
        leds.iter().map(|(led, _)| index.keys().iter().find(|k| k.led == *led).unwrap().id.clone()).collect()
    }

    #[test]
    fn groups_go_by_the_model_files_ids() {
        let k = ak980();
        assert_eq!(ids(&k, "f-row"), (1..=12).map(|n| format!("f{n}")).collect::<Vec<_>>());
        assert_eq!(ids(&k, "numbers"), ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]);
        assert_eq!(ids(&k, "wasd"), ["w", "a", "s", "d"]);
        assert_eq!(ids(&k, "arrows"), ["up", "left", "down", "right"]);
        assert_eq!(ids(&k, "letters").len(), 26);
        let numpad = ids(&k, "numpad");
        assert_eq!(numpad.len(), 17, "{numpad:?}");
        assert!(numpad.contains(&"numlock".to_string()) && numpad.contains(&"kp-enter".to_string()));
        assert_eq!(ids(&k, "all").len(), k.keys().len());
        assert!(!in_group(KeyGroup::FRow, "f0") && !in_group(KeyGroup::FRow, "f25") && !in_group(KeyGroup::FRow, "f"));
        assert!(!in_group(KeyGroup::FRow, "f01"));
        assert!(!in_group(KeyGroup::Numbers, "kp1"));
    }

    #[test]
    fn lists_resolve_to_leds_once_each() {
        let k = ak980();
        let esc = k.resolve(&KeySpec::parse("esc").unwrap()).unwrap();
        assert_eq!(esc, vec![(1, Place { x: 0.5, y: 0.5, rank: 0, count: 1 })]);
        // Ranges, groups and keys together; F1 named twice counts once.
        let mixed = k.resolve(&KeySpec::parse("f1-f4,f1,wasd").unwrap()).unwrap();
        assert_eq!(mixed.len(), 8);
        // Places run from 0 to 1 over the keys named, in reading order.
        let row = k.resolve(&KeySpec::parse("f1-f12").unwrap()).unwrap();
        assert_eq!(row.first().unwrap().1.x, 0.0);
        assert_eq!(row.last().unwrap().1.x, 1.0);
        assert!(row.windows(2).all(|w| w[0].1.x < w[1].1.x && w[0].1.rank + 1 == w[1].1.rank));
        assert!(row.iter().all(|(_, p)| p.count == 12));
    }

    #[test]
    fn a_key_or_group_the_keyboard_lacks_is_refused() {
        let k = ak980();
        for (spec, what) in [("f13", "f13"), ("f1-f13", "f13"), ("esc,menu", "menu")] {
            let e = k.resolve(&KeySpec::parse(spec).unwrap()).unwrap_err();
            assert_eq!((e.code, e.exit_code()), (ErrorCode::UnknownKey, 65));
            assert!(e.message.contains(what), "{}", e.message);
        }
        // A keyboard without a number pad.
        let tkl = KeyIndex::new(
            "tkl",
            "TKL",
            vec![Key { id: "esc".into(), label: "Esc".into(), led: 1, x: 0.0, y: 0.0, w: 1.0, h: 1.0 }],
        );
        let e = tkl.resolve(&KeySpec::parse("numpad").unwrap()).unwrap_err();
        assert_eq!(e.code, ErrorCode::UnknownKey);
        // Kept lights leave out what this keyboard does not have.
        assert_eq!(tkl.resolve_lenient(&KeySpec::parse("esc,numpad,f5").unwrap()).len(), 1);
    }

    #[test]
    fn keys_lists_ids_labels_places_and_groups() {
        let data = ak980().describe();
        assert_eq!(data.model, "ajazz-ak980-pro");
        let w = data.keys.iter().find(|k| k.id == "w").unwrap();
        assert_eq!(w.label, "W");
        assert_eq!(w.groups, ["letters", "wasd"]);
        let names: Vec<&str> = data.groups.iter().map(|g| g.name).collect();
        assert_eq!(names, ["all", "f-row", "numbers", "letters", "wasd", "arrows", "numpad"]);
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.len() < kb_agent::MAX_LINE_BYTES / 2, "{} bytes", json.len());
    }
}
