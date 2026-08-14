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

; ---------------------------------------------------------------------------
; Install log
;
; NSIS's own LogSet/LogText need a build compiled with NSIS_CONFIG_LOG, which
; Tauri does not ship -- using them would fail to compile. Writing the file
; ourselves through the four hooks Tauri does expose needs no special build.
;
; This is the only record the install phase leaves. The app's log stops when it
; exits to hand over to the installer, and the update-cover helper only watches
; for a process, so without this a failure during the install itself was
; invisible. The cover window's "Open log" button reads this file first.
;
; $LOCALAPPDATA, not $INSTDIR: the installer rewrites the install directory, and
; a log living inside it would be destroyed by the very operation it records.
; ---------------------------------------------------------------------------

!define GW_LOG "$LOCALAPPDATA\dev.gitwyrm.app\logs\GitWyrm-Install.log"

; Append one line. Opens and closes per line so a crash mid-install still
; leaves everything written up to that point on disk.
;
; Branchless on purpose. This macro is inserted into BOTH the installer and the
; uninstaller, which are separate scopes, and a label defined in one cannot be
; resolved from the other -- makensis fails the build with "could not resolve
; label ... in uninstall section". `${__LINE__}` does not rescue it either: from
; inside a macro it expands to a nested reference like `1218.7.5`, whose dots
; are not valid in a label name.
;
; No guard is needed anyway. When FileOpen fails it yields an empty handle, and
; FileSeek/FileWrite/FileClose on an empty handle are no-ops in NSIS -- so a log
; that cannot be written is skipped silently, which is what we want. Failing an
; install because its own logging failed would be far worse than a missing
; diagnostic.
!macro GW_LOG_LINE Text
  Push $0
  CreateDirectory "$LOCALAPPDATA\dev.gitwyrm.app\logs"
  FileOpen $0 "${GW_LOG}" a
  FileSeek $0 0 END
  FileWrite $0 "${Text}$\r$\n"
  FileClose $0
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Truncate: each install starts a fresh log, so the file always describes the
  ; run being asked about rather than growing without bound across updates.
  Push $0
  CreateDirectory "$LOCALAPPDATA\dev.gitwyrm.app\logs"
  FileOpen $0 "${GW_LOG}" w
  FileWrite $0 "GitWyrm install log$\r$\n"
  FileClose $0
  Pop $0

  !insertmacro GW_LOG_LINE "Version:    ${VERSION}"
  !insertmacro GW_LOG_LINE "Target dir: $INSTDIR"

  ; $UpdateMode is only ever assigned 1, never 0 -- the template does
  ; `${GetOptions} $CMDLINE "/UPDATE" $UpdateMode` and then sets it to 1 when
  ; that succeeds. So on a normal install it is still empty, and logging it
  ; directly printed a blank field in exactly the case where "was this the
  ; updater or a person?" is the question being asked.
  ;
  ; The branch picks a STRING and logs once, rather than branching around two
  ; !insertmacro calls: each of those expands to several instructions, so the
  ; relative jumps such a branch would need could not be counted reliably.
  Push $0
  Push $1
  StrCpy $1 "a person (fresh install)"
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $0
  IfErrors +2 0
    StrCpy $1 "the updater (/UPDATE)"
  ClearErrors
  !insertmacro GW_LOG_LINE "Started by: $1"
  Pop $1
  Pop $0

  !insertmacro GW_LOG_LINE "Files are being copied now"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Compiles into the uninstaller, which is a separate binary from the installer
  ; -- the macro body is plain instructions, so this is fine, but it means the
  ; line comes from whichever build is being REMOVED.
  ;
  ; So on the update that first ships this logging, the old uninstaller has none
  ; and this line will be absent. That is expected once, not a fault.
  !insertmacro GW_LOG_LINE "Removing the previous version"
!macroend

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

  ; Last line of a healthy install. Its ABSENCE is the diagnostic: a log that
  ; stops after "Files are being copied now" says the install died partway,
  ; which is exactly the failure that previously left no trace at all.
  !insertmacro GW_LOG_LINE "Install finished successfully"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Leave nothing behind pointing at an executable that no longer exists.
  DeleteRegKey HKCU "Software\Classes\Directory\shell\GitWyrm"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\GitWyrm"
!macroend
