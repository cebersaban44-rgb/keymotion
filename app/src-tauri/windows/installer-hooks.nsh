; Keymotion's additions to Tauri's NSIS installer (tauri.conf.json:
; bundle > windows > nsis > installerHooks).
;
; Tauri's uninstaller already removes the app, its shortcuts, its entry in
; Windows' installed apps, the "Run" value named after the product (start
; with Windows), and, when the user ticks "Also delete my settings and saved
; data" (lang/*.nsh, deleteAppData; unticked by default), the
; data folders. Keymotion keeps three more entries under HKCU, removed here
; when the app is uninstalled (not when an update replaces it):
;
;   - Explorer\StartupApproved\Run\Keymotion: Windows' on/off switch for the
;     "start with Windows" entry (kb_winsys::startup);
;   - Software\Classes\AppUserModelId\dev.keymotion: the name and logo that
;     notifications show (kb_winsys::appid, notify.rs);
;   - Software\Classes\keymotion: the app's own address scheme, which the
;     buttons on its notifications open (kb_winsys::protocol, updates.rs).
;
; keymotion-cli, the command-line tool AI coding agents and scripts use
; (crates/keymotion-cli), is installed as $INSTDIR\bin\keymotion-cli.exe: a
; Tauri resource that tauri.bundle-cli.conf.json adds to a release build
; (release.yml builds the tool first, from the same commit and version). Its
; own folder, so that putting it on PATH does not put uninstall.exe there
; too. The app puts that folder on the user's PATH when the user turns
; agent access on (kb_winsys::user_path, recorded under
; Software\Keymotion\UserPath).
;
; When Keymotion is uninstalled (not when an update replaces it), before its
; files go, keymotion-cli takes away what it added:
;   - its connections to Claude Code and Codex (hooks, skill, Codex rule),
;     only those that run this copy (--if-this-copy: the Microsoft Store
;     copy's stay). Claude Code would otherwise report a hook error on every
;     event, as it runs the program it was given and cannot check first;
;   - its folder on PATH, only if Keymotion put it there (`path remove`).
; Each runs hidden, waits at most 15 seconds, and never stops the uninstall:
; its result is ignored. Updates leave PATH and the connections as they are.
;
; Tauri's own "is Keymotion running" check asks Windows' Restart Manager
; about $INSTDIR\keymotion.exe, the file itself: a keymotion-cli running from
; bin\ is never taken for the app, nor the app for it.

; This file is included before the pages are made, so the welcome page
; takes our plain title and text (K3) instead of NSIS's general advice about
; closing other programs. The strings are in lang/*.nsh
; (customLanguageFiles): English, Turkish, Russian and Simplified Chinese.
!define MUI_WELCOMEPAGE_TITLE "$(kmWelcomeTitle)"
!define MUI_WELCOMEPAGE_TEXT "$(kmWelcomeText)"

; Sharp images at every display scale (K15): the welcome and finish pages'
; left image and the top band are drawn at their boxes' real size.
!include "${__FILEDIR__}\installer-images.nsh"

; A keymotion-cli that runs (a coding agent's hook waiting for the app to
; start) cannot be replaced, but Windows lets it be renamed: the update then
; writes the new one beside it. The old one is deleted here next time, or by
; the uninstaller.
!macro NSIS_HOOK_PREINSTALL
  Delete "$INSTDIR\bin\keymotion-cli.old.exe"
  ${If} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
    Delete "$INSTDIR\bin\keymotion-cli.exe"
    ${If} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
      Rename "$INSTDIR\bin\keymotion-cli.exe" "$INSTDIR\bin\keymotion-cli.old.exe"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" disconnect claude-code --yes --if-this-copy'
    Pop $0
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" disconnect codex --yes --if-this-copy'
    Pop $0
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" path remove'
    Pop $0
  ${EndIf}
  Delete "$INSTDIR\bin\keymotion-cli.old.exe"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
    DeleteRegKey HKCU "Software\Classes\keymotion"
    ; `path remove` took its values away; the key, if nothing else is in it.
    DeleteRegKey /ifempty HKCU "Software\Keymotion\UserPath"
  ${EndIf}
!macroend
