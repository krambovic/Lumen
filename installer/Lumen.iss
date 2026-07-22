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
DefaultDirName={code:GetDefaultDirName}
UsePreviousAppDir=no
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

[Registry]
Root: HKLM; Subkey: "Software\Classes\lumen"; ValueType: string; ValueName: ""; ValueData: "URL:Lumen Protocol"; Flags: uninsdeletekey
Root: HKLM; Subkey: "Software\Classes\lumen"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKLM; Subkey: "Software\Classes\lumen\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\Lumen.exe,0"
Root: HKLM; Subkey: "Software\Classes\lumen\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\Lumen.exe"" ""%1"""

[Run]
Filename: "{app}\Lumen.exe"; Description: "Запустить Lumen"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Lumen KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupTask1"
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""LumenKVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupTask2"
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""lumen-kvn"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupTask3"
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Lumen_KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupTask4"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupRun"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""Lumen KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupRun1"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""LumenKVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupRun2"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""lumen-kvn"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupRun3"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ""Lumen_KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupRun4"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""Lumen"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupApproved"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""Lumen KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupApproved1"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""LumenKVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupApproved2"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""lumen-kvn"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupApproved3"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v ""Lumen_KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyStartupApproved4"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Classes\lumen /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteUrlProtocol"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Classes\lumen-kvn /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyUrlProtocol"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Classes\AppUserModelId\Lumen.Lumen /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteAumid"
Filename: "{cmd}"; Parameters: "/C reg delete HKCU\Software\Classes\AppUserModelId\Lumen.LumenKVN /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteLegacyAumid"

[InstallDelete]
Type: files; Name: "{app}\LumenKVN.exe"
Type: files; Name: "{app}\LumenKVN-qml.exe"
Type: files; Name: "{app}\assets\LumenKVN.ico"
Type: files; Name: "{app}\assets\LumenKVN.png"
Type: files; Name: "{commondesktop}\Lumen KVN.lnk"
Type: files; Name: "{userdesktop}\Lumen KVN.lnk"
Type: files; Name: "{commondesktop}\LumenKVN.lnk"
Type: files; Name: "{userdesktop}\LumenKVN.lnk"
Type: files; Name: "{commondesktop}\lumen-kvn.lnk"
Type: files; Name: "{userdesktop}\lumen-kvn.lnk"
Type: files; Name: "{commondesktop}\Lumen_KVN.lnk"
Type: files; Name: "{userdesktop}\Lumen_KVN.lnk"
Type: filesandordirs; Name: "{userappdata}\Microsoft\Windows\Start Menu\Programs\Lumen KVN"
Type: filesandordirs; Name: "{commonprograms}\Lumen KVN"
Type: filesandordirs; Name: "{userappdata}\Microsoft\Windows\Start Menu\Programs\LumenKVN"
Type: filesandordirs; Name: "{commonprograms}\LumenKVN"
Type: filesandordirs; Name: "{userappdata}\Microsoft\Windows\Start Menu\Programs\lumen-kvn"
Type: filesandordirs; Name: "{commonprograms}\lumen-kvn"
Type: filesandordirs; Name: "{userappdata}\Microsoft\Windows\Start Menu\Programs\Lumen_KVN"
Type: filesandordirs; Name: "{commonprograms}\Lumen_KVN"

[Code]
var
  PreviousInstallDir: String;

function HasLegacyInstallName(const Dir: String): Boolean;
var
  NormalizedDir: String;
  Leaf: String;
begin
  NormalizedDir := RemoveBackslashUnlessRoot(Dir);
  Leaf := ExtractFileName(NormalizedDir);
  Result :=
    (CompareText(Leaf, 'Lumen KVN') = 0) or
    (CompareText(Leaf, 'LumenKVN') = 0) or
    (CompareText(Leaf, 'lumen-kvn') = 0) or
    (CompareText(Leaf, 'Lumen_KVN') = 0) or
    (CompareText(NormalizedDir, 'C:\Program') = 0);
end;

function IsLegacyInstallDirectory(const Dir: String): Boolean;
var
  NormalizedDir: String;
begin
  NormalizedDir := RemoveBackslashUnlessRoot(Dir);
  Result := HasLegacyInstallName(NormalizedDir) and
    (FileExists(AddBackslash(NormalizedDir) + 'LumenKVN.exe') or
     FileExists(AddBackslash(NormalizedDir) + 'Lumen.exe'));
end;

function ReadRegisteredInstallDir(): String;
begin
  Result := '';
  if not RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1', 'InstallLocation', Result) then
    if not RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1', 'InstallLocation', Result) then
      if not RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1', 'InstallLocation', Result) then
        if not RegQueryStringValue(HKLM64, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1', 'InstallLocation', Result) then
          if not RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1', 'InstallLocation', Result) then
            RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1', 'InstallLocation', Result);
end;

function GetDefaultDirName(Param: String): String;
var
  RegisteredDir: String;
begin
  RegisteredDir := ReadRegisteredInstallDir();
  if (RegisteredDir <> '') and not HasLegacyInstallName(RegisteredDir) then
    Result := RegisteredDir
  else
    Result := ExpandConstant('{commonpf64}\Lumen');
end;

procedure UseCanonicalInstallDir;
begin
  if HasLegacyInstallName(WizardDirValue) then
    WizardForm.DirEdit.Text := ExpandConstant('{commonpf64}\Lumen');
end;

procedure InitializeWizard;
begin
  { /DIR from the pre-rename updater is already reflected by the wizard. }
  UseCanonicalInstallDir;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  if CurPageID = wpSelectDir then
    UseCanonicalInstallDir;
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  { Silent/very-silent updates do not visit the visible directory page. }
  UseCanonicalInstallDir;
  Result := '';
end;

function InitializeSetup(): Boolean;
begin
  PreviousInstallDir := ReadRegisteredInstallDir();
  if not IsLegacyInstallDirectory(PreviousInstallDir) then
    PreviousInstallDir := '';
  Result := True;
end;

procedure CleanLegacySystemEntries;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v "Lumen KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v "LumenKVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v "lumen-kvn" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v "Lumen_KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v "Lumen KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v "LumenKVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v "lumen-kvn" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run /v "Lumen_KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C schtasks /Delete /TN "Lumen KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C schtasks /Delete /TN "LumenKVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C schtasks /Delete /TN "lumen-kvn" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C schtasks /Delete /TN "Lumen_KVN" /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Classes\lumen-kvn /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\Software\Classes\AppUserModelId\Lumen.LumenKVN /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1 /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1 /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C reg delete HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1 /F >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure StopZapretDrivers;
var
  ResultCode: Integer;
begin
  CleanLegacySystemEntries;
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM LumenKVN.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{cmd}'), '/C taskkill /F /T /IM LumenKVN-qml.exe >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
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
  if (CurStep = ssPostInstall) and (PreviousInstallDir <> '') and
     (CompareText(RemoveBackslashUnlessRoot(PreviousInstallDir), RemoveBackslashUnlessRoot(ExpandConstant('{app}'))) <> 0) then
    DelTree(PreviousInstallDir, True, True, True);
end;
