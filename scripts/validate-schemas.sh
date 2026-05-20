#!/usr/bin/env bash
# Validate checked-in JSON against schemas/ (Program IR fixtures + bench manifest case slices).
# Requires: `pip install check-jsonschema`, `jq`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v check-jsonschema >/dev/null 2>&1; then
  echo "check-jsonschema not found. Install with: pip install check-jsonschema" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found." >&2
  exit 1
fi

echo "==> Program IR v2 schema (.vcir fixtures)"
check-jsonschema \
  --schemafile schemas/program_ir_v2.schema.json \
  benchmarks/programs/add.vcir \
  benchmarks/programs/mul.vcir \
  benchmarks/programs/const42.vcir \
  benchmarks/programs/max_f32.vcir

echo "==> Bench spec v1 schema ({ cases } slice from each manifest)"
shopt -s nullglob
for m in benchmarks/manifests/*.json; do
  echo "    $m"
  jq '{cases: .cases}' "$m" | check-jsonschema --schemafile schemas/bench_spec_v1.schema.json -
done

echo "==> Dataset manifest v1 schema (example + fixture + training slices)"
check-jsonschema \
  --schemafile schemas/dataset_manifest_v1.schema.json \
  benchmarks/examples/dataset_manifest.example.json \
  benchmarks/datasets/fixture_slice_v0/manifest.json
if [[ -f benchmarks/datasets/training_slice_v0/manifest.json ]]; then
  jq 'del(.training_index)' benchmarks/datasets/training_slice_v0/manifest.json | check-jsonschema \
    --schemafile schemas/dataset_manifest_v1.schema.json -
fi

echo "==> Training shard v1 schema (row example + rows.jsonl lines)"
check-jsonschema \
  --schemafile schemas/training_row_v1.schema.json \
  benchmarks/examples/training_row.example.json
if [[ -f benchmarks/datasets/training_slice_v0/rows.jsonl ]]; then
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    printf '%s\n' "$line" | check-jsonschema --schemafile schemas/training_row_v1.schema.json -
  done < benchmarks/datasets/training_slice_v0/rows.jsonl
fi
if [[ -f benchmarks/datasets/training_slice_v0/manifest.json ]]; then
  jq '.training_index' benchmarks/datasets/training_slice_v0/manifest.json | check-jsonschema \
    --schemafile schemas/training_manifest_index_v1.schema.json -
fi

echo "Schema validation OK."
