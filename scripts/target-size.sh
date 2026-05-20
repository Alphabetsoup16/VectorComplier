#!/usr/bin/env bash
# Print workspace target directory size (for quick disk checks).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

if [[ ! -d "$TARGET" ]]; then
  echo "No target directory at: $TARGET"
  exit 0
fi

du -sh "$TARGET"
echo "Path: $TARGET"

if [[ -f "$ROOT/.local/target-hygiene.log" ]]; then
  echo ""
  echo "Last clean (from hygiene log):"
  tail -n 4 "$ROOT/.local/target-hygiene.log" | sed 's/^/  /'
fi
