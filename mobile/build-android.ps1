param([switch]$Release)
$ErrorActionPreference = 'Stop'
$project = Join-Path $PSScriptRoot 'android'
if (-not $env:JAVA_HOME) { throw 'Set JAVA_HOME to JDK 17 or 21 (not a JRE).' }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android/Sdk' }
if (-not (Test-Path -LiteralPath (Join-Path $env:ANDROID_HOME 'ndk/27.2.12479018'))) {
    throw 'Install Android SDK platform 35, build-tools 35.0.0 and NDK 27.2.12479018 using SDK Manager.'
}
& rustup target add aarch64-linux-android x86_64-linux-android
if ($LASTEXITCODE -ne 0) { throw 'Could not install Rust Android targets.' }
if (-not (Get-Command cargo-ndk -ErrorAction SilentlyContinue)) {
    & cargo install cargo-ndk --version 4.1.2 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Could not install cargo-ndk.' }
}
Push-Location $project
try {
    $task = if ($Release) { 'assembleRelease' } else { 'assembleDebug' }
    & ./gradlew.bat $task testDebugUnitTest lintDebug
    if ($LASTEXITCODE -ne 0) { throw 'Android build or verification failed.' }
} finally { Pop-Location }
