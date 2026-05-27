#!/usr/bin/env bash
# Rank .vcir candidates by VectorBench execute_rate (decode-time best-of-N).
#
# Usage:
#   ./scripts/decode_rank_eval.sh CANDIDATE_DIR TASK_ID [SUITE.json]
#
# If CANDIDATE_DIR has no *.vcir, emits three golden decode-z samples from a fixed z fixture.
# Prints JSON array sorted by execute_rate descending.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/vectorc-prefix.sh"

CAND_DIR="${1:?candidate directory}"
TASK="${2:?task_id e.g. add_i32}"
SUITE="${3:-$ROOT/benchmarks/vectorbench_v0/suite.json}"
Z_FIXTURE="${ROOT}/benchmarks/datasets/training_slice_v0/rows.jsonl"

mkdir -p "$CAND_DIR"
shopt -s nullglob
vcirs=( "$CAND_DIR"/*.vcir )

if [[ ${#vcirs[@]} -eq 0 ]]; then
  # Use first row z from training shard when present; else minimal z JSON.
  if [[ -f "$Z_FIXTURE" ]]; then
    Z_JSON="$(mktemp)"
    python3 - "$Z_FIXTURE" "$Z_JSON" <<'PY'
import json, sys
line = open(sys.argv[1]).readline()
row = json.loads(line)
open(sys.argv[2], "w").write(json.dumps({"z": row["z"]}))
PY
  else
    Z_JSON="$(mktemp)"
    python3 - "$Z_JSON" <<'PY'
import json, sys
open(sys.argv[1], "w").write(json.dumps({"z": [0.0] * 256}))
PY
  fi
  for i in 0 1 2; do
    out="$CAND_DIR/cand_${i}.vcir"
    $VECTORC decode-z -z "$Z_JSON" -o "$out" --decoder golden
    vcirs+=( "$out" )
  done
  rm -f "$Z_JSON"
fi

export VECTORCOMPILER_ROOT="$ROOT"
python3 - "$SUITE" "$TASK" "${vcirs[@]}" <<'PY'
import json, subprocess, sys, os

suite, task = sys.argv[1], sys.argv[2]
paths = sys.argv[3:]
vectorc = os.environ.get("VECTORC", "vectorc")
root = os.environ.get("VECTORCOMPILER_ROOT", ".")

rows = []
for p in paths:
    proc = subprocess.run(
        vectorc.split() + ["eval", "-i", p, "--suite", suite, "--task", task, "--json"],
        cwd=root,
        capture_output=True,
        text=True,
        env={**os.environ, "RUST_LOG": "warn"},
    )
    if proc.returncode != 0:
        rate = 0.0
        err = (proc.stderr or proc.stdout).strip()
    else:
        data = json.loads(proc.stdout)
        rate = float(data.get("metrics", {}).get("execute_rate", 0.0))
        err = None
    rows.append({"vcir": p, "execute_rate": rate, "error": err})

rows.sort(key=lambda r: r["execute_rate"], reverse=True)
print(json.dumps(rows, indent=2))
PY
