//! Profiles: named sets of the keyboard's settings (lighting, key mapping,
//! macros and, when the user asks, a screen picture) the user puts on the
//! keyboard in one go. They live in `profiles.json` in the config folder,
//! versioned and written like the other settings: a damaged file is kept
//! aside as `profiles.json.bak-…` and reported once (`settings.rs`).
//!
//! # What "current" means
//!
//! A profile made "from the current settings" takes what this computer
//! recorded as saved on the keyboard, never an editor's unsaved draft:
//!
//! * lighting: the Lighting page's record (`lighting.json`), its
//!   `savedPerKey` design or else its `appliedEffect`; unknown when the app
//!   never stored either;
//! * key mapping and macros: the records the device actor keeps
//!   (`keymap.json`, `macros.json`); a layer never written is the factory
//!   layout, macros never written are none;
//! * screen: the library item the GIF page shows, as far as the app knows.
//!
//! # Loading
//!
//! A profile goes to the keyboard one part at a time, in the order of
//! [`ProfilePart`], through the device actor's own requests (effect,
//! per-key, keymap, macros), so every check before a write applies. A part
//! that already equals the record is skipped and nothing is written for it;
//! a keymap layer that already matches is skipped too. The UI runs the parts
//! one by one, stops at the first failure and names it. The screen part is
//! an ordinary library upload the UI starts (`screen_upload`).
//!
//! # The built-in profile
//!
//! "Varsayılan" cannot be renamed, changed or deleted. It holds the factory
//! key layout (both layers back to the default, the Key mapping page's
//! "back to factory" write), no macros, and the factory effect the protocol
//! crate documents (`EffectSettings::factory`: Rolling, rainbow, brightness
//! 5, speed 3; docs/research/protokol-ff13.md §4.1, docs/donanim-dogrulama.md
//! "Işık yazma"). No firmware reset command is ever sent.
//!
//! # Profile files
//!
//! One profile exported on its own, beside the backup format (`backup.rs`):
//!
//! ```text
//! {
//!   "kind": "keymotion-profile",   // always this
//!   "version": 1,                  // a newer number is refused
//!   "createdMs": 1794000000000,
//!   "appVersion": "0.1.0",
//!   "profile": {
//!     "name": "Oyun", "modelId": "ajazz-ak980-pro",
//!     "lighting": { "kind": "per-key", "colors": { "33": "#ff8a3d" }, "brightness": 5 },
//!     "keymap": { "base": { "33": "02000400" }, "fn": {} },
//!     "macros": [ { "name": "imza", "steps": [["down", 4, 20]] }, null ],
//!     "screen": { "name", "kind", "createdMs", "data": "<base64>" }   // as in a backup
//!   }
//! }
//! ```
//!
//! Every part is optional. The screen picture travels with the file (the
//! factory animation never does, as in a backup). An import is checked
//! whole before anything is stored.

use crate::backup::ScreenItemBackup;
use crate::dto::{ScreenItemKind, MACRO_NAME_MAX};
use crate::settings::{self, StoredKeymap, StoredLayer, StoredMacro, StoredMacros};
use base64::Engine;
use kb_devices::Model;
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::screen::{self, ScreenAnimation};
use kb_protocol::sonix::{caps, EffectSettings, MAX_LEVEL, PER_KEY_MAX_IDX};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

pub const FILE: &str = "profiles.json";
const VERSION: u32 = 1;
/// Id of the built-in profile ("Varsayılan").
pub const DEFAULT_ID: &str = "default";
/// Longest profile name.
pub const NAME_MAX: usize = 40;
/// Profiles a user can keep.
pub const MAX_PROFILES: usize = 100;

/// `kind` of a profile file.
pub const FILE_KIND: &str = "keymotion-profile";
pub const FILE_VERSION: u32 = 1;
/// Larger profile files are refused before they are read (a picture of 140
/// frames is about 12 MB in base64).
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

// Errors the UI words itself (lib/errors.ts).
pub const ERR_NOT_PROFILE: &str = "not a profile file";
pub const ERR_BACKUP_FILE: &str = "a backup file, not a profile";
pub const ERR_NEWER: &str = "profile from a newer version";
pub const ERR_DAMAGED: &str = "profile file damaged";
pub const ERR_TOO_LARGE: &str = "profile file too large";
pub const ERR_OTHER_MODEL: &str = "profile for another keyboard model";
pub const ERR_NAME_TAKEN: &str = "profile name taken";
pub const ERR_NAME_EMPTY: &str = "profile name empty";
pub const ERR_FULL: &str = "profile limit reached";
pub const ERR_NOTHING: &str = "nothing to put in the profile";
pub const ERR_BUILT_IN: &str = "the default profile cannot be changed";
pub const ERR_UNKNOWN: &str = "unknown profile";
pub const ERR_SCREEN_PART: &str = "the screen part is uploaded from the library";

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

// ------------------------------------------------------------------ parts

/// A part of a profile, in loading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ProfilePart {
    Lighting,
    Keymap,
    Macros,
    Screen,
}

impl ProfilePart {
    pub const ALL: [Self; 4] = [Self::Lighting, Self::Keymap, Self::Macros, Self::Screen];

    fn name(self) -> &'static str {
        match self {
            Self::Lighting => "lighting",
            Self::Keymap => "keymap",
            Self::Macros => "macros",
            Self::Screen => "screen",
        }
    }
}

impl std::fmt::Display for ProfilePart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The lighting of a profile, in the shape the Lighting page keeps what it
/// stored on the keyboard (`lighting.ts` `appliedEffect` / `savedPerKey`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProfileLighting {
    /// A built-in effect.
    Effect { mode: u8, color: String, rainbow: bool, brightness: u8, speed: u8, direction: u8 },
    /// A per-key design: LED (decimal) → `#rrggbb`; keys not listed are off.
    PerKey { colors: BTreeMap<String, String>, brightness: u8 },
}

/// One LED of a per-key design and its colour.
pub type LitKey = (u8, [u8; 3]);

/// `#rrggbb` in lower case, or `None`.
fn hex(v: &str) -> Option<String> {
    let h = v.trim().strip_prefix('#')?;
    (h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("#{}", h.to_ascii_lowercase()))
}

impl ProfileLighting {
    /// The factory effect (`EffectSettings::factory`).
    pub fn factory() -> Self {
        let f = EffectSettings::factory();
        let [r, g, b] = f.color;
        Self::Effect {
            mode: f.mode,
            color: format!("#{r:02x}{g:02x}{b:02x}"),
            rainbow: f.rainbow,
            brightness: f.brightness,
            speed: f.speed,
            direction: f.direction,
        }
    }

    /// Checked and in one spelling (lower-case colours, plain LED numbers);
    /// `None` when the keyboard would not take it.
    pub fn checked(self) -> Option<Self> {
        match self {
            Self::Effect { mode, color, rainbow, brightness, speed, direction } => {
                let e = Self::Effect { mode, color: hex(&color)?, rainbow, brightness, speed, direction };
                kb_protocol::sonix::lighting_data(&e.settings()?).ok()?;
                Some(e)
            }
            Self::PerKey { colors, brightness } => {
                if brightness > MAX_LEVEL {
                    return None;
                }
                let mut out = BTreeMap::new();
                for (led, c) in colors {
                    let led = led.trim().parse::<u8>().ok().filter(|&l| l <= PER_KEY_MAX_IDX)?;
                    if out.insert(led.to_string(), hex(&c)?).is_some() {
                        return None;
                    }
                }
                Some(Self::PerKey { colors: out, brightness })
            }
        }
    }

    /// The effect as the keyboard takes it (`None` for a per-key design).
    pub fn settings(&self) -> Option<EffectSettings> {
        let Self::Effect { mode, color, rainbow, brightness, speed, direction } = self else { return None };
        Some(EffectSettings {
            mode: *mode,
            color: crate::dto::parse_hex(color).ok()?,
            rainbow: *rainbow,
            brightness: *brightness,
            speed: *speed,
            direction: *direction,
        })
    }

    /// Per-key colours as the keyboard takes them (`None` for an effect).
    pub fn per_key(&self) -> Option<(Vec<LitKey>, u8)> {
        let Self::PerKey { colors, brightness } = self else { return None };
        let keys = colors
            .iter()
            .filter_map(|(led, c)| Some((led.parse::<u8>().ok()?, crate::dto::parse_hex(c).ok()?)))
            .collect();
        Some((keys, *brightness))
    }

    /// What the keyboard has according to the Lighting page's record
    /// (`lighting.json`): its per-key design, else its effect; `None` when
    /// the app has not stored either.
    pub fn from_record(state: &Map<String, Value>) -> Option<Self> {
        if let Some(saved) = state.get("savedPerKey").filter(|v| v.is_object()) {
            let colors = saved.get("colors").and_then(Value::as_object)?;
            let colors =
                colors.iter().map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect::<Option<_>>()?;
            let brightness = saved.get("brightness").and_then(Value::as_u64).and_then(|b| u8::try_from(b).ok())?;
            return Self::PerKey { colors, brightness }.checked();
        }
        let applied = state.get("appliedEffect").filter(|v| v.is_object())?;
        let e: crate::dto::EffectRequest = serde_json::from_value(applied.clone()).ok()?;
        Self::Effect {
            mode: e.mode,
            color: e.color,
            rainbow: e.rainbow,
            brightness: e.brightness,
            speed: e.speed,
            direction: e.direction,
        }
        .checked()
    }

    /// The same light on the keyboard. Effects compare only what the effect
    /// uses: the colour does not count while its rainbow is on, nor the
    /// speed for an effect without one.
    pub fn same(&self, other: &Self) -> bool {
        match (self.settings(), other.settings()) {
            (Some(a), Some(b)) => same_effect(&a, &b),
            (None, None) => self == other,
            _ => false,
        }
    }

    /// The record the Lighting page keeps after this went to the keyboard.
    pub fn stored(&self) -> crate::lighting::StoredLight {
        match self {
            Self::Effect { mode, color, rainbow, brightness, speed, direction } => {
                crate::lighting::StoredLight::Effect(serde_json::json!({
                    "mode": mode, "color": color, "rainbow": rainbow,
                    "brightness": brightness, "speed": speed, "direction": direction,
                }))
            }
            Self::PerKey { colors, brightness } => {
                crate::lighting::StoredLight::PerKey { colors: serde_json::json!(colors), brightness: *brightness }
            }
        }
    }
}

fn same_effect(a: &EffectSettings, b: &EffectSettings) -> bool {
    if a.mode != b.mode {
        return false;
    }
    let Some(fx) = kb_protocol::sonix::effect(a.mode) else { return a == b };
    let rainbow = fx.has(caps::RAINBOW);
    let color = fx.has(caps::COLOR) && !(rainbow && a.rainbow);
    (!rainbow || a.rainbow == b.rainbow)
        && (!color || a.color == b.color)
        && (!fx.has(caps::BRIGHTNESS) || a.brightness == b.brightness)
        && (!fx.has(caps::SPEED) || a.speed == b.speed)
        && (!fx.has(caps::DIRECTION2 | caps::DIRECTION4) || a.direction == b.direction)
}

/// Both key mapping layers of a profile, as `keymap.json` keeps a layer:
/// LED (decimal) → table entry as 8 hex digits. Empty is the factory layout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileKeymap {
    #[serde(default)]
    pub base: BTreeMap<String, String>,
    #[serde(default, rename = "fn")]
    pub fn_layer: BTreeMap<String, String>,
}

impl ProfileKeymap {
    /// The record of a model (`None`: never written, the factory layout).
    pub fn from_record(k: Option<&StoredKeymap>) -> Self {
        k.map(|k| Self { base: k.base.entries.clone(), fn_layer: k.fn_layer.entries.clone() }).unwrap_or_default()
    }

    pub fn layer(&self, layer: Layer) -> &BTreeMap<String, String> {
        match layer {
            Layer::Base => &self.base,
            Layer::Fn => &self.fn_layer,
        }
    }

    fn layer_mut(&mut self, layer: Layer) -> &mut BTreeMap<String, String> {
        match layer {
            Layer::Base => &mut self.base,
            Layer::Fn => &mut self.fn_layer,
        }
    }

    /// The actions of `layer`, for a write.
    pub fn actions(&self, layer: Layer) -> Vec<(u8, KeyAction)> {
        StoredLayer { entries: self.layer(layer).clone(), written_ms: None }.actions().into_iter().collect()
    }

    /// Keys that differ from the factory layout, both layers.
    pub fn changes(&self) -> usize {
        self.base.len() + self.fn_layer.len()
    }

    /// Checked against `model` and in the canonical form the record keeps
    /// after a write (so a loaded profile matches its record).
    pub fn checked(self, model: &Model) -> Option<Self> {
        let mut out = Self::default();
        for layer in [Layer::Base, Layer::Fn] {
            let src = self.layer(layer);
            let actions = StoredLayer { entries: src.clone(), written_ms: None }.actions();
            if actions.len() != src.len() {
                return None;
            }
            let map = kb_core::keymap::canonical(model, layer, actions).ok()?;
            *out.layer_mut(layer) = StoredLayer::new(&map, 0).entries;
        }
        Some(out)
    }
}

/// Macros without the unused slots at the end (the record keeps them so).
pub fn trimmed(mut slots: Vec<Option<StoredMacro>>) -> Vec<Option<StoredMacro>> {
    while matches!(slots.last(), Some(None)) {
        slots.pop();
    }
    slots
}

/// Macros the keyboard would take, names cleaned like the editor's.
fn macros_checked(slots: Vec<Option<StoredMacro>>) -> Option<Vec<Option<StoredMacro>>> {
    let slots: Vec<Option<StoredMacro>> = trimmed(slots)
        .into_iter()
        .map(|m| m.map(|m| StoredMacro { name: m.name.trim().chars().take(MACRO_NAME_MAX).collect(), steps: m.steps }))
        .collect();
    let image = StoredMacros { slots: slots.clone(), written_ms: None }.image_slots();
    kb_protocol::sonix::macros::macro_image(&image).ok()?;
    Some(slots)
}

/// A library item a profile shows on the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileScreen {
    /// Library id (content hash).
    pub id: String,
    /// Its name when the profile was made (shown if the item is gone).
    pub name: String,
}

fn is_item_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

// ------------------------------------------------------------------ profile

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub created_ms: f64,
    #[serde(default)]
    pub updated_ms: f64,
    /// Model the key mapping and macros belong to; empty for the built-in
    /// profile, which fits every model.
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<ProfileLighting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap: Option<ProfileKeymap>,
    /// Slot `i` is macro `i`; `null` an unused slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macros: Option<Vec<Option<StoredMacro>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ProfileScreen>,
}

/// What the keyboard has, as this computer recorded it (see the module docs).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Current {
    pub model_id: String,
    /// `None`: the app has not stored any lighting on the keyboard.
    pub lighting: Option<ProfileLighting>,
    pub keymap: ProfileKeymap,
    /// Trimmed (no unused slots at the end).
    pub macros: Vec<Option<StoredMacro>>,
    /// The library item the GIF page shows.
    pub screen: Option<ProfileScreen>,
}

/// The built-in profile (see the module docs).
pub fn default_profile() -> Profile {
    Profile {
        id: DEFAULT_ID.into(),
        name: String::new(),
        created_ms: 0.0,
        updated_ms: 0.0,
        model_id: String::new(),
        lighting: Some(ProfileLighting::factory()),
        keymap: Some(ProfileKeymap::default()),
        macros: Some(Vec::new()),
        screen: None,
    }
}

impl Profile {
    /// The parts it holds, in loading order.
    pub fn parts(&self) -> Vec<ProfilePart> {
        ProfilePart::ALL.into_iter().filter(|&p| self.has(p)).collect()
    }

    pub fn has(&self, part: ProfilePart) -> bool {
        match part {
            ProfilePart::Lighting => self.lighting.is_some(),
            ProfilePart::Keymap => self.keymap.is_some(),
            ProfilePart::Macros => self.macros.is_some(),
            ProfilePart::Screen => self.screen.is_some(),
        }
    }

    /// It can go to a keyboard of this model.
    pub fn fits(&self, model_id: &str) -> bool {
        self.model_id.is_empty() || self.model_id == model_id
    }

    /// `part` equals what the keyboard has (a part not in the profile does).
    pub fn part_matches(&self, part: ProfilePart, current: &Current) -> bool {
        match part {
            ProfilePart::Lighting => {
                self.lighting.as_ref().is_none_or(|l| current.lighting.as_ref().is_some_and(|c| l.same(c)))
            }
            ProfilePart::Keymap => self.keymap.as_ref().is_none_or(|k| *k == current.keymap),
            ProfilePart::Macros => self.macros.as_ref().is_none_or(|m| *m == current.macros),
            ProfilePart::Screen => {
                self.screen.as_ref().is_none_or(|s| current.screen.as_ref().is_some_and(|c| c.id == s.id))
            }
        }
    }

    /// Every part it holds is on the keyboard ("Klavyede").
    pub fn on_keyboard(&self, current: &Current) -> bool {
        self.fits(&current.model_id)
            && !self.parts().is_empty()
            && ProfilePart::ALL.into_iter().all(|p| self.part_matches(p, current))
    }

    /// Checked whole (a profile from a file or a backup): a known model,
    /// parts the keyboard takes, at least one part, a name.
    pub fn checked(mut self) -> Result<Self, String> {
        let model = model(&self.model_id).ok_or(ERR_OTHER_MODEL)?;
        self.name = clean_name(&self.name).ok_or(ERR_DAMAGED)?;
        if let Some(l) = self.lighting.take() {
            self.lighting = Some(l.checked().ok_or(ERR_DAMAGED)?);
        }
        if let Some(k) = self.keymap.take() {
            self.keymap = Some(k.checked(model).ok_or(ERR_DAMAGED)?);
        }
        if let Some(m) = self.macros.take() {
            self.macros = Some(macros_checked(m).ok_or(ERR_DAMAGED)?);
        }
        if let Some(s) = self.screen.take() {
            if !is_item_id(&s.id) {
                return Err(ERR_DAMAGED.into());
            }
            self.screen =
                Some(ProfileScreen { id: s.id.to_ascii_lowercase(), name: crate::screen_lib::clean_name(&s.name) });
        }
        if self.parts().is_empty() {
            return Err(ERR_DAMAGED.into());
        }
        Ok(self)
    }
}

fn model(id: &str) -> Option<&'static Model> {
    kb_devices::catalogue().ok()?.iter().find(|m| m.id == id)
}

/// A usable name: no control characters, trimmed, at most [`NAME_MAX`].
pub fn clean_name(name: &str) -> Option<String> {
    let n: String = name.chars().filter(|c| !c.is_control()).collect();
    let n: String = n.trim().chars().take(NAME_MAX).collect();
    let n = n.trim_end();
    (!n.is_empty()).then(|| n.to_string())
}

// ------------------------------------------------------------------ for the UI

/// Which parts "create from the current settings" takes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProfileParts {
    pub lighting: bool,
    pub keymap: bool,
    pub macros: bool,
    pub screen: bool,
}

/// A profile's lighting, in short.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[ts(export)]
pub enum ProfileLightingInfo {
    Effect { mode: u8, color: String, rainbow: bool, brightness: u8 },
    PerKey { keys: u32, brightness: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileScreenInfo {
    pub id: String,
    /// The library item's name now, else the one kept in the profile.
    pub name: String,
    /// The item is still in the library (else this part cannot be loaded).
    pub in_library: bool,
    /// Pages an upload takes (0 when the item is gone).
    pub pages: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileInfo {
    pub id: String,
    /// Empty for the built-in profile (the UI names it).
    pub name: String,
    pub built_in: bool,
    pub created_ms: f64,
    pub updated_ms: f64,
    pub lighting: Option<ProfileLightingInfo>,
    /// Keys changed from the factory layout; `null`: not in the profile.
    pub keymap: Option<u32>,
    /// Macros; `null`: not in the profile.
    pub macros: Option<u32>,
    pub screen: Option<ProfileScreenInfo>,
    /// Every part it holds is on the keyboard.
    pub on_keyboard: bool,
    /// Parts loading would write (they differ from the keyboard's record),
    /// in order. A screen picture no longer in the library is not among them.
    pub to_write: Vec<ProfilePart>,
    /// Made for another keyboard model: it cannot be loaded here.
    pub other_model: bool,
}

/// What "create from the current settings" can take now.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileCurrent {
    /// The app knows the lighting it last stored on the keyboard.
    pub lighting: bool,
    /// The library item on the screen, by name; `null` when it is not one.
    pub screen: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfilesState {
    /// The user's profiles, oldest first.
    pub profiles: Vec<ProfileInfo>,
    /// The built-in "Varsayılan".
    pub factory: ProfileInfo,
    pub current: ProfileCurrent,
    /// No room for another profile.
    pub full: bool,
}

/// A profile just made (created, duplicated or imported).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileSaved {
    pub id: String,
    pub name: String,
}

/// How a part of a load went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum PartLoaded {
    /// Written to the keyboard.
    Written,
    /// The keyboard had it already; nothing was written.
    Same,
}

/// A library item, as far as a profile needs it: its name and upload pages.
pub struct LibraryItem {
    pub name: String,
    pub pages: u32,
}

/// The profile for the UI. `library` finds an item by id.
pub fn info(
    p: &Profile,
    built_in: bool,
    current: &Current,
    library: &dyn Fn(&str) -> Option<LibraryItem>,
) -> ProfileInfo {
    let other_model = !p.fits(&current.model_id);
    let screen = p.screen.as_ref().map(|s| {
        let item = library(&s.id);
        ProfileScreenInfo {
            id: s.id.clone(),
            name: item.as_ref().map_or_else(|| s.name.clone(), |i| i.name.clone()),
            in_library: item.is_some(),
            pages: item.map_or(0, |i| i.pages),
        }
    });
    let gone = screen.as_ref().is_some_and(|s| !s.in_library);
    let to_write = if other_model {
        Vec::new()
    } else {
        p.parts()
            .into_iter()
            .filter(|&part| !p.part_matches(part, current) && !(part == ProfilePart::Screen && gone))
            .collect()
    };
    ProfileInfo {
        id: p.id.clone(),
        name: if built_in { String::new() } else { p.name.clone() },
        built_in,
        created_ms: p.created_ms,
        updated_ms: p.updated_ms,
        lighting: p.lighting.as_ref().map(|l| match l {
            ProfileLighting::Effect { mode, color, rainbow, brightness, .. } => ProfileLightingInfo::Effect {
                mode: *mode,
                color: color.clone(),
                rainbow: *rainbow,
                brightness: *brightness,
            },
            ProfileLighting::PerKey { colors, brightness } => {
                ProfileLightingInfo::PerKey { keys: colors.len() as u32, brightness: *brightness }
            }
        }),
        keymap: p.keymap.as_ref().map(|k| k.changes() as u32),
        macros: p.macros.as_ref().map(|m| m.iter().flatten().count() as u32),
        screen,
        on_keyboard: p.on_keyboard(current),
        to_write,
        other_model,
    }
}

// ------------------------------------------------------------------ store

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    profiles: Vec<Profile>,
}

/// The user's profiles (managed state; the built-in one is not stored).
pub struct ProfileStore {
    dir: Option<PathBuf>,
    profiles: Vec<Profile>,
}

impl ProfileStore {
    /// Open `profiles.json` in `dir` (in memory only without a folder).
    pub fn open(dir: Option<&Path>) -> Self {
        let profiles =
            dir.and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).map(|s| s.profiles).unwrap_or_default();
        Self { dir: dir.map(Path::to_path_buf), profiles }
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// A profile by id, the built-in one included.
    pub fn get(&self, id: &str) -> Option<Profile> {
        if id == DEFAULT_ID {
            return Some(default_profile());
        }
        self.profiles.iter().find(|p| p.id == id).cloned()
    }

    pub fn state(&self, current: &Current, library: &dyn Fn(&str) -> Option<LibraryItem>) -> ProfilesState {
        ProfilesState {
            profiles: self.profiles.iter().map(|p| info(p, false, current, library)).collect(),
            factory: info(&default_profile(), true, current, library),
            current: ProfileCurrent {
                lighting: current.lighting.is_some(),
                screen: current.screen.as_ref().map(|s| s.name.clone()),
            },
            full: self.profiles.len() >= MAX_PROFILES,
        }
    }

    /// Change the list and save it; nothing changes when saving fails.
    fn change<T>(&mut self, f: impl FnOnce(&mut Vec<Profile>) -> Result<T, String>) -> Result<T, String> {
        let mut next = self.profiles.clone();
        let out = f(&mut next)?;
        if let Some(dir) = &self.dir {
            settings::save(dir, FILE, VERSION, Stored { profiles: next.clone() })
                .map_err(|e| format!("profiles not saved: {e}"))?;
        }
        self.profiles = next;
        Ok(out)
    }

    /// A new profile from the current settings.
    pub fn create(&mut self, name: &str, parts: ProfileParts, current: &Current, now: f64) -> Result<Profile, String> {
        let name = clean_name(name).ok_or(ERR_NAME_EMPTY)?;
        let profile = Profile {
            id: String::new(),
            name,
            created_ms: now,
            updated_ms: now,
            model_id: current.model_id.clone(),
            lighting: if parts.lighting { current.lighting.clone() } else { None },
            keymap: parts.keymap.then(|| current.keymap.clone()),
            macros: parts.macros.then(|| current.macros.clone()),
            screen: if parts.screen { current.screen.clone() } else { None },
        };
        if profile.parts().is_empty() {
            return Err(ERR_NOTHING.into());
        }
        self.change(|list| {
            if list.len() >= MAX_PROFILES {
                return Err(ERR_FULL.into());
            }
            if name_taken(list, &profile.name, None) {
                return Err(ERR_NAME_TAKEN.into());
            }
            let profile = Profile { id: new_id(list, now), ..profile };
            list.push(profile.clone());
            Ok(profile)
        })
    }

    /// The profile's parts from the current settings (the same parts; one
    /// the computer has no record of, such as unknown lighting, is kept).
    pub fn update(&mut self, id: &str, current: &Current, now: f64) -> Result<(), String> {
        self.change(|list| {
            let p = find(list, id)?;
            p.model_id = current.model_id.clone();
            if p.lighting.is_some() && current.lighting.is_some() {
                p.lighting = current.lighting.clone();
            }
            if p.keymap.is_some() {
                p.keymap = Some(current.keymap.clone());
            }
            if p.macros.is_some() {
                p.macros = Some(current.macros.clone());
            }
            if p.screen.is_some() && current.screen.is_some() {
                p.screen = current.screen.clone();
            }
            p.updated_ms = now;
            Ok(())
        })
    }

    pub fn rename(&mut self, id: &str, name: &str, now: f64) -> Result<(), String> {
        let name = clean_name(name).ok_or(ERR_NAME_EMPTY)?;
        self.change(|list| {
            if name_taken(list, &name, Some(id)) {
                return Err(ERR_NAME_TAKEN.into());
            }
            let p = find(list, id)?;
            if p.name != name {
                p.name = name;
                p.updated_ms = now;
            }
            Ok(())
        })
    }

    /// A copy named `name` (made unique: "Oyun kopyası (2)").
    pub fn duplicate(&mut self, id: &str, name: &str, now: f64) -> Result<Profile, String> {
        let name = clean_name(name).ok_or(ERR_NAME_EMPTY)?;
        let source = self.get(id).ok_or(ERR_UNKNOWN)?;
        self.add(Profile { name, ..source }, now)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.change(|list| {
            find(list, id)?;
            list.retain(|p| p.id != id);
            Ok(())
        })
    }

    /// Store a checked profile as a new one (an import or a copy): a new
    /// id, its name made unique.
    pub fn add(&mut self, profile: Profile, now: f64) -> Result<Profile, String> {
        self.change(|list| {
            if list.len() >= MAX_PROFILES {
                return Err(ERR_FULL.into());
            }
            let profile = Profile {
                id: new_id(list, now),
                name: unique_name(list, &profile.name),
                created_ms: now,
                updated_ms: now,
                ..profile
            };
            list.push(profile.clone());
            Ok(profile)
        })
    }

    /// Profiles from a backup: one with the same id replaces this
    /// computer's, the others are added (names made unique). Returns the
    /// ones left out because the list is full.
    pub fn merge(&mut self, profiles: Vec<Profile>, now: f64) -> Result<u32, String> {
        self.change(|list| {
            let mut skipped = 0;
            for p in profiles {
                if let Some(pos) = list.iter().position(|q| q.id == p.id && p.id != DEFAULT_ID) {
                    let others: Vec<Profile> = list.iter().filter(|q| q.id != p.id).cloned().collect();
                    list[pos] = Profile { name: unique_name(&others, &p.name), ..p };
                } else if list.len() >= MAX_PROFILES {
                    skipped += 1;
                } else {
                    let id = if p.id.is_empty() || p.id == DEFAULT_ID { new_id(list, now) } else { p.id.clone() };
                    let name = unique_name(list, &p.name);
                    list.push(Profile { id, name, ..p });
                }
            }
            Ok(skipped)
        })
    }
}

fn find<'a>(list: &'a mut [Profile], id: &str) -> Result<&'a mut Profile, String> {
    if id == DEFAULT_ID {
        return Err(ERR_BUILT_IN.into());
    }
    list.iter_mut().find(|p| p.id == id).ok_or_else(|| ERR_UNKNOWN.to_string())
}

fn name_taken(list: &[Profile], name: &str, except: Option<&str>) -> bool {
    let name = name.to_lowercase();
    list.iter().any(|p| Some(p.id.as_str()) != except && p.name.to_lowercase() == name)
}

/// `name`, or `name (2)`, `name (3)` … when taken.
fn unique_name(list: &[Profile], name: &str) -> String {
    if !name_taken(list, name, None) {
        return name.to_string();
    }
    for n in 2..1000 {
        let suffix = format!(" ({n})");
        let base: String = name.chars().take(NAME_MAX.saturating_sub(suffix.chars().count())).collect();
        let candidate = format!("{}{suffix}", base.trim_end());
        if !name_taken(list, &candidate, None) {
            return candidate;
        }
    }
    name.to_string()
}

fn new_id(list: &[Profile], now: f64) -> String {
    let mut n = now.max(0.0) as u64;
    loop {
        let id = format!("p{n:x}");
        if !list.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

// ------------------------------------------------------------------ loading

/// The writes a load makes. The app's goes through the device actor
/// (`lib.rs`); the tests use a stand-in.
pub trait Writer {
    fn effect(&mut self, effect: EffectSettings) -> Result<(), String>;
    fn per_key(&mut self, colors: Vec<(u8, [u8; 3])>, brightness: u8) -> Result<(), String>;
    fn keymap(&mut self, layer: Layer, entries: Vec<(u8, KeyAction)>) -> Result<(), String>;
    fn macros(&mut self, slots: Vec<Option<StoredMacro>>) -> Result<(), String>;
    /// The lighting is on the keyboard: update the Lighting page's record.
    fn lighting_stored(&mut self, lighting: &ProfileLighting) -> Result<(), String>;
}

/// The error of a part that failed: `profile part failed (keymap): …`. The
/// UI names the part and words the cause (the backend's own error).
fn part_failed(part: ProfilePart, e: String) -> String {
    format!("profile part failed ({part}): {e}")
}

/// Put one part of `profile` on the keyboard, unless it is there already.
pub fn load_part(
    profile: &Profile,
    part: ProfilePart,
    current: &Current,
    w: &mut dyn Writer,
) -> Result<PartLoaded, String> {
    if !profile.fits(&current.model_id) {
        return Err(ERR_OTHER_MODEL.into());
    }
    if profile.part_matches(part, current) {
        return Ok(PartLoaded::Same);
    }
    match part {
        ProfilePart::Lighting => {
            let Some(l) = &profile.lighting else { return Ok(PartLoaded::Same) };
            let written = match (l.settings(), l.per_key()) {
                (Some(effect), _) => w.effect(effect),
                (None, Some((colors, brightness))) => w.per_key(colors, brightness),
                (None, None) => Err("invalid lighting".into()),
            };
            written.map_err(|e| part_failed(part, e))?;
            if let Err(e) = w.lighting_stored(l) {
                // The keyboard has it; only the Lighting page's record is behind.
                tracing::warn!("lighting record not updated: {e}");
            }
        }
        ProfilePart::Keymap => {
            let Some(k) = &profile.keymap else { return Ok(PartLoaded::Same) };
            for layer in [Layer::Base, Layer::Fn] {
                if k.layer(layer) != current.keymap.layer(layer) {
                    w.keymap(layer, k.actions(layer)).map_err(|e| part_failed(part, e))?;
                }
            }
        }
        ProfilePart::Macros => {
            let Some(m) = &profile.macros else { return Ok(PartLoaded::Same) };
            w.macros(m.clone()).map_err(|e| part_failed(part, e))?;
        }
        ProfilePart::Screen => return Err(ERR_SCREEN_PART.into()),
    }
    Ok(PartLoaded::Written)
}

// ------------------------------------------------------------------ files

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFile {
    pub kind: String,
    pub version: u32,
    #[serde(default)]
    pub created_ms: f64,
    #[serde(default)]
    pub app_version: String,
    pub profile: ExportedProfile,
}

/// A profile as it travels in a file: the screen picture with its data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedProfile {
    pub name: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<ProfileLighting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap: Option<ProfileKeymap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macros: Option<Vec<Option<StoredMacro>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenItemBackup>,
}

/// The file for `profile`. `screen` is its library item (name, kind,
/// created, stored data) when it has one; the factory animation stays out.
pub fn export(
    profile: &Profile,
    screen: Option<(&str, ScreenItemKind, f64, &[u8])>,
    app_version: &str,
    now: f64,
) -> Result<Vec<u8>, String> {
    let screen = screen
        .filter(|(_, kind, _, _)| *kind != ScreenItemKind::Factory)
        .map(|(name, kind, created, blob)| ScreenItemBackup::new(name, kind, created, blob));
    let file = ProfileFile {
        kind: FILE_KIND.into(),
        version: FILE_VERSION,
        created_ms: now,
        app_version: app_version.into(),
        profile: ExportedProfile {
            name: profile.name.clone(),
            model_id: profile.model_id.clone(),
            lighting: profile.lighting.clone(),
            keymap: profile.keymap.clone(),
            macros: profile.macros.clone(),
            screen,
        },
    };
    serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())
}

/// Read a profile file: kind and version first (so a newer file says so
/// even when its fields changed), then the whole of it.
pub fn parse(bytes: &[u8]) -> Result<ProfileFile, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ERR_TOO_LARGE.into());
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ERR_NOT_PROFILE.to_string())?;
    match value.get("kind").and_then(Value::as_str) {
        Some(FILE_KIND) => {}
        Some(crate::backup::KIND) => return Err(ERR_BACKUP_FILE.into()),
        _ => return Err(ERR_NOT_PROFILE.into()),
    }
    match value.get("version").and_then(Value::as_u64) {
        Some(v) if v == u64::from(FILE_VERSION) => {}
        Some(v) if v > u64::from(FILE_VERSION) => return Err(ERR_NEWER.into()),
        _ => return Err(ERR_DAMAGED.into()),
    }
    serde_json::from_value(value).map_err(|e| {
        tracing::info!("profile file not read: {e}");
        ERR_DAMAGED.to_string()
    })
}

/// A profile file checked for import: the profile (its screen part still
/// without a library id) and the picture to add to the library first.
pub struct Imported {
    pub profile: Profile,
    pub picture: Option<(String, ScreenItemKind, ScreenAnimation)>,
}

pub fn check_import(file: ProfileFile) -> Result<Imported, String> {
    let p = file.profile;
    let picture = match p.screen {
        Some(item) if item.kind != ScreenItemKind::Factory => {
            let blob = B64.decode(item.data.as_bytes()).map_err(|_| ERR_DAMAGED.to_string())?;
            let anim = screen::from_blob(&blob).map_err(|_| ERR_DAMAGED.to_string())?;
            Some((item.name, item.kind, anim))
        }
        _ => None,
    };
    // A placeholder id passes the check; the library gives the real one.
    let screen = picture.as_ref().map(|(name, _, _)| ProfileScreen { id: "0".repeat(16), name: name.clone() });
    let profile = Profile {
        id: String::new(),
        name: p.name,
        created_ms: 0.0,
        updated_ms: 0.0,
        model_id: p.model_id,
        lighting: p.lighting,
        keymap: p.keymap,
        macros: p.macros,
        screen,
    }
    .checked()?;
    Ok(Imported { profile, picture })
}

/// Profiles from a backup, checked: unknown models are left out (as the
/// backup's keymaps are), anything else damaged refuses the file.
pub fn check_backup(profiles: Vec<Profile>) -> Result<Vec<Profile>, String> {
    profiles
        .into_iter()
        .filter(|p| model(&p.model_id).is_some())
        .map(|p| p.checked().map_err(|_| crate::backup::ERR_DAMAGED.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_core::keymap::LayerMap;
    use kb_protocol::sonix::macros::{MacroStep, StepKind};
    use kb_protocol::sonix::screen::{rgb565, Frame};
    use std::fs;

    const MODEL: &str = "ajazz-ak980-pro";

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klavye-profiles-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn effect(mode: u8, color: &str, rainbow: bool, brightness: u8) -> ProfileLighting {
        ProfileLighting::Effect { mode, color: color.into(), rainbow, brightness, speed: 3, direction: 0 }
    }

    fn per_key(keys: &[(&str, &str)], brightness: u8) -> ProfileLighting {
        ProfileLighting::PerKey {
            colors: keys.iter().map(|(l, c)| (l.to_string(), c.to_string())).collect(),
            brightness,
        }
    }

    fn keymap_with_a_on_fn() -> ProfileKeymap {
        let map = LayerMap::from([(33, KeyAction::key(0x04))]);
        ProfileKeymap { base: BTreeMap::new(), fn_layer: StoredLayer::new(&map, 1).entries }
    }

    fn a_macro(name: &str) -> StoredMacro {
        StoredMacro {
            name: name.into(),
            steps: vec![settings::StoredStep(MacroStep::new(StepKind::KeyDown, 0x04, 20))],
        }
    }

    fn current() -> Current {
        Current {
            model_id: MODEL.into(),
            lighting: Some(per_key(&[("33", "#ff8a3d")], 5)),
            keymap: keymap_with_a_on_fn(),
            macros: vec![Some(a_macro("imza")), None, Some(a_macro("ok"))],
            screen: Some(ProfileScreen { id: "00112233aabbccdd".into(), name: "Kedi".into() }),
        }
    }

    fn all_parts() -> ProfileParts {
        ProfileParts { lighting: true, keymap: true, macros: true, screen: true }
    }

    #[test]
    fn create_update_rename_delete_and_reload() {
        let dir = temp_dir("crud");
        let mut store = ProfileStore::open(Some(&dir));
        assert!(store.profiles().is_empty());
        let now = 1_794_000_000_000.0;
        let cur = current();

        let oyun = store.create("  Oyun ", all_parts(), &cur, now).unwrap();
        assert_eq!(oyun.name, "Oyun");
        assert_eq!(oyun.model_id, MODEL);
        assert_eq!(oyun.parts(), ProfilePart::ALL.to_vec());
        assert!(oyun.on_keyboard(&cur), "made from the current settings");
        let parts = ProfileParts { lighting: true, ..ProfileParts::default() };
        let ofis = store.create("Ofis", parts, &cur, now).unwrap();
        assert_eq!(ofis.parts(), vec![ProfilePart::Lighting]);
        assert_ne!(oyun.id, ofis.id);
        assert_eq!(store.create("oyun", parts, &cur, now).err(), Some(ERR_NAME_TAKEN.into()), "names ignore case");
        assert_eq!(store.create(" ", parts, &cur, now).err(), Some(ERR_NAME_EMPTY.into()));
        assert_eq!(store.create("Boş", ProfileParts::default(), &cur, now).err(), Some(ERR_NOTHING.into()));
        let no_light = Current { lighting: None, ..cur.clone() };
        assert_eq!(store.create("Işıksız", parts, &no_light, now).err(), Some(ERR_NOTHING.into()), "lighting unknown");

        // Update takes the current settings for the profile's own parts.
        let changed = Current { lighting: Some(effect(1, "#ffffff", false, 2)), ..cur.clone() };
        assert!(!store.get(&ofis.id).unwrap().on_keyboard(&changed));
        store.update(&ofis.id, &changed, now + 1.0).unwrap();
        let ofis2 = store.get(&ofis.id).unwrap();
        assert_eq!(ofis2.lighting, Some(effect(1, "#ffffff", false, 2)));
        assert_eq!(ofis2.keymap, None, "parts not in it stay out");
        assert_eq!(ofis2.updated_ms, now + 1.0);
        assert!(ofis2.on_keyboard(&changed));

        store.rename(&ofis.id, "İş", now + 2.0).unwrap();
        store.rename(&ofis.id, "iş", now + 2.0).unwrap();
        assert_eq!(store.rename(&ofis.id, "OYUN", now).err(), Some(ERR_NAME_TAKEN.into()));
        let copy = store.duplicate(&oyun.id, "Oyun", now + 3.0).unwrap();
        assert_eq!(copy.name, "Oyun (2)");
        assert_eq!(copy.keymap, oyun.keymap);

        // Everything is on disk.
        let reopened = ProfileStore::open(Some(&dir));
        assert_eq!(reopened.profiles(), store.profiles());
        assert_eq!(reopened.profiles().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Oyun", "iş", "Oyun (2)"]);

        store.delete(&copy.id).unwrap();
        assert_eq!(store.delete(&copy.id).err(), Some(ERR_UNKNOWN.into()));
        assert_eq!(ProfileStore::open(Some(&dir)).profiles().len(), 2);
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains(r#""version": 1"#) && text.contains(r#""kind": "per-key""#), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_profile_is_protected() {
        let mut store = ProfileStore::open(None);
        let cur = current();
        for r in [store.rename(DEFAULT_ID, "Benim", 1.0), store.update(DEFAULT_ID, &cur, 1.0), store.delete(DEFAULT_ID)]
        {
            assert_eq!(r.err(), Some(ERR_BUILT_IN.into()));
        }
        let d = store.get(DEFAULT_ID).unwrap();
        assert_eq!(d.lighting, Some(ProfileLighting::factory()));
        assert_eq!(d.keymap, Some(ProfileKeymap::default()), "factory layout");
        assert_eq!(d.macros, Some(Vec::new()), "no macros");
        assert!(d.fits("any-model"));
        // A copy of it is an ordinary profile.
        let copy = store.duplicate(DEFAULT_ID, "Fabrika", 1.0).unwrap();
        assert_ne!(copy.id, DEFAULT_ID);
        store.rename(&copy.id, "Fabrika 2", 2.0).unwrap();
        let state = store.state(&cur, &|_| None);
        assert!(state.factory.built_in && state.factory.name.is_empty());
        assert_eq!(state.profiles.len(), 1);
    }

    #[test]
    fn the_limit_holds() {
        let mut store = ProfileStore::open(None);
        let cur = current();
        let parts = ProfileParts { macros: true, ..ProfileParts::default() };
        for i in 0..MAX_PROFILES {
            store.create(&format!("P{i}"), parts, &cur, i as f64).unwrap();
        }
        assert!(store.state(&cur, &|_| None).full);
        assert_eq!(store.create("Bir tane daha", parts, &cur, 0.0).err(), Some(ERR_FULL.into()));
        let first = store.profiles()[0].id.clone();
        assert_eq!(store.duplicate(&first, "Kopya", 0.0).err(), Some(ERR_FULL.into()));
    }

    #[test]
    fn a_damaged_file_is_kept_aside_and_reported() {
        let dir = temp_dir("damaged");
        fs::create_dir_all(&dir).unwrap();
        let damaged = r#"{"version":1,"profiles":[{"id":"p1","name":"Oyun","#;
        fs::write(dir.join(FILE), damaged).unwrap();
        let mut store = ProfileStore::open(Some(&dir));
        assert!(store.profiles().is_empty(), "defaults");
        let reset = settings::unshown_resets().into_iter().find(|r| r.dir == dir && r.file == FILE).expect("reported");
        let backup = reset.backup.expect("kept aside");
        assert!(backup.starts_with("profiles.json.bak-"));
        assert_eq!(fs::read_to_string(dir.join(&backup)).unwrap(), damaged);
        // The next save writes a new file; the copy stays.
        store.create("Yeni", ProfileParts { keymap: true, ..ProfileParts::default() }, &current(), 1.0).unwrap();
        assert_eq!(ProfileStore::open(Some(&dir)).profiles().len(), 1);
        assert_eq!(fs::read_to_string(dir.join(&backup)).unwrap(), damaged);

        fs::write(dir.join(FILE), r#"{"version":9,"profiles":[]}"#).unwrap();
        assert!(ProfileStore::open(Some(&dir)).profiles().is_empty(), "newer version");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn matching_compares_what_the_keyboard_shows() {
        let cur = current();
        let profile = |lighting| Profile { lighting: Some(lighting), ..default_profile() };
        let on = |p: &Profile, c: &Current| p.part_matches(ProfilePart::Lighting, c);
        // Per-key: same keys, colours (any case) and brightness.
        let pk = profile(per_key(&[("033", "#FF8A3D")], 5).checked().unwrap());
        assert!(on(&pk, &cur));
        assert!(!on(&profile(per_key(&[("33", "#ff8a3d")], 4)), &cur));
        assert!(!on(&profile(per_key(&[("33", "#ff8a3d"), ("34", "#ff8a3d")], 5)), &cur));
        // Effects: the colour does not count while rainbow is on.
        let rolling = Current { lighting: Some(effect(11, "#ff8a3d", true, 5)), ..cur.clone() };
        assert!(on(&default_profile(), &rolling), "the editor's default effect is the factory one");
        assert!(!on(&profile(effect(11, "#ff8a3d", false, 5)), &rolling));
        let static_red = Current { lighting: Some(effect(1, "#ff0000", false, 3)), ..cur.clone() };
        assert!(on(&profile(effect(1, "#FF0000", false, 3)), &static_red));
        assert!(!on(&profile(effect(1, "#00ff00", false, 3)), &static_red));
        // Static has no speed: a different one is the same light.
        let mut fast = effect(1, "#ff0000", false, 3);
        if let ProfileLighting::Effect { speed, .. } = &mut fast {
            *speed = 5;
        }
        assert!(on(&profile(fast), &static_red));
        assert!(!on(&profile(effect(1, "#ff0000", false, 3)), &Current { lighting: None, ..cur.clone() }));

        // Keymap, macros and screen.
        let full = Profile {
            id: "p1".into(),
            name: "Oyun".into(),
            model_id: MODEL.into(),
            lighting: cur.lighting.clone(),
            keymap: Some(cur.keymap.clone()),
            macros: Some(cur.macros.clone()),
            screen: cur.screen.clone(),
            created_ms: 0.0,
            updated_ms: 0.0,
        };
        assert!(full.on_keyboard(&cur));
        assert!(!full.on_keyboard(&Current { keymap: ProfileKeymap::default(), ..cur.clone() }));
        assert!(!full.on_keyboard(&Current { macros: Vec::new(), ..cur.clone() }));
        assert!(!full.on_keyboard(&Current { screen: None, ..cur.clone() }));
        assert!(!full.on_keyboard(&Current { model_id: "other".into(), ..cur.clone() }), "another model");

        // A fresh computer: factory keymap, no macros, lighting unknown.
        let fresh = Current { model_id: MODEL.into(), ..Current::default() };
        let d = default_profile();
        assert!(!d.on_keyboard(&fresh), "the lighting is not known");
        let st = ProfileStore::open(None).state(&fresh, &|_| None);
        assert_eq!(st.factory.to_write, vec![ProfilePart::Lighting], "only the lighting would be written");
        assert!(d.on_keyboard(&Current { lighting: Some(ProfileLighting::factory()), ..fresh }));
    }

    #[test]
    fn current_lighting_comes_from_the_lighting_record() {
        use serde_json::json;
        let rec = |v: Value| ProfileLighting::from_record(v.as_object().unwrap());
        let rolling =
            json!({ "mode": 11, "color": "#FF8A3D", "rainbow": true, "brightness": 5, "speed": 3, "direction": 0 });
        assert_eq!(
            rec(json!({ "savedPerKey": { "colors": { "33": "#FF8A3D" }, "brightness": 4 }, "appliedEffect": null })),
            Some(per_key(&[("33", "#ff8a3d")], 4))
        );
        assert_eq!(rec(json!({ "savedPerKey": null, "appliedEffect": rolling })), Some(effect(11, "#ff8a3d", true, 5)));
        assert_eq!(
            rec(json!({ "savedPerKey": null, "appliedEffect": null, "effect": rolling, "draft": "effect" })),
            None,
            "an edit is not what the keyboard has"
        );
        assert_eq!(rec(json!({ "appliedEffect": { "mode": 1 } })), None);
        assert_eq!(rec(json!({})), None);
    }

    #[test]
    fn info_lists_the_parts_to_write() {
        let cur = current();
        let mut store = ProfileStore::open(None);
        let p = store.create("Oyun", all_parts(), &cur, 1.0).unwrap();
        let moved = Current { macros: Vec::new(), screen: None, ..cur.clone() };
        let lib = |id: &str| (id == "00112233aabbccdd").then(|| LibraryItem { name: "Kedi 2".into(), pages: 16 });
        let st = store.state(&moved, &lib);
        let info = &st.profiles[0];
        assert_eq!(info.id, p.id);
        assert!(!info.on_keyboard);
        assert_eq!(info.to_write, vec![ProfilePart::Macros, ProfilePart::Screen]);
        assert_eq!(info.keymap, Some(1));
        assert_eq!(info.macros, Some(2));
        assert_eq!(info.lighting, Some(ProfileLightingInfo::PerKey { keys: 1, brightness: 5 }));
        let screen = info.screen.as_ref().unwrap();
        assert_eq!((screen.name.as_str(), screen.in_library, screen.pages), ("Kedi 2", true, 16));
        assert_eq!(st.current, ProfileCurrent { lighting: true, screen: None });
        // A picture gone from the library cannot be loaded.
        let st = store.state(&moved, &|_| None);
        assert_eq!(st.profiles[0].to_write, vec![ProfilePart::Macros]);
        assert!(!st.profiles[0].screen.as_ref().unwrap().in_library);
        assert_eq!(st.profiles[0].screen.as_ref().unwrap().name, "Kedi");
    }

    #[derive(Default)]
    struct FakeKeyboard {
        writes: Vec<String>,
        fail: Option<&'static str>,
        lighting: Option<ProfileLighting>,
    }

    impl FakeKeyboard {
        fn write(&mut self, what: String) -> Result<(), String> {
            if self.fail.is_some_and(|f| what.starts_with(f)) {
                return Err("untested keyboard firmware 1.07".into());
            }
            self.writes.push(what);
            Ok(())
        }
    }

    impl Writer for FakeKeyboard {
        fn effect(&mut self, e: EffectSettings) -> Result<(), String> {
            self.write(format!("effect {}", e.mode))
        }
        fn per_key(&mut self, colors: Vec<(u8, [u8; 3])>, brightness: u8) -> Result<(), String> {
            self.write(format!("per-key {} {brightness}", colors.len()))
        }
        fn keymap(&mut self, layer: Layer, entries: Vec<(u8, KeyAction)>) -> Result<(), String> {
            self.write(format!("keymap {layer:?} {}", entries.len()))
        }
        fn macros(&mut self, slots: Vec<Option<StoredMacro>>) -> Result<(), String> {
            self.write(format!("macros {}", slots.len()))
        }
        fn lighting_stored(&mut self, l: &ProfileLighting) -> Result<(), String> {
            self.lighting = Some(l.clone());
            Ok(())
        }
    }

    #[test]
    fn loading_skips_what_the_keyboard_has() {
        let cur = current();
        let mut kb = FakeKeyboard::default();
        let d = default_profile();
        let run = |kb: &mut FakeKeyboard, c: &Current| {
            d.parts().into_iter().map(|part| load_part(&d, part, c, kb)).collect::<Result<Vec<_>, _>>()
        };
        // Lighting, the Fn layer (the base layer is already factory) and macros.
        assert_eq!(run(&mut kb, &cur).unwrap(), vec![PartLoaded::Written; 3]);
        assert_eq!(kb.writes, ["effect 11", "keymap Fn 0", "macros 0"]);
        assert_eq!(kb.lighting, Some(ProfileLighting::factory()), "the Lighting page's record follows");
        // Once the records say so, nothing is written.
        let after = Current {
            lighting: Some(ProfileLighting::factory()),
            keymap: ProfileKeymap::default(),
            macros: Vec::new(),
            ..cur.clone()
        };
        let mut again = FakeKeyboard::default();
        assert_eq!(run(&mut again, &after).unwrap(), vec![PartLoaded::Same; 3]);
        assert!(again.writes.is_empty());
        assert_eq!(load_part(&d, ProfilePart::Screen, &after, &mut again), Ok(PartLoaded::Same), "not in it");
    }

    #[test]
    fn a_failing_part_is_named() {
        let cur = Current { model_id: MODEL.into(), ..Current::default() };
        let profile = Profile {
            keymap: Some(keymap_with_a_on_fn()),
            lighting: Some(per_key(&[("33", "#00ff00")], 4)),
            screen: Some(ProfileScreen { id: "00112233aabbccdd".into(), name: "Kedi".into() }),
            ..default_profile()
        };
        let mut kb = FakeKeyboard { fail: Some("keymap"), ..FakeKeyboard::default() };
        assert_eq!(load_part(&profile, ProfilePart::Lighting, &cur, &mut kb), Ok(PartLoaded::Written));
        assert_eq!(kb.writes, ["per-key 1 4"]);
        let err = load_part(&profile, ProfilePart::Keymap, &cur, &mut kb).unwrap_err();
        assert_eq!(err, "profile part failed (keymap): untested keyboard firmware 1.07");
        assert_eq!(load_part(&profile, ProfilePart::Screen, &cur, &mut kb).err(), Some(ERR_SCREEN_PART.into()));
        let mine = Profile { model_id: "other-model".into(), ..profile };
        assert_eq!(load_part(&mine, ProfilePart::Lighting, &cur, &mut kb).err(), Some(ERR_OTHER_MODEL.into()));
    }

    #[test]
    fn a_profile_file_reads_back_and_imports() {
        let cur = current();
        let mut store = ProfileStore::open(None);
        let p = store.create("Oyun", all_parts(), &cur, 1.0).unwrap();
        let anim = ScreenAnimation::new(vec![(Frame::filled(rgb565(255, 0, 0)), 50), (Frame::filled(0), 50)]).unwrap();
        let blob = screen::blob_unpadded(&anim);
        let bytes = export(&p, Some(("Kedi", ScreenItemKind::Animation, 5.0, &blob)), "0.1.0", 9.0).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(r#""kind": "keymotion-profile""#) && text.contains(r#""version": 1"#), "{text}");
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(&bytes);
        let imported = check_import(parse(&bom).unwrap()).unwrap();
        let (name, kind, back) = imported.picture.expect("the picture travels with it");
        assert_eq!((name.as_str(), kind), ("Kedi", ScreenItemKind::Animation));
        assert_eq!(screen::blob_unpadded(&back), blob);
        assert_eq!(imported.profile.keymap, p.keymap);
        assert_eq!(imported.profile.macros, p.macros);
        assert_eq!(imported.profile.lighting, p.lighting);
        // Added with a free name.
        let added = store.add(imported.profile, 2.0).unwrap();
        assert_eq!(added.name, "Oyun (2)");

        // The factory animation never leaves.
        let bytes = export(&p, Some(("AJAZZ", ScreenItemKind::Factory, 0.0, &blob)), "0.1.0", 9.0).unwrap();
        let imported = check_import(parse(&bytes).unwrap()).unwrap();
        assert!(imported.picture.is_none() && imported.profile.screen.is_none());
    }

    #[test]
    fn wrong_files_are_refused_each_with_its_reason() {
        assert_eq!(parse(b"not json").err(), Some(ERR_NOT_PROFILE.into()));
        assert_eq!(parse(br#"{"version":1}"#).err(), Some(ERR_NOT_PROFILE.into()));
        assert_eq!(parse(br#"{"kind":"kb-backup","version":1}"#).err(), Some(ERR_BACKUP_FILE.into()));
        assert_eq!(parse(br#"{"kind":"keymotion-profile","version":2,"profile":7}"#).err(), Some(ERR_NEWER.into()));
        assert_eq!(parse(br#"{"kind":"keymotion-profile","profile":{}}"#).err(), Some(ERR_DAMAGED.into()));
        assert_eq!(parse(br#"{"kind":"keymotion-profile","version":1,"profile":"x"}"#).err(), Some(ERR_DAMAGED.into()));
        assert_eq!(parse(&vec![b' '; MAX_FILE_BYTES + 1]).err(), Some(ERR_TOO_LARGE.into()));

        let file = |profile: &str| {
            let text = format!(r#"{{"kind":"keymotion-profile","version":1,"profile":{profile}}}"#);
            parse(text.as_bytes()).and_then(check_import).map(|i| i.profile)
        };
        let ok = file(
            r##"{"name":"Ofis","modelId":"ajazz-ak980-pro","lighting":{"kind":"effect","mode":1,"color":"#FFFFFF","rainbow":false,"brightness":2,"speed":3,"direction":0}}"##,
        );
        assert_eq!(ok.unwrap().lighting, Some(effect(1, "#ffffff", false, 2)));
        let err = |p: &str| file(p).err();
        assert_eq!(err(r#"{"name":"X","modelId":"another-keyboard","macros":[]}"#), Some(ERR_OTHER_MODEL.into()));
        assert_eq!(err(r#"{"name":"X","modelId":"ajazz-ak980-pro"}"#), Some(ERR_DAMAGED.into()), "nothing in it");
        assert_eq!(err(r#"{"name":"  ","modelId":"ajazz-ak980-pro","macros":[]}"#), Some(ERR_DAMAGED.into()));
        assert_eq!(
            err(
                r##"{"name":"X","modelId":"ajazz-ak980-pro","lighting":{"kind":"effect","mode":99,"color":"#ffffff","rainbow":false,"brightness":2,"speed":3,"direction":0}}"##
            ),
            Some(ERR_DAMAGED.into()),
            "unknown effect"
        );
        assert_eq!(
            err(
                r##"{"name":"X","modelId":"ajazz-ak980-pro","lighting":{"kind":"per-key","colors":{"200":"#ffffff"},"brightness":2}}"##
            ),
            Some(ERR_DAMAGED.into()),
            "no such LED"
        );
        assert_eq!(
            err(r#"{"name":"X","modelId":"ajazz-ak980-pro","keymap":{"base":{"33":"zz"}}}"#),
            Some(ERR_DAMAGED.into())
        );
        assert_eq!(
            err(r#"{"name":"X","modelId":"ajazz-ak980-pro","screen":{"name":"K","kind":"image","data":"AAAA"}}"#),
            Some(ERR_DAMAGED.into()),
            "a broken picture"
        );
        // A on the A key (LED 56) is the default on the base layer; A on
        // Num / (LED 33) is a change.
        let km = file(
            r#"{"name":"X","modelId":"ajazz-ak980-pro","keymap":{"base":{"56":"02000400","33":"02000400"},"fn":{"033":"02000400"}}}"#,
        )
        .unwrap()
        .keymap
        .unwrap();
        assert_eq!(km.base.keys().collect::<Vec<_>>(), ["33"], "canonical, as the record keeps it");
        assert_eq!(km.fn_layer.keys().collect::<Vec<_>>(), ["33"], "plain LED numbers");
        assert_eq!(
            err(r#"{"name":"X","modelId":"ajazz-ak980-pro","keymap":{"base":{"250":"02000400"}}}"#),
            Some(ERR_DAMAGED.into())
        );
    }

    #[test]
    fn backup_profiles_merge_by_id() {
        let cur = current();
        let mut store = ProfileStore::open(None);
        let a = store.create("Oyun", all_parts(), &cur, 1.0).unwrap();
        let b = store.create("Ofis", ProfileParts { lighting: true, ..ProfileParts::default() }, &cur, 2.0).unwrap();
        let mut from_backup = store.profiles().to_vec();
        from_backup[1].lighting = Some(effect(7, "#ffffff", false, 1));
        let other = Profile { id: "p999".into(), name: "Oyun".into(), ..a.clone() };
        from_backup.push(other);
        let mut local = ProfileStore::open(None);
        local.create("Gece", ProfileParts { keymap: true, ..ProfileParts::default() }, &cur, 3.0).unwrap();
        local.merge(store.profiles().to_vec(), 4.0).unwrap();
        assert_eq!(local.profiles().len(), 3);
        assert_eq!(local.merge(check_backup(from_backup).unwrap(), 5.0).unwrap(), 0);
        let names: Vec<&str> = local.profiles().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Gece", "Oyun", "Ofis", "Oyun (2)"]);
        assert_eq!(local.get(&b.id).unwrap().lighting, Some(effect(7, "#ffffff", false, 1)), "same id replaced");
        // Unknown models are left out; a damaged one refuses the backup.
        let alien = Profile { model_id: "another-keyboard".into(), ..a.clone() };
        assert!(check_backup(vec![alien]).unwrap().is_empty());
        let broken = Profile { lighting: Some(per_key(&[("33", "red")], 5)), ..a };
        assert_eq!(check_backup(vec![broken]).err(), Some(crate::backup::ERR_DAMAGED.into()));
    }
}
