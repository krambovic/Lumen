#ifndef AppVersion
#define AppVersion "0.0.0"
#endif

#ifndef VersionInfo
#define VersionInfo "0.0.0.0"
#endif

#ifndef SourceDir
#define SourceDir "..\dist\LumenKVN"
#endif

#ifndef OutputDir
#define OutputDir "..\dist"
#endif

#ifndef AppId
#define AppId "{{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}"
#endif

#ifndef AppNameValue
#define AppNameValue "Lumen KVN"
#endif

#ifndef OutputBaseName
#define OutputBaseName "LumenKVN-Setup-windows-x64"
#endif

[Setup]
AppId={#AppId}
AppName={#AppNameValue}
AppVersion={#AppVersion}
AppPublisher=Lumen KVN
AppCopyright=Copyright (c) youtubediscord/zapret-kvn contributors and krambovic/lumen-kvn contributors
AppPublisherURL=https://github.com/krambovic/lumen-kvn
AppSupportURL=https://github.com/krambovic/lumen-kvn/issues
AppUpdatesURL=https://github.com/krambovic/lumen-kvn/releases
DefaultDirName={commonpf64}\{#AppNameValue}
DefaultGroupName={#AppNameValue}
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseName}
SetupIconFile=..\assets\LumenKVN.ico
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\LumenKVN.exe
VersionInfoCompany=Lumen KVN
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
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppNameValue}"; Filename: "{app}\LumenKVN.exe"; WorkingDir: "{app}"
Name: "{group}\Удалить {#AppNameValue}"; Filename: "{uninstallexe}"
Name: "{commondesktop}\{#AppNameValue}"; Filename: "{app}\LumenKVN.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\LumenKVN.exe"; Description: "Запустить Lumen KVN"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C schtasks /Delete /TN ""Lumen KVN"" /F >nul 2>nul"; Flags: runhidden; RunOnceId: "DeleteStartupTask"
