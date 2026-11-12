# Changelog

All notable changes to Keymotion (formerly Klavye Stüdyo) are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (0.x until 1.0).

## [Unreleased]

The first release is in preparation. It supports the AJAZZ AK980 PRO on Windows.

### Added

- **Connection:** detects the keyboard over USB, the 2.4 GHz receiver and
  Bluetooth, and notices when it is plugged in or removed. A Connection page
  shows what can be done in each mode; settings need the USB cable. Battery
  level over Bluetooth.
- **Overview:** the measured 97-key layout, a preview of the keyboard's screen,
  connection mode, battery, and the keyboard clock, which is set automatically
  when the keyboard connects.
- **Lighting:** the 20 built-in effects with colour, brightness, speed and
  direction; a live mirror of the keyboard's colours; per-key colour with brush
  and selection tools, ready-made patterns, live preview and saving to the
  keyboard.
- **Colour calibration:** gamma and white balance, with test patterns shown on
  the keyboard.
- **Key assignment:** base and Fn layers. Keys, shortcuts, media keys, mouse
  buttons, scrolling or nothing; a layer can be reset to the factory layout.
- **Macros:** recorded by pressing keys, edited step by step (delays, order,
  mouse clicks), assigned to keys with a play mode; shows how much keyboard
  memory is left.
- **Screen studio** for the 240×135 screen: images, GIFs and videos (crop,
  length), text cards and slideshows; preview before upload, a library, and a
  way back to the factory animation. "Match lights" designs key colours from
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
  colour on the keys, and optionally a screen card and a sound. "Try" shows the
  card too, and the page says why a card was not shown (the upload limit, the
  live screen off, no USB cable, a meeting card).
- **System gauges:** processor, graphics and memory load as bars on key rows,
  volume on the function keys, microphone mute on a chosen key.
- **Focus timer:** a sequence you build from focus and break blocks of any
  length, in any order (drag to reorder), run once, a few times or until you
  stop it; "25 / 5 classic" by default, and older settings become the
  sequence they made. The whole keyboard fills with the block's colour and
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
  profile's load confirmation and quits the app. Settings › Background can turn
  this off, and can start Keymotion hidden when Windows starts. The first time
  the window is closed after the app starts, a notice says it is still running.
- **Launch animation:** when the window opens, the logo's "K" is drawn stroke
  by stroke and the name opens, then the logo moves to its place in the
  sidebar (about a second, while the app gets ready). It does not play when
  Keymotion starts hidden with Windows or comes back from the tray, a click or
  a key ends it, and with Windows' animation effects off it simply fades.
- Protection for unsaved changes when leaving a page or closing the app.
- Turkish and English interface, light and dark theme.
- Project: MIT/Apache-2.0 licence, NOTICE (unofficial project, use at your own
  risk), third-party licence notices, and automatic checks for broken
  characters, vendor files, versions and dependencies.

[Unreleased]: https://github.com/cebersaban44-rgb/keymotion/commits/develop
