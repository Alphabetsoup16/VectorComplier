#!/usr/bin/env python3
"""Call `vectorc eval` and return VectorBench metrics for training loops.

Stdlib only. Run from VectorCompiler root or set VECTORC_ROOT.

Example:
  python3 scripts/vectorbench_oracle.py --vcir /tmp/out.vcir
  python3 scripts/vectorbench_oracle.py --vcir /tmp/out.vcir --metric execute_rate
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="VectorBench oracle via vectorc eval")
    parser.add_argument("--vcir", required=True, type=Path, help="Program IR JSON to score")
    parser.add_argument(
        "--suite",
        type=Path,
        default=None,
        help="Suite JSON (default: benchmarks/vectorbench_v0/suite.json)",
    )
    parser.add_argument(
        "--vectorc-root",
        type=Path,
        default=None,
        help="VectorCompiler repo root (default: parent of scripts/)",
    )
    parser.add_argument(
        "--vectorc",
        default=None,
        help='vectorc command prefix (default: cargo run -p vc-cli --quiet --)',
    )
    parser.add_argument(
        "--task",
        default=None,
        help="Single task_id from suite (recommended for training samples)",
    )
    parser.add_argument(
        "--metric",
        choices=("execute_rate", "validate_rate", "compile_rate", "ok"),
        default=None,
        help="Print a single metric line instead of full JSON",
    )
    args = parser.parse_args()

    root = args.vectorc_root or Path(__file__).resolve().parents[1]
    suite = args.suite or (root / "benchmarks/vectorbench_v0/suite.json")
    vectorc = args.vectorc or "cargo run --locked -p vc-cli --quiet --"

    cmd = [
        *vectorc.split(),
        "eval",
        "-i",
        str(args.vcir.resolve()),
        "--suite",
        str(suite.resolve()),
        "--json",
    ]
    if args.task:
        cmd.extend(["--task", args.task])
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "warn")

    proc = subprocess.run(
        cmd,
        cwd=root,
        env=env,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr or proc.stdout or "vectorc eval failed\n")
        return proc.returncode

    summary = json.loads(proc.stdout.strip())
    if args.metric:
        if args.metric == "ok":
            print("1" if summary.get("ok") else "0")
        else:
            print(summary["metrics"][args.metric])
        return 0 if summary.get("ok") else 1

    print(json.dumps(summary, indent=2))
    return 0 if summary.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
