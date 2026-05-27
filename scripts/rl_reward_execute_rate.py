#!/usr/bin/env python3
"""Return VectorBench execute_rate for a .vcir file (RL / GRPO reward hook).

Stdout: single float in [0, 1]. Exit 0 on successful eval invocation (even if rate is 0).

Examples:
  python3 scripts/rl_reward_execute_rate.py --vcir out.vcir --task add_i32
  source scripts/vectorc-prefix.sh && python3 scripts/rl_reward_execute_rate.py --vcir out.vcir --task add_i32
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from vectorc_invoke import repo_root, run_vectorc  # noqa: E402


def default_suite(root: Path) -> Path:
    return root / "benchmarks/vectorbench_v0/suite.json"


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
    args = parser.parse_args()

    vcir = args.vcir
    if not vcir.is_file():
        print(f"error: missing vcir: {vcir}", file=sys.stderr)
        return 2

    suite = args.suite or default_suite(root)
    if not suite.is_file():
        print(f"error: missing suite: {suite}", file=sys.stderr)
        return 2

    try:
        proc = run_vectorc(
            [
                "eval",
                "-i",
                str(vcir.resolve()),
                "--suite",
                str(suite.resolve()),
                "--task",
                args.task,
                "--json",
            ],
            root=root,
            check=False,
        )
    except OSError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if proc.returncode != 0:
        print(
            f"vectorc eval failed (exit {proc.returncode}):\n{proc.stderr or proc.stdout}",
            file=sys.stderr,
        )
        return 1

    try:
        summary = json.loads(proc.stdout.strip())
    except json.JSONDecodeError as e:
        print(f"error: parse eval JSON: {e}", file=sys.stderr)
        return 1

    rate = float(summary.get("metrics", {}).get("execute_rate", 0.0))
    print(rate)
    return 0


if __name__ == "__main__":
    sys.exit(main())
