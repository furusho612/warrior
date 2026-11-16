#!/bin/sh
# staging の D1 から、レスト終了から通知の受信までの遅れを集計する。
# 実行には staging の Cloudflare の資格情報と、実機で計測した予約の行が必要である。
set -eu

env=${1:-staging}
json=$(wrangler d1 execute DB --remote --env "$env" --json --command \
    "SELECT end_at, sent_at, received_at FROM rest_push_reservations WHERE sent_at IS NOT NULL ORDER BY created_at DESC LIMIT 500")

printf '%s\n' "$json" | jq -r '
    (.[0].results // []) as $rows
    | ($rows | map(select(.received_at != null))) as $done
    | ($done | map((.received_at - .end_at) * 1000) | sort) as $latencies
    | ($latencies | length) as $count
    | "attempts=\($rows | length)",
      "received=\($count)",
      "missing=\(($rows | length) - $count)",
      "within_5s=\($latencies | map(select(. <= 5000)) | length)",
      if $count == 0 then "median_ms=null", "max_ms=null"
      else
        "median_ms=\($latencies[($count / 2 | floor)] )",
        "max_ms=\($latencies[-1])"
      end
'
