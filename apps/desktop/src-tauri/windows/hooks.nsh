; Installer hooks for Cilica's NSIS setup (bundle.windows.nsis.installerHooks).
;
; One-off for the rename from Margin (0.3.0 and earlier): before Cilica installs, silently
; uninstall Margin so people don't end up with both. The silent uninstaller keeps app data (that
; needs the checkbox in its window), and Cilica moves that data to its own folder on first launch.
; Remove after every install has updated, with the rest listed in src/legacy.rs.

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
