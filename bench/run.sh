#!/usr/bin/env bash
# Load-tests every server in src/bin with oha and writes a markdown table to results.md.
# Requires: oha, jq, curl.
set -euo pipefail
cd "$(dirname "$0")"

FRAMEWORKS=(yam hyper axum actix)
ROUTES=(/ /json /users/42)
URL=http://127.0.0.1:3000
DURATION=${DURATION:-10s}
CONNECTIONS=${CONNECTIONS:-64}

cargo build --release --quiet

{
  echo "## Benchmarks"
  echo
  echo "\`oha -z $DURATION -c $CONNECTIONS\` · $(nproc) CPUs · keep-alive"
  echo
  echo "| framework | route | req/s | p50 (ms) | p99 (ms) |"
  echo "|---|---|--:|--:|--:|"
} > results.md

for fw in "${FRAMEWORKS[@]}"; do
  ./target/release/"$fw" &
  pid=$!
  trap 'kill $pid 2>/dev/null' EXIT

  until curl -s -o /dev/null "$URL/"; do sleep 0.1; done

  for route in "${ROUTES[@]}"; do
    oha -z 2s -c "$CONNECTIONS" --no-tui "$URL$route" > /dev/null
    oha -z "$DURATION" -c "$CONNECTIONS" --no-tui --output-format json "$URL$route" |
      jq -r --arg fw "$fw" --arg route "$route" \
        '"| \($fw) | `\($route)` | \(.summary.requestsPerSec | floor) | \(.latencyPercentiles.p50 * 1000 * 100 | round / 100) | \(.latencyPercentiles.p99 * 1000 * 100 | round / 100) |"' \
      >> results.md
  done

  kill "$pid"
  wait "$pid" 2>/dev/null || true
done

cat results.md
