; Explorer's "Open with GitWyrm" right-click entry.
;
; These are plain shell verbs, the same shape Visual Studio's "AnyCode" verb and
; GitKraken use. Windows 11 promotes verbs like these into the top-level context
; menu -- no packaged app or IExplorerCommand handler is involved.
;
; Written under HKCU to match the installer's currentUser install mode, so no
; elevation prompt appears. Keep the verb name and value shapes in step with
; src/commands/shell_integration.rs, which manages the same keys for the
; Settings > Behavior toggle.
;
; Two targets, because Explorer treats them separately:
;   Directory\shell            - right-clicking a folder            (%1)
;   Directory\Background\shell - right-clicking inside a folder     (%V)

!macro NSIS_HOOK_POSTINSTALL
  ; Right-click a folder.
  WriteRegStr HKCU "Software\Classes\Directory\shell\GitWyrm" "" "Open with GitWyrm"
  ; Windows 11 appears to need an icon before it will lift a verb into the
  ; top-level menu; every confirmed top-level entry sets one.
  WriteRegStr HKCU "Software\Classes\Directory\shell\GitWyrm" "Icon" "$INSTDIR\GitWyrm.exe"
  WriteRegStr HKCU "Software\Classes\Directory\shell\GitWyrm\command" "" '"$INSTDIR\GitWyrm.exe" "%1"'

  ; Right-click empty space inside an open folder. %1 is empty here, so this
  ; target needs %V or the app launches with no folder.
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\GitWyrm" "" "Open with GitWyrm"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\GitWyrm" "Icon" "$INSTDIR\GitWyrm.exe"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\GitWyrm\command" "" '"$INSTDIR\GitWyrm.exe" "%V"'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Leave nothing behind pointing at an executable that no longer exists.
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GitWyrm"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\GitWyrm"
!macroend
