//! Windows notifications, and opening the project's own pages in the
//! browser.
//!
//! Notifications need an app id Windows knows. The installer gives the
//! Start menu shortcut the bundle identifier as its id, so an installed app
//! uses that. A development build has no shortcut; it borrows PowerShell's
//! id, as tauri-plugin-notification does, so the notification still shows.

/// PowerShell's app id: registered on every Windows, used by dev builds.
const DEV_APP_ID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// Pages the app may open: only the project's own GitHub pages.
const PROJECT_PAGES: &str = "https://github.com/cebersaban44-rgb/keymotion/";

/// The app id notifications are shown under.
pub fn app_id(identifier: &str) -> String {
    if tauri::is_dev() {
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
