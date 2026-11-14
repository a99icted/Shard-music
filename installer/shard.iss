#ifndef AppVersion
  #define AppVersion "0.1.2"
#endif
#ifndef PackageDir
  #error PackageDir must point to the prepared application directory.
#endif
#ifndef OutputPath
  #define OutputPath "..\dist"
#endif

[Setup]
AppId={{5CBCE71D-68EF-4E41-9565-401586E1F370}
AppName=Shard
AppVersion={#AppVersion}
AppPublisher=Shard contributors
AppVerName=Shard {#AppVersion}
DefaultDirName={localappdata}\Programs\Shard
DefaultGroupName=Shard
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir={#OutputPath}
OutputBaseFilename=Shard-{#AppVersion}-Setup-x64
SetupIconFile=..\assets\shard.ico
UninstallDisplayIcon={app}\shard.exe
UninstallDisplayName=Shard
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
LZMAUseSeparateProcess=yes
LZMANumBlockThreads=2
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Shard"; Filename: "{app}\shard.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Shard"; Filename: "{app}\shard.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\shard.exe"; Description: "{cm:LaunchProgram,Shard}"; Flags: nowait postinstall skipifsilent

; User music, SQLite, covers, backgrounds and preferences live outside {app}.
; Deliberately no UninstallDelete entries for the user's data directories.
