#!/usr/bin/env bash
# Batch VectorBench eval over a training shard rows.jsonl.
#
# Usage (from repo root):
#   bash scripts/batch-eval-training-shard.sh
#   bash scripts/batch-eval-training-shard.sh benchmarks/datasets/training_slice_v0
#   bash scripts/batch-eval-training-shard.sh benchmarks/datasets/training_slice_v0 --out /tmp/shard-report.json
#   bash scripts/batch-eval-training-shard.sh benchmarks/datasets/training_slice_v0 --dump-failures /tmp/fail-dumps
#
# Each JSONL row must include: "task_id" and a vcir path field ("vcir" or "vcir_path").
# Optional: "row_id" or "program_id", "suite" (per-row suite override).
#
# Prints aggregate JSON to stdout (or --out). Exit 1 if any row has ok=false.
#
# Environment:
#   VECTORC_ROOT, VECTORC, SUITE (same as dump-eval-failures.sh)
set -euo pipefail

ROOT="${VECTORC_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"
# shellcheck source=vectorc-prefix.sh
source "$ROOT/scripts/vectorc-prefix.sh"

export RUST_LOG="${RUST_LOG:-warn}"

SUITE="${SUITE:-$ROOT/benchmarks/vectorbench_v0/suite.json}"

SHARD_DIR="$ROOT/benchmarks/datasets/training_slice_v0"
OUT_FILE=""
DUMP_FAILURES=""
LIMIT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_FILE="$2"
      shift 2
      ;;
    --dump-failures)
      DUMP_FAILURES="$2"
      shift 2
      ;;
    --limit)
      LIMIT="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '1,20p' "$0"
      exit 0
      ;;
    --*)
      echo "unknown option: $1" >&2
      exit 2
      ;;
    *)
      SHARD_DIR="$1"
      shift
      ;;
  esac
done

ROWS="$SHARD_DIR/rows.jsonl"
if [[ ! -f "$ROWS" ]]; then
  echo "batch-eval-training-shard: rows.jsonl not found: $ROWS" >&2
  exit 2
fi

TMPDIR="${TMPDIR:-/tmp}"
STATE="$TMPDIR/vectorc-batch-eval-$$.jsonl"
trap 'rm -f "$STATE"' EXIT

: >"$STATE"

python3 - "$ROOT" "$SHARD_DIR" "$ROWS" "$SUITE" "$VECTORC" "$STATE" "$LIMIT" <<'PY'
import json
import os
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
shard_dir = Path(sys.argv[2])
rows_path = Path(sys.argv[3])
default_suite = Path(sys.argv[4])
vectorc = sys.argv[5].split()
state_path = Path(sys.argv[6])
limit = int(sys.argv[7]) if sys.argv[7] else None

env = os.environ.copy()
env.setdefault("RUST_LOG", "warn")

def resolve_vcir(raw: str) -> Path:
    p = Path(raw)
    if p.is_absolute():
        resolved = p.resolve()
    else:
        resolved = (shard_dir / p).resolve()
    try:
        resolved.relative_to(shard_dir.resolve())
    except ValueError as e:
        raise SystemExit(
            f"vcir path escapes shard directory: {raw!r} -> {resolved}"
        ) from e
    return resolved


def resolve_suite(path: Path) -> Path:
    resolved = path.resolve() if path.is_absolute() else (root / path).resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError as e:
        raise SystemExit(f"suite path escapes repo root: {path}") from e
    return resolved

count = 0
with open(rows_path, encoding="utf-8") as f:
    for line_no, line in enumerate(f, start=1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if limit is not None and count >= limit:
            break
        try:
            row = json.loads(line)
        except json.JSONDecodeError as e:
            raise SystemExit(f"{rows_path}:{line_no}: invalid JSON: {e}") from e

        row_id = row.get("row_id") or row.get("program_id") or f"line-{line_no}"
        task_id = row.get("task_id")
        vcir_raw = row.get("vcir") or row.get("vcir_path")
        if not task_id or not vcir_raw:
            raise SystemExit(
                f"{rows_path}:{line_no}: row needs task_id and vcir/vcir_path "
                f"(got keys {sorted(row)})"
            )

        vcir = resolve_vcir(vcir_raw)
        suite = resolve_suite(
            Path(row["suite"]) if row.get("suite") else default_suite
        )

        eval_stderr = None
        if not vcir.is_file():
            summary = {
                "ok": False,
                "row_id": row_id,
                "vcir": str(vcir),
                "task_id": task_id,
                "error": f"vcir not found: {vcir}",
            }
            eval_stderr = summary["error"]
        else:
            cmd = [
                *vectorc,
                "eval",
                "-i",
                str(vcir),
                "--suite",
                str(suite),
                "--task",
                task_id,
                "--json",
            ]
            proc = subprocess.run(
                cmd,
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            eval_stderr = (proc.stderr or "").strip() or None
            summary = None
            if proc.stdout.strip():
                try:
                    summary = json.loads(proc.stdout.strip())
                except json.JSONDecodeError:
                    summary = None
            if summary is None:
                summary = {
                    "ok": False,
                    "stderr": eval_stderr,
                    "stdout_preview": (proc.stdout or "")[:500] or None,
                    "eval_exit_code": proc.returncode,
                }
            summary.setdefault("ok", proc.returncode == 0 and summary.get("ok", False))

        record = {
            "row_id": row_id,
            "line": line_no,
            "vcir": str(vcir),
            "task_id": task_id,
            "ok": bool(summary.get("ok")),
            "metrics": summary.get("metrics"),
            "task": (summary.get("tasks") or {}).get(task_id),
        }
        if not record["ok"] and eval_stderr:
            record["eval_stderr"] = eval_stderr[:500]
        with open(state_path, "a", encoding="utf-8") as out:
            out.write(json.dumps(record) + "\n")
        count += 1

if count == 0:
    raise SystemExit(f"{rows_path}: no data rows")
PY

python3 - "$SHARD_DIR" "$ROWS" "$STATE" "$OUT_FILE" "$DUMP_FAILURES" "$ROOT" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

shard_dir = Path(sys.argv[1])
rows_path = Path(sys.argv[2])
state_path = Path(sys.argv[3])
out_file = sys.argv[4] or None
dump_failures = sys.argv[5] or None
root = Path(sys.argv[6])

rows = []
with open(state_path, encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if line:
            rows.append(json.loads(line))

failures = [r["vcir"] for r in rows if not r.get("ok")]
rows_passed = sum(1 for r in rows if r.get("ok"))
rows_total = len(rows)

execute_rates = []
for r in rows:
    m = r.get("metrics") or {}
    if isinstance(m.get("execute_rate"), (int, float)):
        execute_rates.append(float(m["execute_rate"]))

aggregate = {
    "shard_dir": str(shard_dir.resolve()),
    "rows_file": str(rows_path.resolve()),
    "rows_total": rows_total,
    "rows_passed": rows_passed,
    "rows_failed": rows_total - rows_passed,
    "pass_rate": (rows_passed / rows_total) if rows_total else 0.0,
    "execute_rate_mean": (
        sum(execute_rates) / len(execute_rates) if execute_rates else 0.0
    ),
    "failures": failures,
    "rows": rows,
    "ok": rows_passed == rows_total and rows_total > 0,
}

if dump_failures and failures:
    dump_root = Path(dump_failures)
    dump_root.mkdir(parents=True, exist_ok=True)
    script = root / "scripts" / "dump-eval-failures.sh"
    for r in rows:
        if r.get("ok"):
            continue
        slug = "".join(
            c if c.isalnum() or c in "-_" else "_" for c in r["row_id"]
        )
        outdir = dump_root / slug
        subprocess.run(
            [
                "bash",
                str(script),
                "--vcir",
                r["vcir"],
                "--task",
                r["task_id"],
                "--out",
                str(outdir),
                "--inspect",
            ],
            cwd=root,
            check=False,
        )
    aggregate["failure_dumps"] = str(dump_root.resolve())

text = json.dumps(aggregate, indent=2, sort_keys=True) + "\n"
if out_file:
    Path(out_file).write_text(text, encoding="utf-8")
else:
    sys.stdout.write(text)

sys.exit(0 if aggregate["ok"] else 1)
PY
