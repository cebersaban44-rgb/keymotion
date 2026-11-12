; Keymotion's additions to Tauri's NSIS installer (tauri.conf.json:
; bundle > windows > nsis > installerHooks).
;
; Tauri's uninstaller already removes the app, its shortcuts, its entry in
; Windows' installed apps, the "Run" value named after the product (start
; with Windows), and, when the user ticks "Also delete my settings and saved
; data" (lang/*.nsh, deleteAppData; unticked by default), the
; data folders. Keymotion keeps two more values under HKCU, removed here
; when the app is uninstalled (not when an update replaces it):
;
;   - Explorer\StartupApproved\Run\Keymotion: Windows' on/off switch for the
;     "start with Windows" entry (kb_winsys::startup);
;   - Software\Classes\AppUserModelId\dev.keymotion: the name and logo that
;     notifications show (kb_winsys::appid, notify.rs).

; This file is included before the pages are made, so the welcome page
; takes our plain title and text (K3) instead of NSIS's general advice about
; closing other programs. The strings are in lang/*.nsh
; (customLanguageFiles), Turkish and English.
!define MUI_WELCOMEPAGE_TITLE "$(kmWelcomeTitle)"
!define MUI_WELCOMEPAGE_TEXT "$(kmWelcomeText)"

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
  ${EndIf}
!macroend
