param(
  [string]$BaseUrl = "http://127.0.0.1:8080",
  [string]$Query = "rust"
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function New-NoProxyHttpClient {
  $handler = New-Object System.Net.Http.HttpClientHandler
  $handler.UseProxy = $false
  $client = New-Object System.Net.Http.HttpClient($handler)
  $client.Timeout = [TimeSpan]::FromSeconds(20)
  return $client
}

function Invoke-JsonNoProxy {
  param(
    [System.Net.Http.HttpClient]$Client,
    [string]$Url
  )

  $request = New-Object System.Net.Http.HttpRequestMessage ([System.Net.Http.HttpMethod]::Get), $Url
  $request.Headers.TryAddWithoutValidation("Accept", "application/json") | Out-Null
  $request.Headers.TryAddWithoutValidation("User-Agent", "Mozilla/5.0 llm-wiki-searxng-check") | Out-Null

  $response = $Client.SendAsync($request).GetAwaiter().GetResult()
  $raw = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
  $json = $null
  try {
    if ($raw) {
      $json = $raw | ConvertFrom-Json
    }
  } catch {}

  return [PSCustomObject]@{
    StatusCode = [int]$response.StatusCode
    IsSuccess  = $response.IsSuccessStatusCode
    Raw        = $raw
    Json       = $json
  }
}

try {
  $normalized = $BaseUrl.TrimEnd("/")
  $url = "$normalized/search?q=$([uri]::EscapeDataString($Query))&format=json&language=auto"
  $client = New-NoProxyHttpClient

  Step "检查 SearXNG 接口"
  Write-Host "URL: $url"
  $resp = Invoke-JsonNoProxy -Client $client -Url $url

  if (-not $resp.IsSuccess) {
    $preview = ($resp.Raw ?? "").ToString()
    if ($preview.Length -gt 280) { $preview = $preview.Substring(0, 280) + "..." }
    throw "请求失败: HTTP $($resp.StatusCode); body: $preview"
  }

  if ($null -eq $resp.Json) {
    throw "返回非 JSON: $($resp.Raw)"
  }

  $resultCount = 0
  if ($resp.Json.results) {
    $resultCount = @($resp.Json.results).Count
  }

  Write-Host "query: $($resp.Json.query)"
  Write-Host "number_of_results: $($resp.Json.number_of_results)"
  Write-Host "results.count: $resultCount"
  if ($resp.Json.unresponsive_engines) {
    Write-Host "unresponsive_engines: $($resp.Json.unresponsive_engines -join ', ')"
  }

  Step "SearXNG 自检通过"
} catch {
  Write-Host "SearXNG 自检失败: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
}
