//! Keymap rules that depend on the keyboard model.
//!
//! The protocol crate encodes entries; this module decides which keys of a
//! model may be assigned and keeps layers in a canonical form, so two
//! layers that send the same table compare equal.

use kb_devices::{KeySpec, Model};
use kb_protocol::sonix::keymap::{KeyAction, Layer, FN_USAGE};
use std::collections::BTreeMap;

/// One layer: LED/keymap index → action. Keys not listed use the firmware
/// default, which is also how the keyboard stores them.
pub type LayerMap = BTreeMap<u8, KeyAction>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("{model} has no key at index {led}")]
    UnknownKey { model: String, led: u8 },
    #[error("the Fn key cannot be reassigned")]
    FnLocked,
    #[error("{model} has no keyboard layout")]
    NoGeometry { model: String },
}

fn key(model: &Model, led: u8) -> Result<&KeySpec, KeymapError> {
    let g = model.geometry.as_ref().ok_or_else(|| KeymapError::NoGeometry { model: model.id.clone() })?;
    g.keys.iter().find(|k| k.led == led).ok_or_else(|| KeymapError::UnknownKey { model: model.id.clone(), led })
}

/// Check a layer against `model` and bring it to canonical form:
/// `Default` entries are dropped, and on the base layer a key assigned its
/// own usage becomes `Default` (the keyboard sends the same thing either
/// way). Fails on keys the model does not have and on any change to Fn.
pub fn canonical(
    model: &Model,
    layer: Layer,
    entries: impl IntoIterator<Item = (u8, KeyAction)>,
) -> Result<LayerMap, KeymapError> {
    let mut out = LayerMap::new();
    for (led, action) in entries {
        let k = key(model, led)?;
        if k.hid == FN_USAGE {
            if action == KeyAction::Default {
                continue;
            }
            return Err(KeymapError::FnLocked);
        }
        let own = layer == Layer::Base && action == KeyAction::key(k.hid);
        if action != KeyAction::Default && !own {
            out.insert(led, action);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_protocol::sonix::keymap::{usage, MouseButton};

    fn ak980() -> &'static Model {
        &kb_devices::catalogue().unwrap()[0]
    }

    fn led(id: &str) -> u8 {
        ak980().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap().led
    }

    #[test]
    fn every_model_key_has_a_known_usage() {
        for m in kb_devices::catalogue().unwrap() {
            let Some(g) = &m.geometry else { continue };
            for k in &g.keys {
                let known = k.hid == FN_USAGE || usage(k.hid).is_some();
                assert!(known, "{}: {} has unknown usage 0x{:02X}", m.id, k.id, k.hid);
                if let Some(u) = usage(k.hid) {
                    assert_eq!(u.id, k.id, "{}: key id and usage id should agree", m.id);
                }
            }
            assert_eq!(g.keys.iter().filter(|k| k.hid == FN_USAGE).count(), 1, "{}: one Fn key", m.id);
        }
    }

    #[test]
    fn canonical_drops_defaults_and_own_usages() {
        let a = led("a");
        let caps = led("caps");
        let map = canonical(
            ak980(),
            Layer::Base,
            [(a, KeyAction::key(0x04)), (caps, KeyAction::key(0x29)), (led("q"), KeyAction::Default)],
        )
        .unwrap();
        assert_eq!(map, LayerMap::from([(caps, KeyAction::key(0x29))]));
        // On the Fn layer "A sends A" is a real change: the default is a firmware function.
        let fn_map = canonical(ak980(), Layer::Fn, [(a, KeyAction::key(0x04))]).unwrap();
        assert_eq!(fn_map.len(), 1);
    }

    #[test]
    fn fn_and_unknown_keys_are_refused() {
        let f = led("fn");
        assert_eq!(canonical(ak980(), Layer::Base, [(f, KeyAction::Disabled)]), Err(KeymapError::FnLocked));
        assert_eq!(
            canonical(ak980(), Layer::Fn, [(f, KeyAction::Mouse(MouseButton::Left))]),
            Err(KeymapError::FnLocked)
        );
        assert!(canonical(ak980(), Layer::Base, [(f, KeyAction::Default)]).unwrap().is_empty());
        assert!(matches!(
            canonical(ak980(), Layer::Base, [(95, KeyAction::Disabled)]),
            Err(KeymapError::UnknownKey { led: 95, .. })
        ));
    }
}
