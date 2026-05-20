#!/usr/bin/env bash
# Training validation oracle: decode (optional) then VectorBench eval → JSON metrics.
#
# Use from an external training repo while this workspace is on PATH or VECTORC_ROOT is set.
#
# Examples (from VectorCompiler repo root):
#   # Score an existing .vcir (fast path — one vectorc process)
#   bash scripts/training-oracle.sh --vcir /tmp/sample.vcir
#
#   # Decode z → .vcir then score (subprocess decode + eval)
#   DECODER=golden bash scripts/training-oracle.sh \
#     --z benchmarks/fixtures/z_zeros.json --vcir /tmp/out.vcir
#
#   DECODER=onnx ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx \
#     VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
#     bash scripts/training-oracle.sh --z benchmarks/fixtures/z_zeros.json --vcir /tmp/out.vcir
#
# Environment:
#   VECTORC_ROOT   path to this repo (default: parent of scripts/)
#   VECTORC        vectorc prefix (default: cargo run -p vc-cli --quiet --)
#   SUITE          suite JSON (default: benchmarks/vectorbench_v0/suite.json)
#   TASK           vectorbench task_id (default: add_i32)
#   DECODER        stub | golden | onnx (for --z decode path)
#   ONNX_MODEL     required when DECODER=onnx
set -euo pipefail

ROOT="${VECTORC_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"
# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"

export RUST_LOG="${RUST_LOG:-warn}"

SUITE="${SUITE:-$ROOT/benchmarks/vectorbench_v0/suite.json}"
TASK="${TASK:-add_i32}"
DECODER="${DECODER:-golden}"

VCIR=""
Z_JSON=""
DO_DECODE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --vcir)
      VCIR="$2"
      shift 2
      ;;
    --z)
      Z_JSON="$2"
      DO_DECODE=1
      shift 2
      ;;
    --suite)
      SUITE="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '1,22p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$VCIR" ]]; then
  echo "training-oracle: --vcir PATH is required" >&2
  exit 2
fi

if [[ "$DO_DECODE" -eq 1 ]]; then
  if [[ -z "$Z_JSON" ]]; then
    echo "training-oracle: --z PATH required with decode" >&2
    exit 2
  fi
  decode_args=(decode-z -z "$Z_JSON" -o "$VCIR" --decoder "$DECODER")
  if [[ "$DECODER" == "onnx" ]]; then
    ONNX_MODEL="${ONNX_MODEL:-$ROOT/benchmarks/fixtures/decoder_identity_z.onnx}"
    decode_args+=(--onnx-model "$ONNX_MODEL")
  fi
  $VECTORC "${decode_args[@]}" >/dev/null
fi

$VECTORC eval -i "$VCIR" --suite "$SUITE" --task "$TASK" --json
