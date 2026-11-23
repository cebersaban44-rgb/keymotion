# Changelog

All notable changes to Keymotion (formerly Klavye Stüdyo) are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (0.x until 1.0).
Türkçe: [CHANGELOG.tr.md](CHANGELOG.tr.md).

From 1.0.0 on, each version is the same on GitHub and in the Microsoft Store,
with one list of changes for both. Windows shows the Store's copy with a
fourth number, always 0 (1.0.0 is 1.0.0.0). A version comes out in both the
same day: the GitHub release waits until Microsoft has reviewed the Store's
copy. Only an urgent fix may come out on GitHub first, and says so. Test
versions ("-rc") are only on GitHub. A change that concerns only one of them
says so.

## [Unreleased]

### Added

- **Music mode:** Tools › **Music mode** moves the key lights with the
  sound your PC plays, over USB: **Bars** (columns from bass on the left to
  treble on the right, filling from the bottom row, with a mark that holds
  at the top), **Beat** (the whole keyboard flashes on each beat, a ring
  spreads from the middle on a strong beat), **Level** (a band of loudness
  opening from the middle) and **Wave** (each bass beat sends a wave across
  the keys). Colors **Green-yellow-red**, **Rainbow**, **One color** or
  **Two colors**; **Center-out** for Bars and Wave; sensitivity from 50% to
  200%; the other keys dark or dim. A few seconds after the sound stops,
  the keyboard's own light comes back (or every key dim or dark, as you
  choose). The lights wait while the focus timer runs, and while a game is
  in front (**Pause while playing a game**, on at first). Keymotion reads
  only how loud the sound going to your speakers or headphones is right
  now, through Windows' own sound system: nothing is recorded, and the
  microphone isn't used. Also in the tray menu and in backups.
- **Game session:** Tools › **Game session** notices when you play. Games
  come from your launchers' lists (Steam, Epic Games, GOG, Xbox, Riot Games,
  Xbox Game Bar) and from your own word (**Add a game**, **Not a game**,
  **Undo**); a program that fills the screen for a few minutes is offered as
  "Is it a game?". A session starts once a game
  has been in front for 15 seconds and lasts while the game runs, Alt+Tab
  included. With the live screen on, a card goes to the keyboard's screen
  when it starts and a summary (time played, graphics card load and
  temperature, processor load and peak, today's total) when a session of 5
  minutes or more ends; nothing new comes up while you play. The keys the
  game uses can light up (WASD, MOBA, 1–6 or your own), or a profile's
  key-by-key lighting, without saving anything to the keyboard; the lights
  follow the window, so they wait while you Alt+Tab to other work. The games
  list shows each program's icon and the profile it gets, and sorts by last
  or most played; a game without a Steam picture gets its icon on the start
  card. A meeting's or reminder's card that came up during a game goes away
  after the game. The history stays on this computer. "Update automatically"
  waits for the game to end.
- **League of Legends lights:** during a match, from the data the game
  offers on your computer: your health on the whole keyboard, receding the
  way you choose as it drops (as with the focus timer), or on 1–0; mana on
  F1–F12; dark red when you die, the light coming back as respawn nears (on
  1–0, the time left counting down); and an animation of your choice (wave,
  flash, pulse, sparkle, ring, fade from the top) in your color for victory,
  defeat, level ups, kills, an ace, dragons, Baron and Herald. A drawing on the page previews them with the
  match's own code, and "Try on the keyboard" plays them. They're set up
  under League of Legends in the games list (Special lights), grouped as a
  match brings them; dragons, Herald and Baron light up only when your team
  takes them, unless you turn that off. The summary card shows the result,
  K/D/A and CS. No enemy timers or objective timers.
- **Profiles by app:** Profiles › **By app** links a program, or every game,
  to a profile. By default only a per-key design's lighting changes, without
  saving anything to the keyboard; a rule can also save the key mapping and
  macros, at most 20 times a day (the last one is kept for going back; a
  profile for another keyboard model is skipped and not counted). A game's
  key mapping and macros stay for the
  whole game; when it ends, the keyboard goes back to what it had before (or
  to a profile you choose, or stays), unless you changed it by hand meanwhile.
  Rules still hold after a program updates (Microsoft Store apps, Discord,
  Roblox). The tray menu can pause it.
- **Send nothing to the keyboard while playing:** during a game Keymotion
  sends no lights, cards or profiles to the keyboard, for anti-cheat software
  that might mind; the keyboard keeps its own light. Meeting and reminder
  alerts still come unless you turn that off, and quiet mode can be on for a
  single game.
- **Terms of use:** the site has a Terms of use page.
- **Russian and Simplified Chinese:** the app, the user guide, the installer
  and the README are now also in Russian (Русский) and Simplified Chinese
  (简体中文). Keymotion starts in the language Windows uses when it is one
  of the four, and **Settings** › **Language** changes it at once. The
  translations were made with AI help and wait for a review by native
  speakers: corrections are welcome. In Chinese the keyboard is called
  黑爵 AK980 V2（带屏版）, the name it is sold under in China.
- **AI coding agents (in the works, not in the installer yet):**
  `keymotion-cli` lets Claude Code, Codex and scripts light the keys: a
  finished task, a question waiting for you, an error, a progress bar, or
  anything an agent makes of keys, colors and effects. `keymotion-cli
  connect claude-code` or `connect codex` adds a few hooks and a short skill
  to that app's settings (showing what changes first, keeping a copy, never
  touching anything else); then the keyboard lights by itself when the agent
  finishes, waits for your approval (until you answer or touch a key),
  fails, or stops at its usage limit (slow purple), and with two or more
  sessions the waiting one's number lights on the number row. When
  Keymotion is not running, such an event starts it, unless you quit it
  yourself or turned agent access off. Nothing you or the agent writes
  reaches Keymotion: only what happened. `disconnect` takes it all away
  again. Agent access stays off until you turn it on; on the GitHub copy,
  turning it on also puts `keymotion-cli` on your PATH for new terminals.
  Tools › **AI agents** turns it on in two steps (what will be added to
  Claude Code and Codex, then what was done, with Cancel on the first),
  shows how each app stands, and sets the five signals' color, effect and
  length with **Try on the keyboard**, what else agents may do, the extra
  signals, whether agents may change these settings, the session numbers
  and the recent actions, with **Undo** and, for a request that wasn't
  carried out, why. The privacy policy and the security policy say what
  agents pass on and what they can never change.
  Three more things only with their own permission, each off at first:
  agents can start, pause or stop the focus timer (also one block of their
  own length, without changing your settings), show a profile's lighting
  for a while or write it to the keyboard (its lighting only, at most 10
  writes in any 24 hours for all agents together), and put a card with text
  or a picture on the screen (at most 4 an hour for all agents together, as
  you set it; your own picture comes back after it). With screen cards
  allowed, a coding task that took 10 minutes or more says "Done · 14 min"
  on the screen when it ends.

### Changed

- **A new look, in the style of Windows 11:** the pages are listed in a
  pane on the left as in Windows Settings, with the connected keyboard at
  its top (its name and how it is connected, or "Waiting for a keyboard"),
  a search box that narrows the list, and Settings at the bottom. Settings
  are full-width rows with the switch, list or button on the right;
  buttons, switches, sliders, tabs, dialogs and the save bar look like
  Windows' own, and orange stays only as the accent color. The keyboard
  drawing and its lights stay as they were. **Overview** starts with the
  keyboard's glass strip (its screen, the picture on the glass and the
  knob with the brightness it was last set to), then the keyboard, then
  one row for each thing Keymotion does; reminders and the system gauges
  can be switched on and off right there.
  **Lighting** shows the effects as a gallery and their settings as rows,
  with **Direction** as a list. **Screen** shows the GIF page large and the
  clock page beside it; the library is a picture grid with one command
  bar: click a picture to choose it, then **Upload**, **Match lights**,
  **Rename** or **Delete**. **Image or video**, **Text card** and
  **Slideshow** are in the same bar, and a file can be dropped on the
  library. Settings, dialogs and the other pages follow the same look.
- The list of running programs, used to find the official software or
  SignalRGB before a write, no longer opens any program.
- **Recognition:** only a keyboard whose USB name includes "AK980 PRO", as
  the tested one reports, is taken for the AJAZZ AK980 PRO. Another AK980
  with the same USB identity, for example one without the screen, is now
  offered **Request support** instead.
- **AJAZZ's new firmware:** an AK980 PRO on AJAZZ's firmware 769 (3.01),
  which uses a different set of commands, no longer shows as just a keyboard
  that isn't supported yet. The welcome page, the pane on the left and the
  tray menu say it has AJAZZ's new firmware, that Keymotion sends nothing to
  it and that support is planned for a later update, with a link to the FAQ.
  **Request support** stays.
- **Untested firmware:** on a firmware version that hasn't been tested with
  Keymotion, the keyboard clock waits for your go-ahead too, both the sync
  on connect and **Sync now**. Keymotion asks the same question as before
  saving, and a yes holds until it closes. Live lighting still goes on
  without asking.

### Fixed

- **Calendar and reminders:** on the night the clocks go back, a reminder
  at 03:00 (in most of Europe; elsewhere, the hour the change ends) went off
  an hour early, and a calendar event at that time with no time zone of its
  own showed an hour early.
- **Calendar:** an event repeating at midnight no longer goes missing on
  the day a clock change skips or repeats midnight (Chile, Cuba and a few
  other places). A repeat on, say, the fifth-last Monday of a month now
  shows all its dates, not just the first. A weekly repeat whose rule says
  "the first Monday" comes on Mondays only, not every day.
- **2.4 GHz receiver:** with only the receiver plugged in, Keymotion no
  longer says your keyboard works. The Connection page and the Overview say
  that the receiver is plugged in and that Keymotion can't tell yet whether
  the keyboard is turned on, and the dot next to it is grey. With the
  keyboard on Bluetooth and the receiver still plugged in, the app and the
  tray menu show Bluetooth, not the receiver.
- A keyboard with the AK980 PRO's USB identity and name but built
  differently inside showed up as an AK980 PRO on which every action failed.
  It now gets **Request support**, and nothing is sent to it.

## [1.1.0] - 2026-11-16

Keymotion now tells you when a new version is out, and can install it by
itself. It is in the Microsoft Store too.

### Added

- **Microsoft Store:** Keymotion is in the Microsoft Store since
  16 November 2026, signed by Microsoft, so Windows installs it without a
  warning. The release page, the README and the site show Microsoft's
  "Get it from Microsoft" badge beside the GitHub download.
- **Update notifications:** when a new version is out, a Windows
  notification says so, once for each version, with **Update now** and
  **Later**. The menu of the Keymotion icon next to the clock starts with
  **Update available** and the icon shows a small dot; a card at the bottom
  left of the window and a dot next to **Settings** say it too. Before, a
  new version only showed inside the window, so it went unnoticed while
  Keymotion ran next to the clock.
- **Update automatically:** Settings › About › **Updates** now has three
  choices instead of one switch. **Update automatically** downloads a new
  version and installs it while the computer isn't in use, never during a
  write to the keyboard, while the focus timer runs or with changes not
  saved to the keyboard. **Let me know** (the default) tells you and lets
  you choose when; **Off** never looks by itself. If the old switch was off,
  the choice is **Off**.
- After an update, a Windows notification says which version is installed,
  with **What's new**, and Keymotion opens the way it was: with its window,
  or next to the clock.

### Changed

- **Both copies installed:** when Keymotion from GitHub and from the
  Microsoft Store are both on a computer, the GitHub copy now opens the
  Store's copy instead, and starting with Windows moves to the Store's copy.
  Before, both started with Windows and whichever came first ran. The Store's
  copy shows a "GitHub copy" row in Settings › About with a button to
  Windows' Installed apps, where the GitHub copy can be removed; the settings
  stay, as both copies share them.
- Started with Windows a second time while Keymotion already runs, it no
  longer brings the window forward.

### Fixed

- **Microsoft Store:** when the Store installs a new version, Windows closes
  the running Keymotion. It now opens again by itself, next to the clock.
  Before, it stayed closed until the next sign-in.

## [1.0.0] - 2026-11-15

Keymotion 1.0. The app is the same as 0.1.0, ready for the Microsoft Store.
The Microsoft Store takes no version number that starts with 0, so this
release is 1.0.0.

### Added

- **Microsoft Store edition:** the same app, packaged for the Microsoft
  Store, where Microsoft signs it, so Windows installs it without a warning.
  Installed from the Store, Keymotion is updated by the Store (its own update
  check stays off), starts with Windows through Windows' startup settings,
  and its notifications carry the Store app's name and logo. The version on
  GitHub works as before.

## [0.1.0] - 2026-11-12

The first release. It supports the AJAZZ AK980 PRO on Windows.

### Added

- **Connection:** detects the keyboard over USB, the 2.4 GHz receiver and
  Bluetooth, and notices when it is plugged in or removed. A Connection page
  shows what can be done in each mode; settings need the USB cable. Battery
  level over Bluetooth.
- **Overview:** the measured 97-key layout across the page with every key's
  letters, then the picture the keyboard's screen shows now and one tile for
  each tool: the light, the focus timer
  (start it from here), the next reminder, the next meeting, the system
  gauges with live bars, and the profile. Also connection mode, battery, and
  the keyboard clock, which is set automatically when the keyboard connects.
- **Lighting:** the 19 built-in effects with color, brightness, speed and
  direction; a live mirror of the keyboard's colors; per-key color with brush
  and selection tools, ready-made patterns, live preview and saving to the
  keyboard.
- **Color calibration:** gamma and white balance, with test patterns shown on
  the keyboard.
- **Key assignment:** base and Fn layers. Keys, shortcuts, media keys, mouse
  buttons, scrolling or nothing; a layer can be reset to the factory layout.
- **Macros:** recorded by pressing keys, edited step by step (delays, order,
  mouse clicks), assigned to keys with a play mode; shows how much keyboard
  memory is left.
- **Screen studio** for the 240×135 screen: images, GIFs and videos (crop,
  length), text cards and slideshows; preview before upload, a library, and a
  way back to the factory animation. "Match lights" designs key colors from
  the picture.
- **Live screen:** cards that update on their own (the song that is playing,
  focus timer phases, upcoming meetings), with a daily upload limit, a clear
  order of what is shown, and a return to your own picture afterwards. The
  focus timer and the song leave the last upload for reminders and meetings. A
  card that waits for the limit is shown as waiting, apart from what the
  keyboard shows, and "Don't send" drops it.
- **Calendar:** Google, Outlook and iCloud calendars through their share
  address (read only); a card on the screen before a meeting and a blinking key
  a minute before it starts.
- **Reminders:** once, daily or at intervals during working hours; a wave of
  color on the keys, and optionally a screen card and a sound. "Try" shows the
  card too, and the page says why a card was not shown (the upload limit, the
  live screen off, no USB cable, a meeting card).
- **System gauges:** processor, graphics and memory load as bars on key rows,
  volume on the function keys, microphone mute on a chosen key.
- **Focus timer:** a sequence you build from focus and break blocks of any
  length, in any order (drag to reorder), run once, a few times or until you
  stop it; "25 / 5 classic" by default, and older settings become the
  sequence they made. The whole keyboard fills with the block's color and
  empties as time runs out; a card on the screen for each block.
- **Installer:** a per-user Windows installer in English and Turkish, with the
  Keymotion icon and a plain welcome text; no administrator rights needed.
  Uninstalling keeps your settings unless you tick "Also delete my settings and
  saved data", and removes the start-with-Windows entry and the notification
  name.
- **Reporting problems:** Settings › About › Feedback, a calm notice after an
  unexpected error, and a one-time notice after a crash open a report window
  that shows what will be sent first. "Report on GitHub" copies it and opens
  the project's issue form in the default browser; the app sends nothing by itself. A SONiX or AJAZZ
  keyboard that isn't supported yet gets "Request support", with its USB ids
  and HID descriptions read from Windows (nothing is written to it).
- **Keymotion animation:** an animation of its own for the keyboard's screen,
  built into the library: the logo's "K" drawn stroke by stroke, then the
  wordmark (37 frames, about 1.5 minutes to upload). It is offered when there's
  no backup of the factory animation.
- **Tray:** closing the window keeps Keymotion running next to the clock,
  so the focus timer, reminders, calendar cards, the live screen and the key
  gauges go on. The tray menu shows the keyboard and its battery, runs the
  focus timer (phase and minutes left, pause, skip, stop), shows the next
  reminder and turns reminders and the key gauges on or off. It also opens a
  profile's load confirmation and quits the app. Settings › Closing and starting can turn
  this off, and can start Keymotion hidden when Windows starts. The first time
  the window is closed after the app starts, a notice says it is still running.
- **Updates:** Settings › About checks GitHub for a new version, and the app
  looks by itself once a day when it opens (it can be turned off). A new
  version shows a strip above the page; nothing installs until you choose
  "Install and restart". The download is checked against the app's key, and
  installing waits for a write to the keyboard to finish.
- **Launch animation:** when the window opens, the logo's "K" is drawn stroke
  by stroke and the name opens, then the logo moves to its place in the
  sidebar (about a second, while the app gets ready). It does not play when
  Keymotion starts hidden with Windows or comes back from the tray, a click or
  a key ends it, and with Windows' animation effects off it simply fades.
- Protection for unsaved changes when leaving a page or closing the app.
- Turkish and English interface, light and dark theme.
- Project: MIT/Apache-2.0 license, NOTICE (unofficial project, use at your own
  risk), third-party license notices, and automatic checks for broken
  characters, vendor files, versions and dependencies.
- **Documentation:** a user guide in English and Turkish at
  cebersaban44-rgb.github.io/keymotion, with every page of the app, help when
  something goes wrong, the supported keyboards and the ones that could come
  next, and pages for developers. Settings › About opens it.

[Unreleased]: https://github.com/cebersaban44-rgb/keymotion/compare/v1.1.0...develop
[1.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.1.0
[1.0.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v1.0.0
[0.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0
