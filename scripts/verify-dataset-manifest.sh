#!/usr/bin/env bash
# Verify SHA-256 of every artifact listed in a dataset manifest JSON (see schemas/dataset_manifest_v1.schema.json).
# Usage:
#   bash scripts/verify-dataset-manifest.sh DATASET_ROOT MANIFEST.json
# Examples:
#   bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/datasets/fixture_slice_v0/manifest.json
#   bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/examples/dataset_manifest.example.json
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "${1:-}" == "" || "${2:-}" == "" ]]; then
  echo "usage: $0 DATASET_ROOT MANIFEST.json" >&2
  exit 2
fi

DATASET_ROOT="$(cd "$1" && pwd)"
MANIFEST="$(python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" "$2")"

if ! command -v shasum >/dev/null 2>&1 && ! command -v sha256sum >/dev/null 2>&1; then
  echo "need shasum or sha256sum" >&2
  exit 1
fi

sum_file() {
  local f=$1
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$f" | awk '{print $1}'
  else
    shasum -a 256 "$f" | awk '{print $1}'
  fi
}

echo "==> verify dataset manifest"
echo "    root: $DATASET_ROOT"
echo "    manifest: $MANIFEST"

while IFS=$'\t' read -r rel expect; do
  [[ -z "$rel" ]] && continue
  path="$DATASET_ROOT/$rel"
  if [[ ! -f "$path" ]]; then
    echo "MISSING: $rel" >&2
    exit 1
  fi
  actual="$(sum_file "$path")"
  if [[ "$actual" != "$expect" ]]; then
    echo "MISMATCH: $rel" >&2
    echo "  expected: $expect" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
  echo "    OK $rel"
done < <(python3 <<PY
import json, sys
with open("$MANIFEST") as f:
    m = json.load(f)
for a in m.get("artifacts", []):
    print(a["relative_path"] + "\t" + a["sha256"])
PY
)

echo "Dataset manifest checksum verification OK."
