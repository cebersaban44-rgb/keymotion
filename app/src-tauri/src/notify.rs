//! Windows notifications, and opening the project's own pages in the
//! browser.
//!
//! Windows shows a notification under its app id, with that id's name and
//! logo. The app registers its bundle identifier at start
//! (`kb_winsys::appid`: the name "Keymotion" and the logo as a PNG in the
//! local data folder), so notifications say Keymotion, with or without a
//! Start menu shortcut. Should that fail, a development build borrows
//! PowerShell's id, which every Windows knows, so the notification still
//! shows (under PowerShell's name).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// PowerShell's app id: registered on every Windows; the fallback above.
const DEV_APP_ID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// The logo notifications show (written next to the app's local data).
const ICON: &[u8] = include_bytes!("../icons/128x128.png");
const ICON_FILE: &str = "notification-icon.png";

/// The app's own id is registered with its name and logo.
static REGISTERED: AtomicBool = AtomicBool::new(false);

/// Register `identifier` as "Keymotion" with its logo (written into
/// `dir`). Call once at start, before anything is shown.
pub fn register(identifier: &str, dir: &Path) {
    let icon = dir.join(ICON_FILE);
    let result = (|| {
        if std::fs::read(&icon).ok().as_deref() != Some(ICON) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(&icon, ICON).map_err(|e| format!("notification logo not written: {e}"))?;
        }
        kb_winsys::appid::register(identifier, "Keymotion", &icon)
    })();
    match result {
        Ok(()) => REGISTERED.store(true, Ordering::SeqCst),
        Err(e) => tracing::warn!("notifications keep a borrowed name: {e}"),
    }
}

/// Pages the app may open: only the project's own GitHub pages.
const PROJECT_PAGES: &str = "https://github.com/cebersaban44-rgb/keymotion/";

/// The app id notifications are shown under.
pub fn app_id(identifier: &str) -> String {
    if tauri::is_dev() && !REGISTERED.load(Ordering::SeqCst) {
        DEV_APP_ID.to_string()
    } else {
        identifier.to_string()
    }
}

/// Text for the notification XML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// The notification's XML: the title, an optional line under it, and no
/// sound of its own (the reminder plays the Windows sound when asked to).
fn toast_xml(title: &str, body: &str) -> String {
    let body = if body.is_empty() { String::new() } else { format!("<text>{}</text>", escape(body)) };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text>{body}</binding></visual><audio silent=\"true\"/></toast>",
        escape(title)
    )
}

/// Show a notification now.
#[cfg(windows)]
pub fn show(app_id: &str, title: &str, body: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let doc = XmlDocument::new().map_err(|e| e.to_string())?;
    doc.LoadXml(&HSTRING::from(toast_xml(title, body))).map_err(|e| e.to_string())?;
    let toast = ToastNotification::CreateToastNotification(&doc).map_err(|e| e.to_string())?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))
        .and_then(|n| n.Show(&toast))
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn show(_app_id: &str, _title: &str, _body: &str) -> Result<(), String> {
    Err("notifications are only available on Windows".into())
}

/// Whether `url` is one of the project's own pages, written plainly.
fn project_page(url: &str) -> bool {
    url.starts_with(PROJECT_PAGES)
        && url.len() < 300
        && !url.contains("..")
        && url.chars().all(|c| c.is_ascii_alphanumeric() || "-._~/:#".contains(c))
}

/// Open one of the project's own pages in the default browser.
pub fn open_project_page(url: &str) -> Result<(), String> {
    if !project_page(url) {
        return Err("not a project page".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("opening pages is only available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_text_is_escaped() {
        let xml = toast_xml("Su <iç> & \"dinlen\"", "");
        assert!(xml.contains("<text>Su &lt;iç&gt; &amp; &quot;dinlen&quot;</text></binding>"));
        assert!(toast_xml("Ara", "Annemi").contains("<text>Ara</text><text>Annemi</text>"));
    }

    #[test]
    fn only_project_pages_are_opened() {
        assert!(project_page("https://github.com/cebersaban44-rgb/keymotion/blob/develop/docs/destek-haritasi.md"));
        assert!(!project_page("https://github.com/someone-else/repo"));
        assert!(!project_page("https://github.com/cebersaban44-rgb/keymotion/../../someone-else"));
        assert!(!project_page("https://github.com/cebersaban44-rgb/keymotion/\" & calc"));
        assert!(!project_page("file:///C:/Windows/System32/calc.exe"));
    }
}
