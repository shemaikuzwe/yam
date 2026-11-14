#!/usr/bin/env bash
# Load-tests every server in src/bin with oha.
# Raw numbers go to results.jsonl, the markdown report to results.md.
#
# Optional: BASELINE_DIR=<checkout of main> also benches yam built from that
# checkout (with this bench harness) and adds a "This PR vs main" table.
# Requires: oha, jq, curl.
set -euo pipefail
cd "$(dirname "$0")"

FRAMEWORKS=(yam hyper axum actix)
ROUTES=(/ /json /users/42)
URL=http://127.0.0.1:3000
DURATION=${DURATION:-10s}
CONNECTIONS=${CONNECTIONS:-64}

# Copy this harness next to the baseline's library, so only yam itself differs.
if [[ -n "${BASELINE_DIR:-}" ]]; then
  base="$BASELINE_DIR/.bench-baseline"
  rm -rf "$base" && mkdir -p "$base"
  cp -r Cargo.toml Cargo.lock src "$base/"
  if cargo build --release --quiet --bin yam --manifest-path "$base/Cargo.toml" --target-dir target/baseline; then
    FRAMEWORKS=(yam-main "${FRAMEWORKS[@]}")
  else
    echo "baseline build failed, skipping main" >&2
  fi
fi

cargo build --release --quiet
: > results.jsonl

for fw in "${FRAMEWORKS[@]}"; do
  if [[ $fw == yam-main ]]; then bin=target/baseline/release/yam; else bin=target/release/$fw; fi
  # Fail fast if something else (e.g. a dev server) already holds the port.
  if (: < /dev/tcp/127.0.0.1/3000) 2>/dev/null; then
    echo "port 3000 is already in use" >&2
    exit 1
  fi

  "$bin" &
  pid=$!
  trap 'kill $pid 2>/dev/null || true' EXIT

  # Wait up to 10s, stop early if the server exits.
  deadline=$((SECONDS + 10))
  until curl -s -o /dev/null --max-time 1 "$URL/"; do
    if ! kill -0 "$pid" 2>/dev/null || (( SECONDS >= deadline )); then
      echo "$fw failed to start" >&2
      exit 1
    fi
    sleep 0.1
  done

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

# yam (main) and yam (this PR) are treated as two separate frameworks.
jq -rs --arg cmd "oha -z $DURATION -c $CONNECTIONS" --arg cpus "$(nproc)" '
  # always 2 decimals, e.g. 41 -> "41.00"
  def ms: . * 100 | round | tostring | ("00" + .)[-3:] as $p | (.[:-2] | if . == "" then "0" else . end) + "." + $p[-2:];
  def num: floor | tostring | [explode | reverse | _nwise(3) | reverse | implode] | reverse | join(",");
  def x: (if . < 10 then . * 10 | round / 10 else round end) | tostring | . + "×";
  # ratio = row / yam. Within ±10% is CI noise, so call it a tie.
  def vs: if . > 1.1 then "\(x) faster" elif . < 0.9 then "\(1 / . | x) slower" else "≈ same" end;

  any(.[]; .fw == "yam-main") as $has_main
  | def title:
      if . == "yam-main" then "**yam (main)**"
      elif . == "yam" then (if $has_main then "**yam (this PR)**" else "**yam**" end)
      else . end;
    def avg(f): map(f) | add / length;
    (reduce .[] as $r ({}; .[$r.fw][$r.route] = $r.rps)) as $rps
  | (group_by(.fw)
     | map({fw: .[0].fw, rps: avg(.rps), p50: avg(.p50), p99: avg(.p99)})
     | sort_by(-.rps)) as $avg
  | ($avg | map(select(.fw == "yam"))[0]) as $yam
  | ($avg | map(select(.fw == "yam-main"))[0]) as $main
  | [
      "## Benchmarks",
      "",
      (if $has_main then
        ($yam.rps / $main.rps) as $d
        | "**This PR vs main: \(if $d > 1.1 then "✅ \($d | vs)" elif $d < 0.9 then "❌ \($d | vs)" else "≈ no measurable change" end)**"
          + " · \($main.rps | num) → \($yam.rps | num) req/s · p50 \($main.p50 | ms) → \($yam.p50 | ms) ms",
        ""
      else empty end),
      "| framework | req/s | vs yam | p50 (ms) | p99 (ms) |",
      "|---|--:|--:|--:|--:|",
      ($avg[] | "| \(.fw | title) | \(.rps | num) | \(if .fw == "yam" then "—" else .rps / $yam.rps | vs end) | \(.p50 | ms) | \(.p99 | ms) |"),
      "",
      "Averaged over 3 routes. Higher req/s and lower latency is better. Gaps under ±10% are CI noise, shown as **≈ same**.",
      "",
      "<details><summary>Per route</summary>",
      "",
      "| route | framework | req/s | vs yam | p50 (ms) | p99 (ms) |",
      "|---|---|--:|--:|--:|--:|",
      (group_by(.route)[] | sort_by(-.rps)[]
        | "| `\(.route)` | \(.fw | title) | \(.rps | num) | \(if .fw == "yam" then "—" else .rps / $rps.yam[.route] | vs end) | \(.p50 | ms) | \(.p99 | ms) |"),
      "",
      "</details>",
      "",
      "<details><summary>Setup</summary>",
      "",
      "- `\($cmd)` · \($cpus) CPUs · keep-alive · release build. Every server returns identical responses.",
      "- **yam (main)** is yam built from the base branch with this PR'"'"'s bench harness, so only yam itself differs.",
      "- Shared CI runner: absolute req/s shifts ±10–20% between runs. The comparisons hold because all servers run in the same job.",
      "- Routes: `/` plain text · `/json` serde JSON · `/users/{id}` path param.",
      "",
      "</details>"
    ]
  | join("\n")
' results.jsonl > results.md

cat results.md
