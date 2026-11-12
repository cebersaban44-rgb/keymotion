; Keymotion's additions to Tauri's NSIS installer (tauri.conf.json:
; bundle > windows > nsis > installerHooks). docs/yol-haritasi.md, Aşama 5.
;
; Tauri's uninstaller already removes the app, its shortcuts, its entry in
; Windows' installed apps, the "Run" value named after the product (start
; with Windows), and, when the user ticks "Delete the application data", the
; data folders. Keymotion keeps two more values under HKCU, removed here
; when the app is uninstalled (not when an update replaces it):
;
;   - Explorer\StartupApproved\Run\Keymotion: Windows' on/off switch for the
;     "start with Windows" entry (kb_winsys::startup);
;   - Software\Classes\AppUserModelId\dev.keymotion: the name and logo that
;     notifications show (kb_winsys::appid, notify.rs).

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
  ${EndIf}
!macroend
