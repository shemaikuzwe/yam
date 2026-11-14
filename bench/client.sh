#!/usr/bin/env bash
# Load-tests every client in src/bin/client-*.rs against the hyper server bin.
# Run after run.sh: it reuses its builds (incl. target/baseline for yam-main)
# and appends a "Client benchmarks" section to results.md.
# Requires: jq, curl.
set -euo pipefail
cd "$(dirname "$0")"

CLIENTS=(yam reqwest hyper)
ROUTES=(/ /json)
URL=http://127.0.0.1:3000
export CONNECTIONS=${CLIENT_CONNECTIONS:-16} DURATION_SECS=${CLIENT_DURATION_SECS:-10}

[[ -x target/baseline/release/client-yam ]] && CLIENTS=(yam-main "${CLIENTS[@]}")

if (: < /dev/tcp/127.0.0.1/3000) 2>/dev/null; then
  echo "port 3000 is already in use" >&2
  exit 1
fi
target/release/hyper &
pid=$!
trap 'kill $pid 2>/dev/null || true' EXIT

deadline=$((SECONDS + 10))
until curl -s -o /dev/null --max-time 1 "$URL/"; do
  if ! kill -0 "$pid" 2>/dev/null || (( SECONDS >= deadline )); then
    echo "hyper server failed to start" >&2
    exit 1
  fi
  sleep 0.1
done

for c in "${CLIENTS[@]}"; do
  if [[ $c == yam-main ]]; then bin=target/baseline/release/client-yam; else bin=target/release/client-$c; fi
  for route in "${ROUTES[@]}"; do
    "$bin" "$route" | jq -c --arg fw "$c" --arg route "$route" '. + {fw: $fw, route: $route}'
  done
done > client-results.jsonl

jq -L . -rs --arg conns "$CONNECTIONS" --arg secs "$DURATION_SECS" --arg cpus "$(nproc)" '
  include "fmt";

  any(.[]; .fw == "yam-main") as $has_main
  | def title:
      if . == "yam-main" then "**yam-client (main)**"
      elif . == "yam" then (if $has_main then "**yam-client (this PR)**" else "**yam-client**" end)
      else . end;
    def avg(f): map(f) | add / length;
    (group_by(.fw)
     | map({fw: .[0].fw, rps: avg(.rps), p50: avg(.p50), p99: avg(.p99)})
     | sort_by(-.rps)) as $avg
  | ($avg | map(select(.fw == "yam"))[0]) as $yam
  | ($avg | map(select(.fw == "yam-main"))[0]) as $main
  | [
      "",
      "## Client benchmarks",
      "",
      (if $has_main then
        ($yam.rps / $main.rps) as $d
        | "**This PR vs main: \(if $d > 1.1 then "✅ \($d | vs)" elif $d < 0.9 then "❌ \($d | vs)" else "≈ no measurable change" end)**"
          + " · \($main.rps | num) → \($yam.rps | num) req/s · p50 \($main.p50 | ms) → \($yam.p50 | ms) ms",
        ""
      else empty end),
      "| client | req/s | vs yam-client | p50 (ms) | p99 (ms) |",
      "|---|--:|--:|--:|--:|",
      ($avg[] | "| \(.fw | title) | \(.rps | num) | \(if .fw == "yam" then "—" else .rps / $yam.rps | vs end) | \(.p50 | ms) | \(.p99 | ms) |"),
      "",
      "<details><summary>Setup</summary>",
      "",
      "- \($conns) concurrent requests for \($secs)s (after 2s warm-up) · \($cpus) CPUs · keep-alive · release build.",
      "- Every client hits the same raw hyper server on the same runner and checks the body (`/` text, `/json` serde).",
      "- Averaged over `/` and `/json`. Gaps under ±10% are CI noise, shown as **≈ same**.",
      "",
      "</details>"
    ]
  | join("\n")
' client-results.jsonl | tee -a results.md
