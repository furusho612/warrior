#!/bin/sh
# 評価用の記録と、PRD のプロンプトに渡す入力の見本を出力する。
# 外部の AI は呼ばない。
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fixtures="$root/backend/warrior/tests/fixtures/ai_eval"

printf '%s\n' '以下の記録を PRD の AI のプロンプトに渡し、評価、提案、メニューの出力を比較する。'
for file in "$fixtures"/*.json; do
    printf '\n===== %s =====\n' "$(basename "$file")"
    cat "$file"
done
