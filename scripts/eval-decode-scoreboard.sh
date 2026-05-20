#!/usr/bin/env bash
# Decoder eval scoreboard: decode-z → compile → run manifest cases; emit JSON for CI.
#
# Usage (from repo root):
#   bash scripts/eval-decode-scoreboard.sh
#   bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json
#   DECODER=golden VECTORC='cargo run --locked -p vc-cli --quiet --' bash scripts/eval-decode-scoreboard.sh
#   DECODER=onnx ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx \
#     VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
#     bash scripts/eval-decode-scoreboard.sh
#   DECODER=onnx ONNX_MODEL=benchmarks/fixtures/decoder_z_switch.onnx \
#     Z_JSON=benchmarks/fixtures/z_neg_z0.json \
#     VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
#     bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/mul.json
#
# Environment:
#   DECODER       golden | onnx (default: golden)
#   ONNX_MODEL    path when DECODER=onnx (default: benchmarks/fixtures/decoder_identity_z.onnx)
#   VECTORC       vectorc invocation prefix (default depends on DECODER)
#   Z_JSON        latent input (default: benchmarks/fixtures/z_zeros.json)
#   USE_EXISTING_VCIR=1  skip decode-z; compile/run benchmarks/<program_path> from manifest
#
# Prints one JSON object to stdout. Exit 1 if any manifest fails decode, compile, or run.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export RUST_LOG="${RUST_LOG:-warn}"

DECODER="${DECODER:-golden}"
Z_JSON="${Z_JSON:-$ROOT/benchmarks/fixtures/z_zeros.json}"
USE_EXISTING_VCIR="${USE_EXISTING_VCIR:-0}"

case "$DECODER" in
  golden|onnx) ;;
  *)
    echo "DECODER must be golden or onnx (got: $DECODER)" >&2
    exit 2
    ;;
esac

if [[ "$DECODER" == "onnx" ]]; then
  ONNX_MODEL="${ONNX_MODEL:-$ROOT/benchmarks/fixtures/decoder_identity_z.onnx}"
  if [[ ! -f "$ONNX_MODEL" ]]; then
    echo "ONNX_MODEL not found: $ONNX_MODEL" >&2
    exit 2
  fi
  VECTORC=${VECTORC:-cargo run --locked -p vc-cli --features onnx --quiet --}
else
  VECTORC=${VECTORC:-cargo run --locked -p vc-cli --quiet --}
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 2
fi

if [[ "$USE_EXISTING_VCIR" != "1" && ! -f "$Z_JSON" ]]; then
  echo "Z_JSON not found: $Z_JSON" >&2
  exit 2
fi

manifests=()
if [[ $# -gt 0 ]]; then
  for arg in "$@"; do
    if [[ ! -f "$arg" ]]; then
      echo "manifest not found: $arg" >&2
      exit 2
    fi
    manifests+=("$arg")
  done
else
  shopt -s nullglob
  for m in benchmarks/manifests/*.json; do
    manifests+=("$m")
  done
  shopt -u nullglob
  if [[ ${#manifests[@]} -eq 0 ]]; then
    echo "no manifests under benchmarks/manifests/" >&2
    exit 2
  fi
fi

TMPDIR="${TMPDIR:-/tmp}"
STEM="vectorc-scoreboard-$$"
STATE="$TMPDIR/$STEM-state.jsonl"

cleanup() {
  rm -f "$STATE"
  for f in "$TMPDIR/$STEM"-*; do
    [[ -e "$f" ]] || continue
    rm -f "$f"
  done
}
trap cleanup EXIT

vectorc() {
  # shellcheck disable=SC2086
  # Keep scoreboard stdout JSON-only (run prints numeric results).
  $VECTORC "$@" >/dev/null
}

manifest_rel() {
  python3 -c 'import os,sys; print(os.path.relpath(sys.argv[1], sys.argv[2]))' "$1" "$ROOT"
}

run_case() {
  local wasm=$1 export=$2 fuel=$3 expect=$4 args_json=$5
  local -a cmd=(run -i "$wasm" -e "$export" -f "$fuel" "--expect=$expect")
  VECTORC_RUN_ARGS_JSON="$args_json" vectorc "${cmd[@]}"
}

append_state() {
  python3 -c '
import json, sys
print(json.dumps({
    "key": sys.argv[1],
    "decode_ok": sys.argv[2] == "true",
    "compile_ok": sys.argv[3] == "true",
    "run_ok": sys.argv[4] == "true",
    "cases_passed": int(sys.argv[5]),
    "cases_total": int(sys.argv[6]),
    "errors": sys.argv[7:],
}))
' "$@"
}

: >"$STATE"
overall_ok=1
manifest_keys=()

for manifest in "${manifests[@]}"; do
  key="$(manifest_rel "$manifest")"
  manifest_keys+=("$key")

  slug="$(printf '%s' "$key" | tr '/.' '__')"
  vcir="$TMPDIR/$STEM-$slug.vcir"
  wasm="$TMPDIR/$STEM-$slug.wasm"

  decode_ok=false
  compile_ok=false
  run_ok=false
  cases_passed=0
  cases_total=0
  errors=()

  suite_root="$(cd "$(dirname "$manifest")/.." && pwd)"
  program_path="$(jq -r '.program_path' "$manifest")"
  ref_vcir="$suite_root/$program_path"
  export_name="$(jq -r '.export // "run"' "$manifest")"
  fuel="$(jq -r '.fuel // 100000' "$manifest")"
  cases_total="$(jq '.cases | length' "$manifest")"

  if [[ "$USE_EXISTING_VCIR" == "1" ]]; then
    if [[ ! -f "$ref_vcir" ]]; then
      errors+=("reference vcir missing: $ref_vcir")
    else
      cp "$ref_vcir" "$vcir"
      decode_ok=true
    fi
  else
    decode_args=(decode-z -z "$Z_JSON" -o "$vcir" --decoder "$DECODER")
    if [[ "$DECODER" == "onnx" ]]; then
      decode_args+=(--onnx-model "$ONNX_MODEL")
    fi
    decode_err="$TMPDIR/$STEM-$slug-decode.err"
    if vectorc "${decode_args[@]}" 2>"$decode_err"; then
      decode_ok=true
    else
      err_msg="$(tr '\n' ' ' <"$decode_err" | sed 's/  */ /g' | head -c 500)"
      [[ -z "$err_msg" ]] && err_msg="decode-z failed"
      errors+=("decode-z: $err_msg")
    fi
    rm -f "$decode_err"
  fi

  if [[ "$decode_ok" == true ]]; then
    compile_err="$TMPDIR/$STEM-$slug-compile.err"
    if vectorc compile -i "$vcir" -o "$wasm" 2>"$compile_err"; then
      compile_ok=true
    else
      err_msg="$(tr '\n' ' ' <"$compile_err" | sed 's/  */ /g' | head -c 500)"
      [[ -z "$err_msg" ]] && err_msg="compile failed"
      errors+=("compile: $err_msg")
    fi
    rm -f "$compile_err"
  fi

  if [[ "$compile_ok" == true ]]; then
    case_idx=0
    while [[ "$case_idx" -lt "$cases_total" ]]; do
      expect="$(jq -r ".cases[$case_idx].expect_i32" "$manifest")"
      args_json="$(jq -c ".cases[$case_idx].args" "$manifest")"
      run_err="$TMPDIR/$STEM-$slug-run-$case_idx.err"
      if run_case "$wasm" "$export_name" "$fuel" "$expect" "$args_json" 2>"$run_err"; then
        cases_passed=$((cases_passed + 1))
      else
        err_msg="$(tr '\n' ' ' <"$run_err" | sed 's/  */ /g' | head -c 500)"
        [[ -z "$err_msg" ]] && err_msg="run case $case_idx failed"
        errors+=("case $case_idx: $err_msg")
      fi
      rm -f "$run_err"
      case_idx=$((case_idx + 1))
    done
    if [[ "$cases_passed" -eq "$cases_total" ]]; then
      run_ok=true
    fi
  fi

  if [[ "$decode_ok" != true || "$compile_ok" != true || "$run_ok" != true ]]; then
    overall_ok=0
  fi

  if [[ ${#errors[@]} -gt 0 ]]; then
    append_state "$key" "$decode_ok" "$compile_ok" "$run_ok" "$cases_passed" "$cases_total" "${errors[@]}" >>"$STATE"
  else
    append_state "$key" "$decode_ok" "$compile_ok" "$run_ok" "$cases_passed" "$cases_total" >>"$STATE"
  fi
done

python3 - "$DECODER" "$overall_ok" "$STATE" <<'PY'
import json
import sys

decoder = sys.argv[1]
overall_ok = sys.argv[2] == "1"
state_path = sys.argv[3]

rows = []
with open(state_path, encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if line:
            rows.append(json.loads(line))

manifests = [r["key"] for r in rows]
per_manifest = {r["key"]: {k: r[k] for k in (
    "decode_ok", "compile_ok", "run_ok", "cases_passed", "cases_total", "errors"
)} for r in rows}

print(json.dumps({
    "decoder": decoder,
    "manifests": manifests,
    "per_manifest": per_manifest,
    "ok": overall_ok,
}, indent=2))
print()
PY

if [[ "$overall_ok" -eq 0 ]]; then
  exit 1
fi
