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

  # Wait up to ~10s, stop early if the server exits.
  ready=0
  for _ in {1..100}; do
    kill -0 "$pid" 2>/dev/null || break
    if curl -s -o /dev/null --max-time 1 "$URL/"; then ready=1; break; fi
    sleep 0.1
  done
  if (( ! ready )); then
    echo "$fw failed to start" >&2
    exit 1
  fi

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
  def ms: . * 100 | round / 100 | tostring;
  def num: floor | tostring | [explode | reverse | _nwise(3) | reverse | implode] | reverse | join(",");
  def times: (if . < 1 then . * 100 | round / 100 else . * 10 | round / 10 end)
    | tostring | (if test("\\.") then . else . + ".0" end) + "×";

  any(.[]; .fw == "yam-main") as $has_main
  | def title:
      if . == "yam-main" then "**yam (main)**"
      elif . == "yam" then (if $has_main then "**yam (this PR)**" else "**yam**" end)
      else . end;
    # baselines every row is compared against: [fw, column title]
    (if $has_main then [["yam-main", "vs yam (main)"], ["yam", "vs yam (this PR)"]]
     else [["yam", "vs yam"]] end) as $refs
  | (reduce .[] as $r ({}; .[$r.fw][$r.route] = $r.rps)) as $rps
  | (group_by(.fw)
     | map({fw: .[0].fw, rps: (map(.rps) | add / length), p50: (map(.p50) | add / length)})
     | sort_by(-.rps)) as $avg
  | (reduce $avg[] as $a ({}; .[$a.fw] = $a.rps)) as $avg_rps
  | ($refs | map(.[1]) | join(" | ")) as $ref_cols
  | ($refs | map("--:") | join("|")) as $ref_align
  | [
      "## Benchmarks",
      "",
      "yam vs hyper, axum and actix-web. Every server returns identical responses.",
      "",
      "`\($cmd)` · \($cpus) CPUs · keep-alive · release build",
      "",
      "### Summary",
      "",
      "| framework | avg req/s | \($ref_cols) | avg p50 (ms) |",
      "|---|--:|\($ref_align)|--:|",
      ($avg[] | . as $a
        | "| \($a.fw | title) | \($a.rps | num) | \([$refs[] | $a.rps / $avg_rps[.[0]] | times] | join(" | ")) | \($a.p50 | ms) |"),
      "",
      "### Per route",
      "",
      "| framework | route | req/s | \($ref_cols) | p50 (ms) | p99 (ms) |",
      "|---|---|--:|\($ref_align)|--:|--:|",
      (.[] | . as $r
        | "| \($r.fw | title) | `\($r.route)` | \($r.rps | num) | \([$refs[] | $r.rps / $rps[.[0]][$r.route] | times] | join(" | ")) | \($r.p50 | ms) | \($r.p99 | ms) |"),
      "",
      "<details><summary>How to read this</summary>",
      "",
      "- **req/s**: throughput, higher is better. **p50 / p99**: median and slowest-1% latency, lower is better.",
      "- **vs yam (…)**: that row'"'"'s req/s divided by that yam'"'"'s req/s on the same route. Above 1.0× is faster, below is slower.",
      "- **yam (main)**: yam built from the base branch, using this PR'"'"'s bench harness. Compare it with **yam (this PR)** to see what the PR changed.",
      "- Results come from a shared CI runner, so absolute numbers vary by about ±10–20% between runs. Compare the ratios, which are measured on the same machine in the same job.",
      "- Routes: `/` plain text · `/json` serde JSON · `/users/{id}` path param.",
      "",
      "</details>"
    ]
  | join("\n")
' results.jsonl > results.md

cat results.md
