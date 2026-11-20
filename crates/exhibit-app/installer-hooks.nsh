; Hooks into Tauri's NSIS installer (bundle.windows.nsis.installerHooks).
;
; Tauri's uninstaller only knows $APPDATA\${BUNDLEID} and $LOCALAPPDATA\${BUNDLEID}, and
; removes them only when "Delete the application data" is ticked. Exhibit keeps the rest of
; its things elsewhere; keep these paths in step with exhibit_core::db::data_dir,
; exhibit_core::tools::default_tools_dir and exhibit_core::secrets::OsSecrets.

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the old uninstaller too; it must keep everything.
  ${If} $UpdateMode <> 1
    SetShellVarContext current

    ; The mpv and ffmpeg Exhibit downloaded are part of the program, so they always go.
    ; They sit in the default install folder, which Tauri's non-recursive RMDir leaves behind.
    RmDir /r "$LOCALAPPDATA\exhibit\tools"
    RmDir "$LOCALAPPDATA\exhibit"
    RmDir "$INSTDIR"

    ${If} $DeleteAppDataCheckboxState = 1
      ; The library: database and artwork cache.
      RmDir /r "$APPDATA\exhibit"
      ; Secrets in Windows Credential Manager (keyring targets are <name>.<service>).
      nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:tmdb.exhibit'
      Pop $0
      nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:agent.exhibit'
      Pop $0
    ${EndIf}
  ${EndIf}
!macroend
