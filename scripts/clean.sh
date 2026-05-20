#!/usr/bin/env bash
# Remove workspace build artifacts and record hygiene metadata (local only).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LOCAL_DIR="$ROOT/.local"
LOG="$LOCAL_DIR/target-hygiene.log"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

mkdir -p "$LOCAL_DIR"

size_bytes() {
  if [[ -d "$1" ]]; then
    du -sk "$1" 2>/dev/null | awk '{print $1 * 1024}'
  else
    echo 0
  fi
}

human() {
  local b="$1"
  if command -v numfmt >/dev/null 2>&1; then
    numfmt --to=iec-i --suffix=B "$b"
  else
    echo "${b} bytes"
  fi
}

before="$(size_bytes "$TARGET")"
echo "Target directory: $TARGET"
echo "Size before:      $(human "$before")"

if [[ "${1:-}" == "--dry-run" ]]; then
  echo "(dry-run — no artifacts removed)"
  exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not on PATH — source ~/.cargo/env or install rustup" >&2
  exit 1
fi

cargo clean --manifest-path "$ROOT/Cargo.toml"

after="$(size_bytes "$TARGET")"
freed=$((before - after))
if ((freed < 0)); then
  freed=0
fi

ts="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
{
  echo "[$ts] cargo clean"
  echo "  target=$TARGET"
  echo "  before=$(human "$before")"
  echo "  after=$(human "$after")"
  echo "  freed=$(human "$freed")"
} >>"$LOG"

echo "Size after:       $(human "$after")"
echo "Freed:            $(human "$freed")"
echo "Logged to:        $LOG"
