#!/usr/bin/env python3
"""Emit or verify canonical Program IR v2 fixtures (add, mul, const42, max_f32).

Writes JSON under benchmarks/programs/ and can emit a dataset manifest v1 with
SHA-256 checksums. No third-party dependencies.

Examples:
  python3 scripts/generate_program_ir_fixtures.py --verify-only
  python3 scripts/generate_program_ir_fixtures.py --write
  python3 scripts/generate_program_ir_fixtures.py --manifest benchmarks/datasets/fixture_slice_v0/manifest.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

PROGRAM_IR_VERSION = 2
EXPORT_NAME = "run"

# Canonical task family v0 (matches checked-in benchmarks/programs/*.vcir).
FAMILIES: dict[str, dict[str, Any]] = {
    "add": {
        "program_ir_version": PROGRAM_IR_VERSION,
        "export_name": EXPORT_NAME,
        "func": {
            "sig": {"params": ["i32", "i32"], "results": ["i32"]},
            "locals": [],
            "body": [
                {"op": "local_get", "index": 0},
                {"op": "local_get", "index": 1},
                {"op": "i32_add"},
                {"op": "return"},
            ],
        },
    },
    "mul": {
        "program_ir_version": PROGRAM_IR_VERSION,
        "export_name": EXPORT_NAME,
        "func": {
            "sig": {"params": ["i32", "i32"], "results": ["i32"]},
            "locals": [],
            "body": [
                {"op": "local_get", "index": 0},
                {"op": "local_get", "index": 1},
                {"op": "i32_mul"},
                {"op": "return"},
            ],
        },
    },
    "const42": {
        "program_ir_version": PROGRAM_IR_VERSION,
        "export_name": EXPORT_NAME,
        "func": {
            "sig": {"params": [], "results": ["i32"]},
            "locals": [],
            "body": [
                {"op": "i32_const", "value": 42},
                {"op": "return"},
            ],
        },
    },
    "max_f32": {
        "program_ir_version": PROGRAM_IR_VERSION,
        "export_name": EXPORT_NAME,
        "func": {
            "sig": {"params": ["f32", "f32"], "results": ["f32"]},
            "locals": [],
            "body": [
                {"op": "local_get", "index": 0},
                {"op": "local_get", "index": 1},
                {"op": "f32_gt"},
                {
                    "op": "if_else",
                    "result": "f32",
                    "then_body": [{"op": "local_get", "index": 0}],
                    "else_body": [{"op": "local_get", "index": 1}],
                },
                {"op": "return"},
            ],
        },
    },
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _compact_object(obj: dict[str, Any]) -> str:
    """Match checked-in .vcir style: `{ "k": v, ... }` with spaces after braces."""
    inner = json.dumps(obj, ensure_ascii=False, separators=(", ", ": "))
    return "{ " + inner[1:-1] + " }"


def _format_instr(instr: dict[str, Any], indent: int) -> list[str]:
    pad = " " * indent
    op = instr["op"]
    if op == "if_else":
        then_b = ", ".join(_compact_object(i) for i in instr["then_body"])
        else_b = ", ".join(_compact_object(i) for i in instr["else_body"])
        return [
            f"{pad}{{",
            f'{pad}  "op": "if_else",',
            f'{pad}  "result": "{instr["result"]}",',
            f"{pad}  \"then_body\": [{then_b}],",
            f"{pad}  \"else_body\": [{else_b}]",
            f"{pad}}}",
        ]
    return [f"{pad}{_compact_object(instr)}"]


def serialize_module(module: dict[str, Any]) -> bytes:
    """Deterministic bytes matching benchmarks/programs/*.vcir layout."""
    func = module["func"]
    sig = func["sig"]
    params = json.dumps(sig["params"], ensure_ascii=False, separators=(", ", ": "))
    results = json.dumps(sig["results"], ensure_ascii=False, separators=(", ", ": "))
    lines = [
        "{",
        f'  "program_ir_version": {module["program_ir_version"]},',
        f'  "export_name": "{module["export_name"]}",',
        '  "func": {',
        '    "sig": {',
        f'      "params": {params},',
        f'      "results": {results}',
        "    },",
        "    \"locals\": [],",
        '    "body": [',
    ]
    body = func["body"]
    for i, instr in enumerate(body):
        chunk = _format_instr(instr, indent=6)
        suffix = "," if i < len(body) - 1 else ""
        if len(chunk) == 1:
            lines.append(chunk[0] + suffix)
        else:
            lines.extend(chunk[:-1] + [chunk[-1] + suffix])
    lines.extend(
        [
            "    ]",
            "  }",
            "}",
            "",
        ]
    )
    return "\n".join(lines).encode("utf-8")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def validate_module_shape(module: dict[str, Any], name: str) -> None:
    if module.get("program_ir_version") != PROGRAM_IR_VERSION:
        raise ValueError(f"{name}: program_ir_version must be {PROGRAM_IR_VERSION}")
    if module.get("export_name") != EXPORT_NAME:
        raise ValueError(f"{name}: export_name must be {EXPORT_NAME!r}")
    func = module.get("func")
    if not isinstance(func, dict):
        raise ValueError(f"{name}: missing func")
    for key in ("sig", "locals", "body"):
        if key not in func:
            raise ValueError(f"{name}: func missing {key}")
    body = func["body"]
    if not body or body[-1].get("op") != "return":
        raise ValueError(f"{name}: body must end with return")


def artifact_entry(rel: str, data: bytes) -> dict[str, Any]:
    return {
        "relative_path": rel,
        "sha256": sha256_hex(data),
        "bytes": len(data),
        "media_type": "application/json",
    }


def build_manifest(artifacts: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "dataset_id": "vectorcompiler-fixture-slice",
        "dataset_version": "v0",
        "license": "MIT OR Apache-2.0",
        "program_ir_version": PROGRAM_IR_VERSION,
        "notes": (
            "Frozen slice of canonical Program IR fixtures; "
            "regenerate with scripts/generate_program_ir_fixtures.py."
        ),
        "splits": {
            "train": {"relative_prefix": "programs/", "count": len(artifacts)},
        },
        "artifacts": artifacts,
    }


def process_family(
    name: str,
    programs_dir: Path,
    write: bool,
) -> tuple[str, bytes]:
    canonical = FAMILIES[name]
    validate_module_shape(canonical, name)
    data = serialize_module(canonical)
    rel = f"programs/{name}.vcir"
    path = programs_dir / f"{name}.vcir"

    if write:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        print(f"wrote {path}")
    elif path.is_file():
        on_disk = path.read_bytes()
        if on_disk != data:
            raise SystemExit(
                f"{path}: bytes differ from canonical generator output "
                f"(expected sha256 {sha256_hex(data)}, got {sha256_hex(on_disk)})"
            )
        print(f"OK {rel}")
    else:
        raise SystemExit(f"missing fixture: {path} (pass --write to create)")

    return rel, data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--programs-dir",
        type=Path,
        default=None,
        help="Directory for .vcir files (default: benchmarks/programs)",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Write canonical .vcir files (default: verify existing only)",
    )
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="Alias for default mode (verify fixtures, no writes)",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=None,
        help="Write dataset manifest v1 JSON to this path",
    )
    parser.add_argument(
        "--families",
        nargs="*",
        choices=sorted(FAMILIES),
        default=sorted(FAMILIES),
        help="Subset of fixture families (default: all)",
    )
    args = parser.parse_args()
    if args.verify_only and args.write:
        parser.error("--verify-only and --write are mutually exclusive")

    root = repo_root()
    programs_dir = args.programs_dir or (root / "benchmarks/programs")

    artifacts: list[dict[str, Any]] = []
    for name in args.families:
        rel, data = process_family(name, programs_dir, write=args.write)
        artifacts.append(artifact_entry(rel, data))

    if args.manifest:
        manifest = build_manifest(artifacts)
        args.manifest.parent.mkdir(parents=True, exist_ok=True)
        args.manifest.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        print(f"wrote manifest {args.manifest} ({len(artifacts)} artifacts)")

    return 0


if __name__ == "__main__":
    sys.exit(main())
