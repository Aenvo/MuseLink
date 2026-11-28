; MuseLink NSIS installer hooks
; 等待安装目录内随包托管的 Everything.exe 释放文件写锁，避免原地升级时覆盖失败。
; 只探测 $INSTDIR\everything\Everything.exe，绝不终止用户自行安装的系统 Everything 实例。

!define MUSELINK_LOCK_WAIT_MAX_MS 6000
!define MUSELINK_LOCK_WAIT_STEP_MS 250

!macro MuseLinkWaitBundledEverythingUnlocked _uid
  Push $R8
  Push $R9
  Push $0
  IfFileExists "$INSTDIR\everything\Everything.exe" 0 muselink_ev_done_${_uid}
  StrCpy $R9 ${MUSELINK_LOCK_WAIT_MAX_MS}
  muselink_ev_loop_${_uid}:
    ClearErrors
    FileOpen $0 "$INSTDIR\everything\Everything.exe" a
    ${IfNot} ${Errors}
      FileClose $0
      Goto muselink_ev_done_${_uid}
    ${EndIf}
    Sleep ${MUSELINK_LOCK_WAIT_STEP_MS}
    IntOp $R9 $R9 - ${MUSELINK_LOCK_WAIT_STEP_MS}
    ${If} $R9 > 0
      Goto muselink_ev_loop_${_uid}
    ${EndIf}
  muselink_ev_done_${_uid}:
  Pop $0
  Pop $R9
  Pop $R8
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro MuseLinkWaitBundledEverythingUnlocked install
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro MuseLinkWaitBundledEverythingUnlocked uninstall
!macroend
