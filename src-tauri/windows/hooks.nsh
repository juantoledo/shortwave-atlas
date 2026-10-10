; NSIS hooks (bundle.windows.nsis.installerHooks in tauri.windows.conf.json).

; The app was called "SW Atlas" up to 0.3. NSIS keys the install folder, the Add/Remove Programs
; entry and the shortcuts on the product name, so the first install as "Shortwave Atlas" lands
; next to the old one; an update (/UPDATE) also makes no shortcuts. Carry the old shortcuts over to
; the new name and remove the old install. Settings live elsewhere (%APPDATA%\swatlas and the
; webview data under the unchanged identifier) and are kept.
!define OLD_PRODUCTNAME "SW Atlas"
!define OLD_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${OLD_PRODUCTNAME}"

!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $R9 SHCTX "${OLD_UNINSTKEY}" "DisplayName"
  ${If} $R9 == "${OLD_PRODUCTNAME}"
    ${If} ${FileExists} "$SMPROGRAMS\${OLD_PRODUCTNAME}.lnk"
    ${AndIfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}
    ${If} ${FileExists} "$DESKTOP\${OLD_PRODUCTNAME}.lnk"
    ${AndIfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}

    ; the old install folder, as the old installer recorded it
    ReadRegStr $R8 SHCTX "${MANUKEY}\${OLD_PRODUCTNAME}" ""
    ${If} $R8 != ""
    ${AndIf} $R8 != $INSTDIR
    ${AndIf} ${FileExists} "$R8\uninstall.exe"
      ; Its own uninstaller removes the files, shortcuts (unpinning them) and the Add/Remove entry.
      ; `_?=` runs it in place and waits, so it cannot delete itself.
      ExecWait '"$R8\uninstall.exe" /S _?=$R8'
      Delete "$R8\uninstall.exe"
      RMDir "$R8"
    ${Else}
      ; Installed over the old folder, or that folder is gone: only the old entries are left.
      Delete "$SMPROGRAMS\${OLD_PRODUCTNAME}.lnk"
      Delete "$DESKTOP\${OLD_PRODUCTNAME}.lnk"
      DeleteRegKey SHCTX "${OLD_UNINSTKEY}"
    ${EndIf}
    DeleteRegKey SHCTX "${MANUKEY}\${OLD_PRODUCTNAME}"
  ${EndIf}
!macroend
