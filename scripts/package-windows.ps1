param(
    [switch]$SkipBuild,
    [string]$Compiler,
    [string]$RuntimeDirectory
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$projectRoot = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $projectRoot '.local-data/packaging-tools'
$dist = Join-Path $projectRoot 'dist'
New-Item -ItemType Directory -Path $cache, $dist -Force | Out-Null
$headers = @{ 'User-Agent' = 'Shard-packaging' }

function Get-VerifiedDownload([string]$Url, [string]$File, [string]$Sha256) {
    if ((Test-Path -LiteralPath $File) -and (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash -eq $Sha256) { return }
    $partial = $File + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
    try {
        Invoke-WebRequest -Uri $Url -Headers $headers -OutFile $partial -TimeoutSec 600
        if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $Sha256) { throw "Checksum mismatch: $Url" }
        Move-Item -LiteralPath $partial -Destination $File -Force
    } finally {
        if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
    }
}

if (-not $Compiler) {
    $installed = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($installed) { $Compiler = $installed.Source }
}
if (-not $Compiler) {
    # Install the compiler into the ignored local build cache, never into the app payload.
    $compilerDirectory = Join-Path $cache 'inno'
    $Compiler = Join-Path $compilerDirectory 'ISCC.exe'
    if (-not (Test-Path -LiteralPath $Compiler)) {
        $setup = Join-Path $cache 'innosetup-7.1.1-x64.exe'
        Get-VerifiedDownload 'https://github.com/jrsoftware/issrc/releases/download/is-7_1_1/innosetup-7.1.1-x64.exe' $setup '8a5a6711c3766a6e5fb74f280ba0e3551e719ce94538d901fdbd1f8f9920b3f0'
        $process = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/CURRENTUSER', '/NOICONS', '/TYPE=custom', '/COMPONENTS=main', '/TASKS=', ('/DIR="' + $compilerDirectory + '"')) -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "Inno Setup compiler installation failed: $($process.ExitCode)" }
    }
}
if (-not (Test-Path -LiteralPath $Compiler)) { throw 'Inno Setup compiler was not found' }

Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        $previousIncremental = $env:CARGO_INCREMENTAL
        try {
            $env:CARGO_INCREMENTAL = '0'
            cargo build --release --locked
            if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
        } finally { $env:CARGO_INCREMENTAL = $previousIncremental }
    }
    $executable = Join-Path $projectRoot 'target/release/shard.exe'
    if (-not (Test-Path -LiteralPath $executable)) { throw 'target/release/shard.exe is missing' }
    $versionLine = Get-Content Cargo.toml | Where-Object { $_ -match '^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"' } | Select-Object -First 1
    $version = [Regex]::Match($versionLine, '"([^"]+)"').Groups[1].Value
    if (-not $version) { throw 'Package version was not found' }

    & (Join-Path $PSScriptRoot 'setup-youtube.ps1')
    $nodeVersion = 'v24.22.3'
    $nodeArchive = Join-Path $cache "node-$nodeVersion-win-x64.zip"
    Get-VerifiedDownload "https://nodejs.org/dist/$nodeVersion/node-$nodeVersion-win-x64.zip" $nodeArchive 'e612af99f626ab415ea225af6bce5c971feb448cb5cae250bb1c3fa50c5f7d9f'

    # A fresh, explicit staging directory excludes music, settings, developer tools and caches.
    $stage = Join-Path $cache ('payload-' + [Guid]::NewGuid().ToString('N'))
    $stageTools = Join-Path $stage 'tools'
    $licenses = Join-Path $stage 'licenses'
    New-Item -ItemType Directory -Path $stageTools, $licenses -Force | Out-Null
    Copy-Item -LiteralPath $executable -Destination $stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE'), (Join-Path $projectRoot 'THIRD_PARTY_NOTICES.md') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $projectRoot 'vendor/gpui/LICENSE-APACHE') -Destination (Join-Path $licenses 'GPUI-APACHE-2.0.txt')
    $metadataText = cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Unable to collect dependency license notices' }
    $metadata = $metadataText | ConvertFrom-Json
    $licenseIndex = @()
    foreach ($package in $metadata.packages) {
        if ($package.name -eq 'shard') { continue }
        $packageDirectory = Split-Path -Parent $package.manifest_path
        $licenseDestination = Join-Path $licenses ("rust/" + $package.name + '-' + $package.version)
        $licenseFiles = @(Get-ChildItem -LiteralPath $packageDirectory -File | Where-Object { $_.Name -match '^(?i:licen[cs]e|copying|notice|unlicense)' })
        if ($package.license_file) {
            $explicitLicense = [System.IO.Path]::GetFullPath((Join-Path $packageDirectory $package.license_file))
            if (Test-Path -LiteralPath $explicitLicense) { $licenseFiles += Get-Item -LiteralPath $explicitLicense }
        }
        if ($licenseFiles.Count) {
            New-Item -ItemType Directory -Path $licenseDestination -Force | Out-Null
            $licenseFiles | Sort-Object FullName -Unique | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $licenseDestination -Force }
        }
        $licenseIndex += [ordered]@{name = $package.name; version = $package.version; license = $package.license; repository = $package.repository}
    }
    $licenseIndex | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $licenses 'rust-dependencies.json') -Encoding utf8
    foreach ($name in @('yt-dlp.exe', 'ffmpeg.exe', 'ffprobe.exe')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot "tools/$name") -Destination $stageTools
    }
    if (Test-Path -LiteralPath (Join-Path $projectRoot 'tools/licenses')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot 'tools/licenses') -Destination (Join-Path $licenses 'download-tools') -Recurse
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($nodeArchive)
    try {
        foreach ($file in @('node.exe', 'LICENSE')) {
            $entry = $zip.GetEntry("node-$nodeVersion-win-x64/$file")
            if (-not $entry) { throw "Node.js archive is missing $file" }
            $output = if ($file -eq 'node.exe') { Join-Path $stageTools $file } else { Join-Path $licenses 'Node.js-LICENSE.txt' }
            [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $output)
        }
    } finally { $zip.Dispose() }

    if (-not $RuntimeDirectory) {
        $patterns = @()
        foreach ($programRoot in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
            $patterns += Join-Path $programRoot 'Microsoft Visual Studio/*/*/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT'
        }
        $RuntimeDirectory = Get-ChildItem -Path $patterns -Directory -ErrorAction SilentlyContinue |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'vcruntime140.dll') } |
            Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
    if (-not $RuntimeDirectory) { throw 'MSVC redistributable CRT folder was not found; pass -RuntimeDirectory' }
    Copy-Item -Path (Join-Path $RuntimeDirectory '*.dll') -Destination $stage
    @'
Microsoft Visual C++ runtime DLLs are unmodified redistributable files from the
Microsoft Visual Studio VC/Redist/MSVC x64 CRT directory. They are distributed
under Microsoft's applicable Visual Studio / Visual C++ runtime license.
https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
https://visualstudio.microsoft.com/license-terms/
'@ | Set-Content -LiteralPath (Join-Path $licenses 'Microsoft-Visual-C++-Runtime.txt') -Encoding utf8

    $nodeOutput = & (Join-Path $stageTools 'node.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled Node.js failed to start' }
    $ytVersion = & (Join-Path $stageTools 'yt-dlp.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled yt-dlp failed to start' }
    $ffVersion = & (Join-Path $stageTools 'ffmpeg.exe') -version | Select-Object -First 1
    if ($LASTEXITCODE -ne 0) { throw 'Bundled FFmpeg failed to start' }
    & (Join-Path $stageTools 'ffmpeg.exe') -version | Set-Content -LiteralPath (Join-Path $licenses 'FFmpeg-build.txt') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Unable to record FFmpeg build information' }
    $manifest = [ordered]@{
        app = 'Shard'; version = $version; architecture = 'x86_64'; node = "$nodeOutput"
        yt_dlp = "$ytVersion"; ffmpeg = "$ffVersion"
        node_source = "https://nodejs.org/dist/$nodeVersion/"
        downloader_source = 'https://github.com/yt-dlp/yt-dlp'
        ffmpeg_source = 'https://github.com/yt-dlp/FFmpeg-Builds'
        files = @(Get-ChildItem -LiteralPath $stage -File -Recurse | ForEach-Object {
            [ordered]@{path = $_.FullName.Substring($stage.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
        })
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $stage 'package-manifest.json') -Encoding utf8
    & $Compiler /Qp ("/DAppVersion=$version") ("/DPackageDir=$stage") ("/DOutputPath=$dist") (Join-Path $projectRoot 'installer/shard.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    $installer = Join-Path $dist "Shard-$version-Setup-x64.exe"
    $digest = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    "$digest  $([System.IO.Path]::GetFileName($installer))" | Set-Content -LiteralPath ($installer + '.sha256') -Encoding ascii
    Copy-Item -LiteralPath (Join-Path $stage 'package-manifest.json') -Destination (Join-Path $dist "Shard-$version-manifest.json") -Force
    Write-Host "Installer ready: $installer"
    Write-Host "SHA256: $digest"
    Write-Host "Payload retained for verification: $stage"
} finally { Pop-Location }
