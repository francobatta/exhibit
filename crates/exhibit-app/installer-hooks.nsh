; Hooks into Tauri's NSIS installer (bundle.windows.nsis.installerHooks).
;
; Tauri's uninstaller only knows $APPDATA\${BUNDLEID} and $LOCALAPPDATA\${BUNDLEID}, and
; removes them only when "Delete the application data" is ticked. Exhibit keeps the rest of
; its things elsewhere; keep these paths in step with exhibit_core::db::data_dir,
; exhibit_core::tools::default_tools_dir and exhibit_core::secrets::OsSecrets.

; The mpv and ffmpeg Exhibit downloaded. They sit in the default install folder, which Tauri's
; non-recursive RMDir leaves behind.
!macro EXHIBIT_REMOVE_TOOLS
  SetShellVarContext current
  RmDir /r "$LOCALAPPDATA\exhibit\tools"
!macroend

; Everything Exhibit remembers: what a first run starts without.
!macro EXHIBIT_REMOVE_DATA
  SetShellVarContext current
  ; The library: database and artwork cache.
  RmDir /r "$APPDATA\exhibit"
  ; The window's own data: WebView2 storage and the saved window position.
  RmDir /r "$APPDATA\${BUNDLEID}"
  RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ; Secrets in Windows Credential Manager (keyring targets are <name>.<service>).
  nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:tmdb.exhibit'
  Pop $0
  nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:agent.exhibit'
  Pop $0
!macroend

; Clean install: a checkbox on the Welcome page, or /CLEAN on the command line (the page is
; skipped in passive and silent installs). It removes what earlier installs left, as the
; uninstaller does with "Delete the application data" ticked.
;
; Tauri includes this file before its pages, so the next MUI page, Welcome, takes this SHOW
; callback. If Tauri ever defines its own, the build fails on the duplicate !define.
Var ExhibitClean
Var ExhibitCleanBox
!define MUI_PAGE_CUSTOMFUNCTION_SHOW ExhibitWelcomeShow

Function ExhibitWelcomeShow
  ; At the foot of MUI2's welcome text column (120u wide 195u, page 193u tall), below the
  ; default text. Colours are MUI2's defaults: its own defines don't exist yet at this point
  ; in the script.
  ${NSD_CreateCheckbox} 120u 168u 195u 20u "Clean install: erase the library, settings and saved keys left by earlier installs"
  Pop $ExhibitCleanBox
  ${NSD_AddStyle} $ExhibitCleanBox ${BS_MULTILINE}
  SetCtlColors $ExhibitCleanBox "000000" "FFFFFF"
  ${NSD_SetState} $ExhibitCleanBox $ExhibitClean
  ${NSD_OnClick} $ExhibitCleanBox ExhibitCleanClick
FunctionEnd

Function ExhibitCleanClick
  ${NSD_GetState} $ExhibitCleanBox $ExhibitClean
FunctionEnd

; After the files are in place: by then the installer has made sure Exhibit is not running,
; so nothing holds the database open.
!macro NSIS_HOOK_POSTINSTALL
  ClearErrors
  ${GetOptions} $CMDLINE "/CLEAN" $0
  ${IfNot} ${Errors}
    StrCpy $ExhibitClean ${BST_CHECKED}
  ${EndIf}
  ; An update never starts over, whatever it was given.
  ${If} $ExhibitClean = ${BST_CHECKED}
  ${AndIf} $UpdateMode <> 1
    DetailPrint "Clean install: removing data from earlier installs"
    !insertmacro EXHIBIT_REMOVE_TOOLS
    !insertmacro EXHIBIT_REMOVE_DATA
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the old uninstaller too; it must keep everything.
  ${If} $UpdateMode <> 1
    ; The tools are part of the program, so they always go.
    !insertmacro EXHIBIT_REMOVE_TOOLS
    RmDir "$LOCALAPPDATA\exhibit"
    RmDir "$INSTDIR"

    ${If} $DeleteAppDataCheckboxState = 1
      !insertmacro EXHIBIT_REMOVE_DATA
    ${EndIf}
  ${EndIf}
!macroend
