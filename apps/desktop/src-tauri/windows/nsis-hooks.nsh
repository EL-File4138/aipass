!include "LogicLib.nsh"

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing AIPass tray login entries..."
  IfFileExists "$INSTDIR\aipass-desktop.exe" 0 aipass_tray_cleanup_done
    nsExec::ExecToLog '"$INSTDIR\aipass-desktop.exe" --remove-login-start'
    Pop $R0
    ${If} $R0 != 0
      DetailPrint "Could not stop AIPass or remove login entries. Close AIPass and retry."
      SetErrorLevel 2
      Abort
    ${EndIf}
  aipass_tray_cleanup_done:
  DetailPrint "Removing AIPass agent autostart..."
  IfFileExists "$INSTDIR\aipass-agent.exe" 0 aipass_agent_cleanup_done
    nsExec::ExecToLog '"$INSTDIR\aipass-agent.exe" --uninstall-autostart'
    Pop $R0
    ${If} $R0 != 0
      DetailPrint "Could not stop the AIPass Agent. Close AIPass and retry."
      SetErrorLevel 2
      Abort
    ${EndIf}
  aipass_agent_cleanup_done:
  IfFileExists "$INSTDIR\resources\aipass-agent.exe" 0 aipass_resource_agent_cleanup_done
    nsExec::ExecToLog '"$INSTDIR\resources\aipass-agent.exe" --uninstall-autostart'
    Pop $R0
    ${If} $R0 != 0
      SetErrorLevel 2
      Abort
    ${EndIf}
  aipass_resource_agent_cleanup_done:
!macroend
