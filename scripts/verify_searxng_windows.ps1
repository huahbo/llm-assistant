param(
  [string]$BaseUrl = "http://127.0.0.1:8080",
  [string]$Query = "rust",
  [string]$Language = "auto",
  [string]$Categories = "general",
  [int]$SafeSearch = 0,
  [switch]$DisableFallback
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

function Build-SearxngUrl {
  param(
    [string]$NormalizedBaseUrl,
    [string]$QueryText,
    [string]$LanguageCode,
    [string]$CategoriesValue,
    [string]$SafeSearchValue
  )

  $q = [uri]::EscapeDataString($QueryText)
  $lang = [uri]::EscapeDataString($LanguageCode)
  $cats = [uri]::EscapeDataString($CategoriesValue)
  $safe = [uri]::EscapeDataString($SafeSearchValue)
  return "$NormalizedBaseUrl/search?q=$q&format=json&language=$lang&categories=$cats&safesearch=$safe&pageno=1&count=10"
}

try {
  $normalized = $BaseUrl.TrimEnd("/")
  $client = New-NoProxyHttpClient

  $profiles = @(
    [PSCustomObject]@{
      Name       = "primary"
      Language   = $Language
      Categories = $Categories
      SafeSearch = [string]$SafeSearch
    }
  )
  if (-not $DisableFallback) {
    $profiles += [PSCustomObject]@{
      Name       = "fallback"
      Language   = "all"
      Categories = "general,news"
      SafeSearch = "0"
    }
  }

  $attemptErrors = @()
  $best = $null

  foreach ($profile in $profiles) {
    $url = Build-SearxngUrl `
      -NormalizedBaseUrl $normalized `
      -QueryText $Query `
      -LanguageCode $profile.Language `
      -CategoriesValue $profile.Categories `
      -SafeSearchValue $profile.SafeSearch

    Step "检查 SearXNG 接口 ($($profile.Name))"
    Write-Host "URL: $url"

    $resp = Invoke-JsonNoProxy -Client $client -Url $url
    if (-not $resp.IsSuccess) {
      $preview = ($resp.Raw ?? "").ToString()
      if ($preview.Length -gt 280) { $preview = $preview.Substring(0, 280) + "..." }
      $attemptErrors += "$($profile.Name): HTTP $($resp.StatusCode); body: $preview"
      continue
    }
    if ($null -eq $resp.Json) {
      $attemptErrors += "$($profile.Name): 返回非 JSON: $($resp.Raw)"
      continue
    }

    $resultCount = 0
    if ($resp.Json.results) {
      $resultCount = @($resp.Json.results).Count
    }

    $formatted = @()
    if ($resp.Json.unresponsive_engines) {
      $items = @($resp.Json.unresponsive_engines)
      foreach ($item in $items) {
        $line = Format-UnresponsiveEngine -Item $item
        if ($line) {
          $formatted += $line
        }
      }
    }

    Write-Host "profile: $($profile.Name)"
    Write-Host "query: $($resp.Json.query)"
    Write-Host "number_of_results: $($resp.Json.number_of_results)"
    Write-Host "results.count: $resultCount"
    if ($formatted.Count -gt 0) {
      Write-Host "unresponsive_engines: $($formatted -join '; ')"
    }

    $candidate = [PSCustomObject]@{
      ProfileName  = $profile.Name
      Language     = $profile.Language
      Categories   = $profile.Categories
      SafeSearch   = $profile.SafeSearch
      ResultCount  = $resultCount
      Query        = $resp.Json.query
      NumberResult = $resp.Json.number_of_results
      Unresponsive = $formatted
    }
    if ($null -eq $best -or $candidate.ResultCount -gt $best.ResultCount) {
      $best = $candidate
    }
  }

  if ($null -eq $best) {
    throw "全部参数档位请求失败: $($attemptErrors -join ' | ')"
  }

  Step "SearXNG 最优档位"
  Write-Host "best.profile: $($best.ProfileName)"
  Write-Host "best.language: $($best.Language)"
  Write-Host "best.categories: $($best.Categories)"
  Write-Host "best.safesearch: $($best.SafeSearch)"
  Write-Host "best.results.count: $($best.ResultCount)"
  if ($best.ResultCount -eq 0) {
    Write-Warning "最优档位仍为 0 结果：当前实例可用引擎质量偏低（常见于引擎被限流/被封禁）。建议调整 SearXNG engines 配置。"
  }

  Step "SearXNG 自检通过"
} catch {
  Write-Host "SearXNG 自检失败: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
}
