param(
  [string]$ApiUrl = "http://127.0.0.1:19827",
  [switch]$SkipPost
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function New-NoProxyHttpClient {
  $handler = New-Object System.Net.Http.HttpClientHandler
  $handler.UseProxy = $false
  $client = New-Object System.Net.Http.HttpClient($handler)
  $client.Timeout = [TimeSpan]::FromSeconds(15)
  return $client
}

function Invoke-JsonNoProxy {
  param(
    [System.Net.Http.HttpClient]$Client,
    [ValidateSet("GET", "POST")]
    [string]$Method,
    [string]$Url,
    [object]$BodyObject
  )

  $request = New-Object System.Net.Http.HttpRequestMessage (New-Object System.Net.Http.HttpMethod($Method)), $Url
  $request.Headers.TryAddWithoutValidation("Accept", "application/json") | Out-Null

  if ($null -ne $BodyObject) {
    $json = $BodyObject | ConvertTo-Json -Depth 8 -Compress
    $request.Content = New-Object System.Net.Http.StringContent($json, [System.Text.Encoding]::UTF8, "application/json")
  }

  $response = $Client.SendAsync($request).GetAwaiter().GetResult()
  $raw = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
  $json = $null
  try {
    if ($raw) {
      $json = $raw | ConvertFrom-Json
    }
  } catch {
    # 非 JSON 响应保留在 raw 中做诊断
  }

  return [PSCustomObject]@{
    StatusCode = [int]$response.StatusCode
    IsSuccess  = $response.IsSuccessStatusCode
    Raw        = $raw
    Json       = $json
  }
}

try {
  $uri = [System.Uri]$ApiUrl
  $port = $uri.Port
  if (-not $port) { $port = 19827 }

  Step "检查本地端口监听"
  try {
    $listener = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($listener) {
      $proc = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
      if ($proc) {
        Write-Host "监听进程: PID=$($proc.Id) Name=$($proc.ProcessName)"
      } else {
        Write-Host "监听进程: PID=$($listener.OwningProcess)"
      }
    } else {
      Write-Warning "端口 $port 未检测到监听进程。请先启动 tauri 应用。"
    }
  } catch {
    Write-Warning "无法读取端口监听信息：$($_.Exception.Message)"
  }

  $client = New-NoProxyHttpClient

  Step "检查 Clipper 服务状态"
  $statusResp = Invoke-JsonNoProxy -Client $client -Method "GET" -Url "$ApiUrl/status" -BodyObject $null
  if (-not $statusResp.IsSuccess) {
    $preview = ($statusResp.Raw ?? "").ToString()
    if ($preview.Length -gt 280) { $preview = $preview.Substring(0, 280) + "..." }
    throw "GET /status 失败: HTTP $($statusResp.StatusCode); body: $preview"
  }
  $status = $statusResp.Json
  if ($null -eq $status) {
    throw "GET /status 返回非 JSON: $($statusResp.Raw)"
  }
  if (-not $status.ok) {
    throw "status 接口返回 ok=false"
  }
  Write-Host "version: $($status.version)"
  Write-Host "vault_open: $($status.vault_open)"
  Write-Host "vault_path: $($status.vault_path)"

  if (-not $status.vault_open) {
    Write-Warning "服务已启动，但未打开 Vault。请先在应用中初始化/打开 Vault 后重试。"
    exit 1
  }

  Step "检查 /project 与 /projects 接口"
  $projectResp = Invoke-JsonNoProxy -Client $client -Method "GET" -Url "$ApiUrl/project" -BodyObject $null
  $projectsResp = Invoke-JsonNoProxy -Client $client -Method "GET" -Url "$ApiUrl/projects" -BodyObject $null
  if (-not $projectResp.IsSuccess) {
    throw "GET /project 失败: HTTP $($projectResp.StatusCode); body: $($projectResp.Raw)"
  }
  if (-not $projectsResp.IsSuccess) {
    throw "GET /projects 失败: HTTP $($projectsResp.StatusCode); body: $($projectsResp.Raw)"
  }
  $project = $projectResp.Json
  $projects = $projectsResp.Json
  if (-not $project.ok) {
    throw "/project 返回 ok=false"
  }
  if (-not $projects.ok) {
    throw "/projects 返回 ok=false"
  }
  Write-Host "/project.path: $($project.path)"
  Write-Host "/projects.count: $($projects.projects.Count)"

  if ($SkipPost) {
    Step "已跳过 POST /clip（-SkipPost）"
    exit 0
  }

  Step "执行 POST /clip 写入自检样例"
  $now = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
  $body = [ordered]@{
    title       = "clipper-self-test $now"
    url         = "https://example.com/clipper-self-test"
    content     = "This is a clipper self-test entry generated at $now."
    projectPath = $project.path
  }

  $clipResp = Invoke-JsonNoProxy -Client $client -Method "POST" -Url "$ApiUrl/clip" -BodyObject $body
  if (-not $clipResp.IsSuccess) {
    throw "POST /clip 失败: HTTP $($clipResp.StatusCode); body: $($clipResp.Raw)"
  }
  $clip = $clipResp.Json
  if ($null -eq $clip) {
    throw "POST /clip 返回非 JSON: $($clipResp.Raw)"
  }
  if (-not $clip.ok) {
    throw "POST /clip 返回 ok=false: $($clip.error)"
  }
  Write-Host "clip.path: $($clip.path)"
  Step "Clipper 自检通过"
} catch {
  Write-Host "Clipper 自检失败: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
}
