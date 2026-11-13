param(
    [string]$Destination = (Join-Path (Split-Path -Parent $PSScriptRoot) 'tools')
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$headers = @{ 'User-Agent' = 'Shard-setup' }
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $destinationPath -Force | Out-Null
$stage = Join-Path $destinationPath ('.setup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null

function Get-VerifiedAsset($Release, [string]$Name, [string]$ChecksumName) {
    $asset = @($Release.assets | Where-Object { $_.name -eq $Name })
    $checksums = @($Release.assets | Where-Object { $_.name -eq $ChecksumName })
    if ($asset.Count -ne 1 -or $checksums.Count -ne 1) { throw "Release asset not found: $Name" }
    $checksumFile = Join-Path $stage ($ChecksumName + '-' + [Guid]::NewGuid().ToString('N'))
    Invoke-WebRequest -Uri $checksums[0].browser_download_url -Headers $headers -OutFile $checksumFile -TimeoutSec 60
    $matching = @(Get-Content -LiteralPath $checksumFile | Where-Object { ($_ -split '\s+', 2)[1].TrimStart('*', ' ') -eq $Name })
    if ($matching.Count -ne 1) { throw "SHA256 checksum not found for $Name" }
    $expected = ($matching[0] -split '\s+', 2)[0]
    $archive = Join-Path $stage $Name
    Write-Host "Downloading $Name..."
    Invoke-WebRequest -Uri $asset[0].browser_download_url -Headers $headers -OutFile $archive -TimeoutSec 600
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "SHA256 verification failed: $Name" }
    return $archive
}

try {
    if (-not (Test-Path -LiteralPath (Join-Path $destinationPath 'yt-dlp.exe'))) {
        $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest' -Headers $headers
        $downloader = Get-VerifiedAsset $release 'yt-dlp.exe' 'SHA2-256SUMS'
        Move-Item -LiteralPath $downloader -Destination (Join-Path $destinationPath 'yt-dlp.exe')
    }
    if (-not (Test-Path -LiteralPath (Join-Path $destinationPath 'ffmpeg.exe')) -or -not (Test-Path -LiteralPath (Join-Path $destinationPath 'ffprobe.exe')) -or -not (Test-Path -LiteralPath (Join-Path $destinationPath 'licenses/ffmpeg/LICENSE.txt'))) {
        $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/yt-dlp/FFmpeg-Builds/releases/latest' -Headers $headers
        $archive = Get-VerifiedAsset $release 'ffmpeg-master-latest-win64-gpl.zip' 'checksums.sha256'
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
        try {
            foreach ($name in @('ffmpeg.exe', 'ffprobe.exe')) {
                $entries = @($zip.Entries | Where-Object { $_.FullName -match ('(^|/)bin/' + [Regex]::Escape($name) + '$') })
                if ($entries.Count -ne 1) { throw "Executable not found in verified archive: $name" }
                $output = Join-Path $stage $name
                [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $output)
                Move-Item -LiteralPath $output -Destination (Join-Path $destinationPath $name) -Force
            }
            $licenseDirectory = Join-Path $destinationPath 'licenses/ffmpeg'
            New-Item -ItemType Directory -Path $licenseDirectory -Force | Out-Null
            foreach ($entry in $zip.Entries) {
                if ($entry.Name -and $entry.FullName -match '(?i)(^|/)(licen[cs]e[^/]*|copying[^/]*|readme[^/]*|buildconf[^/]*)$') {
                    [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $licenseDirectory $entry.Name), $true)
                }
            }
            @{
                release = $release.html_url
                archive = 'ffmpeg-master-latest-win64-gpl.zip'
                sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
                source = 'https://github.com/yt-dlp/FFmpeg-Builds'
            } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $licenseDirectory 'upstream.json') -Encoding utf8
        } finally { $zip.Dispose() }
    }
    $ytLicenseDirectory = Join-Path $destinationPath 'licenses/yt-dlp'
    if (-not (Test-Path -LiteralPath (Join-Path $ytLicenseDirectory 'THIRD_PARTY_LICENSES.txt'))) {
        New-Item -ItemType Directory -Path $ytLicenseDirectory -Force | Out-Null
        $ytVersion = & (Join-Path $destinationPath 'yt-dlp.exe') --version
        if ($LASTEXITCODE -ne 0 -or "$ytVersion" -notmatch '^\d{4}\.\d{2}\.\d{2}$') { throw 'Unable to determine yt-dlp release for license notices' }
        foreach ($license in @('LICENSE', 'THIRD_PARTY_LICENSES.txt')) {
            Invoke-WebRequest -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$ytVersion/$license" -Headers $headers -OutFile (Join-Path $ytLicenseDirectory $license) -TimeoutSec 60
        }
    }
    & (Join-Path $destinationPath 'yt-dlp.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'yt-dlp verification failed' }
    & (Join-Path $destinationPath 'ffmpeg.exe') -version | Select-Object -First 1
    if ($LASTEXITCODE -ne 0) { throw 'FFmpeg verification failed' }
    & (Join-Path $destinationPath 'ffprobe.exe') -version | Select-Object -First 1
    if ($LASTEXITCODE -ne 0) { throw 'ffprobe verification failed' }
    Write-Host "YouTube components are ready: $destinationPath"
} finally {
    # Only the unique staging folder inside the resolved tools directory is removed.
    $resolvedStage = [System.IO.Path]::GetFullPath($stage)
    if ((Split-Path -Parent $resolvedStage) -ne $destinationPath -or (Split-Path -Leaf $resolvedStage) -notlike '.setup-*') { throw 'Unexpected staging path' }
    if (Test-Path -LiteralPath $resolvedStage) { Remove-Item -LiteralPath $resolvedStage -Recurse -Force }
}
