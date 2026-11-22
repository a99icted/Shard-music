param([string]$Version)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $Version) {
    $versionLine = Get-Content (Join-Path $projectRoot 'Cargo.toml') | Where-Object { $_ -match '^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"' } | Select-Object -First 1
    $Version = [Regex]::Match($versionLine, '"([^"]+)"').Groups[1].Value
}
$releaseVersion = $Version
if ($releaseVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid package version' }
$distRoot = Join-Path $projectRoot 'dist'
$installerPath = Join-Path $distRoot "Shard-$releaseVersion-Setup-x64.exe"
$manifestPath = Join-Path $distRoot "Shard-$releaseVersion-manifest.json"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.version -ne $releaseVersion) { throw 'Manifest version mismatch' }
$stageMatches = @(Get-ChildItem -LiteralPath (Join-Path $projectRoot '.local-data/packaging-tools') -Directory -Filter 'payload-*' | Where-Object {
    $candidate = Join-Path $_.FullName 'package-manifest.json'
    (Test-Path -LiteralPath $candidate) -and ((Get-Content -LiteralPath $candidate -Raw | ConvertFrom-Json).version -eq $releaseVersion)
} | Sort-Object LastWriteTime -Descending)
if (-not $stageMatches.Count) { throw 'No matching staged payload' }
$stageRoot = $stageMatches[0].FullName
if ((Get-FileHash -LiteralPath (Join-Path $stageRoot 'package-manifest.json')).Hash -ne (Get-FileHash -LiteralPath $manifestPath).Hash) { throw 'Staged manifest differs from distribution manifest' }
foreach ($entry in $manifest.files) {
    $filePath = [IO.Path]::GetFullPath((Join-Path $stageRoot $entry.path))
    if (-not $filePath.StartsWith($stageRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Payload path escapes staging directory' }
    if ((Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Payload checksum mismatch: $($entry.path)" }
}
$actualFiles = @(Get-ChildItem -LiteralPath $stageRoot -File -Recurse | Where-Object { $_.FullName -ne (Join-Path $stageRoot 'package-manifest.json') })
if ($actualFiles.Count -ne $manifest.files.Count) { throw 'Unexpected payload files' }
$paths = @($manifest.files | ForEach-Object { $_.path })
foreach ($required in @('shard.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'tools/node.exe', 'tools/yt-dlp.exe', 'tools/ffmpeg.exe', 'tools/ffprobe.exe', 'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'licenses/rust-dependencies.json')) {
    if ($required -notin $paths) { throw "Required component missing: $required" }
}
if (@($paths | Where-Object { $_ -match '(?i)(^|/)(audio|artwork|backgrounds|sync-cache|staging|target|\.git)/|\.(sqlite|sqlite3)(-|$)|(^|/)(settings|preferences)\.json$' }).Count) { throw 'Personal or development data in package' }
$payloadExecutable = Join-Path $stageRoot 'shard.exe'
$releaseExecutable = Join-Path $projectRoot 'target/release/shard.exe'
if ((Get-FileHash -LiteralPath $payloadExecutable).Hash -ne (Get-FileHash -LiteralPath $releaseExecutable).Hash) { throw 'Packaged executable differs from release build' }
$executableInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($payloadExecutable)
if ("$($executableInfo.FileMajorPart).$($executableInfo.FileMinorPart).$($executableInfo.FileBuildPart)" -ne $releaseVersion) { throw 'Executable version mismatch' }
$installerInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($installerPath)
if ($installerInfo.ProductVersion.Trim([char[]]@(0, 32)) -ne $releaseVersion) { throw "Installer version mismatch: $($installerInfo.ProductVersion.Trim([char[]]@(0, 32)))" }
$binary = [IO.File]::ReadAllBytes($payloadExecutable)
$peOffset = [BitConverter]::ToInt32($binary, 0x3c)
if ([BitConverter]::ToUInt16($binary, $peOffset + 4) -ne 0x8664) { throw 'Executable is not AMD64' }
$digest = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$checksumText = (Get-Content -LiteralPath ($installerPath + '.sha256') -Raw).Trim()
if ($checksumText -ne "$digest  $([IO.Path]::GetFileName($installerPath))") { throw 'Installer checksum file mismatch' }
$report = [ordered]@{
    installer = [IO.Path]::GetFileName($installerPath)
    version = $releaseVersion
    bytes = (Get-Item -LiteralPath $installerPath).Length
    sha256 = $digest
    verifiedPayloadFiles = $manifest.files.Count
    payloadHashes = 'passed'
    releaseExecutableMatchesPayload = $true
    executableArchitecture = 'AMD64'
    executableFileVersion = "$($executableInfo.FileMajorPart).$($executableInfo.FileMinorPart).$($executableInfo.FileBuildPart).$($executableInfo.FilePrivatePart)"
    installerProductVersion = $installerInfo.ProductVersion.Trim([char[]]@(0, 32))
    requiredComponents = 'passed'
    personalLibraryFilesIncluded = $false
    installerExecution = 'not performed; verified compiled installer and complete staged payload'
}
$report | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $distRoot "Shard-$releaseVersion-verification.json") -Encoding utf8
$report | ConvertTo-Json


