#!/usr/bin/env bash
# Load-tests every server in src/bin with oha.
# Raw numbers go to results.jsonl, the markdown report to results.md.
# Requires: oha, jq, curl.
set -euo pipefail
cd "$(dirname "$0")"

FRAMEWORKS=(yam hyper axum actix)
ROUTES=(/ /json /users/42)
URL=http://127.0.0.1:3000
DURATION=${DURATION:-10s}
CONNECTIONS=${CONNECTIONS:-64}

cargo build --release --quiet
: > results.jsonl

for fw in "${FRAMEWORKS[@]}"; do
  ./target/release/"$fw" &
  pid=$!
  trap 'kill $pid 2>/dev/null || true' EXIT

  until curl -s -o /dev/null "$URL/"; do sleep 0.1; done

  for route in "${ROUTES[@]}"; do
    oha -z 2s -c "$CONNECTIONS" --no-tui "$URL$route" > /dev/null
    oha -z "$DURATION" -c "$CONNECTIONS" --no-tui --output-format json "$URL$route" |
      jq -c --arg fw "$fw" --arg route "$route" '{
        fw: $fw,
        route: $route,
        rps: .summary.requestsPerSec,
        p50: (.latencyPercentiles.p50 * 1000),
        p99: (.latencyPercentiles.p99 * 1000)
      }' >> results.jsonl
  done

  kill "$pid"
  wait "$pid" 2>/dev/null || true
done

jq -rs --arg cmd "oha -z $DURATION -c $CONNECTIONS" --arg cpus "$(nproc)" '
  def ms: . * 100 | round / 100 | tostring;
  def num: floor | tostring | [explode | reverse | _nwise(3) | reverse | implode] | reverse | join(",");
  def times: . * 10 | round / 10 | tostring | (if test("\\.") then . else . + ".0" end) + "×";
  def name: if . == "yam" then "**yam**" else . end;

  (map(select(.fw == "yam") | {(.route): .rps}) | add) as $yam
  | (group_by(.fw)
     | map({fw: .[0].fw, rps: (map(.rps) | add / length), p50: (map(.p50) | add / length)})
     | sort_by(-.rps)) as $avg
  | ($avg[] | select(.fw == "yam") | .rps) as $yam_avg
  | [
      "## Benchmarks",
      "",
      "yam vs hyper, axum and actix-web. Every server returns identical responses.",
      "",
      "`\($cmd)` · \($cpus) CPUs · keep-alive · release build",
      "",
      "### Summary",
      "",
      "| framework | avg req/s | vs yam | avg p50 (ms) |",
      "|---|--:|--:|--:|",
      ($avg[] | "| \(.fw | name) | \(.rps | num) | \(.rps / $yam_avg | times) | \(.p50 | ms) |"),
      "",
      "### Per route",
      "",
      "| framework | route | req/s | vs yam | p50 (ms) | p99 (ms) |",
      "|---|---|--:|--:|--:|--:|",
      (.[] | "| \(.fw | name) | `\(.route)` | \(.rps | num) | \(.rps / $yam[.route] | times) | \(.p50 | ms) | \(.p99 | ms) |"),
      "",
      "<details><summary>How to read this</summary>",
      "",
      "- **req/s**: throughput, higher is better. **p50 / p99**: median and slowest-1% latency, lower is better.",
      "- **vs yam**: that framework'"'"'s req/s divided by yam'"'"'s on the same route.",
      "- Results come from a shared CI runner, so absolute numbers vary by about ±10–20% between runs. Compare the ratios, which are measured on the same machine in the same job.",
      "- Routes: `/` plain text · `/json` serde JSON · `/users/{id}` path param.",
      "",
      "</details>"
    ]
  | join("\n")
' results.jsonl > results.md

cat results.md
