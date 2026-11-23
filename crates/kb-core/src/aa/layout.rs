//! A model's keys in the "AA" family's index space.
//!
//! The app names keys by their LED number (`KeySpec::led`, the "04 xx"
//! index); the "AA" keymap slots and per-key entries use another numbering
//! (`KeySpec::aa`), which is not the LED number shifted. The driver
//! translates every request through an [`AaLayout`].

use kb_devices::Model;
use kb_protocol::sonix::keymap::FN_USAGE;

/// Keymap slots of the knob (volume up, volume down, mute in the web hub's
/// key table). Kept as read: AJAZZ's WebHID driver offers no knob
/// remapping for this keyboard.
pub const KNOB_SLOTS: [u8; 3] = [13, 14, 15];

/// Highest "AA" index (128 slots).
const SLOTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("{model} has no key layout")]
    NoGeometry { model: String },
    #[error("{model}: key {key} has no \"AA\" index")]
    MissingIndex { model: String, key: String },
    #[error("{model}: \"AA\" index {aa} is used twice or out of range")]
    BadIndex { model: String, aa: u8 },
    #[error("{model}: LED {led} is used twice")]
    DuplicateLed { model: String, led: u8 },
}

/// One key: its LED number and its "AA" index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutKey {
    pub led: u8,
    pub aa: u8,
}

/// LED ↔ "AA" index for one model, in the model file's key order.
#[derive(Debug, Clone)]
pub struct AaLayout {
    model: String,
    keys: Vec<LayoutKey>,
    led_to_aa: [Option<u8>; 256],
    aa_to_led: [Option<u8>; SLOTS],
    fn_slot: Option<u8>,
    indicators: Vec<LayoutKey>,
}

impl AaLayout {
    /// Build the layout; fails unless every key has a unique `aa` below 128.
    pub fn from_model(model: &Model) -> Result<Self, LayoutError> {
        let g = model.geometry.as_ref().ok_or_else(|| LayoutError::NoGeometry { model: model.id.clone() })?;
        let mut led_to_aa = [None; 256];
        let mut aa_to_led = [None; SLOTS];
        let mut keys = Vec::with_capacity(g.keys.len());
        let mut fn_slot = None;
        for k in &g.keys {
            let aa = k.aa.ok_or_else(|| LayoutError::MissingIndex { model: model.id.clone(), key: k.id.clone() })?;
            if usize::from(aa) >= SLOTS || aa_to_led[usize::from(aa)].is_some() {
                return Err(LayoutError::BadIndex { model: model.id.clone(), aa });
            }
            if led_to_aa[usize::from(k.led)].is_some() {
                return Err(LayoutError::DuplicateLed { model: model.id.clone(), led: k.led });
            }
            led_to_aa[usize::from(k.led)] = Some(aa);
            aa_to_led[usize::from(aa)] = Some(k.led);
            if k.hid == FN_USAGE {
                fn_slot = Some(aa);
            }
            keys.push(LayoutKey { led: k.led, aa });
        }
        let indicators = model
            .indicators
            .iter()
            .filter_map(|i| Some(LayoutKey { led: i.led, aa: i.aa.or(led_to_aa[usize::from(i.led)])? }))
            .collect();
        Ok(Self { model: model.id.clone(), keys, led_to_aa, aa_to_led, fn_slot, indicators })
    }

    /// The model file's id.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The keys in the model file's order.
    pub fn keys(&self) -> &[LayoutKey] {
        &self.keys
    }

    /// The "AA" index of the key with LED `led`.
    pub fn aa(&self, led: u8) -> Option<u8> {
        self.led_to_aa[usize::from(led)]
    }

    /// The LED of the key with "AA" index `aa`.
    pub fn led(&self, aa: u8) -> Option<u8> {
        self.aa_to_led.get(usize::from(aa)).copied().flatten()
    }

    /// The Fn key's slot (never reassigned).
    pub fn fn_slot(&self) -> Option<u8> {
        self.fn_slot
    }

    /// Firmware status lights (Num Lock, Fn) with both indices.
    pub fn indicators(&self) -> &[LayoutKey] {
        &self.indicators
    }

    /// Slots a write never changes: the Fn key and the knob.
    pub fn is_protected(&self, aa: u8) -> bool {
        Some(aa) == self.fn_slot || KNOB_SLOTS.contains(&aa)
    }

    /// The "AA" indices in layout order (the real-time frame's order).
    pub fn order(&self) -> Vec<u8> {
        self.keys.iter().map(|k| k.aa).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ak980() -> &'static Model {
        kb_devices::catalogue().unwrap().iter().find(|m| m.id == "ajazz-ak980-pro").unwrap()
    }

    #[test]
    fn the_ak980_pro_translates() {
        let l = AaLayout::from_model(ak980()).unwrap();
        assert_eq!(l.keys().len(), 97);
        // Esc 1→0, W 39→34, Enter 85→76, Fn 96→85 (protocol notes Ek A).
        assert_eq!((l.aa(1), l.aa(39), l.aa(85), l.aa(96)), (Some(0), Some(34), Some(76), Some(85)));
        assert_eq!((l.led(0), l.led(76), l.led(85)), (Some(1), Some(85), Some(96)));
        assert_eq!(l.led(13), None, "a knob slot is no key");
        assert_eq!(l.aa(0), None);
        assert_eq!(l.fn_slot(), Some(85));
        assert!(l.is_protected(85) && l.is_protected(13) && l.is_protected(15));
        assert!(!l.is_protected(76));
        assert_eq!(l.indicators(), [LayoutKey { led: 32, aa: 29 }, LayoutKey { led: 96, aa: 85 }]);
        assert_eq!(l.order()[0], 0, "Esc first, as in the model file");
    }

    #[test]
    fn every_key_needs_a_unique_index() {
        let mut m = ak980().clone();
        m.geometry.as_mut().unwrap().keys[3].aa = None;
        assert!(matches!(AaLayout::from_model(&m), Err(LayoutError::MissingIndex { .. })));
        let mut m = ak980().clone();
        let g = m.geometry.as_mut().unwrap();
        g.keys[3].aa = g.keys[4].aa;
        assert!(matches!(AaLayout::from_model(&m), Err(LayoutError::BadIndex { .. })));
        let mut m = ak980().clone();
        m.geometry.as_mut().unwrap().keys[3].aa = Some(128);
        assert!(matches!(AaLayout::from_model(&m), Err(LayoutError::BadIndex { aa: 128, .. })));
        let mut m = ak980().clone();
        m.geometry = None;
        assert!(matches!(AaLayout::from_model(&m), Err(LayoutError::NoGeometry { .. })));
    }
}
