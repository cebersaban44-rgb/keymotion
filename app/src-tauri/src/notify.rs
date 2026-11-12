//! Windows notifications, and opening the project's own pages in the
//! browser (the support list, a new issue with its form).
//!
//! Windows shows a notification under its app id, with that id's name and
//! logo. The app registers its bundle identifier at start
//! (`kb_winsys::appid`: the name "Keymotion" and the logo as a PNG in the
//! local data folder), so notifications say Keymotion, with or without a
//! Start menu shortcut. Should that fail, a development build borrows
//! PowerShell's id, which every Windows knows, so the notification still
//! shows (under PowerShell's name).

use crate::dto::IssueKind;
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

/// Pages the app may open: only the project's own GitHub pages and its
/// documentation site.
const PROJECT_PAGES: &str = "https://github.com/cebersaban44-rgb/keymotion/";
const DOCS_SITE: &str = "https://cebersaban44-rgb.github.io/keymotion/";

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
    (url.starts_with(PROJECT_PAGES) || url.starts_with(DOCS_SITE))
        && url.len() < 300
        && !url.contains("..")
        && url.chars().all(|c| c.is_ascii_alphanumeric() || "-._~/:#".contains(c))
}

/// Open one of the project's own pages in the default browser.
pub fn open_project_page(url: &str) -> Result<(), String> {
    if !project_page(url) {
        return Err("not a project page".into());
    }
    open(url)
}

/// Longest issue title passed on (GitHub keeps 256 characters).
const TITLE_MAX: usize = 120;
/// Longest keyboard name passed on.
const MODEL_MAX: usize = 80;

/// The address of a new issue on the project's GitHub page, with the form
/// for `kind` (`.github/ISSUE_TEMPLATE`), its title and, for a keyboard,
/// the model field filled in. Everything the caller gives is encoded, so
/// the address stays the project's.
pub fn issue_form_url(kind: IssueKind, title: &str, model: Option<&str>) -> String {
    let template = match kind {
        IssueKind::Bug => "bug.yml",
        IssueKind::Keyboard => "keyboard.yml",
        IssueKind::Idea => "idea.yml",
    };
    let mut url = format!("{PROJECT_PAGES}issues/new?template={template}");
    let title: String = title.trim().chars().take(TITLE_MAX).collect();
    if !title.is_empty() {
        url.push_str("&title=");
        url.push_str(&encode(&title));
    }
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        url.push_str("&model=");
        url.push_str(&encode(&model.chars().take(MODEL_MAX).collect::<String>()));
    }
    url
}

/// Open a new issue with its form in the default browser.
pub fn open_issue_form(kind: IssueKind, title: &str, model: Option<&str>) -> Result<(), String> {
    open(&issue_form_url(kind, title, model))
}

/// Percent-encoding for a query value: only unreserved characters stay.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Open the page in the default browser (`explorer.exe` opened the
/// Documents folder for an address with a query; K4).
fn open(url: &str) -> Result<(), String> {
    kb_winsys::shell::open_url(url).inspect_err(|e| tracing::warn!("{e}"))
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
    fn issue_addresses_are_the_projects_and_encoded() {
        assert_eq!(
            issue_form_url(IssueKind::Idea, "", None),
            "https://github.com/cebersaban44-rgb/keymotion/issues/new?template=idea.yml"
        );
        let url = issue_form_url(IssueKind::Bug, "[Problem] Couldn't save & \"x\" ğ", None);
        assert_eq!(
            url,
            "https://github.com/cebersaban44-rgb/keymotion/issues/new?template=bug.yml\
             &title=%5BProblem%5D%20Couldn%27t%20save%20%26%20%22x%22%20%C4%9F"
        );
        let url = issue_form_url(IssueKind::Keyboard, "[Keyboard] SONiX AK820 PRO", Some("SONiX AK820 PRO"));
        assert!(
            url.contains("template=keyboard.yml&title=%5BKeyboard%5D%20SONiX%20AK820%20PRO&model=SONiX%20AK820%20PRO")
        );
        // Long titles are cut; nothing can leave the project's address.
        let long = issue_form_url(IssueKind::Bug, &"a".repeat(500), Some("&template=x"));
        assert!(long.len() < 400 && long.ends_with("&model=%26template%3Dx"), "{long}");
    }

    #[test]
    fn only_project_pages_are_opened() {
        assert!(project_page("https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0"));
        assert!(project_page("https://cebersaban44-rgb.github.io/keymotion/tr/help/keyboards/"));
        assert!(!project_page("https://cebersaban44-rgb.github.io/keymotion.example.com/"));
        assert!(!project_page("https://cebersaban44-rgb.github.io/other-site/"));
        assert!(!project_page("https://github.com/someone-else/repo"));
        assert!(!project_page("https://github.com/cebersaban44-rgb/keymotion/../../someone-else"));
        assert!(!project_page("https://github.com/cebersaban44-rgb/keymotion/\" & calc"));
        assert!(!project_page("file:///C:/Windows/System32/calc.exe"));
    }
}
