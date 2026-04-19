#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKDIR="${TMPDIR:-/tmp}/llm-wiki-e2e"

rm -rf "$WORKDIR"
mkdir -p "$WORKDIR"
cd "$WORKDIR"

echo "[1/8] ingest + projection"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db --wiki-dir wiki --sync-wiki \
  ingest "file:///tmp/a.md" "项目使用 Redis"$'\n'"Authorization: Bearer secret" --scope private:cli

echo "[2/8] file claim"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db file-claim "项目使用 Redis" --scope private:cli --tier semantic

echo "[3/8] supersede claim"
old_id="$(cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db file-claim "v1" --scope private:cli --tier semantic | sed -n 's/^claim_id=//p')"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db supersede-claim "$old_id" "v2" --scope private:cli --tier semantic

echo "[4/8] query with write-page"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db --wiki-dir wiki --sync-wiki \
  query "Redis API" --write-page --page-title "analysis-redis-api"

echo "[5/8] lint + report"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db --wiki-dir wiki --sync-wiki lint

echo "[6/8] outbox export + ack"
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db export-outbox-ndjson-from --last-id 0 | sed -n '1,5p'
cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db ack-outbox --up-to-id 999999 --consumer-tag e2e

echo "[7/8] mempalace consume"
consume_output="$(cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
  --db wiki.db consume-to-mempalace --last-id 0)"
echo "$consume_output"
consumed_count="$(echo "$consume_output" | sed -n 's/^consumed=//p' | tail -n 1)"
if [[ -z "$consumed_count" || "$consumed_count" -le 0 ]]; then
  echo "Expected consumed>0, got: ${consumed_count:-empty}" >&2
  exit 1
fi

echo "[8/8] llm smoke (optional)"
if [[ -f "$REPO_ROOT/llm-config.toml" ]]; then
  llm_out="$(cargo run -q -p wiki-cli --manifest-path "$REPO_ROOT/Cargo.toml" -- \
    llm-smoke --config "$REPO_ROOT/llm-config.toml" --prompt "Say 'ok' only.")"
  echo "$llm_out"
else
  echo "skip: llm-config.toml not found"
fi

test -f wiki/index.md
test -f wiki/log.md
test -d wiki/reports

echo "E2E PASS: $WORKDIR"
