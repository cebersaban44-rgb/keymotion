# Privacy

This desktop app controls your keyboard (lighting, key mapping, macros, the screen and the clock) and shows information on it. Everything it does happens on your computer. A Turkish version of this page is in [docs/gizlilik.md](docs/gizlilik.md).

## No telemetry

- The app collects no usage data, statistics or crash reports, and sends nothing about you or your computer anywhere.
- There is no account and no sign-in.
- Logs and crash notes stay on your computer (see [Where your data lives](#where-your-data-lives)).

## When the app uses the network

- **Calendars you add.** The app reads each calendar address you add, every 15 minutes and when you ask. It uses the Windows HTTP client, so your system proxy and certificates apply. Only `https://` (and `webcal://`, read as `https://`) addresses are accepted. Nothing else is sent to the calendar server.
- **Updates (planned).** A future version will check for app updates. This page will say what that check sends before it ships.

The app makes no other network connections.

## What the app reads on your computer

These readings are used only to draw on the keyboard and in the app. They are not stored, except where noted, and never leave your computer.

| What | When | Why |
|---|---|---|
| Now playing (title, artist, cover, the playing app) from the Windows media session | While Live mode and its "Now playing" source are on | The song card on the keyboard screen |
| Processor, graphics and memory load (performance counters) | While System gauges are on, or their page is open | The gauge bars on the keys |
| Speaker volume and mute, microphone mute (Windows audio) | While System gauges are on, or their page is open | The volume bar and the mute light |
| Time since your last keyboard or mouse input | While Reminders are on | Repeating reminders wait while you are away |
| Your Windows keyboard layout | When the app opens or gets focus | Key labels in your language |
| Connected keyboards (USB devices, Bluetooth battery) | While the app runs | Finding and controlling the keyboard |
| Windows and WebView2 versions | When you copy diagnostics | The diagnostics report |

## Where your data lives

The folder names contain the app's identifier. **Settings → About → Data folder** and **Log files** open them.

- **Settings and records** (`%APPDATA%\<app identifier>\`):
  - `calibration.json`, `clock.json`, `lighting.json`, `pomodoro.json`, `gauges.json`, `reminders.json`, `battery.json`;
  - `keymap.json` and `macros.json`: the app's only record of what it wrote to the keyboard (the keyboard cannot report them);
  - `calendar.json`: your calendars. Their addresses work like passwords, so they are encrypted for your Windows user account (Windows Data Protection); another user or computer cannot read them;
  - `screen\`: your picture library and the last live card.
- **Logs** (`%LOCALAPPDATA%\<app identifier>\logs\`): one file per day, the last 7 days, at most 4 MB each, and short crash notes. Web addresses are cut to their host name and your Windows user name is removed from paths before anything is written. Reminder titles and meeting names are never logged.
- **Interface preferences** (`%LOCALAPPDATA%\<app identifier>\EBWebView\`): theme, language and edits you have not saved yet, kept by the app's web view.

## Diagnostics

**Settings → About → Diagnostics → Copy** puts a text report on your clipboard: app, Windows and WebView2 versions, the keyboard model, firmware and connection, the names and sizes of the settings files, and the last 150 log lines (cleaned as described above). It is not sent anywhere; you decide whether to paste it into a message.

## Export, import and deletion

- **Export.** **Settings → Backup → Export** saves one JSON file in your Downloads folder with your settings, key mapping and macro records, reminders, calendar settings, lighting and picture library. Calendar addresses are included only if you turn on **Include calendar addresses**; they are then in plain text in the file, so keep it private.
- **Import.** **Settings → Backup → Choose file** shows what the file holds and what it will replace before anything changes. Import writes nothing to the keyboard: key mapping, macros and lighting open as unsaved changes for you to save.
- **Deletion.** Uninstall the app, then delete the folders above. You can also delete single files while the app is closed; the app starts again with defaults.

## What stays on the keyboard

The keyboard stores some settings in its own memory and keeps them after you uninstall the app or move it to another computer:

- key mapping and macros;
- the lighting effect or per-key design;
- the picture on the screen;
- the clock.

To clear them before you give the keyboard away, set the key mapping back to default, delete the macros and save, and upload a neutral picture with the app.
