#!/usr/bin/env bash
# Fast PR gate: fmt, clippy, tests, schemas, training shard verify. No audit/deny/ONNX/scoreboards.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"

echo "==> cargo fmt --check"
cargo fmt --all --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "==> cargo test --workspace"
cargo test --workspace --locked

if command -v check-jsonschema >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
  echo "==> JSON schemas"
  bash scripts/validate-schemas.sh
else
  echo "==> skipping validate-schemas (install: pip install check-jsonschema, apt/brew jq)"
fi

echo "==> training slice verify"
python3 scripts/gen_training_rows.py --verify-only

echo "==> vectorc check + eval smoke"
$VECTORC check -i benchmarks/programs/add.vcir -m benchmarks/manifests/add.json --json >/dev/null
$VECTORC eval -i benchmarks/programs/add.vcir \
  --suite benchmarks/vectorbench_v0/suite.json \
  --task add_i32 --json >/dev/null

echo "==> python reward smoke"
python3 scripts/rl_reward_execute_rate.py \
  --vcir benchmarks/programs/add.vcir \
  --task add_i32 >/dev/null

echo "preflight-lite: OK"
