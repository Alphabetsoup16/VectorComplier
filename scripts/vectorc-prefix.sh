#!/usr/bin/env bash
# Source from other scripts: sets VECTORC default from rust-toolchain.toml when unset.
# Usage: source "$(dirname "$0")/vectorc-prefix.sh"
set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
  _vectorc_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
  _vectorc_script_dir="$(cd "$(dirname "$0")" && pwd)"
fi
_vectorc_root="${VECTORC_ROOT:-$(cd "$_vectorc_script_dir/.." && pwd)}"

if [[ -z "${VECTORC:-}" ]]; then
  _channel=""
  if [[ -f "$_vectorc_root/rust-toolchain.toml" ]]; then
    _channel="$(grep -E '^channel\s*=' "$_vectorc_root/rust-toolchain.toml" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
  fi
  if [[ -n "$_channel" ]] && command -v rustup >/dev/null 2>&1; then
    VECTORC="rustup run $_channel cargo run --locked -p vc-cli --quiet --"
  else
    VECTORC="cargo run --locked -p vc-cli --quiet --"
  fi
  export VECTORC
fi
