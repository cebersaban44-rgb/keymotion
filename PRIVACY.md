# Privacy policy

*Türkçe: [Gizlilik politikası](https://cebersaban44-rgb.github.io/keymotion/tr/about/privacy/).* This file is the same text as the [privacy policy on the website](https://cebersaban44-rgb.github.io/keymotion/about/privacy/).

Keymotion is a desktop app that controls your keyboard (lighting, key mapping, macros, the screen and the clock) and shows information on it. Everything it does happens on your computer.

## No usage data

- Keymotion collects no usage data, statistics or crash reports, and sends nothing about you or your computer anywhere.
- There's no account and no sign-in.
- Log files and crash notes stay on your computer (see [Where your data lives](#where-your-data-lives)).

## When Keymotion goes online

Keymotion connects to the internet for two things only: the calendars you add, and the update check. It makes no other network connections.

### Calendars you add

- Keymotion reads each calendar address you add, every 15 minutes and whenever you ask. It keeps doing so while it runs, also when its window is closed and it runs next to the clock.
- It only reads the calendar. Nothing else is sent to the calendar server.
- It uses Windows' own internet connection, so your system's proxy and certificate settings apply.
- Only secure `https://` addresses are accepted. `webcal://` addresses are read as `https://`.

### The update check

- **When.** Unless **Updates** is set to **Off** in **Settings**, under **About** (it starts as **Let me know**), Keymotion looks at most once a day: a little after it starts, and again each day while it keeps running. **Check for updates** in the same place does the same at any time. With **Off**, Keymotion looks only when you click that button.
- **What it reads.** A small file, `latest.json`, from the project's release page on GitHub. The request goes to `github.com`, and the file comes from GitHub's download server.
- **What it sends.** An ordinary download request. Apart from the updater component's name and version (`tauri-plugin-updater`), it holds nothing about you: no account, no usage data, no settings. Like any server, GitHub sees the IP address the request comes from.
- **When a new version is found.** With **Let me know**, Keymotion asks you; with **Update automatically**, it downloads the new version and installs it while the computer isn't in use. Either way the installer comes from GitHub and its signature is checked with the key built into Keymotion. If it doesn't match, it isn't installed.
- **Test versions.** Versions with a suffix in their number, such as "-rc", also read GitHub's public list of releases (`api.github.com`), to see test releases too. Normal versions don't read that list.
- **Microsoft Store version.** Keymotion installed from the Microsoft Store never makes this check: the Store updates it.

## Other companies' privacy policies

Keymotion itself sends nothing about you. When you use the services below, they see an ordinary request, and their own privacy policies apply:

- **GitHub**: the update check, downloads, and reporting a problem or asking for support. See the [GitHub General Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
- **Microsoft**: on a computer without WebView2, the part of Windows that draws the app's window, the installer downloads it from Microsoft and installs it. Windows 11 already has it. See the [Microsoft Privacy Statement](https://www.microsoft.com/privacy/privacystatement).
- **The service of each calendar you add**, for example [Google](https://policies.google.com/privacy), [Microsoft Outlook](https://www.microsoft.com/privacy/privacystatement) or [Apple iCloud](https://www.apple.com/legal/privacy/).

## What Keymotion reads on your computer

These readings are used only to show things on the keyboard and in the app. They aren't stored, except where noted, and never leave your computer.

| What | When | Why |
|---|---|---|
| The song playing (title, artist, cover, the playing app), from Windows | While the live screen and its **Now playing** source are on | The song card on the keyboard's screen |
| Processor, graphics and memory load, from Windows' performance counters | While system gauges are on, or their page is open | The bars on the keys |
| Speaker volume and mute, microphone mute, from Windows' sound system | While system gauges are on, or their page is open | The volume bar and the mute light |
| How loud the sound your PC plays is right now, from Windows' sound system (the mixed sound going to your speakers or headphones; never the microphone) | While music mode is on, or its page is open | The music lights on the keys. The sound isn't recorded, stored or sent anywhere; only its loudness is kept, for a moment |
| Time since you last used the keyboard or mouse | While reminders are on, or agent access is on | Repeating reminders wait while you're away; an agent's waiting light goes off at your first touch, and what happened while you were away flashes when you're back |
| Which program is in front: its file name and folder, and whether its window fills the screen. Never window titles, and no program is opened | While **Notice games** or profiles **By app** are on | Telling a game from other programs, and the profile for each program |
| A program's icon, from its file (the program isn't opened) | When a list of games or programs shows it, and when a game without a Steam picture (or with one that can't be read) starts | The icons in the lists and on the start card |
| Your launchers' game lists (Steam, Epic Games, GOG, Xbox, Riot Games, Xbox Game Bar) | While **Notice games** is on, every few minutes | Telling which programs are games |
| Processor and graphics load and the graphics card's temperature | While a game session runs | The summary card and the history |
| League of Legends' match data (your health, mana, level, score, the match's events, and which players are on your team, by the names the scoreboard shows), from the service the game runs on your computer | During a League of Legends match, while its lights are on | The lights and the summary card |
| Your Windows keyboard layout | When the app opens or comes to the front | Key labels in your language |
| Connected keyboards (USB devices, Bluetooth battery level) | While the app runs | Finding and controlling the keyboard |
| A keyboard's HID descriptions (what Windows recorded about it) | When you ask for support for a keyboard that isn't supported yet | The support request |
| Windows and WebView2 versions | When you copy diagnostics or report a problem | The diagnostics text |

## AI coding agents

This part works only after you turn on **Agents may use the keyboard** under **Tools › AI agents**, and it all stays on your computer.

- **What the coding apps pass on.** A connected Claude Code or Codex runs `keymotion-cli`, Keymotion's command-line tool, when something happens: a task finished, a question waits, an error. Of what the coding app hands it, keymotion-cli passes on only the event's name, the session's id (as a short code), the name of the project folder the agent works in (the last folder only, shown beside the session's number), a notification's type, the kind of a failure and a helper agent's type. Your prompts, the agent's messages, the conversation's transcript, and what a tool was given or returned are never passed on, logged or kept. To pair a question with its answer, a tool call is known only by a code computed from it on your computer (a hash), from which the call can't be read back.
- **Nothing leaves your computer.** keymotion-cli talks to Keymotion through a connection that only your Windows user on this computer can open; nothing goes over the network. Keymotion sends nothing about agents anywhere.
- **What Keymotion keeps.** The agents' settings, in `agents.json`. **Recent actions** (which agent asked for what and when, the result, and the settings it changed) is kept only while Keymotion runs. The log files get at most the agent's name, the command and the result.
- **What Keymotion writes in the coding apps' folders.** Only after you approve the list the **AI agents** page shows you; **Remove connection** takes it away again.
  - Claude Code, in `%USERPROFILE%\.claude\`: its hooks in `settings.json`, and the skill in `skills\keymotion\SKILL.md`.
  - Codex, in `%USERPROFILE%\.codex\`: its hooks in `hooks.json`, and a rule in `rules\keymotion.rules` that lets the agent's keyboard commands run outside Codex's sandbox. The skill goes in `%USERPROFILE%\.agents\skills\keymotion\SKILL.md`.
  - Before a file is changed, a copy of it as it was is kept beside it (`settings.json.before-keymotion`, `hooks.json.before-keymotion`). Each skill folder also holds `connection.json`, Keymotion's record of what it added, and, while you've quit Keymotion or turned agent access off, `stopped.json`, which tells the hooks not to start it.
  - Your own settings in those files stay as they are. When Keymotion starts, it updates only its own skill file and Codex rule, and only when they're older than it.
- **PATH, Keymotion from GitHub only.** Connecting also adds Keymotion's `bin` folder to your user PATH, so terminals find `keymotion-cli` by its name. The value as it was before is kept in the registry, under `HKCU\Software\Keymotion\UserPath`. Uninstalling Keymotion removes the folder again if Keymotion added it.

## Where your data lives

Everything is in your Windows user folder. In **Settings**, under **About**, **Data folder** and **Log files** open these folders.

- **Settings and records**, in `%APPDATA%\dev.keymotion\`:
  - `calibration.json`, `clock.json`, `lighting.json`, `pomodoro.json`, `gauges.json`, `music.json`, `reminders.json`, `battery.json`, `profiles.json`, `background.json`, `updates.json`, `games.json`, `agents.json`;
  - `auto-profiles.json`: your profiles **By app** and, after a profile was saved to the keyboard for a program, a copy of the keyboard's lighting, key mapping and macros from before, to go back to;
  - `game-history.json`: your game sessions (game, start and end, the readings, a League of Legends result). **Delete history** on the **Game session** page empties it;
  - `keymap.json` and `macros.json`: Keymotion's only record of what it wrote to the keyboard, because the keyboard can't report them;
  - `calendar.json`: your calendars. Their addresses work like passwords, so they're encrypted for your Windows user account (Windows Data Protection). Another user or another computer can't read them;
  - `screen\`: your picture library, the copy of the factory animation if you made one, and the last live card.
- **Log files**, in `%LOCALAPPDATA%\dev.keymotion\logs\`: one file per day for the last 7 days, at most 4 MB each, and short crash notes. Before anything is written, web addresses are cut to their server name and your Windows user name is removed from folder paths. Reminder titles and meeting names are never written.
- **Interface preferences**, in `%LOCALAPPDATA%\dev.keymotion\EBWebView\`: theme, language and edits you haven't saved yet, kept by the app's window.

Keymotion installed from the Microsoft Store uses the same folders. If they weren't there before (Keymotion from GitHub was never installed), Windows keeps them in the app's own folder, `%LOCALAPPDATA%\Packages\<the app's package name>\LocalCache\`, and deletes them when you uninstall the app; the buttons in **Settings** then open that folder.

## Diagnostics

In **Settings**, under **About**, **Copy** next to **Diagnostics** puts a text on your clipboard: the app, Windows and WebView2 versions, the keyboard model, its firmware and connection, the names and sizes of the settings files, and the last 150 lines of the log (cleaned as described above). It isn't sent anywhere. You decide whether to paste it into a message.

## Reporting a problem or asking for support

You report problems on GitHub yourself. Keymotion sends nothing by itself.

- **Report a problem** (in **Settings**, under **About** › **Feedback**; in the notice after an unexpected error; in the notice after a crash) opens a window that shows the text first: the diagnostics above, with what happened at the top. **Report on GitHub** puts the text on your clipboard and opens the project's form on GitHub in your browser. You paste it there and decide whether to send it.
- **Request support** appears when a SONiX or AJAZZ keyboard isn't supported yet. It works the same way, with the keyboard's details: its USB identifiers, its name and the HID descriptions Windows keeps of it. Nothing is written to the keyboard. Its serial number and device paths are left out.
- **After a crash**, the next start offers the newest crash note from the last 7 days, once.

## Export, import and deletion

- **Export.** In **Settings**, **Export** under **Backup** saves one JSON file in your Downloads folder, with your settings, key mapping and macro records, reminders, calendar settings, lighting and picture library. Calendar addresses are included only if you turn on **Include calendar addresses**. They're then in plain text in the file, so keep it private.
- **Import.** **Choose file** under **Backup** shows what the file holds and what it will replace before anything changes. Importing writes nothing to the keyboard: key mapping, macros and lighting open as unsaved changes for you to save.
- **Deletion.** Uninstall Keymotion and tick **Also delete my settings and saved data**, or delete the folders above yourself after uninstalling. You can also delete single files while Keymotion is closed; it then starts again with defaults.

## What stays on the keyboard

The keyboard keeps some settings in its own memory. They stay after you uninstall Keymotion or connect the keyboard to another computer:

- key mapping and macros;
- the lighting effect or per-key color design;
- the picture on the screen;
- the clock.

To clear them before you give the keyboard away, reset the key mapping, delete the macros and save, then upload a neutral picture with Keymotion. See [If something goes wrong](https://cebersaban44-rgb.github.io/keymotion/help/recovery/).

How Keymotion's releases are built and checked is on the [code signing policy](https://cebersaban44-rgb.github.io/keymotion/about/code-signing/) page.
