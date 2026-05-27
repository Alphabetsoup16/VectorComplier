#!/usr/bin/env bash
# Check local dev prerequisites and print fixes. Does not install Rust automatically.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

need() { echo "  MISSING: $*"; MISSING=1; }
ok() { echo "  OK: $*"; }

MISSING=0
echo "==> VectorCompiler dev bootstrap"
echo "Root: $ROOT"

if command -v rustup >/dev/null 2>&1; then
  ok "rustup"
  CHANNEL="$(grep -E '^channel\s*=' rust-toolchain.toml 2>/dev/null | sed -E 's/.*"([^"]+)".*/\1/' || true)"
  if [[ -n "$CHANNEL" ]]; then
    if rustup run "$CHANNEL" rustc --version >/dev/null 2>&1; then
      ok "toolchain $CHANNEL ($(rustup run "$CHANNEL" rustc --version))"
    else
      need "rustup toolchain $CHANNEL (run: rustup toolchain install $CHANNEL)"
    fi
    OVERRIDE="$(rustup show 2>/dev/null | awk '/directory override/ {print $3}' || true)"
    if [[ -n "$OVERRIDE" && "$OVERRIDE" != "$CHANNEL" ]]; then
      echo "  WARN: directory override is $OVERRIDE but rust-toolchain.toml wants $CHANNEL"
      echo "        Plain 'cargo test' may fail. Fix: rustup override unset  OR  use scripts/vectorc-prefix.sh"
    fi
  fi
else
  need "rustup (https://rustup.rs/)"
fi

for bin in cargo jq python3; do
  if command -v "$bin" >/dev/null 2>&1; then ok "$bin"; else need "$bin"; fi
done

if python3 -c "import check_jsonschema" 2>/dev/null; then
  ok "python check-jsonschema"
else
  echo "  OPTIONAL (preflight schemas): pip install check-jsonschema"
fi

for bin in cargo-audit cargo-deny; do
  if command -v "$bin" >/dev/null 2>&1; then ok "$bin"; else
    echo "  OPTIONAL (full preflight): $bin"
  fi
done

# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"
echo "VECTORC=$VECTORC"

echo ""
echo "Quick verify (no ONNX):"
echo "  bash scripts/preflight-lite.sh"
echo ""
echo "Full gate (training / release):"
echo "  bash scripts/preflight.sh"
echo "  RUN_ONNX=0 bash scripts/preflight.sh   # skip ONNX build"

if [[ "$MISSING" -ne 0 ]]; then
  exit 1
fi
