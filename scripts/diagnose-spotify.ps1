param(
    [Parameter(Mandatory = $true)]
    [string]$Url
)

# Check the anonymous embed response without application data, cookies or login.
# Uses Windows/.NET HTTP; the application itself uses Rust/ureq.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Net.Http

$source = [uri]$Url.Trim()
if ($source.Scheme -ne 'https' -or $source.Host -ne 'open.spotify.com' -or
    $source.UserInfo -ne '' -or -not $source.IsDefaultPort -or
    $source.AbsolutePath -cnotmatch '^/(?:(?:intl-[^/]+|embed)/)?(track|album|playlist)/([A-Za-z0-9]{22})$') {
    throw 'Use an HTTPS open.spotify.com track, album or playlist URL.'
}
$kind = $Matches[1]
$id = $Matches[2]
$endpoint = "https://open.spotify.com/embed/$kind/$id"
$internetSettings = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
$systemProxyEnabled = $false
$pacConfigured = $false
if ($null -ne $internetSettings) {
    $enabledProperty = $internetSettings.PSObject.Properties['ProxyEnable']
    $pacProperty = $internetSettings.PSObject.Properties['AutoConfigURL']
    $systemProxyEnabled = $null -ne $enabledProperty -and $enabledProperty.Value -eq 1
    $pacConfigured = $null -ne $pacProperty -and -not [string]::IsNullOrWhiteSpace($pacProperty.Value)
}
$proxyEnvironmentNames = @('ALL_PROXY', 'HTTPS_PROXY', 'HTTP_PROXY', 'NO_PROXY') | Where-Object {
    -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_))
}
$handler = [System.Net.Http.HttpClientHandler]::new()
$handler.UseCookies = $false
$client = [System.Net.Http.HttpClient]::new($handler)
$client.Timeout = [TimeSpan]::FromSeconds(25)
$null = $client.DefaultRequestHeaders.TryAddWithoutValidation('User-Agent', 'Mozilla/5.0')
$response = $null
try {
    $response = $client.GetAsync($endpoint).GetAwaiter().GetResult()
    $html = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    $match = [regex]::Match($html, '<script\b[^>]*\bid="__NEXT_DATA__"[^>]*>(.*?)</script>', 'Singleline')
    $report = [ordered]@{
        transport = 'Windows/.NET HttpClient (application uses Rust/ureq)'
        windows_system_proxy_enabled = $systemProxyEnabled
        windows_proxy_pac_configured = $pacConfigured
        proxy_environment_variables_set = @($proxyEnvironmentNames)
        endpoint = $endpoint
        final_url = $response.RequestMessage.RequestUri.AbsoluteUri
        http_status = [int]$response.StatusCode
        content_type = [string]$response.Content.Headers.ContentType
        response_bytes = [System.Text.Encoding]::UTF8.GetByteCount($html)
        has_next_data = $match.Success
        page_status = $null
        entity_matches = $false
        track_count = 0
        result = 'missing-next-data'
    }
    if ($match.Success) {
        try {
            $data = $match.Groups[1].Value | ConvertFrom-Json
            # Optional JSON members are expected on Spotify error pages.
            Set-StrictMode -Off
            $report.page_status = $data.props.pageProps.status
            $entity = $data.props.pageProps.state.data.entity
            $report.entity_matches = $entity.uri -ceq "spotify:${kind}:$id"
            if ($report.entity_matches) {
                $report.track_count = if ($kind -eq 'track') { 1 } else { @($entity.trackList).Where({ $_.uri -like 'spotify:track:*' }).Count }
                $report.result = if ($report.track_count -gt 0) { 'metadata-available' } else { 'no-tracks' }
            } elseif ($report.page_status -ge 400) {
                $report.result = 'spotify-error-page'
            } elseif ($null -ne $entity) {
                $report.result = 'different-entity'
            } else {
                $report.result = 'missing-entity'
            }
        } catch {
            $report.result = 'invalid-next-data-json'
        } finally {
            Set-StrictMode -Version Latest
        }
    }
    if (-not $response.IsSuccessStatusCode) { $report.result = 'http-error' }
    # Never print HTML or session/access tokens embedded in __NEXT_DATA__.
    $report | ConvertTo-Json
} finally {
    if ($null -ne $response) { $response.Dispose() }
    $client.Dispose()
}
