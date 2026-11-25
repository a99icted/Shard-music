#ifndef AppVersion
  #define AppVersion "0.1.8"
#endif
#ifndef AppNumericVersion
  #define AppNumericVersion AppVersion
#endif
#ifdef TestBuild
  #define EditionName "Shard test"
  #define InstallFolder "Shard Test"
  #define EditionId "{{9FCE8209-88EF-4E16-A917-5FB7D60F703A}"
  #define ShellId "Shard.Test"
#else
  #define EditionName "Shard"
  #define InstallFolder "Shard"
  #define EditionId "{{5CBCE71D-68EF-4E41-9565-401586E1F370}"
  #define ShellId "Shard"
#endif
#ifndef PackageDir
  #error PackageDir must point to the prepared application directory.
#endif
#ifndef OutputPath
  #define OutputPath "..\dist"
#endif

[Setup]
AppId={#EditionId}
AppName={#EditionName}
AppVersion={#AppVersion}
VersionInfoVersion={#AppNumericVersion}
VersionInfoProductVersion={#AppNumericVersion}
VersionInfoProductTextVersion={#AppVersion}
VersionInfoTextVersion={#AppVersion}
AppPublisher=Shard contributors
AppVerName={#EditionName} {#AppVersion}
DefaultDirName={localappdata}\Programs\{#InstallFolder}
DefaultGroupName={#EditionName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir={#OutputPath}
OutputBaseFilename=Shard-{#AppVersion}-Setup-x64
SetupIconFile=..\assets\shard.ico
UninstallDisplayIcon={app}\shard.exe
UninstallDisplayName={#EditionName}
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
Name: "{group}\{#EditionName}"; Filename: "{app}\shard.exe"; WorkingDir: "{app}"; AppUserModelID: "{#ShellId}"
Name: "{autodesktop}\{#EditionName}"; Filename: "{app}\shard.exe"; WorkingDir: "{app}"; Tasks: desktopicon; AppUserModelID: "{#ShellId}"

[Run]
Filename: "{app}\shard.exe"; Description: "{cm:LaunchProgram,{#EditionName}}"; Flags: nowait postinstall skipifsilent; Check: not IsShardUpdate
Filename: "{app}\shard.exe"; Flags: nowait; Check: IsShardUpdate

; User music, SQLite, covers, backgrounds and preferences live outside {app}.
; Deliberately no UninstallDelete entries for the user's data directories.

[Code]
function IsShardUpdate: Boolean;
begin
  Result := ExpandConstant('{param:SHARDUPDATE|0}') = '1';
end;

function OpenProcess(dwDesiredAccess: LongWord; bInheritHandle: Boolean; dwProcessId: LongWord): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function WaitForSingleObject(hHandle: THandle; dwMilliseconds: LongWord): LongWord;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function CloseHandle(hObject: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';
function FindWindowEx(Parent, ChildAfter: THandle; ClassName: String; WindowName: LongWord): THandle;
  external 'FindWindowExW@user32.dll stdcall';
function GetWindowThreadProcessId(Window: THandle; var ProcessId: LongWord): LongWord;
  external 'GetWindowThreadProcessId@user32.dll stdcall';
function QueryFullProcessImageName(Process: THandle; Flags: LongWord; FileName: String; var Size: LongWord): Boolean;
  external 'QueryFullProcessImageNameW@kernel32.dll stdcall';
function PostThreadMessage(ThreadId, Msg, WParam, LParam: LongWord): Boolean;
  external 'PostThreadMessageW@user32.dll stdcall';

function CloseInstalledShard: String;
var
  Window, NextWindow, Process: THandle;
  ProcessId, ThreadId, PathLength: LongWord;
  ProcessPath, TargetPath: String;
begin
  Result := '';
  TargetPath := ExpandFileName(ExpandConstant('{app}\shard.exe'));
  Window := FindWindowEx(0, 0, 'ShardTrayWindow', 0);
  while Window <> 0 do begin
    NextWindow := FindWindowEx(0, Window, 'ShardTrayWindow', 0);
    ThreadId := GetWindowThreadProcessId(Window, ProcessId);
    { Query + synchronize only: never terminate a process or close another edition. }
    Process := OpenProcess($00101000, False, ProcessId);
    if Process <> 0 then begin
      PathLength := 32768;
      SetLength(ProcessPath, PathLength);
      if QueryFullProcessImageName(Process, 0, ProcessPath, PathLength) then begin
        SetLength(ProcessPath, PathLength);
        if CompareText(ExpandFileName(ProcessPath), TargetPath) = 0 then begin
          Log('Requesting graceful exit of installed Shard: ' + ProcessPath);
          { Shard 0.1.7 hides on WM_CLOSE. WM_QUIT stops its GPUI message loop,
            which runs the normal app shutdown hooks, including player/settings
            flush. This also works when the main window is hidden in the tray. }
          if not PostThreadMessage(ThreadId, $0012, 0, 0) then
            Log('Could not post the graceful exit request');
          if WaitForSingleObject(Process, 120000) <> 0 then begin
            Log('Installation refused: installed Shard has not exited');
            if ActiveLanguage = 'russian' then
              Result := 'Не удалось завершить Shard. Выберите «Выйти» в меню значка Shard в трее и повторите установку.'
            else
              Result := 'Could not exit Shard. Choose Quit from the Shard tray menu and retry installation.';
          end else
            Log('Installed Shard exited before file replacement');
        end;
      end;
      CloseHandle(Process);
    end;
    if Result <> '' then Exit;
    Window := NextWindow;
  end;
end;

function InitializeSetup: Boolean;
var
  ParentId: Integer;
  Parent: THandle;
begin
  Result := True;
  if not IsShardUpdate then Exit;
#ifdef TestBuild
  Result := False;
  Exit;
#endif
  ParentId := StrToIntDef(ExpandConstant('{param:SHARDPID|0}'), 0);
  if ParentId <= 0 then begin
    Log('Update refused: missing parent process ID');
    Result := False;
    Exit;
  end;
  Parent := OpenProcess($00100000, False, ParentId);
  if Parent = 0 then begin
    { ERROR_INVALID_PARAMETER means the parent has already exited. }
    Result := DLLGetLastError = 87;
    Exit;
  end;
  Log('Waiting for Shard to save settings and exit before installing');
  Result := WaitForSingleObject(Parent, 120000) = 0;
  CloseHandle(Parent);
  if not Result then Log('Update refused: Shard has not exited');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
#ifdef TestBuild
var
  ReleaseDirectory: String;
  Destination: String;
#endif
begin
  Result := '';
#ifdef TestBuild
  Destination := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  ReleaseDirectory := '';
  RegQueryStringValue(HKCU,
    'Software\Microsoft\Windows\CurrentVersion\Uninstall\{5CBCE71D-68EF-4E41-9565-401586E1F370}_is1',
    'InstallLocation', ReleaseDirectory);
  if (CompareText(Destination, RemoveBackslashUnlessRoot(ExpandConstant('{localappdata}\Programs\Shard'))) = 0) or
     ((ReleaseDirectory <> '') and (CompareText(Destination, RemoveBackslashUnlessRoot(ReleaseDirectory)) = 0)) then
    Result := 'Choose a separate folder for Shard test. The regular Shard installation cannot be overwritten.';
  if Result <> '' then Exit;
#endif
  Result := CloseInstalledShard;
end;
