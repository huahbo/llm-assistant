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
Write-Host "如需另一个模型，重新执行: .\scripts\download-embed-models.ps1 -Model bge-small-zh-v1.5"
