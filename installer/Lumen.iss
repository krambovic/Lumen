#ifndef AppVersion
#define AppVersion "0.0.0"
#endif

#ifndef VersionInfo
#define VersionInfo "0.0.0.0"
#endif

#ifndef SourceDir
#define SourceDir "..\dist\Lumen"
#endif

#ifndef OutputDir
#define OutputDir "..\dist"
#endif

#ifndef AppId
#define AppId "{{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}"
#endif

#ifndef AppNameValue
#define AppNameValue "Lumen"
#endif

#ifndef OutputBaseName
#define OutputBaseName "Lumen-Setup-windows-x64"
#endif

[Setup]
AppId={#AppId}
AppName={#AppNameValue}
AppVersion={#AppVersion}
AppPublisher=krambovic
AppCopyright=Copyright (c) youtubediscord/zapret-kvn contributors and krambovic/Lumen contributors
AppPublisherURL=https://github.com/krambovic/Lumen
AppSupportURL=https://github.com/krambovic/Lumen/issues
AppUpdatesURL=https://github.com/krambovic/Lumen/releases
DefaultDirName={commonpf64}\{#AppNameValue}
DefaultGroupName={#AppNameValue}
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseName}
SetupIconFile=..\assets\Lumen.ico
LicenseFile=..\LICENSE
Compression=lzma2/fast
SolidCompression=no
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\Lumen.exe
VersionInfoCompany=krambovic
VersionInfoDescription={#AppNameValue} installer
VersionInfoProductName={#AppNameValue}
VersionInfoProductVersion={#VersionInfo}
VersionInfoVersion={#VersionInfo}

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Excludes: "zapret\exe\*.sys,portable"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceDir}\zapret\exe\*.sys"; DestDir: "{app}\zapret\exe"; Flags: ignoreversion restartreplace uninsrestartdelete

[Icons]
Name: "{group}\{#AppNameValue}"; Filename: "{app}\Lumen.exe"; WorkingDir: "{app}"
Name: "{group}\Удалить {#AppNameValue}"; Filename: "{uninstallexe}"
Name: "{commondesktop}\{#AppNameValue}"; Filename: "{app}\Lumen.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\Lumen.exe"; Description: "Запустить Lumen"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupRun"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupApproved"

[InstallDelete]
Type: files; Name: "{app}\LumenKVN.exe"

[Code]
procedure StopZapretDrivers;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM LumenKVN.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM Lumen.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM winws.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM winws2.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C for %S in (Monkey WinDivert WinDivert14 WinDivert64 WinDivert2) do @sc stop %S >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C for %S in (Monkey WinDivert WinDivert14 WinDivert64 WinDivert2) do @sc delete %S >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C del /F /Q "{app}\zapret\exe\Monkey64.sys" "{app}\zapret\exe\WinDivert*.sys" >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    StopZapretDrivers;
end;
