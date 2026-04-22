param(
  [string]$ContainerName = "searxng",
  [int]$Port = 8080,
  [string]$ConfigPath = "$PSScriptRoot/../configs/searxng/settings.windows.example.yml",
  [string]$ForwardedIp = "127.0.0.1",
  [switch]$Recreate,
  [int]$StartupTimeoutSec = 45
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Get-ExceptionChainText {
  param([System.Exception]$Exception)

  if ($null -eq $Exception) {
    return "unknown error"
  }

  $messages = @()
  $current = $Exception
  while ($null -ne $current) {
    if ($current.Message) {
      $messages += $current.Message
    }
    $current = $current.InnerException
  }
  if ($messages.Count -eq 0) {
    return "unknown error"
  }
  return ($messages -join " | ")
}

function Ensure-CommandSucceeded {
  param(
    [string]$Action,
    [int]$Code
  )

  if ($Code -ne 0) {
    throw "$Action 失败（exit code=$Code）"
  }
}

function Start-SearxngContainer {
  param(
    [string]$Name,
    [int]$HostPort,
    [string]$ConfigFile,
    [bool]$UseConfig
  )

  if ($UseConfig) {
    docker run -d --name $Name `
      -p "$HostPort`:8080" `
      -e SEARXNG_LIMITER=false `
      -e SEARXNG_PUBLIC_INSTANCE=false `
      -v "${ConfigFile}:/etc/searxng/settings.yml:ro" `
      searxng/searxng | Out-Null
  } else {
    docker run -d --name $Name `
      -p "$HostPort`:8080" `
      -e SEARXNG_LIMITER=false `
      -e SEARXNG_PUBLIC_INSTANCE=false `
      searxng/searxng | Out-Null
  }
  Ensure-CommandSucceeded -Action "docker run" -Code $LASTEXITCODE
}

function Test-SearxngReady {
  param(
    [int]$HostPort,
    [string]$ForwardedIpValue
  )

  $handler = New-Object System.Net.Http.HttpClientHandler
  $handler.UseProxy = $false
  $client = New-Object System.Net.Http.HttpClient($handler)
  $client.Timeout = [TimeSpan]::FromSeconds(5)
  try {
    $url = "http://127.0.0.1:$HostPort/search?q=health&format=json&language=auto"
    $request = New-Object System.Net.Http.HttpRequestMessage ([System.Net.Http.HttpMethod]::Get), $url
    $request.Headers.TryAddWithoutValidation("Accept", "application/json") | Out-Null
    $request.Headers.TryAddWithoutValidation("User-Agent", "Mozilla/5.0 llm-wiki-searxng-ready-check") | Out-Null
    $request.Headers.TryAddWithoutValidation("X-Forwarded-For", $ForwardedIpValue) | Out-Null
    $request.Headers.TryAddWithoutValidation("X-Real-IP", $ForwardedIpValue) | Out-Null
    $request.Headers.TryAddWithoutValidation("X-Forwarded-Proto", "http") | Out-Null
    $resp = $client.SendAsync($request).GetAwaiter().GetResult()
    return $resp.IsSuccessStatusCode
  } catch {
    return $false
  } finally {
    $client.Dispose()
    $handler.Dispose()
  }
}

function Wait-SearxngReady {
  param(
    [string]$Name,
    [int]$HostPort,
    [string]$ForwardedIpValue,
    [int]$TimeoutSec
  )

  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $status = docker inspect -f "{{.State.Status}}" $Name 2>$null
    if ($LASTEXITCODE -eq 0 -and "$status".Trim() -eq "running") {
      if (Test-SearxngReady -HostPort $HostPort -ForwardedIpValue $ForwardedIpValue) {
        return $true
      }
    }
    Start-Sleep -Seconds 2
  }
  return $false
}

function Show-ContainerLogs {
  param([string]$Name)

  Step "输出容器最近日志（排错）"
  docker logs --tail 120 $Name
}

try {
  Step "解析配置文件路径"
  $resolvedConfig = (Resolve-Path -Path $ConfigPath).Path
  Write-Host "config: $resolvedConfig"

  if ($Recreate) {
    Step "删除旧容器（如存在）"
    docker rm -f $ContainerName | Out-Null
  }

  Step "启动 SearXNG 容器（优先使用推荐配置）"
  Start-SearxngContainer -Name $ContainerName -HostPort $Port -ConfigFile $resolvedConfig -UseConfig $true

  if (-not (Wait-SearxngReady -Name $ContainerName -HostPort $Port -ForwardedIpValue $ForwardedIp -TimeoutSec $StartupTimeoutSec)) {
    Show-ContainerLogs -Name $ContainerName
    Write-Warning "推荐配置启动失败，自动回退到镜像默认配置重试。"
    docker rm -f $ContainerName | Out-Null
    Start-SearxngContainer -Name $ContainerName -HostPort $Port -ConfigFile $resolvedConfig -UseConfig $false
    if (-not (Wait-SearxngReady -Name $ContainerName -HostPort $Port -ForwardedIpValue $ForwardedIp -TimeoutSec $StartupTimeoutSec)) {
      Show-ContainerLogs -Name $ContainerName
      throw "SearXNG 在 $StartupTimeoutSec 秒内未就绪（推荐配置与默认配置均失败）"
    }
  }

  Step "运行自检"
  & "$PSScriptRoot/verify_searxng_windows.ps1" `
    -BaseUrl "http://127.0.0.1:$Port" `
    -ForwardedIp $ForwardedIp `
    -Query "rust async runtime"
  if ($LASTEXITCODE -ne 0) {
    Show-ContainerLogs -Name $ContainerName
    throw "verify_searxng_windows.ps1 返回非零退出码：$LASTEXITCODE"
  }

  Step "完成"
  Write-Host "SearXNG 已按推荐模板启动。"
} catch {
  $detail = Get-ExceptionChainText -Exception $_.Exception
  Write-Host "SearXNG 启动失败: $detail" -ForegroundColor Red
  exit 1
}
