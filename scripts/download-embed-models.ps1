# download-embed-models.ps1
# 下载 ONNX embedding 模型文件到 src-tauri/resources/embed-models/
# 用法: .\scripts\download-embed-models.ps1
# 用法（指定模型）: .\scripts\download-embed-models.ps1 -Model bge-small-zh-v1.5

param(
    [string]$Model = "multilingual-e5-small"
)

$ErrorActionPreference = "Stop"

$models = @{
    "multilingual-e5-small" = @{
        repo  = "Xenova/multilingual-e5-small"
        files = @("onnx/model.onnx", "tokenizer.json", "tokenizer_config.json", "config.json", "special_tokens_map.json")
    }
    "bge-small-zh-v1.5" = @{
        repo  = "Xenova/bge-small-zh-v1.5"
        files = @("onnx/model.onnx", "tokenizer.json", "tokenizer_config.json", "config.json", "special_tokens_map.json")
    }
}

if (-not $models.ContainsKey($Model)) {
    Write-Error "未知模型: $Model`n可用: $($models.Keys -join ', ')"
    exit 1
}

$repo  = $models[$Model].repo
$files = $models[$Model].files
$dest  = Join-Path $PSScriptRoot "..\src-tauri\resources\embed-models\$Model"

New-Item -ItemType Directory -Force $dest | Out-Null

Write-Host "=== 下载模型: $Model (来源: $repo) ===" -ForegroundColor Cyan
Write-Host "目标目录: $dest`n"

$hfBase = "https://huggingface.co/$repo/resolve/main"

foreach ($f in $files) {
    $url    = "$hfBase/$f"
    $fname  = Split-Path $f -Leaf
    $outDir = Join-Path $dest (Split-Path $f -Parent)
    New-Item -ItemType Directory -Force $outDir | Out-Null
    $out = Join-Path $outDir $fname

    if (Test-Path $out) {
        Write-Host "  已存在，跳过: $f" -ForegroundColor Gray
        continue
    }

    Write-Host "  下载: $f" -ForegroundColor Yellow
    try {
        Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
        $size = [math]::Round((Get-Item $out).Length / 1MB, 1)
        Write-Host "  完成: $fname ($size MB)" -ForegroundColor Green
    } catch {
        Write-Warning "  失败: $f — $_"
    }
}

Write-Host "`n=== 完成 ===" -ForegroundColor Cyan
Write-Host "文件已下载到: $dest"
Write-Host ""

# ── 下载 onnxruntime DLL（load-dynamic 模式需要）────────────────────────
$ortVersion = "1.20.1"
$ortDll = Join-Path $PSScriptRoot "..\src-tauri\resources\onnxruntime.dll"
if (-not (Test-Path $ortDll)) {
    Write-Host "=== 下载 onnxruntime.dll v$ortVersion ===" -ForegroundColor Cyan
    $ortZipUrl = "https://github.com/microsoft/onnxruntime/releases/download/v$ortVersion/onnxruntime-win-x64-$ortVersion.zip"
    $ortZip = Join-Path $env:TEMP "onnxruntime-win-x64-$ortVersion.zip"
    try {
        Write-Host "  下载 onnxruntime zip..."
        Invoke-WebRequest -Uri $ortZipUrl -OutFile $ortZip -UseBasicParsing
        $tmpDir = Join-Path $env:TEMP "ort-extract"
        Expand-Archive -Path $ortZip -DestinationPath $tmpDir -Force
        $dllSrc = Get-ChildItem $tmpDir -Filter "onnxruntime.dll" -Recurse | Select-Object -First 1
        if ($dllSrc) {
            Copy-Item $dllSrc.FullName -Destination $ortDll
            Write-Host "  完成: onnxruntime.dll -> src-tauri/resources/" -ForegroundColor Green
        } else {
            Write-Warning "  未在 zip 中找到 onnxruntime.dll"
        }
        Remove-Item $ortZip, $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    } catch {
        Write-Warning "  onnxruntime.dll 下载失败: $_"
        Write-Host "  可手动从 https://github.com/microsoft/onnxruntime/releases 下载并放到 src-tauri/resources/"
    }
} else {
    Write-Host "  onnxruntime.dll 已存在，跳过" -ForegroundColor Gray
}

Write-Host ""
Write-Host "如需另一个模型，重新执行: .\scripts\download-embed-models.ps1 -Model bge-small-zh-v1.5"
