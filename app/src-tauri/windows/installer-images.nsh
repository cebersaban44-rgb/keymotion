; Sharp installer images at every display scale (K15). Included from
; installer-hooks.nsh, before Tauri's template defines its pages.
;
; NSIS shows the welcome and finish pages' left image (images/sidebar.bmp,
; 164x314) and the top band's image (images/header.bmp, 150x57) at their
; 100% size and, when Windows' display scale is bigger, stretches them to the
; bigger box without smoothing: at 125% they looked pixelated. Here, once the
; window exists, the images drawn four times larger (images/*@4x.bmp, from
; tools/installer-images.py) are scaled down smoothly (GDI's HALFTONE mode)
; to each box's real size:
;
;   - the top band (MUI2's picture box 1046): straight into the box, in the
;     installer and in the uninstaller;
;   - the welcome and finish pages: their box (109 x 193 dialog units, as in
;     MUI2's Pages\Welcome.nsh and Finish.nsh) does not exist yet, so the
;     image is written, at the size the box will have, over the file both
;     pages load ($PLUGINSDIR\modern-wizard.bmp). NSIS then has nothing left
;     to stretch.
;
; At 100% the boxes have the 1x images' size and nothing is changed. If any
; step fails, the 1x images stay, as before.

!include LogicLib.nsh

; The @4x images' sizes (tools/installer-images.py).
!define kmSidebar4xW 656
!define kmSidebar4xH 1256
!define kmHeader4xW 600
!define kmHeader4xH 228
; The images' background colours, as Windows writes them (0x00BBGGRR):
; the sidebar's dark (13, 14, 16) and the header's white.
!define kmSidebarBg 1052173
!define kmHeaderBg 16777215

!ifndef STM_SETIMAGE
  !define STM_SETIMAGE 0x0172
!endif
!ifndef IMAGE_BITMAP
  !define IMAGE_BITMAP 0
!endif

!define MUI_CUSTOMFUNCTION_GUIINIT kmImagesGUIInit
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.kmImagesGUIInit

!macro kmImagesFunctions UN

; Scales a bitmap file smoothly into a new bitmap of the box's size. The
; picture keeps its proportions and is centred; the rest of the box gets the
; background colour.
; In: $R0 the file, $R1 x $R2 its size, $R3 x $R4 the box's size,
;     $R5 the background colour. Out: $R6, the new bitmap, or 0.
Function ${UN}kmScaleBitmap
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  Push $8
  Push $9
  StrCpy $R6 0
  ; IMAGE_BITMAP, LR_LOADFROMFILE: the file at its own size.
  System::Call 'user32::LoadImageW(p 0, w R0, i 0, i 0, i 0, i 0x10) p .r0'
  ${If} $0 != 0
    System::Call 'user32::GetDC(p 0) p .r1'
    System::Call 'gdi32::CreateCompatibleDC(p r1) p .r2'
    System::Call 'gdi32::CreateCompatibleDC(p r1) p .r3'
    System::Call 'gdi32::CreateCompatibleBitmap(p r1, i R3, i R4) p .R6'
    System::Call 'user32::ReleaseDC(p 0, p r1)'
    System::Call 'gdi32::SelectObject(p r2, p r0) p .r4'
    System::Call 'gdi32::SelectObject(p r3, p R6) p .r5'
    ; The background.
    System::Call 'gdi32::CreateSolidBrush(i R5) p .r9'
    System::Call '*(i 0, i 0, i R3, i R4) p .r1'
    System::Call 'user32::FillRect(p r3, p r1, p r9)'
    System::Free $1
    System::Call 'gdi32::DeleteObject(p r9)'
    ; The picture's size: the box's width, unless that makes it too tall.
    IntOp $6 $R3 * $R2
    IntOp $6 $6 / $R1
    ${If} $6 > $R4
      StrCpy $6 $R4
      IntOp $7 $R4 * $R1
      IntOp $7 $7 / $R2
    ${Else}
      StrCpy $7 $R3
    ${EndIf}
    ; Centred.
    IntOp $1 $R3 - $7
    IntOp $1 $1 / 2
    IntOp $8 $R4 - $6
    IntOp $8 $8 / 2
    ; HALFTONE averages the source pixels behind each new pixel.
    System::Call 'gdi32::SetStretchBltMode(p r3, i 4)'
    System::Call 'gdi32::SetBrushOrgEx(p r3, i 0, i 0, p 0)'
    System::Call 'gdi32::StretchBlt(p r3, i r1, i r8, i r7, i r6, p r2, i 0, i 0, i R1, i R2, i 0x00CC0020)'
    System::Call 'gdi32::SelectObject(p r2, p r4)'
    System::Call 'gdi32::SelectObject(p r3, p r5)'
    System::Call 'gdi32::DeleteDC(p r2)'
    System::Call 'gdi32::DeleteDC(p r3)'
    System::Call 'gdi32::DeleteObject(p r0)'
  ${EndIf}
  Pop $9
  Pop $8
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; The top band's picture box: the @4x header at the box's real size.
Function ${UN}kmSharpHeader
  Push $0
  Push $1
  Push $R0
  Push $R1
  Push $R2
  Push $R3
  Push $R4
  Push $R5
  Push $R6
  GetDlgItem $0 $HWNDPARENT 1046
  System::Call '*(i 0, i 0, i 0, i 0) p .r1'
  System::Call 'user32::GetClientRect(p r0, p r1)'
  System::Call '*$1(i, i, i .R3, i .R4)'
  System::Free $1
  ${If} $R3 > 0
  ${AndIf} $R4 > 0
  ${AndIfNot} "$R3,$R4" == "150,57"
    InitPluginsDir
    File "/oname=$PLUGINSDIR\km-header.bmp" "${__FILEDIR__}\images\header@4x.bmp"
    StrCpy $R0 "$PLUGINSDIR\km-header.bmp"
    StrCpy $R1 ${kmHeader4xW}
    StrCpy $R2 ${kmHeader4xH}
    StrCpy $R5 ${kmHeaderBg}
    Call ${UN}kmScaleBitmap
    ${If} $R6 != 0
      ; The box hands back the bitmap it showed (NSIS's): delete it.
      SendMessage $0 ${STM_SETIMAGE} ${IMAGE_BITMAP} $R6 $1
      ${If} $1 != 0
        System::Call 'gdi32::DeleteObject(p r1)'
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Pop $R6
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
  Pop $1
  Pop $0
FunctionEnd

!macroend

!insertmacro kmImagesFunctions ""
!insertmacro kmImagesFunctions "un."

; Writes the bitmap $R6 to the file $R0 as a 24-bit BMP, $R3 x $R4 pixels.
Function kmWriteBitmap
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  ; Each row is padded to 4 bytes.
  IntOp $0 $R3 * 3
  IntOp $0 $0 + 3
  IntOp $0 $0 / 4
  IntOp $0 $0 * 4
  IntOp $1 $0 * $R4
  ; BITMAPINFOHEADER: bottom-up rows, 24 bits, not compressed.
  System::Call '*(i 40, i R3, i R4, &i2 1, &i2 24, i 0, i r1, i 0, i 0, i 0, i 0) p .r2'
  System::Alloc $1
  Pop $3
  System::Call 'user32::GetDC(p 0) p .r4'
  System::Call 'gdi32::GetDIBits(p r4, p R6, i 0, i R4, p r3, p r2, i 0) i .r5'
  System::Call 'user32::ReleaseDC(p 0, p r4)'
  ${If} $5 == $R4
    ; BITMAPFILEHEADER, 14 bytes, as seven 16-bit words: "BM", the file's
    ; size (low, high), two reserved words, where the pixels start (54).
    IntOp $5 $1 + 54
    IntOp $6 $5 >> 16
    IntOp $5 $5 & 0xFFFF
    System::Call '*(&i2 0x4D42, &i2 r5, &i2 r6, &i2 0, &i2 0, &i2 54, &i2 0) p .r6'
    ; GENERIC_WRITE, CREATE_ALWAYS.
    System::Call 'kernel32::CreateFileW(w R0, i 0x40000000, i 0, p 0, i 2, i 0x80, p 0) p .r4'
    ${If} $4 != -1
    ${AndIf} $4 != 4294967295
    ${AndIf} $4 != 0
      System::Call 'kernel32::WriteFile(p r4, p r6, i 14, *i .r5, p 0)'
      System::Call 'kernel32::WriteFile(p r4, p r2, i 40, *i .r5, p 0)'
      System::Call 'kernel32::WriteFile(p r4, p r3, i r1, *i .r5, p 0)'
      System::Call 'kernel32::CloseHandle(p r4)'
    ${EndIf}
    System::Free $6
  ${EndIf}
  System::Free $3
  System::Free $2
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; The welcome and finish pages' image: the @4x sidebar at the size their box
; will have, over the file they load.
Function kmSharpWizardImage
  Push $1
  Push $R0
  Push $R1
  Push $R2
  Push $R3
  Push $R4
  Push $R5
  Push $R6
  ; 109 x 193 dialog units in pixels, in the window's font and scale.
  System::Call '*(i 0, i 0, i 109, i 193) p .r1'
  System::Call 'user32::MapDialogRect(p $HWNDPARENT, p r1)'
  System::Call '*$1(i, i, i .R3, i .R4)'
  System::Free $1
  ${If} $R3 > 0
  ${AndIf} $R4 > 0
  ${AndIfNot} "$R3,$R4" == "164,314"
  ${AndIf} ${FileExists} "$PLUGINSDIR\modern-wizard.bmp"
    File "/oname=$PLUGINSDIR\km-sidebar.bmp" "${__FILEDIR__}\images\sidebar@4x.bmp"
    StrCpy $R0 "$PLUGINSDIR\km-sidebar.bmp"
    StrCpy $R1 ${kmSidebar4xW}
    StrCpy $R2 ${kmSidebar4xH}
    StrCpy $R5 ${kmSidebarBg}
    Call kmScaleBitmap
    ${If} $R6 != 0
      StrCpy $R0 "$PLUGINSDIR\modern-wizard.bmp"
      Call kmWriteBitmap
      System::Call 'gdi32::DeleteObject(p R6)'
    ${EndIf}
  ${EndIf}
  Pop $R6
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
  Pop $1
FunctionEnd

; Called by MUI2 at the end of .onGUIInit, after it has shown the 1x header
; and unpacked the 1x welcome image.
Function kmImagesGUIInit
  Call kmSharpHeader
  Call kmSharpWizardImage
FunctionEnd

Function un.kmImagesGUIInit
  Call un.kmSharpHeader
FunctionEnd
