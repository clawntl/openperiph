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
CloseApplications=yes
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

[Run]
; Also runs after a silent install (how OpenPeriph updates itself), so the app comes back.
Filename: "{app}\openperiph.exe"; Description: "{cm:LaunchProgram,OpenPeriph}"; Flags: nowait postinstall
