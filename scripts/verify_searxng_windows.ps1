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

function Format-UnresponsiveEngine {
  param([object]$Item)

  if ($null -eq $Item) { return "" }
  if ($Item -is [string]) { return $Item }

  # 常见结构：{name, reason} 或 {engine, reason}
  $name = $null
  $reason = $null
  try { $name = $Item.name } catch {}
  try { if (-not $name) { $name = $Item.engine } } catch {}
  try { $reason = $Item.reason } catch {}

  if ($name -and $reason) { return "$name ($reason)" }
  if ($name) { return "$name" }

  # 兜底：如果是数组/可枚举对象，拼接为字符串
  if ($Item -is [System.Collections.IEnumerable]) {
    $parts = @()
    foreach ($p in $Item) {
      if ($null -ne $p) {
        $parts += [string]$p
      }
    }
    if ($parts.Count -gt 0) {
      return ($parts -join " ")
    }
  }

  return [string]$Item
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
  if ($resultCount -eq 0) {
    Write-Warning "results.count=0：当前可用搜索引擎可能不足或被限流，建议检查 SearXNG engines 配置。"
  }
  if ($resp.Json.unresponsive_engines) {
    $items = @($resp.Json.unresponsive_engines)
    $formatted = @()
    foreach ($item in $items) {
      $line = Format-UnresponsiveEngine -Item $item
      if ($line) {
        $formatted += $line
      }
    }
    if ($formatted.Count -gt 0) {
      Write-Host "unresponsive_engines: $($formatted -join '; ')"
    } else {
      Write-Host "unresponsive_engines: (present but unparsable)"
    }
  }

  Step "SearXNG 自检通过"
} catch {
  Write-Host "SearXNG 自检失败: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
}
