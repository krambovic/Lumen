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
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Bebra VPN"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"
Name: "{group}\Удалить Bebra VPN"; Filename: "{uninstallexe}"
Name: "{commondesktop}\Bebra VPN"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\BebraVPN.exe"; Description: "Запустить Bebra VPN"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Bebra VPN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"
