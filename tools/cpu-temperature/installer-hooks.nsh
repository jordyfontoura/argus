!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    MessageBox MB_YESNO|MB_ICONQUESTION "Remover também o serviço de leitura da CPU? O Windows pedirá permissão. PawnIO compartilhado e seus dados serão mantidos." IDNO argus_keep_service
    nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\cpu-temperature\uninstall.ps1"'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONINFORMATION "Permissão recusada ou remoção incompleta. O serviço pode permanecer em Program Files\Argus\CpuTemperature. Seus dados e PawnIO foram mantidos. Reinstale o Argus para tentar a remoção novamente."
    ${Else}
      MessageBox MB_OK|MB_ICONINFORMATION "Serviço do Argus removido. PawnIO compartilhado permanece instalado; remova-o pelos Aplicativos do Windows somente se nenhum outro programa precisar dele. Seus dados foram mantidos."
    ${EndIf}
    argus_keep_service:
  ${EndIf}
!macroend
