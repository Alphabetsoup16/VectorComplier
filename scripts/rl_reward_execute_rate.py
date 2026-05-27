#!/usr/bin/env python3
"""Return VectorBench execute_rate for a .vcir file (RL / GRPO reward hook).

Stdout: single float in [0, 1]. Exit 0 on successful eval invocation (even if rate is 0).

Examples:
  python3 scripts/rl_reward_execute_rate.py --vcir out.vcir --task add_i32
  VECTORC=./target/debug/vectorc python3 scripts/rl_reward_execute_rate.py --vcir out.vcir --task mul_i32
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_suite(root: Path) -> Path:
    return root / "benchmarks/vectorbench_v0/suite.json"


def vectorc_bin() -> str:
    return os.environ.get("VECTORC", "vectorc")


def run_eval(
    vcir: Path,
    suite: Path,
    task: str,
    vectorc: str,
    cwd: Path,
) -> dict:
    cmd = [
        vectorc,
        "eval",
        "-i",
        str(vcir.resolve()),
        "--suite",
        str(suite.resolve()),
        "--task",
        task,
        "--json",
    ]
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        capture_output=True,
        text=True,
        env={**os.environ, "RUST_LOG": "warn"},
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"vectorc eval failed (exit {proc.returncode}):\n"
            f"{proc.stderr or proc.stdout}"
        )
    return json.loads(proc.stdout.strip())


def main() -> int:
    root = Path(os.environ.get("VECTORCOMPILER_ROOT", repo_root()))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vcir", type=Path, required=True, help="Program IR JSON path")
    parser.add_argument("--task", required=True, help="VectorBench task_id (e.g. add_i32)")
    parser.add_argument(
        "--suite",
        type=Path,
        default=None,
        help="suite.json (default: benchmarks/vectorbench_v0/suite.json)",
    )
    parser.add_argument(
        "--vectorc",
        default=None,
        help="vectorc binary (default: $VECTORC or vectorc)",
    )
    args = parser.parse_args()

    vcir = args.vcir
    if not vcir.is_file():
        print(f"error: missing vcir: {vcir}", file=sys.stderr)
        return 2

    suite = args.suite or default_suite(root)
    if not suite.is_file():
        print(f"error: missing suite: {suite}", file=sys.stderr)
        return 2

    vectorc = args.vectorc or vectorc_bin()
    try:
        summary = run_eval(vcir, suite, args.task, vectorc, root)
    except (RuntimeError, json.JSONDecodeError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    rate = float(summary.get("metrics", {}).get("execute_rate", 0.0))
    print(rate)
    return 0


if __name__ == "__main__":
    sys.exit(main())
