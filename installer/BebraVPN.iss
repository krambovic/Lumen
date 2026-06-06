#ifndef AppVersion
#define AppVersion "0.0.0"
#endif

#ifndef SourceDir
#define SourceDir "..\dist\BebraVPN"
#endif

#ifndef OutputDir
#define OutputDir "..\dist"
#endif

[Setup]
AppId={{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}
AppName=Bebra VPN
AppVersion={#AppVersion}
AppPublisher=Bebra VPN
AppCopyright=Copyright (c) youtubediscord/zapret-kvn contributors and krambovic/bebra-kvn contributors
AppPublisherURL=https://github.com/krambovic/bebra-kvn
AppSupportURL=https://github.com/krambovic/bebra-kvn/issues
AppUpdatesURL=https://github.com/krambovic/bebra-kvn/releases
DefaultDirName={autopf}\Bebra VPN
DefaultGroupName=Bebra VPN
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=BebraVPN-Setup-windows-x64
SetupIconFile=..\assets\BebraVPN.ico
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\BebraVPN.exe
VersionInfoCompany=Bebra VPN
VersionInfoDescription=Bebra VPN installer
VersionInfoProductName=Bebra VPN
VersionInfoProductVersion={#AppVersion}

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "download-latest.ps1"; Flags: dontcopy
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\Bebra VPN"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"
Name: "{group}\Удалить Bebra VPN"; Filename: "{uninstallexe}"
Name: "{commondesktop}\Bebra VPN"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\BebraVPN.exe"; Description: "Запустить Bebra VPN"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Bebra VPN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"

[Code]
function DownloadLatestRelease(): Boolean;
var
  ResultCode: Integer;
  ScriptPath: String;
  Params: String;
begin
  Result := False;
  ForceDirectories(ExpandConstant('{app}'));
  ExtractTemporaryFile('download-latest.ps1');
  ScriptPath := ExpandConstant('{tmp}\download-latest.ps1');
  Params :=
    '-NoProfile -ExecutionPolicy Bypass -File "' + ScriptPath + '"' +
    ' -InstallDir "' + ExpandConstant('{app}') + '"';

  if Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    Result := ResultCode = 0;
  end;

  if not Result then
  begin
    MsgBox('Не удалось скачать последнюю версию Bebra VPN. Проверьте интернет и попробуйте снова.', mbError, MB_OK);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
  begin
    if not DownloadLatestRelease() then
    begin
      RaiseException('Latest release download failed');
    end;
  end;
end;
