; Windows installer for OpenPeriph (Inno Setup 6).
; Built by CI:  iscc /DAppVersion=0.5.0 packaging\windows\openperiph.iss
; Installs for the current user (no administrator rights) into
; %LOCALAPPDATA%\Programs\OpenPeriph, with a Start menu entry and an uninstaller.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{0DC0E32C-BE0B-4132-B0F4-7D5BB88BD599}
AppName=OpenPeriph
AppVersion={#AppVersion}
AppVerName=OpenPeriph {#AppVersion}
AppPublisher=OpenPeriph
AppPublisherURL=https://github.com/clawntl/openperiph
AppSupportURL=https://github.com/clawntl/openperiph/issues
AppUpdatesURL=https://github.com/clawntl/openperiph/releases
DefaultDirName={autopf}\OpenPeriph
DefaultGroupName=OpenPeriph
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=openperiph-setup-x64
SetupIconFile=..\..\assets\icons\openperiph.ico
UninstallDisplayIcon={app}\openperiph.exe
UninstallDisplayName=OpenPeriph
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; force: a silent update must not stop to ask about files in use
CloseApplications=force
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\openperiph.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OpenPeriph"; Filename: "{app}\openperiph.exe"
Name: "{autodesktop}\OpenPeriph"; Filename: "{app}\openperiph.exe"; Tasks: desktopicon

[Code]
// OpenPeriph starts this installer to update itself and quits straight away. Wait until it's
// really gone (it holds the mutex below while running), so openperiph.exe isn't in use when
// it's replaced; a silent install that finds it in use gives up, and nothing starts again.
function InitializeSetup(): Boolean;
var
  Waited: Integer;
begin
  // versions before 0.6.5 don't hold the mutex: give them a moment to exit as well
  if WizardSilent then
    Sleep(2000);
  Waited := 0;
  while CheckForMutexes('OpenPeriph.Running') and (Waited < 20000) do
  begin
    Sleep(250);
    Waited := Waited + 250;
  end;
  Result := True;
end;

[Run]
; Also runs after a silent install (how OpenPeriph updates itself), so the app comes back.
Filename: "{app}\openperiph.exe"; Description: "{cm:LaunchProgram,OpenPeriph}"; Flags: nowait postinstall
