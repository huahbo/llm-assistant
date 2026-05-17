#!/usr/bin/env bash
# download-embed-models.sh
# 下载 ONNX embedding 模型文件到 src-tauri/resources/embed-models/
# 用法: bash scripts/download-embed-models.sh
# 用法（指定模型）: bash scripts/download-embed-models.sh bge-small-zh-v1.5

set -euo pipefail

MODEL="${1:-multilingual-e5-small}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$SCRIPT_DIR/../src-tauri/resources/embed-models/$MODEL"

declare -A REPOS
REPOS["multilingual-e5-small"]="Xenova/multilingual-e5-small"
REPOS["bge-small-zh-v1.5"]="Xenova/bge-small-zh-v1.5"

if [[ -z "${REPOS[$MODEL]+x}" ]]; then
    echo "ERROR: 未知模型: $MODEL"
    echo "可用: ${!REPOS[*]}"
    exit 1
fi

REPO="${REPOS[$MODEL]}"
FILES=("onnx/model.onnx" "tokenizer.json" "tokenizer_config.json" "config.json" "special_tokens_map.json")
HF_BASE="https://huggingface.co/$REPO/resolve/main"

echo "=== 下载模型: $MODEL (来源: $REPO) ==="
echo "目标目录: $DEST"
echo ""

mkdir -p "$DEST/onnx"

for f in "${FILES[@]}"; do
    url="$HF_BASE/$f"
    out="$DEST/$f"
    outdir="$(dirname "$out")"
    mkdir -p "$outdir"

    if [[ -f "$out" ]]; then
        echo "  已存在，跳过: $f"
        continue
    fi

    echo "  下载: $f"
    if curl -fL --progress-bar -o "$out" "$url"; then
        size=$(du -sh "$out" 2>/dev/null | cut -f1)
        echo "  完成: $f ($size)"
    else
        echo "  警告: 下载失败 — $f"
    fi
done

echo ""
echo "=== 完成 ==="
echo "文件已下载到: $DEST"
