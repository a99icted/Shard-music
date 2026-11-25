param([string]$Compiler)

# Exercise the production Inno update code using an unrelated AppId, a tiny
# fixture executable, and temporary install/data directories. No real Shard
# installation, profile, shortcuts or running process is changed.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Compiler) { $Compiler = Join-Path $projectRoot '.local-data/packaging-tools/inno/ISCC.exe' }
if (-not (Test-Path -LiteralPath $Compiler)) { throw 'Build the Windows package first to cache Inno Setup' }
$smokeRoot = Join-Path ([IO.Path]::GetTempPath()) ('shard-updater-smoke-' + [Guid]::NewGuid().ToString('N'))
$payload = Join-Path $smokeRoot 'payload'
$destination = Join-Path $smokeRoot 'installed'
$data = Join-Path $smokeRoot 'data'
New-Item -ItemType Directory -Path $payload, $data -Force | Out-Null
$parent = $null
$setup = $null
$restarted = $null
$legacy = $null
$decoy = $null
$previousSmokeRoot = $env:SHARD_UPDATER_SMOKE_ROOT
try {
    $fixture = @'
#![windows_subsystem = "windows"]
use std::ffi::c_void;
type Handle = *mut c_void;
#[repr(C)]
struct WindowClass { style: u32, procedure: unsafe extern "system" fn(Handle,u32,usize,isize)->isize,
    class_extra: i32, window_extra: i32, instance: Handle, icon: Handle, cursor: Handle,
    background: Handle, menu: *const u16, name: *const u16 }
#[repr(C)]
struct Message { window: Handle, message: u32, wparam: usize, lparam: isize, time: u32,
    x: i32, y: i32, private: u32 }
#[link(name="user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WindowClass) -> u16;
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32,
        x:i32,y:i32,w:i32,h:i32,parent:Handle,menu:Handle,instance:Handle,param:Handle)->Handle;
    fn DefWindowProcW(window:Handle,msg:u32,wparam:usize,lparam:isize)->isize;
    fn PostMessageW(window:Handle,msg:u32,wparam:usize,lparam:isize)->i32;
    fn GetMessageW(message:*mut Message,window:Handle,min:u32,max:u32)->i32;
    fn DispatchMessageW(message:*const Message)->isize;
}
#[link(name="kernel32")]
unsafe extern "system" { fn GetModuleHandleW(name:*const u16)->Handle; }
fn marker(name:&str) {
    let root = std::path::PathBuf::from(std::env::var_os("SHARD_UPDATER_SMOKE_ROOT").unwrap());
    let prefix = if std::env::args().any(|arg|arg=="--unrelated") { "decoy" } else { "legacy" };
    std::fs::write(root.join(format!("{prefix}-{name}")), std::process::id().to_string()).unwrap();
}
unsafe extern "system" fn window_proc(window:Handle,msg:u32,wparam:usize,lparam:isize)->isize {
    if msg == 0x10 { marker("close-hidden"); return 0; } // Same hide-on-close behavior as 0.1.7.
    unsafe { DefWindowProcW(window,msg,wparam,lparam) }
}
fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("SHARD_UPDATER_SMOKE_ROOT").unwrap());
    if std::env::args().any(|arg|arg=="--legacy") {
        unsafe {
            let class:Vec<u16>="ShardTrayWindow\0".encode_utf16().collect();
            let instance=GetModuleHandleW(std::ptr::null());
            let window_class=WindowClass { style:0,procedure:window_proc,class_extra:0,window_extra:0,
                instance,icon:std::ptr::null_mut(),cursor:std::ptr::null_mut(),background:std::ptr::null_mut(),
                menu:std::ptr::null(),name:class.as_ptr() };
            assert_ne!(RegisterClassW(&window_class),0);
            let window=CreateWindowExW(0,class.as_ptr(),class.as_ptr(),0,0,0,0,0,
                std::ptr::null_mut(),std::ptr::null_mut(),instance,std::ptr::null_mut());
            assert!(!window.is_null());
            PostMessageW(window,0x10,0,0);
            let mut message:Message=std::mem::zeroed();
            marker("ready");
            while GetMessageW(&mut message,std::ptr::null_mut(),0,0)>0 { DispatchMessageW(&message); }
        }
        // Stand in for the GPUI quit hooks that flush playback/settings.
        marker("flushed");
        return;
    }
    if std::env::args().any(|arg| arg == "--parent") {
        std::fs::write(root.join("parent-ready"), std::process::id().to_string()).unwrap();
    } else {
        std::fs::write(root.join("restarted"), std::process::id().to_string()).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_secs(180));
}
'@
    $fixture | Set-Content -LiteralPath (Join-Path $smokeRoot 'fixture.rs') -Encoding utf8
    rustc --edition=2024 --crate-name updater_fixture (Join-Path $smokeRoot 'fixture.rs') -o (Join-Path $payload 'shard.exe')
    if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed' }
    $source = Get-Content -LiteralPath (Join-Path $projectRoot 'installer/shard.iss') -Raw
    $smokeId = [Guid]::NewGuid().ToString().ToUpperInvariant()
    $source = $source.Replace('5CBCE71D-68EF-4E41-9565-401586E1F370', $smokeId)
    $source = $source.Replace('#define EditionName "Shard"', '#define EditionName "Shard updater smoke"')
    $source = $source.Replace('SetupIconFile=..\assets\shard.ico', ('SetupIconFile=' + (Join-Path $projectRoot 'assets/shard.ico')))
    $source = $source.Replace('LicenseFile=..\LICENSE', ('LicenseFile=' + (Join-Path $projectRoot 'LICENSE')))
    $source = [regex]::Replace($source, '(?ms)^\[Icons\].*?(?=^\[Run\])', '')
    $sourcePath = Join-Path $smokeRoot 'smoke.iss'
    $source | Set-Content -LiteralPath $sourcePath -Encoding utf8
    & $Compiler /Qp '/DAppVersion=0.1.8' "/DPackageDir=$payload" "/DOutputPath=$smokeRoot" $sourcePath
    if ($LASTEXITCODE -ne 0) { throw 'Smoke installer compilation failed' }

    # Sentinels stand in for the external library and preferences.
    [IO.File]::WriteAllText((Join-Path $data 'library.sqlite'), 'library sentinel')
    [IO.File]::WriteAllText((Join-Path $data 'settings.json'), '{"update_notifications":false}')
    $before = @(Get-ChildItem -LiteralPath $data -File | Get-FileHash | ForEach-Object Hash)
    $env:SHARD_UPDATER_SMOKE_ROOT = $smokeRoot
    $parent = Start-Process -FilePath (Join-Path $payload 'shard.exe') -ArgumentList '--parent' -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath (Join-Path $smokeRoot 'parent-ready'))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Parent fixture did not start' }
        Start-Sleep -Milliseconds 100
    }
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/CURRENTUSER', '/SHARDUPDATE=1', "/SHARDPID=$($parent.Id)", "/DIR=`"$destination`"", "/LOG=`"$(Join-Path $smokeRoot 'install.log')`"")
    $setup = Start-Process -FilePath (Join-Path $smokeRoot 'Shard-0.1.8-Setup-x64.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 200
        if ($setup.HasExited) { throw 'Installer exited before waiting for the parent' }
        $log = if (Test-Path -LiteralPath (Join-Path $smokeRoot 'install.log')) { Get-Content -LiteralPath (Join-Path $smokeRoot 'install.log') -Raw } else { '' }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Installer did not enter the parent wait' }
    } until ($log -match 'Waiting for Shard to save settings and exit')
    if (Test-Path -LiteralPath (Join-Path $destination 'shard.exe')) { throw 'Installation started before parent exit' }
    Stop-Process -Id $parent.Id
    $parent.WaitForExit()
    if (-not $setup.WaitForExit(60000)) { throw 'Installation timed out' }
    if ($setup.ExitCode -ne 0) { throw "Installer failed: $($setup.ExitCode). See $smokeRoot" }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath (Join-Path $smokeRoot 'restarted'))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Automatic restart did not happen' }
        Start-Sleep -Milliseconds 100
    }
    $restartId = [int](Get-Content -LiteralPath (Join-Path $smokeRoot 'restarted') -Raw)
    $restarted = Get-Process -Id $restartId
    if ($restarted.Path -ne (Join-Path $destination 'shard.exe')) { throw 'Unexpected restarted executable' }
    if ((Get-FileHash -LiteralPath (Join-Path $destination 'shard.exe')).Hash -ne (Get-FileHash -LiteralPath (Join-Path $payload 'shard.exe')).Hash) { throw 'Installed file mismatch' }
    $after = @(Get-ChildItem -LiteralPath $data -File | Get-FileHash | ForEach-Object Hash)
    if (@(Compare-Object $before $after).Count) { throw 'External data changed' }
    Stop-Process -Id $restarted.Id
    $restarted.WaitForExit()
    $restarted = $null

    # Manual 0.1.7 -> 0.1.8 path: the running target hides on ordinary close.
    # An identically named process/window from another directory must survive.
    $decoyDirectory = Join-Path $smokeRoot 'unrelated-edition'
    New-Item -ItemType Directory -Path $decoyDirectory | Out-Null
    Copy-Item -LiteralPath (Join-Path $payload 'shard.exe') -Destination $decoyDirectory
    $decoy = Start-Process -FilePath (Join-Path $decoyDirectory 'shard.exe') -ArgumentList '--legacy', '--unrelated' -WindowStyle Hidden -PassThru
    $legacy = Start-Process -FilePath (Join-Path $destination 'shard.exe') -ArgumentList '--legacy' -WindowStyle Hidden -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not ((Test-Path -LiteralPath (Join-Path $smokeRoot 'legacy-close-hidden')) -and (Test-Path -LiteralPath (Join-Path $smokeRoot 'decoy-close-hidden')))) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Legacy hide-on-close fixtures did not start' }
        Start-Sleep -Milliseconds 100
    }
    if ($legacy.HasExited -or $decoy.HasExited) { throw 'Ordinary close unexpectedly exited a fixture' }
    $manualLog = Join-Path $smokeRoot 'manual-install.log'
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/CURRENTUSER', "/DIR=`"$destination`"", "/LOG=`"$manualLog`"")
    $setup = Start-Process -FilePath (Join-Path $smokeRoot 'Shard-0.1.8-Setup-x64.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru
    if (-not $setup.WaitForExit(30000)) { throw "Manual update timed out. See $manualLog" }
    if ($setup.ExitCode -ne 0) { throw "Manual update failed: $($setup.ExitCode). See $manualLog" }
    if (-not $legacy.WaitForExit(5000) -or $legacy.ExitCode -ne 0) { throw 'Legacy fixture did not exit normally' }
    if (-not (Test-Path -LiteralPath (Join-Path $smokeRoot 'legacy-flushed'))) { throw 'Legacy shutdown flush did not run' }
    if ($decoy.HasExited -or (Test-Path -LiteralPath (Join-Path $smokeRoot 'decoy-flushed'))) { throw 'Updater affected an unrelated edition' }
    $manualText = Get-Content -LiteralPath $manualLog -Raw
    if ($manualText -notmatch 'Installed Shard exited before file replacement') { throw 'Missing graceful exit confirmation in installer log' }
    $after = @(Get-ChildItem -LiteralPath $data -File | Get-FileHash | ForEach-Object Hash)
    if (@(Compare-Object $before $after).Count) { throw 'Manual update changed external data' }
    $report = [ordered]@{ parentExitWait = 'passed'; silentInstall = 'passed'; automaticRestart = 'passed'; manualUpdateOfRunningTrayApp = 'passed'; gracefulShutdownFlush = 'passed'; unrelatedEditionUntouched = 'passed'; externalDataUnchanged = 'passed'; isolatedAppId = $smokeId; logDirectory = $smokeRoot }
    $report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $projectRoot 'dist/windows-updater-smoke-verification.json') -Encoding utf8
    $report | ConvertTo-Json
} finally {
    $env:SHARD_UPDATER_SMOKE_ROOT = $previousSmokeRoot
    foreach ($owned in @($parent, $restarted, $legacy, $decoy, $setup)) {
        if ($owned -and -not $owned.HasExited) { Stop-Process -Id $owned.Id -ErrorAction SilentlyContinue }
    }
    $uninstaller = Join-Path $destination 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        $cleanup = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -WindowStyle Hidden -PassThru
        if (-not $cleanup.WaitForExit(30000) -or $cleanup.ExitCode -ne 0) { Write-Warning "Uninstall smoke fixture manually: $uninstaller" }
    }
    # Keep temporary fixtures and logs for diagnosis. The real install/profile
    # and registry identity were never used.
}
