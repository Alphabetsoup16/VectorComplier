#!/usr/bin/env bash
# End-to-end smoke: latent JSON → decode-z → Wasm → run.
# Optional second stage: compile-from-.vcir path (compile → run).
#
# Usage (from repo root):
#   VECTORC="cargo run --locked -p vc-cli --quiet --" bash scripts/eval-decode-pipeline.sh
#
# Decoder selection (default: golden):
#   DECODER=golden|stub|onnx bash scripts/eval-decode-pipeline.sh
#   DECODER=onnx ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx bash scripts/eval-decode-pipeline.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DECODER="${DECODER:-golden}"
ONNX_MODEL="${ONNX_MODEL:-$ROOT/benchmarks/fixtures/decoder_identity_z.onnx}"

case "$DECODER" in
  golden|stub)
    VECTORC=${VECTORC:-cargo run --locked -p vc-cli --quiet --}
    DECODE_EXTRA=()
    ;;
  onnx)
    VECTORC=${VECTORC:-cargo run --locked -p vc-cli --features onnx --quiet --}
    DECODE_EXTRA=(--onnx-model "$ONNX_MODEL")
    ;;
  *)
    echo "eval-decode-pipeline: unknown DECODER=$DECODER (expected golden|stub|onnx)" >&2
    exit 1
    ;;
esac

Z_JSON="$ROOT/benchmarks/fixtures/z_zeros.json"
TMPDIR="${TMPDIR:-/tmp}"
STEM="vectorc-eval-$(printf '%s' "$RANDOM")"
VCIR="$TMPDIR/$STEM.vcir"
WASM_DECODE="$TMPDIR/$STEM-decode.wasm"
WASM_COMPILE="$TMPDIR/$STEM-compile.wasm"

cleanup() {
  rm -f "$VCIR" "$WASM_DECODE" "$WASM_COMPILE"
}
trap cleanup EXIT

echo "==> eval decode pipeline (decoder=$DECODER)"
$VECTORC decode-z \
  -z "$Z_JSON" \
  -o "$VCIR" \
  --wasm-out "$WASM_DECODE" \
  --decoder "$DECODER" \
  "${DECODE_EXTRA[@]}"

echo "    decode-z OK → $VCIR"

$VECTORC run \
  -i "$WASM_DECODE" \
  -e run \
  -f 100000 \
  -a 40,2 \
  --expect 42

echo "    run(decode wasm) OK"

echo "==> compile-from-.vcir path"
$VECTORC compile -i "$VCIR" -o "$WASM_COMPILE"

$VECTORC run \
  -i "$WASM_COMPILE" \
  -e run \
  -f 100000 \
  -a 40,2 \
  --expect 42

echo "    run(compiled wasm) OK"
echo "eval-decode-pipeline: all stages passed (decoder=$DECODER)"
