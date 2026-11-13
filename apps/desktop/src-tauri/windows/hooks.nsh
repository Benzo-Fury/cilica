; Installer hooks for Cilica's NSIS setup (bundle.windows.nsis.installerHooks).
;
; One-off for the rename from Margin (0.3.0 and earlier): before Cilica installs, silently
; uninstall Margin so people don't end up with both. The silent uninstaller keeps app data (that
; needs the checkbox in its window), and Cilica moves that data to its own folder on first launch.
; Remove this hook (not the file) after every install has updated, with the rest listed in
; src/legacy.rs.

!macro NSIS_HOOK_PREINSTALL
  Push $0
  Push $1
  Push $R9
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Margin" "UninstallString"
  ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Margin" "InstallLocation"
  ${If} $0 != ""
  ${AndIf} $1 != ""
    ; InstallLocation is saved in quotes; _?= wants the bare folder.
    StrCpy $R9 $1 1
    ${If} $R9 == '"'
      StrCpy $1 $1 -1 1
    ${EndIf}
    DetailPrint "Removing Margin"
    ExecWait '$0 /S _?=$1'
    ; Run in place (_?=), the uninstaller can't delete itself or its folder.
    Delete "$1\uninstall.exe"
    RMDir "$1"
  ${EndIf}
  Pop $R9
  Pop $1
  Pop $0
!macroend

; Cilica Setup (apps/setup) runs this installer silently under its own window and removes Cilica
; with that window too; this hook is the installer's half of that. Run on its own (the updater, or
; the wizard when WebView2 is missing), it changes nothing.
!macro NSIS_HOOK_POSTINSTALL
  ; A silent install always makes a desktop shortcut. Cilica Setup passes /NODESKTOP when the
  ; Desktop shortcut switch is off. Tauri's own switches (/P, /NS, /R, /UPDATE, /ARGS) don't
  ; appear inside it, so GetOptions can't mistake one for another.
  ClearErrors
  ${GetOptions} $CMDLINE "/NODESKTOP" $R0
  ${IfNot} ${Errors}
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}

  ; The release build puts Cilica Setup next to Cilica (tauri.windows-setup.conf.json). When it
  ; is there, Installed apps opens it to remove Cilica; tools that remove apps without asking still
  ; get Tauri's uninstaller.
  ${If} ${FileExists} "$INSTDIR\cilica-setup.exe"
    WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\cilica-setup.exe$\" --uninstall"
    WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  ${EndIf}
!macroend
