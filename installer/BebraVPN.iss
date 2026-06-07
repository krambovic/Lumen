#ifndef AppVersion
#define AppVersion "0.0.0"
#endif

#ifndef SourceDir
#define SourceDir "..\dist\BebraVPN"
#endif

#ifndef OutputDir
#define OutputDir "..\dist"
#endif

; Per-edition identity. Defaults = Stable (classic) build.
; build_qml.py overrides these via /D so the Nightly (QML) edition installs
; side-by-side with its own AppId, name, install dir and setup filename.
#ifndef AppId
#define AppId "{{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}"
#endif

#ifndef AppNameValue
#define AppNameValue "Bebra VPN"
#endif

#ifndef OutputBaseName
#define OutputBaseName "BebraVPN-Setup-windows-x64"
#endif

[Setup]
AppId={#AppId}
AppName={#AppNameValue}
AppVersion={#AppVersion}
AppPublisher=Bebra VPN
AppCopyright=Copyright (c) youtubediscord/zapret-kvn contributors and krambovic/bebra-kvn contributors
AppPublisherURL=https://github.com/krambovic/bebra-kvn
AppSupportURL=https://github.com/krambovic/bebra-kvn/issues
AppUpdatesURL=https://github.com/krambovic/bebra-kvn/releases
DefaultDirName={autopf}\{#AppNameValue}
DefaultGroupName={#AppNameValue}
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseName}
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
VersionInfoDescription={#AppNameValue} installer
VersionInfoProductName={#AppNameValue}
VersionInfoProductVersion={#AppVersion}

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppNameValue}"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"
Name: "{group}\Удалить {#AppNameValue}"; Filename: "{uninstallexe}"
Name: "{commondesktop}\{#AppNameValue}"; Filename: "{app}\BebraVPN.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\BebraVPN.exe"; Description: "Запустить Bebra VPN"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Bebra VPN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"
