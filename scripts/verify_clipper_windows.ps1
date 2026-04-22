param(
  [string]$ApiUrl = "http://127.0.0.1:19827",
  [switch]$SkipPost
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

try {
  Step "检查 Clipper 服务状态"
  $status = Invoke-RestMethod "$ApiUrl/status"
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
  $project = Invoke-RestMethod "$ApiUrl/project"
  $projects = Invoke-RestMethod "$ApiUrl/projects"
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
  $body = @{
    title       = "clipper-self-test $now"
    url         = "https://example.com/clipper-self-test"
    content     = "This is a clipper self-test entry generated at $now."
    projectPath = $project.path
  } | ConvertTo-Json -Depth 4 -Compress

  $clip = Invoke-RestMethod -Method Post -Uri "$ApiUrl/clip" -ContentType "application/json" -Body $body
  if (-not $clip.ok) {
    throw "POST /clip 返回 ok=false: $($clip.error)"
  }
  Write-Host "clip.path: $($clip.path)"
  Step "Clipper 自检通过"
} catch {
  Write-Host "Clipper 自检失败: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
}
