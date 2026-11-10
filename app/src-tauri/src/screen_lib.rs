//! The screen library: every animation the user prepared, stored exactly as
//! the keyboard gets it (validated blob), plus what each model's GIF page
//! shows and how many uploads were made.
//!
//! The keyboard cannot give its animation back, so this is the only copy.
//! Files live in `<config>/screen/`: `library.json` and `library/<id>.bin`.
//! The id is a hash of the data, so the same content is stored once.

use crate::dto::{LiveItem, LiveRestore, ScreenItem, ScreenItemKind, ScreenLimits, ScreenState};
use crate::live::LiveStored;
use kb_protocol::sonix::screen::{self, from_blob, ScreenAnimation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "library.json";
const INDEX_VERSION: u32 = 1;
/// Factory animation backup taken from the official driver package
/// (docs/donanim-dogrulama.md, "Ekran yüklemesi").
const FACTORY_FILE: &str = "factory/ak980-pro-factory.gif";
/// The last card the live mode uploaded (not a library item).
const LIVE_FILE: &str = "live.bin";
/// What `shown` holds while the GIF page shows a live card.
pub const LIVE_ID: &str = "live";
/// Upload counts are kept per day for this long.
const KEEP_DAYS: usize = 60;
/// Longest stored name.
pub const NAME_MAX: usize = 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Index {
    items: Vec<ScreenItem>,
    /// Model id → id of the item its GIF page shows (as far as the app knows).
    shown: BTreeMap<String, String>,
    total_uploads: u32,
    /// Local date (YYYY-MM-DD) → uploads that day.
    uploads_by_day: BTreeMap<String, u32>,
    /// Live mode settings, budget and its last card.
    #[serde(default)]
    live: LiveStored,
}

pub struct ScreenLibrary {
    dir: Option<PathBuf>,
    index: Index,
}

/// FNV-1a 64: a stable content id (std's hasher may change between Rust
/// versions; ids end up in file names).
pub(crate) fn content_id(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

pub(crate) fn clean_name(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
    let n = n.trim();
    if n.is_empty() {
        "Adsız".to_string()
    } else {
        n.to_string()
    }
}

impl ScreenLibrary {
    /// Open the library in `config_dir/screen` (in memory only without a
    /// directory). Items whose data file is missing are dropped. The factory
    /// backup is added once if it exists.
    pub fn open(config_dir: Option<&Path>) -> Self {
        let dir = config_dir.map(|d| d.join("screen"));
        let mut index: Index =
            dir.as_deref().and_then(|d| crate::settings::load(d, INDEX_FILE, INDEX_VERSION)).unwrap_or_default();
        if let Some(d) = &dir {
            index.items.retain(|i| d.join("library").join(format!("{}.bin", i.id)).is_file());
        }
        let mut lib = Self { dir, index };
        lib.import_factory();
        lib
    }

    fn import_factory(&mut self) {
        if self.index.items.iter().any(|i| i.kind == ScreenItemKind::Factory) {
            return;
        }
        let Some(path) = self.dir.as_ref().map(|d| d.join(FACTORY_FILE)) else { return };
        let Ok(bytes) = fs::read(&path) else { return };
        let anim = kb_core::screen_import::decode_gif(&bytes)
            .and_then(|d| kb_core::screen_import::exact_size_animation(&d, None));
        match anim {
            Ok(anim) => match self.add("AJAZZ fabrika animasyonu", ScreenItemKind::Factory, &anim) {
                Ok(item) => {
                    tracing::info!(id = %item.id, frames = item.frames, "factory animation added to the library")
                }
                Err(e) => tracing::warn!("factory animation not stored: {e}"),
            },
            Err(e) => tracing::warn!("factory backup unreadable: {e}"),
        }
    }

    fn data_path(&self, id: &str) -> Option<PathBuf> {
        // Ids are our own hex hashes; refuse anything else (path safety).
        if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.dir.as_ref().map(|d| d.join("library").join(format!("{id}.bin")))
    }

    fn save_index(&self) -> io::Result<()> {
        match &self.dir {
            Some(d) => crate::settings::save(d, INDEX_FILE, INDEX_VERSION, &self.index),
            None => Ok(()),
        }
    }

    /// Store a validated animation. The same content keeps its first entry
    /// (the name is updated).
    pub fn add(&mut self, name: &str, kind: ScreenItemKind, anim: &ScreenAnimation) -> io::Result<ScreenItem> {
        let data = screen::blob_unpadded(anim);
        let id = content_id(&data);
        let name = clean_name(name);
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id) {
            if item.kind != ScreenItemKind::Factory {
                item.name = name;
            }
            let item = item.clone();
            self.save_index()?;
            return Ok(item);
        }
        if let Some(path) = self.data_path(&id) {
            fs::create_dir_all(path.parent().expect("library dir"))?;
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, &data)?;
            fs::rename(&tmp, &path)?;
        }
        let item = ScreenItem {
            id,
            name,
            kind,
            frames: anim.frames().len() as u32,
            loop_ms: anim.loop_ms(),
            pages: anim.pages() as u32,
            created_ms: now_ms(),
            uploaded_ms: None,
        };
        self.index.items.push(item.clone());
        self.save_index()?;
        Ok(item)
    }

    /// The stored data, validated again on the way out.
    pub fn animation(&self, id: &str) -> Result<ScreenAnimation, String> {
        let data = self.data(id)?;
        from_blob(&data).map_err(|e| e.to_string())
    }

    /// Raw stored data (header + frames), optionally only the first frames.
    /// `live` gives the last live card.
    pub fn data(&self, id: &str) -> Result<Vec<u8>, String> {
        if id == LIVE_ID {
            let path = self.dir.as_ref().map(|d| d.join(LIVE_FILE)).ok_or("library has no storage")?;
            return fs::read(path).map_err(|e| e.to_string());
        }
        if !self.index.items.iter().any(|i| i.id == id) {
            return Err("unknown library item".into());
        }
        let path = self.data_path(id).ok_or("library has no storage")?;
        fs::read(path).map_err(|e| e.to_string())
    }

    pub fn rename(&mut self, id: &str, name: &str) -> io::Result<()> {
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id && i.kind != ScreenItemKind::Factory) {
            item.name = clean_name(name);
            self.save_index()?;
        }
        Ok(())
    }

    /// Remove an item (the factory backup cannot be removed).
    pub fn remove(&mut self, id: &str) -> io::Result<()> {
        let Some(pos) = self.index.items.iter().position(|i| i.id == id) else { return Ok(()) };
        if self.index.items[pos].kind == ScreenItemKind::Factory {
            return Err(io::Error::other("the factory backup cannot be removed"));
        }
        self.index.items.remove(pos);
        if let Some(path) = self.data_path(id) {
            let _ = fs::remove_file(path);
        }
        self.save_index()
    }

    /// Record an upload the keyboard confirmed.
    pub fn uploaded(&mut self, model_id: &str, id: &str) -> io::Result<()> {
        let now = now_ms();
        if let Some(item) = self.index.items.iter_mut().find(|i| i.id == id) {
            item.uploaded_ms = Some(now);
        }
        self.index.shown.insert(model_id.to_string(), id.to_string());
        self.count_upload(true);
        self.save_index()
    }

    /// Count an upload that reached the keyboard's flash, finished or not.
    /// `charge`: also take it from the live budget (manual uploads; the live
    /// mode takes its token before uploading).
    pub fn count_upload(&mut self, charge: bool) {
        if charge {
            self.index.live.budget.charge(crate::live::now_ms());
        }
        self.index.total_uploads += 1;
        *self.index.uploads_by_day.entry(today()).or_default() += 1;
        while self.index.uploads_by_day.len() > KEEP_DAYS {
            let oldest = self.index.uploads_by_day.keys().next().cloned().expect("non-empty");
            self.index.uploads_by_day.remove(&oldest);
        }
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_index()
    }

    pub fn live(&self) -> &LiveStored {
        &self.index.live
    }

    pub fn live_mut(&mut self) -> &mut LiveStored {
        &mut self.index.live
    }

    /// The library item to put back when the live mode is turned off: what
    /// the GIF page showed before a live card replaced it.
    pub fn live_restore_target(&self) -> Option<LiveRestore> {
        if !self.index.shown.values().any(|v| v == LIVE_ID) {
            return None;
        }
        let id = self.index.live.before.as_deref()?;
        let item = self.index.items.iter().find(|i| i.id == id)?;
        Some(LiveRestore { id: item.id.clone(), name: item.name.clone(), frames: item.frames, pages: item.pages })
    }

    /// The GIF page shows this live card already (as far as the app knows).
    pub fn live_is_showing(&self, content: &str) -> bool {
        self.index.live.last_content.as_deref() == Some(content) && self.index.shown.values().any(|v| v == LIVE_ID)
    }

    /// Record a live card the keyboard confirmed and keep its data for
    /// the preview of the GIF page.
    pub fn live_uploaded(
        &mut self,
        model_id: &str,
        anim: &ScreenAnimation,
        content: &str,
        item: &LiveItem,
    ) -> io::Result<()> {
        if let Some(d) = &self.dir {
            fs::create_dir_all(d)?;
            let path = d.join(LIVE_FILE);
            let tmp = path.with_extension("tmp");
            fs::write(&tmp, screen::blob_unpadded(anim))?;
            fs::rename(&tmp, &path)?;
        }
        if !model_id.is_empty() {
            // Remember what the live mode replaced (the first time only), so
            // it can be put back. Unknown means the factory animation the
            // keyboard shipped with, as far as the app can tell.
            let prev = self.index.shown.get(model_id).cloned();
            if prev.as_deref() != Some(LIVE_ID) {
                let factory = self.index.items.iter().find(|i| i.kind == ScreenItemKind::Factory).map(|i| i.id.clone());
                self.index.live.before = prev.or(factory);
            }
            self.index.shown.insert(model_id.to_string(), LIVE_ID.to_string());
        }
        let live = &mut self.index.live;
        live.last_content = Some(content.to_string());
        live.last_source = Some(item.source.clone());
        live.last_name = Some(item.name.clone());
        live.last_ms = Some(now_ms());
        self.count_upload(false);
        self.save_index()
    }

    pub fn state(&self, model_id: &str) -> ScreenState {
        let mut items = self.index.items.clone();
        // Factory first, then newest first.
        items.sort_by(|a, b| {
            (b.kind == ScreenItemKind::Factory)
                .cmp(&(a.kind == ScreenItemKind::Factory))
                .then(b.created_ms.total_cmp(&a.created_ms))
        });
        ScreenState {
            items,
            shown: self.index.shown.get(model_id).cloned(),
            uploads_today: self.index.uploads_by_day.get(&today()).copied().unwrap_or(0),
            uploads_total: self.index.total_uploads,
            limits: ScreenLimits::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_protocol::sonix::screen::{rgb565, test_pattern, Frame};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klavye-screen-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn add_dedupe_reload_and_remove() {
        let dir = temp_dir("lib");
        let mut lib = ScreenLibrary::open(Some(&dir));
        assert!(lib.state("m").items.is_empty(), "no factory backup in a fresh dir");
        let still = ScreenAnimation::still(test_pattern());
        let a = lib.add("Desen", ScreenItemKind::Image, &still).unwrap();
        let again = lib.add("Desen 2", ScreenItemKind::Image, &still).unwrap();
        assert_eq!(a.id, again.id, "same content, same entry");
        assert_eq!(lib.state("m").items.len(), 1);
        assert_eq!(lib.state("m").items[0].name, "Desen 2");
        let anim = ScreenAnimation::new(vec![(Frame::filled(rgb565(255, 0, 0)), 50), (Frame::filled(0), 50)]).unwrap();
        let b = lib.add("  ", ScreenItemKind::Animation, &anim).unwrap();
        assert_eq!(b.name, "Adsız");
        assert_eq!((b.frames, b.loop_ms, b.pages), (2, 200, 32));

        lib.uploaded("m", &b.id).unwrap();
        let reopened = ScreenLibrary::open(Some(&dir));
        let s = reopened.state("m");
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.shown.as_deref(), Some(b.id.as_str()));
        assert_eq!((s.uploads_today, s.uploads_total), (1, 1));
        assert_eq!(reopened.animation(&b.id).unwrap(), anim, "stored bytes round-trip");
        assert!(reopened.state("other").shown.is_none());

        let mut lib = reopened;
        lib.remove(&a.id).unwrap();
        assert_eq!(lib.state("m").items.len(), 1);
        assert!(lib.animation(&a.id).is_err());
        assert!(lib.data("../../etc").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_cards_remember_what_they_replaced() {
        let dir = temp_dir("live");
        let mut lib = ScreenLibrary::open(Some(&dir));
        let mine = lib.add("Benim", ScreenItemKind::Image, &ScreenAnimation::still(test_pattern())).unwrap();
        lib.uploaded("m", &mine.id).unwrap();
        assert!(lib.live_restore_target().is_none(), "no live card on the page yet");

        let card = ScreenAnimation::still(Frame::filled(rgb565(0, 0, 255)));
        let item = LiveItem { source: "now-playing".into(), name: "Şarkı".into() };
        lib.live_uploaded("m", &card, "c1", &item).unwrap();
        lib.live_uploaded("m", &card, "c2", &item).unwrap();
        let back = lib.live_restore_target().expect("the user's picture can be put back");
        assert_eq!(back.id, mine.id, "the second card does not overwrite what the first replaced");
        assert_eq!(lib.state("m").shown.as_deref(), Some(LIVE_ID));
        assert!(lib.data(LIVE_ID).is_ok(), "the live card is kept for the preview");

        // Putting it back (or any manual upload) leaves nothing to restore.
        lib.uploaded("m", &mine.id).unwrap();
        assert!(lib.live_restore_target().is_none());
        let reopened = ScreenLibrary::open(Some(&dir));
        assert_eq!(reopened.state("m").uploads_today, 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn factory_backup_is_imported_once_and_kept() {
        let dir = temp_dir("factory");
        let factory = dir.join("screen").join(FACTORY_FILE);
        fs::create_dir_all(factory.parent().unwrap()).unwrap();
        // A one-frame 240×135 GIF.
        let mut gif = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut gif, 240, 135, &[0, 0, 0]).unwrap();
            let f = gif::Frame {
                width: 240,
                height: 135,
                buffer: std::borrow::Cow::Owned(vec![0; 240 * 135]),
                ..Default::default()
            };
            enc.write_frame(&f).unwrap();
        }
        fs::write(&factory, gif).unwrap();
        let mut lib = ScreenLibrary::open(Some(&dir));
        let items = lib.state("m").items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ScreenItemKind::Factory);
        assert!(lib.remove(&items[0].id).is_err(), "factory backup stays");
        let lib = ScreenLibrary::open(Some(&dir));
        assert_eq!(lib.state("m").items.len(), 1, "imported once");
        let _ = fs::remove_dir_all(&dir);
    }
}
