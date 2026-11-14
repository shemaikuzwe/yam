# Number formatting shared by run.sh and client.sh.

# always 2 decimals, e.g. 41 -> "41.00"
def ms: . * 100 | round | tostring | ("00" + .)[-3:] as $p | (.[:-2] | if . == "" then "0" else . end) + "." + $p[-2:];
def num: floor | tostring | [explode | reverse | _nwise(3) | reverse | implode] | reverse | join(",");
def x: (if . < 10 then . * 10 | round / 10 else round end) | tostring | . + "×";
# ratio = row / yam. Within ±10% is CI noise, so call it a tie.
def vs: if . > 1.1 then "\(x) faster" elif . < 0.9 then "\(1 / . | x) slower" else "≈ same" end;
