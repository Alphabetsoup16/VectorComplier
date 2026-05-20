#!/usr/bin/env bash
# Capture VectorBench eval output for one (.vcir, task_id) pair — for training failure triage.
#
# Usage (from repo root):
#   bash scripts/dump-eval-failures.sh --vcir /tmp/bad.vcir --task add_i32 --out /tmp/fail-001
#   bash scripts/dump-eval-failures.sh --vcir /tmp/bad.vcir --task add_i32 --out /tmp/fail-001 --inspect
#
# Writes:
#   OUTDIR/summary.json   full eval JSON (or error envelope if eval did not emit JSON)
#   OUTDIR/errors.txt     human-readable error lines
#   OUTDIR/program.vcir   copy of input
#   OUTDIR/inspect.json   optional (--inspect): vectorc inspect --json
#
# Environment:
#   VECTORC_ROOT   repo root (default: parent of scripts/)
#   VECTORC        vectorc prefix (default: cargo run --locked -p vc-cli --quiet --)
#   SUITE          suite JSON (default: benchmarks/vectorbench_v0/suite.json)
set -euo pipefail

ROOT="${VECTORC_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"
# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"

export RUST_LOG="${RUST_LOG:-warn}"

SUITE="${SUITE:-$ROOT/benchmarks/vectorbench_v0/suite.json}"

VCIR=""
TASK=""
OUTDIR=""
DO_INSPECT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --vcir)
      VCIR="$2"
      shift 2
      ;;
    --task)
      TASK="$2"
      shift 2
      ;;
    --out)
      OUTDIR="$2"
      shift 2
      ;;
    --suite)
      SUITE="$2"
      shift 2
      ;;
    --inspect)
      DO_INSPECT=1
      shift
      ;;
    -h|--help)
      sed -n '1,18p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$VCIR" || -z "$TASK" || -z "$OUTDIR" ]]; then
  echo "dump-eval-failures: --vcir, --task, and --out are required" >&2
  exit 2
fi

if [[ ! -f "$VCIR" ]]; then
  echo "dump-eval-failures: vcir not found: $VCIR" >&2
  exit 2
fi

if [[ ! -f "$SUITE" ]]; then
  echo "dump-eval-failures: suite not found: $SUITE" >&2
  exit 2
fi

mkdir -p "$OUTDIR"
cp "$VCIR" "$OUTDIR/program.vcir"

TMPDIR="${TMPDIR:-/tmp}"
EVAL_JSON="$TMPDIR/vectorc-dump-eval-$$.json"
EVAL_ERR="$TMPDIR/vectorc-dump-eval-$$.err"
trap 'rm -f "$EVAL_JSON" "$EVAL_ERR"' EXIT

set +e
# shellcheck disable=SC2086
$VECTORC eval -i "$VCIR" --suite "$SUITE" --task "$TASK" --json >"$EVAL_JSON" 2>"$EVAL_ERR"
EVAL_RC=$?
set -e

python3 - "$OUTDIR" "$EVAL_JSON" "$EVAL_ERR" "$EVAL_RC" "$TASK" <<'PY'
import json
import sys
from pathlib import Path

outdir = Path(sys.argv[1])
eval_json_path = Path(sys.argv[2])
eval_err_path = Path(sys.argv[3])
eval_rc = int(sys.argv[4])
task_id = sys.argv[5]

stderr_text = eval_err_path.read_text(encoding="utf-8", errors="replace").strip()
stdout_text = eval_json_path.read_text(encoding="utf-8", errors="replace").strip()

summary = None
parse_error = None
if stdout_text:
    try:
        summary = json.loads(stdout_text)
    except json.JSONDecodeError as e:
        parse_error = f"eval stdout is not JSON: {e}"

if summary is None:
    summary = {
        "ok": False,
        "eval_exit_code": eval_rc,
        "parse_error": parse_error,
        "stderr": stderr_text or None,
        "stdout_preview": stdout_text[:2000] if stdout_text else None,
    }

(outdir / "summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)

lines: list[str] = []
if eval_rc != 0:
    lines.append(f"vectorc eval exited {eval_rc}")
if stderr_text:
    lines.append(stderr_text)
if parse_error:
    lines.append(parse_error)

if isinstance(summary, dict) and "tasks" in summary:
    task = summary.get("tasks", {}).get(task_id)
    if task is None:
        lines.append(f"task_id {task_id!r} missing from eval summary")
    else:
        if not task.get("ok", False):
            flags = [
                f"parse_ok={task.get('parse_ok')}",
                f"validate_ok={task.get('validate_ok')}",
                f"compile_ok={task.get('compile_ok')}",
                f"run_ok={task.get('run_ok')}",
                f"cases={task.get('cases_passed')}/{task.get('cases_total')}",
            ]
            lines.append(f"task {task_id}: " + ", ".join(flags))
        for err in task.get("errors") or []:
            lines.append(str(err))
    if not summary.get("ok", False):
        metrics = summary.get("metrics") or {}
        lines.append(
            "metrics: "
            f"execute_rate={metrics.get('execute_rate')} "
            f"validate_rate={metrics.get('validate_rate')} "
            f"compile_rate={metrics.get('compile_rate')}"
        )

if not lines:
    lines.append("eval reported ok=true (no errors to dump)")

(outdir / "errors.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

if [[ "$DO_INSPECT" -eq 1 ]]; then
  INSPECT_ERR="$TMPDIR/vectorc-dump-inspect-$$.err"
  set +e
  # shellcheck disable=SC2086
  $VECTORC inspect -i "$VCIR" --json >"$OUTDIR/inspect.json" 2>"$INSPECT_ERR"
  INSPECT_RC=$?
  set -e
  if [[ "$INSPECT_RC" -ne 0 ]]; then
    {
      echo "vectorc inspect exited $INSPECT_RC"
      cat "$INSPECT_ERR"
    } >>"$OUTDIR/errors.txt"
  fi
  rm -f "$INSPECT_ERR"
fi

# Propagate eval failure so callers can detect dumps of failing programs.
if [[ "$EVAL_RC" -ne 0 ]]; then
  exit "$EVAL_RC"
fi

python3 -c 'import json,sys; s=json.load(open(sys.argv[1])); sys.exit(0 if s.get("ok") else 1)' "$OUTDIR/summary.json"
