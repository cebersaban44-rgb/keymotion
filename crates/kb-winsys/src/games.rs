//! Games installed on this computer, from the launchers' own records, so
//! the game session can tell a game from any other program
//! (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md §2.2). Only files
//! and registry values the launchers keep are read; nothing leaves the
//! computer and no game is opened.
//!
//! * Steam: `steamapps\libraryfolders.vdf` names the libraries, each
//!   `appmanifest_<id>.acf` a game and its folder under `steamapps\common`.
//!   Covers are in `appcache\librarycache`.
//! * Epic Games: the `.item` manifests (JSON) in its data folder.
//! * GOG: `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`.
//! * Xbox / Game Pass: the `XboxGames` folder at the root of a disk inside
//!   the computer that has a `.GamingRoot` file.
//! * Riot Games: `Metadata\<product>\<product>.product_settings.yaml`.
//!   Only the match program counts (League of Legends' client is a
//!   launcher).
//! * Windows: games the Xbox Game Bar remembers
//!   (`HKCU\System\GameConfigStore\Children`, `MatchedExeFullPath`).

use crate::registry::{self, Root};
use std::path::{Path, PathBuf};

/// Where a game was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    Steam,
    Epic,
    Gog,
    Xbox,
    Riot,
    Windows,
}

/// A game a launcher knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownGame {
    pub name: String,
    pub source: Source,
    /// Install folder: any program in it counts as the game, except
    /// launchers and helpers ([`is_helper`]). `None` when only `exes`
    /// count.
    pub dir: Option<String>,
    /// Programs that are the game itself.
    pub exes: Vec<String>,
    /// Steam's id, for the cover.
    pub steam_app: Option<u32>,
}

/// Launchers, stores, anti-cheat starters, crash reporters and installers:
/// they run next to games, often from the same folder, but are not the game.
const HELPERS: [&str; 36] = [
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
    "epicgameslauncher.exe",
    "epicwebhelper.exe",
    "battle.net.exe",
    "eadesktop.exe",
    "eabackgroundservice.exe",
    "upc.exe",
    "ubisoftconnect.exe",
    "uplaywebcore.exe",
    "galaxyclient.exe",
    "galaxyclient helper.exe",
    "riot client.exe",
    "riotclientservices.exe",
    "riotclientcrashhandler.exe",
    "riotclientux.exe",
    "riotclientuxrender.exe",
    "leagueclient.exe",
    "leagueclientux.exe",
    "leagueclientuxrender.exe",
    "valorant.exe",
    "vgc.exe",
    "vgtray.exe",
    "start_protected_game.exe",
    "easyanticheat_eos_setup.exe",
    "easyanticheat_setup.exe",
    "battleye launcher.exe",
    "beservice.exe",
    "crashreportclient.exe",
    "unitycrashhandler64.exe",
    "unitycrashhandler32.exe",
    "vc_redist.x64.exe",
    "vc_redist.x86.exe",
    "dxsetup.exe",
    "uninstall.exe",
];

/// A launcher or helper program, never a game itself.
pub fn is_helper(exe_name: &str) -> bool {
    let lower = exe_name.to_ascii_lowercase();
    HELPERS.contains(&lower.as_str())
        || lower.starts_with("unins")
        || lower.contains("crashhandler")
        || lower.contains("crashreport")
        || lower.ends_with("_setup.exe")
}

/// A path as Windows compares it: `/` as `\`, no `\` at the end, every
/// letter in lowercase (`Ü` and `ü` alike, not only A–Z).
fn norm(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn same_path(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// `path` lies inside the folder `dir` (any depth).
fn inside(path: &str, dir: &str) -> bool {
    let (path, dir) = (norm(path), norm(dir));
    !dir.is_empty() && path.len() > dir.len() && path.starts_with(&dir) && path.as_bytes()[dir.len()] == b'\\'
}

/// A key to compare a program's path or a folder with, which stays the same
/// when the program updates itself: the path as Windows compares it (`/` as
/// `\`, no `\` at the end, lowercase letters), with the folders that carry
/// a version made alike:
///
/// * a Microsoft Store package folder,
///   `Microsoft.WindowsNotepad_11.2409.9.0_x64__8wekyb3d8bbwe`, keeps only
///   its name and publisher: `microsoft.windowsnotepad__8wekyb3d8bbwe`;
/// * Squirrel's `app-1.0.9165` (Discord, Slack…) becomes `app-*`;
/// * Roblox's `version-<hex>` becomes `version-*`.
pub fn stable_key(path: &str) -> String {
    norm(path).split('\\').map(stable_part).collect::<Vec<_>>().join("\\")
}

/// One folder or file name of [`stable_key`], already in lowercase.
fn stable_part(part: &str) -> String {
    if let Some(family) = package_family(part) {
        return family;
    }
    if part.strip_prefix("app-").is_some_and(is_squirrel_version) {
        return "app-*".into();
    }
    if part.strip_prefix("version-").is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit())) {
        return "version-*".into();
    }
    part.into()
}

/// A package's full name, `name_1.2.3.0_x64__publisher` (name, dotted
/// version, processor or none, an empty resource id, publisher id), as
/// `name__publisher`.
fn package_family(part: &str) -> Option<String> {
    let [name, version, arch, resource, publisher] = part.split('_').collect::<Vec<_>>()[..] else { return None };
    let word = |s: &str| s.bytes().all(|b| b.is_ascii_alphanumeric());
    let named = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let dotted =
        version.contains('.') && version.split('.').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let ok = named && dotted && word(arch) && resource.is_empty() && !publisher.is_empty() && word(publisher);
    ok.then(|| format!("{name}__{publisher}"))
}

/// Squirrel's version after `app-`: `1.0.9165`, `2.1.0-beta3`.
fn is_squirrel_version(v: &str) -> bool {
    v.starts_with(|c: char| c.is_ascii_digit())
        && v.contains('.')
        && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

/// The game a program belongs to, if any: an exact program first, then an
/// install folder.
pub fn find<'a>(games: &'a [KnownGame], exe_path: &str) -> Option<&'a KnownGame> {
    let name = crate::foreground::file_name(exe_path);
    if let Some(g) = games.iter().find(|g| g.exes.iter().any(|e| same_path(e, exe_path))) {
        return Some(g);
    }
    if is_helper(name) {
        return None;
    }
    // The deepest folder wins (a game inside another's folder).
    games
        .iter()
        .filter(|g| g.dir.as_deref().is_some_and(|d| inside(exe_path, d)))
        .max_by_key(|g| g.dir.as_ref().map_or(0, String::len))
}

// ---------------------------------------------------------------- Steam

/// A KeyValues (`.vdf`, `.acf`) node.
#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Text(String),
    Block(Vec<(String, Kv)>),
}

impl Kv {
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Block(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Kv::Text(_) => None,
        }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Kv::Text(t) => Some(t),
            Kv::Block(_) => None,
        }
    }

    pub fn items(&self) -> &[(String, Kv)] {
        match self {
            Kv::Block(items) => items,
            Kv::Text(_) => &[],
        }
    }
}

/// Blocks nested deeper than this are refused, so a broken or hostile file
/// cannot exhaust the stack. Steam's own files nest a few levels.
const KV_MAX_DEPTH: usize = 64;

/// Parse Valve's KeyValues text: quoted keys and values, `{ }` blocks,
/// `//` comments, `\\` and `\"` escapes. The whole file is one block.
/// `None` for broken text or blocks nested deeper than [`KV_MAX_DEPTH`].
pub fn parse_kv(text: &str) -> Option<Kv> {
    #[derive(Debug)]
    enum Tok {
        Str(String),
        Open,
        Close,
    }
    let mut toks = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            other => s.push(other),
                        },
                        other => s.push(other),
                    }
                }
                toks.push(Tok::Str(s));
            }
            '{' => toks.push(Tok::Open),
            '}' => toks.push(Tok::Close),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                // Unquoted word.
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                toks.push(Tok::Str(s));
            }
        }
    }
    /// The items up to the `}` closing a block `depth` levels deep (0: the
    /// whole file, up to its end).
    fn block(toks: &[Tok], i: &mut usize, depth: usize) -> Option<Kv> {
        let nested = depth > 0;
        let mut items = Vec::new();
        loop {
            match toks.get(*i) {
                None => return (!nested).then_some(Kv::Block(items)),
                Some(Tok::Close) => {
                    *i += 1;
                    return nested.then_some(Kv::Block(items));
                }
                Some(Tok::Open) => return None,
                Some(Tok::Str(k)) => {
                    *i += 1;
                    match toks.get(*i)? {
                        Tok::Str(v) => {
                            items.push((k.clone(), Kv::Text(v.clone())));
                            *i += 1;
                        }
                        Tok::Open => {
                            if depth >= KV_MAX_DEPTH {
                                return None;
                            }
                            *i += 1;
                            let b = block(toks, i, depth + 1)?;
                            items.push((k.clone(), b));
                        }
                        Tok::Close => return None,
                    }
                }
            }
        }
    }
    let mut i = 0;
    block(&toks, &mut i, 0)
}

/// Library folders from `libraryfolders.vdf`: numbered blocks with a
/// `"path"` (since 2021) or, in older files, numbered values
/// (`"1" "D:\\SteamLibrary"`, beside other values such as
/// `"ContentStatsID"`). Older files leave out Steam's own folder.
pub fn steam_libraries(vdf: &str) -> Vec<String> {
    let Some(root) = parse_kv(vdf) else { return Vec::new() };
    let Some(folders) = root.get("libraryfolders") else { return Vec::new() };
    let numbered = |k: &str| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit());
    folders
        .items()
        .iter()
        .filter_map(|(key, f)| match f {
            Kv::Block(_) => f.text("path").map(str::to_string),
            Kv::Text(path) => (numbered(key) && !path.trim().is_empty()).then(|| path.clone()),
        })
        .collect()
}

/// Steam apps that are tools, not games: they run full screen or in front
/// for hours, and the game session must not start for them.
const STEAM_TOOLS: [u32; 9] = [
    228_980,   // Steamworks Common Redistributables
    250_820,   // SteamVR
    1_070_560, // Steam Linux Runtime
    1_391_110, // Steam Linux Runtime - Soldier
    431_960,   // Wallpaper Engine
    993_090,   // Lossless Scaling
    1_905_180, // OBS Studio
    365_670,   // Blender
    629_520,   // Soundpad
];

/// A game from an `appmanifest_<id>.acf`, in `library`.
pub fn steam_game(acf: &str, library: &str) -> Option<KnownGame> {
    let root = parse_kv(acf)?;
    let app = root.get("AppState")?;
    let id: u32 = app.text("appid")?.parse().ok()?;
    if STEAM_TOOLS.contains(&id) {
        return None;
    }
    let name = app.text("name")?.trim().to_string();
    let dir = app.text("installdir")?.trim();
    if name.is_empty() || dir.is_empty() {
        return None;
    }
    let lib = library.replace('/', "\\");
    Some(KnownGame {
        name,
        source: Source::Steam,
        dir: Some(format!("{}\\steamapps\\common\\{dir}", lib.trim_end_matches('\\'))),
        exes: Vec::new(),
        steam_app: Some(id),
    })
}

fn steam_root() -> Option<PathBuf> {
    let p = registry::text(Root::User, r"Software\Valve\Steam", "SteamPath")?;
    let p = PathBuf::from(p.replace('/', "\\"));
    p.is_dir().then_some(p)
}

fn steam_games() -> Vec<KnownGame> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut libraries = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf"))
        .map(|t| steam_libraries(&t))
        .unwrap_or_default();
    // Steam's own folder is a library, also where an older file leaves it out.
    let own = root.to_string_lossy().into_owned();
    if !libraries.iter().any(|l| same_path(l, &own)) {
        libraries.insert(0, own);
    }
    let mut out = Vec::new();
    for lib in libraries {
        let Ok(entries) = std::fs::read_dir(Path::new(&lib).join("steamapps")) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                if let Some(g) = std::fs::read_to_string(e.path()).ok().and_then(|t| steam_game(&t, &lib)) {
                    out.push(g);
                }
            }
        }
    }
    out
}

/// Steam's own picture of a game (`header.jpg`), when it has one.
pub fn steam_cover(app: u32) -> Option<PathBuf> {
    steam_cover_in(&steam_root()?.join(r"appcache\librarycache"), app)
}

/// The cover of `app` in Steam's picture cache `cache`, in this order:
/// `<app>\header.jpg`; `<app>\<hash>\header.jpg` (Steam's newer layout);
/// a cover kept only in a language, `<app>\header_<language>.jpg` or
/// `<app>\<hash>\header_<language>.jpg`; the older flat `<app>_header.jpg`.
/// Of several in one step, the newest file.
fn steam_cover_in(cache: &Path, app: u32) -> Option<PathBuf> {
    let dir = cache.join(app.to_string());
    let own = dir.join("header.jpg");
    if own.is_file() {
        return Some(own);
    }
    let subdirs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    let in_language = |n: &str| n.starts_with("header_") && n.ends_with(".jpg");
    let mut all = vec![dir];
    all.extend(subdirs.iter().cloned());
    newest_file(&subdirs, |n| n == "header.jpg").or_else(|| newest_file(&all, in_language)).or_else(|| {
        let flat = cache.join(format!("{app}_header.jpg"));
        flat.is_file().then_some(flat)
    })
}

/// The file written last, among those in `dirs` (not below) whose
/// lowercase name `wanted` accepts.
fn newest_file(dirs: &[PathBuf], wanted: impl Fn(&str) -> bool) -> Option<PathBuf> {
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|e| wanted(&e.file_name().to_string_lossy().to_lowercase()))
        .filter_map(|e| {
            let meta = e.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some((meta.modified().ok(), e.path()))
        })
        .max()
        .map(|(_, path)| path)
}

// ---------------------------------------------------------------- Epic

/// A game from an Epic Games `.item` manifest.
pub fn epic_game(json: &str) -> Option<KnownGame> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("bIsIncompleteInstall").and_then(serde_json::Value::as_bool) == Some(true) {
        return None;
    }
    if let Some(cats) = v.get("AppCategories").and_then(serde_json::Value::as_array) {
        if !cats.iter().any(|c| c.as_str() == Some("games")) {
            return None;
        }
    }
    let name = v.get("DisplayName")?.as_str()?.trim().to_string();
    let dir = v.get("InstallLocation")?.as_str()?.trim().replace('/', "\\");
    if name.is_empty() || dir.is_empty() {
        return None;
    }
    let exes = v
        .get("LaunchExecutable")
        .and_then(serde_json::Value::as_str)
        .filter(|e| !e.trim().is_empty())
        .map(|e| vec![format!("{}\\{}", dir.trim_end_matches('\\'), e.replace('/', "\\"))])
        .unwrap_or_default();
    Some(KnownGame { name, source: Source::Epic, dir: Some(dir), exes, steam_app: None })
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

fn epic_games() -> Vec<KnownGame> {
    let dir = program_data().join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok().and_then(|t| epic_game(&t)))
        .collect()
}

// ---------------------------------------------------------------- GOG

fn gog_games() -> Vec<KnownGame> {
    let base = r"SOFTWARE\WOW6432Node\GOG.com\Games";
    registry::subkeys(Root::Machine, base)
        .into_iter()
        .filter_map(|id| {
            let key = format!(r"{base}\{id}");
            let name = registry::text(Root::Machine, &key, "gameName")?;
            let dir = registry::text(Root::Machine, &key, "path")?;
            let exes = registry::text(Root::Machine, &key, "exe").into_iter().collect();
            Some(KnownGame { name, source: Source::Gog, dir: Some(dir), exes, steam_app: None })
        })
        .collect()
}

// ---------------------------------------------------------------- Xbox

fn xbox_games() -> Vec<KnownGame> {
    let mut out = Vec::new();
    for letter in b'C'..=b'Z' {
        let root = format!("{}:\\", char::from(letter));
        // Only disks inside the computer: a network drive that is gone can
        // hang, and an empty card reader can ask for a disk. The Xbox app's
        // own folder name in `.GamingRoot` is not read: its format is not
        // documented, and `XboxGames` is what the app makes.
        if !sys::fixed_drive(&root) || !Path::new(&root).join(".GamingRoot").is_file() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(Path::new(&root).join("XboxGames")) else { continue };
        for e in entries.flatten() {
            if e.path().is_dir() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.eq_ignore_ascii_case("GameSave") {
                    continue;
                }
                out.push(KnownGame {
                    name,
                    source: Source::Xbox,
                    dir: Some(e.path().to_string_lossy().into_owned()),
                    exes: Vec::new(),
                    steam_app: None,
                });
            }
        }
    }
    out
}

#[cfg(windows)]
mod sys {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;

    /// `DRIVE_FIXED` (its feature, `Win32_System_WindowsProgramming`, is
    /// not otherwise needed).
    const DRIVE_FIXED: u32 = 3;

    /// `root` (`D:\`) is a disk inside the computer. Windows answers from
    /// its list of drives; the drive itself is not touched.
    pub fn fixed_drive(root: &str) -> bool {
        // SAFETY: a NUL-terminated path that outlives the call.
        unsafe { GetDriveTypeW(&HSTRING::from(root)) == DRIVE_FIXED }
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn fixed_drive(_root: &str) -> bool {
        false
    }
}

// ---------------------------------------------------------------- Riot

/// Riot products: (product id, name, the match programs inside its folder).
const RIOT: [(&str, &str, &[&str]); 3] = [
    ("league_of_legends.live", "League of Legends", &[r"Game\League of Legends.exe"]),
    ("league_of_legends.pbe", "League of Legends (PBE)", &[r"Game\League of Legends.exe"]),
    ("valorant.live", "VALORANT", &[r"ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"]),
];

/// `product_install_full_path` from a Riot `product_settings.yaml`.
pub fn riot_install_path(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("product_install_full_path:")?;
        let v = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        (!v.is_empty()).then(|| v.replace('/', "\\"))
    })
}

fn riot_games() -> Vec<KnownGame> {
    let meta = program_data().join(r"Riot Games\Metadata");
    RIOT.iter()
        .filter_map(|(id, name, exes)| {
            let yaml = std::fs::read_to_string(meta.join(id).join(format!("{id}.product_settings.yaml"))).ok()?;
            let dir = riot_install_path(&yaml)?;
            // The settings file stays for a product removed or never installed.
            let exes: Vec<String> = exes
                .iter()
                .map(|e| format!("{}\\{e}", dir.trim_end_matches('\\')))
                .filter(|e| Path::new(e).is_file())
                .collect();
            (!exes.is_empty()).then(|| KnownGame {
                name: (*name).into(),
                source: Source::Riot,
                dir: None,
                exes,
                steam_app: None,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- Windows

fn windows_games() -> Vec<KnownGame> {
    let base = r"System\GameConfigStore\Children";
    registry::subkeys(Root::User, base)
        .into_iter()
        .filter_map(|id| {
            let exe = registry::text(Root::User, &format!(r"{base}\{id}"), "MatchedExeFullPath")?;
            let file = crate::foreground::file_name(&exe);
            // Game Bar keeps games that were removed since.
            if is_helper(file) || !Path::new(&exe).is_file() {
                return None;
            }
            let name = file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_string();
            Some(KnownGame { name, source: Source::Windows, dir: None, exes: vec![exe], steam_app: None })
        })
        .collect()
}

/// Every game the launchers on this computer know. Reads a few files and
/// registry keys: call it now and then (on start, every few minutes), not
/// for every window.
pub fn installed() -> Vec<KnownGame> {
    let mut all = Vec::new();
    all.extend(steam_games());
    all.extend(epic_games());
    all.extend(gog_games());
    all.extend(xbox_games());
    all.extend(riot_games());
    // Game Bar's list last: a launcher's name is better than a file name.
    for g in windows_games() {
        if find(&all, &g.exes[0]).is_none() {
            all.push(g);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARIES: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"171183395"
			"570"		"40000000"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { }
	}
}
"#;

    const MANIFEST: &str = r#"
"AppState"
{
	"appid"		"570"
	"Universe"		"1"
	"name"		"Dota 2"
	"StateFlags"		"4"
	"installdir"		"dota 2 beta"
	// a comment
	"UserConfig" { "language" "turkish" }
}
"#;

    #[test]
    fn reads_steam_libraries_and_games() {
        assert_eq!(steam_libraries(LIBRARIES), vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        let g = steam_game(MANIFEST, r"D:\SteamLibrary").unwrap();
        assert_eq!(g.name, "Dota 2");
        assert_eq!(g.steam_app, Some(570));
        assert_eq!(g.dir.as_deref(), Some(r"D:\SteamLibrary\steamapps\common\dota 2 beta"));
        // Redistributables are not a game.
        let tool = MANIFEST.replace("\"570\"", "\"228980\"");
        assert_eq!(steam_game(&tool, "C:\\Steam"), None);
    }

    #[test]
    fn reads_the_older_libraries_file() {
        // Before 2021: numbered values, Steam's own folder left out.
        let old = r#"
"LibraryFolders"
{
	"TimeNextStatsReport"		"1612345678"
	"ContentStatsID"		"-4567890123456789"
	"1"		"D:\\SteamLibrary"
	"2"		"E:\\Oyunlar\\Steam"
}
"#;
        assert_eq!(steam_libraries(old), vec![r"D:\SteamLibrary", r"E:\Oyunlar\Steam"]);
        assert_eq!(steam_libraries("\"libraryfolders\" { \"1\" \"\" }"), Vec::<String>::new());
    }

    #[test]
    fn steam_tools_are_not_games() {
        for tool in ["431960", "993090", "1905180", "365670", "629520", "250820"] {
            let acf = MANIFEST.replace("\"570\"", &format!("\"{tool}\""));
            assert_eq!(steam_game(&acf, r"C:\Steam"), None, "{tool}");
        }
    }

    #[test]
    fn broken_keyvalues_are_refused() {
        assert_eq!(parse_kv("\"a\" { \"b\" \"c\""), None);
        assert_eq!(parse_kv("\"a\" }"), None);
        assert!(parse_kv("").is_some());
    }

    #[test]
    fn deep_keyvalues_stop_at_the_limit() {
        let nested = |n: usize| format!("{}\"k\" \"v\"{}", "\"a\" { ".repeat(n), " }".repeat(n));
        let deepest = parse_kv(&nested(KV_MAX_DEPTH)).unwrap();
        let mut node = &deepest;
        for _ in 0..KV_MAX_DEPTH {
            node = node.get("a").unwrap();
        }
        assert_eq!(node.text("k"), Some("v"));
        assert_eq!(parse_kv(&nested(KV_MAX_DEPTH + 1)), None);
        // Far too deep for the stack if nothing stopped it.
        assert_eq!(parse_kv(&nested(200_000)), None);
        assert_eq!(parse_kv(&"\"a\" {".repeat(200_000)), None);
    }

    #[test]
    fn stable_keys_survive_updates() {
        // A Microsoft Store app: version and processor dropped.
        let notepad =
            r"C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_11.2409.9.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe";
        let key = r"c:\program files\windowsapps\microsoft.windowsnotepad__8wekyb3d8bbwe\notepad\notepad.exe";
        assert_eq!(stable_key(notepad), key);
        assert_eq!(stable_key(&notepad.replace("11.2409.9.0_x64", "11.2609.11.0_arm64")), key);
        assert_eq!(stable_key(&notepad.replace("11.2409.9.0_x64", "11.2609.11.0_")), key);
        assert_eq!(
            stable_key(r"C:\Program Files\WindowsApps\Microsoft.WindowsCalculator_11.2607.0.0_x64__8wekyb3d8bbwe"),
            r"c:\program files\windowsapps\microsoft.windowscalculator__8wekyb3d8bbwe"
        );
        // Not a package's full name: kept.
        for kept in [
            "microsoft.windowsnotepad_8wekyb3d8bbwe",
            "name_1.2.3.0_x64_~_8wekyb3d8bbwe",
            "name_1.2.x.0_x64__8wekyb3d8bbwe",
            "name_1_x64__8wekyb3d8bbwe",
            "name_1.2.3.0_x64__",
            "_1.2.3.0_x64__8wekyb3d8bbwe",
            "my_game_v1.2_final_build",
        ] {
            assert_eq!(stable_key(kept), kept);
        }

        // Squirrel (Discord).
        let discord = r"%LOCALAPPDATA%\Discord\app-1.0.9165\Discord.exe";
        assert_eq!(stable_key(discord), r"%localappdata%\discord\app-*\discord.exe");
        assert_eq!(stable_key(&discord.replace("1.0.9165", "1.0.9212")), stable_key(discord));
        assert_eq!(stable_key(r"C:\x\app-2.1.0-beta3\x.exe"), r"c:\x\app-*\x.exe");
        for kept in ["app-store", "app-", "app-1", "app-1.0 (old)", "myapp-1.0.9165"] {
            assert_eq!(stable_key(kept), kept);
        }

        // Roblox.
        let roblox = r"C:\Users\Ali\AppData\Local\Roblox\Versions\version-1a2b3c4d5e6f7890\RobloxPlayerBeta.exe";
        assert_eq!(stable_key(roblox), r"c:\users\ali\appdata\local\roblox\versions\version-*\robloxplayerbeta.exe");
        assert_eq!(stable_key(&roblox.replace("1a2b3c4d5e6f7890", "ffee0011aabb2233")), stable_key(roblox));
        for kept in ["version-", "version-2g", "version-1.2"] {
            assert_eq!(stable_key(kept), kept);
        }

        // Anything else: only letters and slashes, as Windows compares them.
        assert_eq!(stable_key("D:/Oyunlar/ŞEHİR Ü/Game.exe/"), stable_key(r"d:\oyunlar\şehİr ü\game.exe"),);
        assert_eq!(stable_key(r"C:\Games\Dota 2\\"), r"c:\games\dota 2");
        assert_eq!(stable_key(r"\\NAS\Games\x.exe"), r"\\nas\games\x.exe");
        assert_eq!(stable_key(""), "");
    }

    #[test]
    fn finds_steam_covers_in_every_layout() {
        let cache = std::env::temp_dir().join(format!("kb-winsys-covers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let put = |rel: &str| {
            let p = cache.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"jpg").unwrap();
            p
        };
        // Newer layout, in a folder named after a hash.
        let hashed = put(r"730\162664aa5da85f418105350c5d67ca565f6c3713\header.jpg");
        put(r"730\162664aa5da85f418105350c5d67ca565f6c3713\library_capsule.jpg");
        assert_eq!(steam_cover_in(&cache, 730), Some(hashed));
        // Right in the app's folder: first.
        let own = put(r"730\header.jpg");
        assert_eq!(steam_cover_in(&cache, 730), Some(own));
        // Only in a language, in the app's folder or below it.
        let turkish = put(r"1070910\header_turkish.jpg");
        put(r"1070910\library_600x900.jpg");
        assert_eq!(steam_cover_in(&cache, 1_070_910), Some(turkish));
        let below = put(r"42\0123abcd\header_english.jpg");
        assert_eq!(steam_cover_in(&cache, 42), Some(below));
        // The older flat layout.
        let flat = put("570_header.jpg");
        assert_eq!(steam_cover_in(&cache, 570), Some(flat));
        put(r"10\library_hero.jpg");
        assert_eq!(steam_cover_in(&cache, 10), None);
        assert_eq!(steam_cover_in(&cache, 99), None);
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[cfg(windows)]
    #[test]
    fn the_system_drive_is_fixed() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert!(sys::fixed_drive(&format!("{system}\\")));
    }

    #[test]
    fn reads_an_epic_manifest() {
        let json = r#"{"DisplayName":"Rocket League","InstallLocation":"C:\\Program Files\\Epic Games\\rocketleague",
            "LaunchExecutable":"Binaries/Win64/RocketLeague.exe","AppCategories":["public","games","applications"],
            "bIsIncompleteInstall":false}"#;
        let g = epic_game(json).unwrap();
        assert_eq!(g.name, "Rocket League");
        assert_eq!(g.exes, vec![r"C:\Program Files\Epic Games\rocketleague\Binaries\Win64\RocketLeague.exe"]);
        assert_eq!(epic_game(&json.replace("\"games\",", "")), None, "not a game");
        assert_eq!(epic_game(&json.replace("\"bIsIncompleteInstall\":false", "\"bIsIncompleteInstall\":true")), None);
    }

    #[test]
    fn reads_a_riot_install_path() {
        let yaml =
            "product_install_full_path: \"C:/Riot Games/League of Legends\"\nproduct_install_root: \"C:/Riot Games\"\n";
        assert_eq!(riot_install_path(yaml).as_deref(), Some(r"C:\Riot Games\League of Legends"));
        assert_eq!(riot_install_path("other: 1"), None);
    }

    #[test]
    fn finds_the_game_of_a_program() {
        let games = vec![
            KnownGame {
                name: "Dota 2".into(),
                source: Source::Steam,
                dir: Some(r"C:\Steam\steamapps\common\dota 2 beta".into()),
                exes: vec![],
                steam_app: Some(570),
            },
            KnownGame {
                name: "League of Legends".into(),
                source: Source::Riot,
                dir: None,
                exes: vec![r"C:\Riot Games\League of Legends\Game\League of Legends.exe".into()],
                steam_app: None,
            },
        ];
        let dota = r"C:\Steam\steamapps\common\dota 2 beta\game\bin\win64\dota2.exe";
        assert_eq!(find(&games, dota).map(|g| g.name.as_str()), Some("Dota 2"));
        // Letters and slashes as Windows compares them.
        assert_eq!(find(&games, &dota.to_uppercase().replace('\\', "/")).map(|g| g.name.as_str()), Some("Dota 2"));
        let lol = r"c:\riot games\league of legends\game\league of legends.exe";
        assert_eq!(find(&games, lol).map(|g| g.name.as_str()), Some("League of Legends"));
        // The client is a launcher, the folder alone is not a game.
        assert_eq!(find(&games, r"C:\Riot Games\League of Legends\LeagueClientUx.exe"), None);
        // A crash reporter in the game's folder is not the game.
        assert_eq!(find(&games, r"C:\Steam\steamapps\common\dota 2 beta\UnityCrashHandler64.exe"), None);
        // A folder whose name only starts the same.
        assert_eq!(find(&games, r"C:\Steam\steamapps\common\dota 2 beta2\x.exe"), None);
    }

    #[test]
    fn folder_names_match_in_any_case_beyond_a_to_z() {
        let games = vec![KnownGame {
            name: "Şehir".into(),
            source: Source::Gog,
            dir: Some(r"D:\Oyunlar\ŞEHİR ÜSTÜ".into()),
            exes: vec![r"D:\Oyunlar\ŞEHİR ÜSTÜ\ÇÖĞÜŞ.exe".into()],
            steam_app: None,
        }];
        assert_eq!(find(&games, r"d:\oyunlar\şehİr üstü\bin\x.exe").map(|g| g.name.as_str()), Some("Şehir"));
        assert_eq!(find(&games, r"d:\oyunlar\şehİr üstü\çöğüş.exe").map(|g| g.name.as_str()), Some("Şehir"));
        assert!(same_path(r"D:\Oyunlar\ÖĞÜ\", r"d:/oyunlar/öğü"));
    }

    #[test]
    fn helpers() {
        assert!(is_helper("Steam.exe"));
        assert!(is_helper("unins000.exe"));
        assert!(is_helper("LeagueClientUx.exe"));
        assert!(is_helper("Riot Client.exe"));
        assert!(is_helper("RiotClientServices.exe"));
        assert!(is_helper("RiotClientCrashHandler.exe"));
        assert!(!is_helper("League of Legends.exe"));
        assert!(!is_helper("VALORANT-Win64-Shipping.exe"));
    }

    /// By hand: `cargo test -p kb-winsys -- --ignored --nocapture installed`
    /// lists how many games each launcher gave (no names).
    #[test]
    #[ignore]
    fn count_installed_games() {
        let all = installed();
        let mut counts = std::collections::BTreeMap::new();
        for g in &all {
            *counts.entry(g.source).or_insert(0) += 1;
        }
        println!("{counts:?}");
    }
}
