; Impulse NSIS installer hooks.
; After installing Impulse, make sure Equalizer APO (the processing backend)
; is present. If it isn't, download the official installer from SourceForge
; and run it interactively — its own setup handles APO registration, and
; Impulse's onboarding covers per-device enablement afterwards.

!macro NSIS_HOOK_POSTINSTALL
  ; EAPO writes HKLM\SOFTWARE\EqualizerAPO on install (64-bit view).
  SetRegView 64
  ReadRegStr $0 HKLM "SOFTWARE\EqualizerAPO" "InstallPath"
  SetRegView lastused
  StrCmp $0 "" 0 eapo_done

  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Impulse uses Equalizer APO to process system audio, and it is not installed.$\r$\n$\r$\nDownload and install it now (~10 MB from SourceForge)?$\r$\nA reboot will be needed once after its installation." \
    IDNO eapo_done

  DetailPrint "Downloading Equalizer APO..."
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command \
    "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
     Invoke-WebRequest -UseBasicParsing -Uri \
     ''https://sourceforge.net/projects/equalizerapo/files/latest/download'' \
     -OutFile ''$TEMP\EqualizerAPO-Setup.exe''"'
  Pop $1
  StrCmp $1 "0" 0 eapo_download_failed

  DetailPrint "Running the Equalizer APO installer..."
  ExecWait '"$TEMP\EqualizerAPO-Setup.exe"'
  Delete "$TEMP\EqualizerAPO-Setup.exe"
  Goto eapo_done

eapo_download_failed:
  MessageBox MB_OK|MB_ICONEXCLAMATION \
    "Could not download Equalizer APO. Impulse will guide you through installing it on first launch."

eapo_done:
!macroend
