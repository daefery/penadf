; PenaDF installer (NSIS 3).
;
; Installs for the current user only, into %LOCALAPPDATA%\Programs\PenaDF, so it needs no
; administrator rights. Shows a short licence and privacy agreement, in Indonesian or
; English to match Windows. Registers PenaDF as a PDF app (ProgID, Capabilities,
; RegisteredApplications, OpenWithProgids). Windows does not let installers make themselves
; the default, so the finish page offers to open Settings at PenaDF's Default apps page.
;
; Flow: Welcome > Licence and privacy > Folder > Installing > Finish (start / make default)
;
; Build: scripts/build-installer.sh  (passes VERSION, DIST and OUTFILE).

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef DIST
  !define DIST "..\dist\PenaDF"
!endif
!ifndef OUTFILE
  !define OUTFILE "..\dist\PenaDF-Setup-${VERSION}.exe"
!endif

!define APP        "PenaDF"
!define EXE        "PenaDF.exe"
!define PROGID     "PenaDF.Document"   ; keep in sync with src/winshell.rs PROG_ID
!define REGAPP     "PenaDF"            ; keep in sync with src/winshell.rs REGISTERED_APP
!define CAPS       "Software\PenaDF\Capabilities"
!define UNINST     "Software\Microsoft\Windows\CurrentVersion\Uninstall\PenaDF"
!define APPPATH    "Software\Microsoft\Windows\CurrentVersion\App Paths\${EXE}"

Name "${APP}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\PenaDF"
InstallDirRegKey HKCU "Software\PenaDF" "InstallDir"
RequestExecutionLevel user
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=0 "ProductName" "${APP}"
VIAddVersionKey /LANG=0 "FileDescription" "${APP} Setup"
VIAddVersionKey /LANG=0 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "LegalCopyright" "Apache License 2.0"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define MUI_ICON "..\assets\penadf.ico"
!define MUI_UNICON "..\assets\penadf.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP "wizard.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "wizard.bmp"
!define MUI_ABORTWARNING
!define MUI_LANGDLL_ALLLANGUAGES

; ---- Pages ----
!define MUI_WELCOMEPAGE_TITLE "$(WelcomeTitle)"
!define MUI_WELCOMEPAGE_TEXT "$(WelcomeText)"
!insertmacro MUI_PAGE_WELCOME

!define MUI_PAGE_HEADER_TEXT "$(LicenceHeader)"
!define MUI_PAGE_HEADER_SUBTEXT "$(LicenceSub)"
!define MUI_LICENSEPAGE_TEXT_TOP "$(LicenceTop)"
!insertmacro MUI_PAGE_LICENSE "$(AgreementFile)"

!define MUI_PAGE_HEADER_SUBTEXT "$(FolderSub)"
!define MUI_DIRECTORYPAGE_TEXT_TOP "$(FolderTop)"
!insertmacro MUI_PAGE_DIRECTORY

!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_TITLE "$(FinishTitle)"
!define MUI_FINISHPAGE_TEXT "$(FinishText)"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "$(FinishRun)"
!define MUI_FINISHPAGE_SHOWREADME ""
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(FinishDefault)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION OpenDefaultApps
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Indonesian first: it is the default when Windows is in Indonesian, English otherwise.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Indonesian"

LicenseLangString AgreementFile ${LANG_ENGLISH} "agreement-en.txt"
LicenseLangString AgreementFile ${LANG_INDONESIAN} "agreement-id.txt"

LangString WelcomeTitle ${LANG_ENGLISH} "Install PenaDF ${VERSION}"
LangString WelcomeTitle ${LANG_INDONESIAN} "Pasang PenaDF ${VERSION}"
LangString WelcomeText ${LANG_ENGLISH} "PenaDF opens, signs, comments on and organizes PDFs on this PC, without the internet.$\r$\n$\r$\nIt installs for your Windows account only, so you do not need administrator rights. It needs about 16 MB of space.$\r$\n$\r$\nClick Next to continue."
LangString WelcomeText ${LANG_INDONESIAN} "PenaDF membuka, menandatangani, memberi komentar, dan mengatur PDF di PC ini, tanpa internet.$\r$\n$\r$\nAplikasi terpasang untuk akun Windows Anda saja, jadi tidak perlu hak administrator. Butuh ruang sekitar 16 MB.$\r$\n$\r$\nKlik Lanjut untuk meneruskan."

LangString LicenceHeader ${LANG_ENGLISH} "Licence and privacy"
LangString LicenceHeader ${LANG_INDONESIAN} "Lisensi dan privasi"
LangString LicenceSub ${LANG_ENGLISH} "PenaDF is free, and your files stay on this PC."
LangString LicenceSub ${LANG_INDONESIAN} "PenaDF gratis, dan file Anda tetap di PC ini."
LangString LicenceTop ${LANG_ENGLISH} "Scroll to read the short summary below."
LangString LicenceTop ${LANG_INDONESIAN} "Gulir untuk membaca ringkasan singkat di bawah ini."

LangString FolderSub ${LANG_ENGLISH} "Most people keep the folder below."
LangString FolderSub ${LANG_INDONESIAN} "Kebanyakan orang memakai folder di bawah ini."
LangString FolderTop ${LANG_ENGLISH} "PenaDF will be installed in this folder. To use another folder, click Browse."
LangString FolderTop ${LANG_INDONESIAN} "PenaDF akan dipasang di folder ini. Untuk memakai folder lain, klik Cari."

LangString FinishTitle ${LANG_ENGLISH} "PenaDF is ready"
LangString FinishTitle ${LANG_INDONESIAN} "PenaDF siap dipakai"
LangString FinishText ${LANG_ENGLISH} "You can find PenaDF in the Start menu.$\r$\n$\r$\nTo open PDFs with PenaDF when you double-click them, tick the box below. Windows Settings will open: click the box next to .pdf and choose PenaDF."
LangString FinishText ${LANG_INDONESIAN} "PenaDF ada di menu Start.$\r$\n$\r$\nAgar PDF langsung terbuka di PenaDF saat diklik dua kali, centang kotak di bawah. Settings Windows akan terbuka: klik kotak di samping .pdf, lalu pilih PenaDF."
LangString FinishRun ${LANG_ENGLISH} "Start PenaDF"
LangString FinishRun ${LANG_INDONESIAN} "Buka PenaDF"
LangString FinishDefault ${LANG_ENGLISH} "Make PenaDF my default PDF app"
LangString FinishDefault ${LANG_INDONESIAN} "Jadikan PenaDF aplikasi PDF utama"

LangString CloseApp ${LANG_ENGLISH} "PenaDF is open. Please close it, then click Retry."
LangString CloseApp ${LANG_INDONESIAN} "PenaDF sedang terbuka. Tutup dulu, lalu klik Retry."

; Tells Explorer that file associations changed (SHCNE_ASSOCCHANGED, SHCNF_IDLIST).
!macro NotifyShell
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

Function .onInit
  ; Indonesian Windows gets the Indonesian installer; everything else English.
  ; "/LANG=id" or "/LANG=en" on the command line overrides (for IT and for testing).
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  IntOp $0 $0 & 0x3FF
  ${If} $0 == 0x21
    StrCpy $LANGUAGE ${LANG_INDONESIAN}
  ${Else}
    StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${EndIf}
  ${GetParameters} $1
  ClearErrors
  ${GetOptions} $1 "/LANG=" $2
  ${IfNot} ${Errors}
    ${If} $2 == "id"
      StrCpy $LANGUAGE ${LANG_INDONESIAN}
    ${ElseIf} $2 == "en"
      StrCpy $LANGUAGE ${LANG_ENGLISH}
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  System::Call 'kernel32::GetUserDefaultUILanguage() i .r0'
  IntOp $0 $0 & 0x3FF
  ${If} $0 == 0x21
    StrCpy $LANGUAGE ${LANG_INDONESIAN}
  ${Else}
    StrCpy $LANGUAGE ${LANG_ENGLISH}
  ${EndIf}
FunctionEnd

Function OpenDefaultApps
  ; Windows 11 (2023-04 update or later) opens PenaDF's own page; older builds show the
  ; general Default apps page.
  ExecShell "open" "ms-settings:defaultapps?registeredAppUser=PenaDF"
FunctionEnd

Section "Install"
  ; The exe cannot be replaced while PenaDF runs.
  retry:
  ClearErrors
  ${If} ${FileExists} "$INSTDIR\${EXE}"
    FileOpen $0 "$INSTDIR\${EXE}" a
    ${If} ${Errors}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(CloseApp)" IDRETRY retry
      Abort
    ${EndIf}
    FileClose $0
  ${EndIf}

  SetOutPath "$INSTDIR"
  File "${DIST}\${EXE}"
  File "${DIST}\pdfium.dll"
  File "${DIST}\THIRD-PARTY-NOTICES.txt"
  File "${DIST}\LICENSE.txt"
  File "${DIST}\README.txt"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; --- ProgID: how a PDF opened with PenaDF looks and launches ---
  WriteRegStr HKCU "Software\Classes\${PROGID}" "" "PDF Document"
  WriteRegStr HKCU "Software\Classes\${PROGID}" "FriendlyTypeName" "PDF Document"
  WriteRegStr HKCU "Software\Classes\${PROGID}\DefaultIcon" "" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "Software\Classes\${PROGID}\shell" "" "open"
  WriteRegStr HKCU "Software\Classes\${PROGID}\shell\open" "FriendlyAppName" "${APP}"
  WriteRegStr HKCU "Software\Classes\${PROGID}\shell\open\command" "" '"$INSTDIR\${EXE}" "%1"'

  ; --- Offer PenaDF in "Open with" for .pdf, without taking over the default ---
  WriteRegStr HKCU "Software\Classes\.pdf\OpenWithProgids" "${PROGID}" ""

  ; --- Application registration (Open with, and Settings > Default apps) ---
  WriteRegStr HKCU "Software\Classes\Applications\${EXE}" "FriendlyAppName" "${APP}"
  WriteRegStr HKCU "Software\Classes\Applications\${EXE}\DefaultIcon" "" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "Software\Classes\Applications\${EXE}\SupportedTypes" ".pdf" ""
  WriteRegStr HKCU "Software\Classes\Applications\${EXE}\shell\open\command" "" '"$INSTDIR\${EXE}" "%1"'

  WriteRegStr HKCU "${CAPS}" "ApplicationName" "${APP}"
  WriteRegStr HKCU "${CAPS}" "ApplicationDescription" "Open, sign, comment on and organize PDFs on this PC, without the internet."
  WriteRegStr HKCU "${CAPS}" "ApplicationIcon" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "${CAPS}\FileAssociations" ".pdf" "${PROGID}"
  WriteRegStr HKCU "Software\RegisteredApplications" "${REGAPP}" "${CAPS}"

  ; --- "PenaDF" from Win+R / Start search ---
  WriteRegStr HKCU "${APPPATH}" "" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${APPPATH}" "Path" "$INSTDIR"

  CreateShortCut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\${EXE}" 0

  ; --- Settings > Apps entry ---
  WriteRegStr HKCU "Software\PenaDF" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST}" "Publisher" "${APP}"
  WriteRegStr HKCU "${UNINST}" "DisplayIcon" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "${UNINST}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINST}" "EstimatedSize" $0

  !insertmacro NotifyShell
SectionEnd

Section "Uninstall"
  retry:
  ClearErrors
  Delete "$INSTDIR\${EXE}"
  ${If} ${Errors}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(CloseApp)" IDRETRY retry
    Abort
  ${EndIf}
  Delete "$INSTDIR\pdfium.dll"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP}.lnk"

  DeleteRegKey HKCU "Software\Classes\${PROGID}"
  DeleteRegValue HKCU "Software\Classes\.pdf\OpenWithProgids" "${PROGID}"
  DeleteRegKey HKCU "Software\Classes\Applications\${EXE}"
  DeleteRegValue HKCU "Software\RegisteredApplications" "${REGAPP}"
  DeleteRegKey HKCU "Software\PenaDF"
  DeleteRegKey HKCU "${APPPATH}"
  DeleteRegKey HKCU "${UNINST}"
  ; Settings (saved signature, "don't ask again") in %APPDATA%\PenaDF are kept.

  !insertmacro NotifyShell
SectionEnd
