#!/usr/bin/env bash
# Full local gate before training / large eval runs. Mirrors CI + scoreboards.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"
RUN_ONNX="${RUN_ONNX:-1}"

echo "==> cargo fmt --check"
cargo fmt --all --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "==> cargo test --workspace"
cargo test --workspace --locked

echo "==> JSON schemas + dataset manifest"
bash scripts/validate-schemas.sh
bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/datasets/fixture_slice_v0/manifest.json

echo "==> training slice v0 (verify + batch eval)"
python3 scripts/gen_training_rows.py --verify-only
bash scripts/batch-eval-training-shard.sh benchmarks/datasets/training_slice_v0 --out /tmp/preflight-shard-eval.json

echo "==> vectorc bench (all manifests)"
shopt -s nullglob
manifests=(benchmarks/manifests/*.json)
if [ "${#manifests[@]}" -eq 0 ]; then
  echo "No benchmark manifests under benchmarks/manifests/"
  exit 1
fi
for manifest in "${manifests[@]}"; do
  echo "    $manifest"
  $VECTORC bench -m "$manifest"
done

echo "==> vectorc check (in-process spec run)"
$VECTORC check -i benchmarks/programs/add.vcir -m benchmarks/manifests/add.json --json >/dev/null

echo "==> eval decode scoreboard (golden)"
VECTORC="$VECTORC" bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json

if [ "$RUN_ONNX" = "1" ]; then
  echo "==> vc-bridge onnx + scoreboard"
  cargo test -p vc-bridge --features onnx --locked
  cargo test -p vc-cli --features onnx --locked --test onnx_fixture_decode
  DECODER=onnx \
    ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx \
    VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
    bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json
else
  echo "==> skipping ONNX (RUN_ONNX=0)"
fi

echo "==> cargo audit"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  echo "    (cargo-audit not installed; run: cargo install cargo-audit --locked)"
  exit 1
fi

# Licenses + duplicate crate bans only (RustSec advisories use cargo audit above).
echo "==> cargo deny check licenses bans"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check licenses bans
else
  echo "    (cargo-deny not installed; run: cargo install cargo-deny --locked --version 0.18.3)"
  exit 1
fi

# Optional locally; CI builds fuzz on nightly (see .github/workflows/ci.yml fuzz-build job).
if command -v cargo-fuzz >/dev/null 2>&1 && rustup toolchain list | grep -q nightly; then
  echo "==> cargo fuzz build (optional)"
  (cd fuzz && cargo fuzz build vc_ir_parse_validate)
else
  echo "==> skipping cargo fuzz (optional; needs nightly + cargo-fuzz)"
fi

echo "preflight: OK"
