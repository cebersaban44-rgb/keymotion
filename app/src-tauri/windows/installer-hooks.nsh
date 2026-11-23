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
; When Keymotion is uninstalled, before its files go, keymotion-cli takes
; away what it added:
;   - its connections to Claude Code and Codex (hooks, skill, Codex rule),
;     only when all of them name this copy (--if-this-copy: the Microsoft
;     Store copy's stay). Claude Code would otherwise report a hook error on
;     every event, as it runs the program it was given and cannot check
;     first;
;   - its folder on PATH, only if Keymotion put it there (`path remove`).
; Each runs hidden, waits at most 15 seconds, and never stops the uninstall:
; its result is ignored. Keymotion is closed first (the question Tauri asks
; right after, asked here before anything of ours goes): a close the user
; cancels stops the uninstall with everything still in place.
;
; Not when the uninstall only makes room for a new version, which keeps
; using them:
;   - an update (the updater runs the new setup with /UPDATE, and that never
;     runs the old uninstaller);
;   - a new setup run over the installed one, whose "already installed" page
;     runs the old uninstaller first (without /UPDATE): the setup sets
;     KEYMOTION_SETUP_REPLACES in its environment as its window opens, and
;     the old uninstaller, started by it, sees it. It keeps its
;     keymotion-cli as bin\keymotion-cli.replaced.exe: should the setup end
;     without installing (cancelled on a later page, or failed), the setup
;     takes the connections and PATH entry away with it
;     (kmEndWithoutInstalling); once installed, the file goes;
;   - an uninstall run as another Windows user (another administrator, by
;     elevation): that account's settings are not the ones the connections
;     and PATH entry are in. Keymotion's own record of the installation
;     (Software\Keymotion\Keymotion in the user's registry, which Tauri writes)
;     must name this folder.
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

; The setup's window opens: say, for an old uninstaller this setup may run
; from its "already installed" page, that a new version takes this one's
; place (see above). Then the sharp images.
!define MUI_CUSTOMFUNCTION_GUIINIT kmSetupGUIInit
Function kmSetupGUIInit
  System::Call 'kernel32::SetEnvironmentVariable(t "KEYMOTION_SETUP_REPLACES", t "1") i'
  Call kmImagesGUIInit
FunctionEnd

; Cancel on any page of the setup (MUI's .onUserAbort).
!define MUI_CUSTOMFUNCTION_ABORT kmEndWithoutInstalling

; The setup ends without installing: cancelled, or failed (.onInstFailed).
; If its "already installed" page had the installed copy uninstalled first,
; that uninstaller left the connections and PATH entry for this setup and
; kept its keymotion-cli as keymotion-cli.replaced.exe (see above). With
; that copy gone, they go now, as its uninstaller would have taken them,
; by that keymotion-cli at the path they name (--if-this-copy); with the
; copy still there (its uninstall did not finish), the file goes back. The
; copy's folder is Tauri's record of the installation, which this setup
; rewrites only once it installs (the folder page may have changed
; $INSTDIR). This file is included before Tauri defines MANUPRODUCTKEY and
; MAINBINARYNAME, so their values are written out here.
Function kmEndWithoutInstalling
  Push $R9
  ReadRegStr $R9 HKCU "Software\Keymotion\Keymotion" ""
  ${If} $R9 == ""
    StrCpy $R9 $INSTDIR
  ${EndIf}
  ${If} ${FileExists} "$R9\bin\keymotion-cli.replaced.exe"
    ${IfNot} ${FileExists} "$R9\bin\keymotion-cli.exe"
      Rename "$R9\bin\keymotion-cli.replaced.exe" "$R9\bin\keymotion-cli.exe"
    ${EndIf}
    Delete "$R9\bin\keymotion-cli.replaced.exe"
    ${IfNot} ${FileExists} "$R9\keymotion.exe"
    ${AndIf} ${FileExists} "$R9\bin\keymotion-cli.exe"
      nsExec::Exec /TIMEOUT=15000 '"$R9\bin\keymotion-cli.exe" disconnect claude-code --yes --if-this-copy'
      Pop $0
      nsExec::Exec /TIMEOUT=15000 '"$R9\bin\keymotion-cli.exe" disconnect codex --yes --if-this-copy'
      Pop $0
      nsExec::Exec /TIMEOUT=15000 '"$R9\bin\keymotion-cli.exe" path remove'
      Pop $0
      Delete "$R9\bin\keymotion-cli.exe"
      RMDir "$R9\bin"
      RMDir "$R9"
      DeleteRegKey /ifempty HKCU "Software\Keymotion\UserPath"
    ${EndIf}
  ${EndIf}
  ClearErrors
  Pop $R9
FunctionEnd

; keymotion-cli renamed out of the way by this setup (below), to put back
; if the setup fails.
Var kmCliMoved

; Earlier keymotion-cli files renamed out of the way: those no longer
; running go. $R8 and $R9 are kept.
!macro kmDeleteOldClis
  Push $R8
  Push $R9
  FindFirst $R8 $R9 "$INSTDIR\bin\keymotion-cli.old*.exe"
  ${DoWhile} $R9 != ""
    Delete "$INSTDIR\bin\$R9"
    FindNext $R8 $R9
  ${Loop}
  FindClose $R8
  ClearErrors
  Pop $R9
  Pop $R8
!macroend

; A keymotion-cli that runs now (a coding agent's hook waiting for the app
; to start) cannot be replaced, but Windows lets it be renamed: it is moved
; to a name of its own and the new one is written in its place. One that
; does not run is left for the setup to replace. Nothing is deleted before
; the setup has gone past "Keymotion is running"; if it fails after all,
; .onInstFailed puts the moved one back.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro kmDeleteOldClis
  StrCpy $kmCliMoved ""
  ${If} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
    Push $R9
    ClearErrors
    FileOpen $R9 "$INSTDIR\bin\keymotion-cli.exe" a
    ${If} ${Errors}
      ; In use: under a name no earlier one has.
      System::Call 'kernel32::GetTickCount() i .R9'
      StrCpy $kmCliMoved "$INSTDIR\bin\keymotion-cli.old-$R9.exe"
      ClearErrors
      Rename "$INSTDIR\bin\keymotion-cli.exe" "$kmCliMoved"
      ${If} ${Errors}
        StrCpy $kmCliMoved ""
      ${EndIf}
    ${Else}
      FileClose $R9
    ${EndIf}
    ClearErrors
    Pop $R9
  ${EndIf}
!macroend

; The setup failed (cancelled at "Keymotion is running", or a file could not
; be written): the keymotion-cli moved out of the way goes back, and what
; a replaced copy left for this setup is dealt with.
Function .onInstFailed
  ${If} $kmCliMoved != ""
  ${AndIf} ${FileExists} "$kmCliMoved"
    Delete "$INSTDIR\bin\keymotion-cli.exe"
    ${IfNot} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
      Rename "$kmCliMoved" "$INSTDIR\bin\keymotion-cli.exe"
    ${EndIf}
  ${EndIf}
  Call kmEndWithoutInstalling
FunctionEnd

; Installed: the keymotion-cli a replaced copy kept is not needed any more.
!macro NSIS_HOOK_POSTINSTALL
  Delete "$INSTDIR\bin\keymotion-cli.replaced.exe"
  ClearErrors
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Keymotion closed first, as Tauri's check right after this asks: a
  ; cancelled close stops the uninstall here, before anything of ours goes.
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  Push $R8
  Push $R9
  ReadEnvStr $R9 KEYMOTION_SETUP_REPLACES
  ReadRegStr $R8 HKCU "${MANUPRODUCTKEY}" ""
  ${If} $UpdateMode <> 1
  ${AndIf} $R9 != "1"
  ${AndIf} $R8 == $INSTDIR
  ${AndIf} ${FileExists} "$INSTDIR\bin\keymotion-cli.exe"
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" disconnect claude-code --yes --if-this-copy'
    Pop $0
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" disconnect codex --yes --if-this-copy'
    Pop $0
    nsExec::Exec /TIMEOUT=15000 '"$INSTDIR\bin\keymotion-cli.exe" path remove'
    Pop $0
  ${EndIf}
  !insertmacro kmDeleteOldClis
  Delete "$INSTDIR\bin\keymotion-cli.replaced.exe"
  ; A new setup takes this copy's place: its keymotion-cli stays, renamed,
  ; until the setup has installed (kmEndWithoutInstalling).
  ${If} $UpdateMode <> 1
  ${AndIf} $R9 == "1"
  ${AndIf} $R8 == $INSTDIR
    Rename "$INSTDIR\bin\keymotion-cli.exe" "$INSTDIR\bin\keymotion-cli.replaced.exe"
  ${EndIf}
  ClearErrors
  Pop $R9
  Pop $R8
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
    DeleteRegKey HKCU "Software\Classes\keymotion"
    ; `path remove` took its values away; the keys, if nothing else is in
    ; them (Software\Keymotion\Keymotion, the installation's record, stays
    ; unless "delete my data" was ticked).
    DeleteRegKey /ifempty HKCU "Software\Keymotion\UserPath"
    DeleteRegKey /ifempty HKCU "Software\Keymotion"
  ${EndIf}
!macroend
