; The engine outlives GaLa if GaLa crashes or is closed by this installer, and it holds
; gala-engine.exe open. Ask it to stop so it saves torrent progress, then force it after 20s.
; The encoded command is this PowerShell, in UTF-16 base64:
;   $p=New-Object IO.Pipes.NamedPipeClientStream('.','gala-engine');try{$p.Connect(2000);
;   $w=New-Object IO.StreamWriter($p);$w.AutoFlush=1;$w.WriteLine('{"cmd":"shutdown"}')}catch{};
;   Wait-Process -N gala-engine -T 20 -EA 0;Stop-Process -N gala-engine -F -EA 0
!macro StopEngine
  DetailPrint "Stopping the GaLa engine"
  nsExec::Exec 'powershell -NoProfile -NonInteractive -EncodedCommand JABwAD0ATgBlAHcALQBPAGIAagBlAGMAdAAgAEkATwAuAFAAaQBwAGUAcwAuAE4AYQBtAGUAZABQAGkAcABlAEMAbABpAGUAbgB0AFMAdAByAGUAYQBtACgAJwAuACcALAAnAGcAYQBsAGEALQBlAG4AZwBpAG4AZQAnACkAOwB0AHIAeQB7ACQAcAAuAEMAbwBuAG4AZQBjAHQAKAAyADAAMAAwACkAOwAkAHcAPQBOAGUAdwAtAE8AYgBqAGUAYwB0ACAASQBPAC4AUwB0AHIAZQBhAG0AVwByAGkAdABlAHIAKAAkAHAAKQA7ACQAdwAuAEEAdQB0AG8ARgBsAHUAcwBoAD0AMQA7ACQAdwAuAFcAcgBpAHQAZQBMAGkAbgBlACgAJwB7ACIAYwBtAGQAIgA6ACIAcwBoAHUAdABkAG8AdwBuACIAfQAnACkAfQBjAGEAdABjAGgAewB9ADsAVwBhAGkAdAAtAFAAcgBvAGMAZQBzAHMAIAAtAE4AIABnAGEAbABhAC0AZQBuAGcAaQBuAGUAIAAtAFQAIAAyADAAIAAtAEUAQQAgADAAOwBTAHQAbwBwAC0AUAByAG8AYwBlAHMAcwAgAC0ATgAgAGcAYQBsAGEALQBlAG4AZwBpAG4AZQAgAC0ARgAgAC0ARQBBACAAMAA='
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro StopEngine
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro StopEngine
!macroend
