#define MyAppName "Obsidian Indexer"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Obsidian Indexer"
#define MyAppExeName "obsidian-indexer-tray.exe"

[Setup]
AppId={{D7C03A93-C8C2-4A97-B5AF-53D2DA0345E5}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\Obsidian Indexer
DefaultGroupName=Obsidian Indexer
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=obsidian-indexer-setup

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "autostart"; Description: "Démarrer automatiquement avec Windows"; GroupDescription: "Options :"; Flags: unchecked

[Files]
Source: "..\target\release\obsidian-indexer-tray.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\obsidian-indexer.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\pdfium.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\indexer\assets\tray-icon.png"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\Obsidian Indexer (tray)"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Désinstaller Obsidian Indexer"; Filename: "{uninstallexe}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "ObsidianIndexerTray"; ValueData: """{app}\{#MyAppExeName}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Lancer Obsidian Indexer"; Flags: nowait postinstall skipifsilent
