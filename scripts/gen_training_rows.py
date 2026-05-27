#!/usr/bin/env python3
"""Emit or verify training rows for VectorBench v0 canonical programs.

Reads Program IR from benchmarks/programs/ (add, mul, const42) or regenerates
the same canonical bytes as scripts/generate_program_ir_fixtures.py. Builds
deterministic latent vectors ``z`` (256-d f32) per the Z_BUILD recipe documented
below. Writes a frozen shard under benchmarks/datasets/training_slice_v0/.

Z_BUILD recipe (phase 2a plumbing; see docs/Z_CONTRACT.md for runtime contract):
  1. ``digest = SHA256(program_id.encode("utf-8"))`` (32 bytes).
  2. Expand with a counter-chained hash: ``block_k = SHA256(block_{k-1} || u32_le(k))``.
  3. For each 4-byte little-endian chunk in the stream, map
     ``u32 -> f32 in [-1, 1]`` via ``(u / 2**32) * 2.0 - 1.0`` until 256 values.
  Alternative (not used here): interpret the first 64 digest bytes as sixteen
  little-endian f32 values and tile/pad to 256 — documented for cross-tool parity.

Stdlib only. No third-party dependencies.

Examples:
  python3 scripts/gen_training_rows.py --verify-only
  python3 scripts/gen_training_rows.py --write
  python3 scripts/gen_training_rows.py --write --count 10
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

Z_DIM = 256
MANIFEST_REL = "manifest.json"
ROWS_REL = "rows.jsonl"
PROGRAMS_PREFIX = "programs/"

# Canonical families aligned with benchmarks/vectorbench_v0/suite.json
FAMILY_TO_TASK: dict[str, str] = {
    "add": "add_i32",
    "mul": "mul_i32",
    "const42": "const42_i32",
}
CANONICAL_FAMILIES = tuple(FAMILY_TO_TASK.keys())
DEFAULT_SPLIT = "train"


def assign_split(program_id: str) -> str:
    """Deterministic 80/10/10 train/val/test from program_id (reproducible across machines)."""
    h = int(hashlib.sha256(program_id.encode("utf-8")).hexdigest()[:8], 16)
    r = h % 10
    if r < 8:
        return "train"
    if r == 8:
        return "val"
    return "test"


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def load_fixture_module():
    path = repo_root() / "scripts" / "generate_program_ir_fixtures.py"
    spec = importlib.util.spec_from_file_location("generate_program_ir_fixtures", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load fixture generator from {path}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_z(program_id: str, dim: int = Z_DIM) -> list[float]:
    """Deterministic Z_BUILD: SHA256(program_id) hash chain -> dim floats in [-1, 1]."""
    block = hashlib.sha256(program_id.encode("utf-8")).digest()
    out: list[float] = []
    counter = 0
    while len(out) < dim:
        if counter > 0:
            block = hashlib.sha256(block + counter.to_bytes(4, "little")).digest()
        for i in range(0, len(block), 4):
            if len(out) >= dim:
                break
            chunk = block[i : i + 4]
            if len(chunk) < 4:
                chunk = chunk.ljust(4, b"\x00")
            u = int.from_bytes(chunk, "little")
            out.append((u / (2**32)) * 2.0 - 1.0)
        counter += 1
    return out


def program_ids_for_count(count: int) -> list[tuple[str, str]]:
    """Return (program_id, family) pairs; length == count."""
    if count < len(CANONICAL_FAMILIES):
        raise ValueError(
            f"--count must be at least {len(CANONICAL_FAMILIES)} "
            f"({', '.join(CANONICAL_FAMILIES)})"
        )
    pairs: list[tuple[str, str]] = [
        (family, family) for family in CANONICAL_FAMILIES
    ]
    n_extra = count - len(CANONICAL_FAMILIES)
    for i in range(n_extra):
        family = CANONICAL_FAMILIES[i % len(CANONICAL_FAMILIES)]
        pairs.append((f"{family}__syn_{i:04d}", family))
    return pairs


def artifact_entry(rel: str, data: bytes) -> dict[str, Any]:
    return {
        "relative_path": rel,
        "sha256": sha256_hex(data),
        "bytes": len(data),
        "media_type": "application/json" if rel.endswith(".vcir") else None,
    }


def build_manifest(
    vcir_artifacts: list[dict[str, Any]],
    rows_bytes: bytes | None,
    row_count: int,
    rows: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    artifacts = list(vcir_artifacts)
    training_index: dict[str, Any] | None = None
    if rows_bytes is not None:
        row_art = artifact_entry(ROWS_REL, rows_bytes)
        row_art.pop("media_type", None)
        artifacts.append(row_art)
        training_index = {
            "rows_jsonl": {
                "relative_path": ROWS_REL,
                "sha256": row_art["sha256"],
                "bytes": row_art["bytes"],
                "row_count": row_count,
            }
        }
    manifest: dict[str, Any] = {
        "schema_version": 1,
        "dataset_id": "vectorcompiler-training-slice",
        "dataset_version": "v0",
        "license": "Apache-2.0",
        "program_ir_version": 2,
        "notes": (
            "Frozen training slice for VectorBench v0 canonical i32 tasks; "
            "regenerate with scripts/gen_training_rows.py."
        ),
        "splits": (
            {
                name: {"count": cnt}
                for name, cnt in sorted(Counter(r["split"] for r in rows).items())
            }
            if rows
            else {
                "train": {
                    "relative_prefix": PROGRAMS_PREFIX,
                    "count": len({a["relative_path"] for a in vcir_artifacts}),
                },
            }
        ),
        "artifacts": artifacts,
    }
    if training_index is not None:
        manifest["training_index"] = training_index
    # Drop null media_type entries for cleaner JSON.
    for art in manifest["artifacts"]:
        if art.get("media_type") is None:
            art.pop("media_type", None)
    return manifest


def copy_vcir(src: Path, dst: Path) -> None:
    """Copy bytes into the shard (no symlinks — safe for git and external training repos)."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists() or dst.is_symlink():
        dst.unlink()
    shutil.copy2(src, dst)


def rust_toolchain_channel(root: Path) -> str | None:
    path = root / "rust-toolchain.toml"
    if not path.is_file():
        return None
    match = re.search(r'channel\s*=\s*"([^"]+)"', path.read_text(encoding="utf-8"))
    return match.group(1) if match else None


def vectorc_prefix(root: Path) -> list[str]:
    """Command prefix through ``cargo run -p vc-cli --quiet --`` (excludes subcommand)."""
    channel = rust_toolchain_channel(root)
    if channel and shutil.which("rustup"):
        return [
            "rustup",
            "run",
            channel,
            "cargo",
            "run",
            "--locked",
            "-p",
            "vc-cli",
            "--quiet",
            "--",
        ]
    return ["cargo", "run", "--locked", "-p", "vc-cli", "--quiet", "--"]


def cargo_vectorc_available(root: Path) -> bool:
    channel = rust_toolchain_channel(root)
    if channel and shutil.which("rustup"):
        try:
            proc = subprocess.run(
                ["rustup", "run", channel, "cargo", "--version"],
                cwd=root,
                capture_output=True,
                text=True,
            )
            if proc.returncode == 0:
                return True
        except OSError:
            pass
    try:
        proc = subprocess.run(
            ["cargo", "--version"],
            cwd=root,
            capture_output=True,
            text=True,
        )
        return proc.returncode == 0
    except OSError:
        return False


def eval_vcir(
    root: Path,
    vcir_path: Path,
    suite: Path,
    task_id: str,
) -> dict[str, Any]:
    cmd = [
        *vectorc_prefix(root),
        "eval",
        "-i",
        str(vcir_path.resolve()),
        "--suite",
        str(suite.resolve()),
        "--task",
        task_id,
        "--json",
    ]
    proc = subprocess.run(
        cmd,
        cwd=root,
        capture_output=True,
        text=True,
        env={**os.environ, "RUST_LOG": "warn"},
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"vectorc eval failed for {vcir_path} task {task_id}:\n"
            f"{proc.stderr or proc.stdout}"
        )
    return json.loads(proc.stdout.strip())


def verify_row_shape(row: dict[str, Any]) -> None:
    for key in ("program_id", "task_id", "split", "z", "vcir_path", "manifest"):
        if key not in row:
            raise ValueError(f"row missing {key!r}")
    if len(row["z"]) != Z_DIM:
        raise ValueError(
            f"program_id={row['program_id']!r}: z length {len(row['z'])} != {Z_DIM}"
        )
    expected_z = build_z(row["program_id"])
    if row["z"] != expected_z:
        raise ValueError(
            f"program_id={row['program_id']!r}: z does not match Z_BUILD recipe"
        )


def build_rows(
    shard_root: Path,
    programs_dir: Path,
    fixture_mod: Any,
    count: int,
    split: str,
    auto_split: bool,
) -> tuple[list[dict[str, Any]], dict[str, bytes]]:
    """Return rows and unique family -> vcir bytes for programs/."""
    family_bytes: dict[str, bytes] = {}
    rows: list[dict[str, Any]] = []

    for program_id, family in program_ids_for_count(count):
        canonical = fixture_mod.FAMILIES[family]
        fixture_mod.validate_module_shape(canonical, family)
        data = fixture_mod.serialize_module(canonical)
        family_bytes[family] = data

        vcir_rel = f"{PROGRAMS_PREFIX}{family}.vcir"
        row_split = assign_split(program_id) if auto_split else split
        rows.append(
            {
                "program_id": program_id,
                "task_id": FAMILY_TO_TASK[family],
                "split": row_split,
                "z": build_z(program_id),
                "vcir_path": vcir_rel,
                "manifest": MANIFEST_REL,
            }
        )

    # Prefer on-disk benchmarks/programs when present and matching.
    for family in family_bytes:
        on_disk = programs_dir / f"{family}.vcir"
        if on_disk.is_file():
            disk = on_disk.read_bytes()
            expected = family_bytes[family]
            if disk != expected:
                raise SystemExit(
                    f"{on_disk}: bytes differ from canonical generator "
                    f"(expected sha256 {sha256_hex(expected)}, "
                    f"got {sha256_hex(disk)})"
                )
            family_bytes[family] = disk

    return rows, family_bytes


def rows_jsonl_bytes(rows: list[dict[str, Any]]) -> bytes:
    lines = [json.dumps(row, ensure_ascii=False, separators=(",", ":")) for row in rows]
    return ("\n".join(lines) + "\n").encode("utf-8")


def write_shard(
    shard_root: Path,
    programs_dir: Path,
    fixture_mod: Any,
    count: int,
    split: str,
    auto_split: bool,
    skip_eval: bool,
) -> bool:
    rows, family_bytes = build_rows(
        shard_root, programs_dir, fixture_mod, count, split, auto_split
    )
    shard_programs = shard_root / PROGRAMS_PREFIX.rstrip("/")

    for family, data in family_bytes.items():
        dst = shard_programs / f"{family}.vcir"
        src = programs_dir / f"{family}.vcir"
        if src.is_file():
            copy_vcir(src, dst)
        else:
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(data)

    rows_path = shard_root / ROWS_REL
    rows_bytes = rows_jsonl_bytes(rows)
    rows_path.write_bytes(rows_bytes)

    vcir_artifacts = [
        artifact_entry(f"{PROGRAMS_PREFIX}{family}.vcir", data)
        for family, data in sorted(family_bytes.items())
    ]
    manifest = build_manifest(vcir_artifacts, rows_bytes, len(rows), rows=rows)
    manifest_path = shard_root / MANIFEST_REL
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    print(f"wrote {rows_path} ({len(rows)} rows)")
    print(f"wrote {manifest_path}")
    for family in sorted(family_bytes):
        print(f"wrote {shard_programs / f'{family}.vcir'}")

    if skip_eval:
        return True
    return verify_eval(shard_root, rows, strict=True)


def verify_shard(
    shard_root: Path,
    programs_dir: Path,
    fixture_mod: Any,
    count: int,
    split: str,
    auto_split: bool,
    skip_eval: bool,
) -> int:
    rows_path = shard_root / ROWS_REL
    manifest_path = shard_root / MANIFEST_REL
    if not rows_path.is_file():
        raise SystemExit(f"missing {rows_path} (pass --write to create)")
    if not manifest_path.is_file():
        raise SystemExit(f"missing {manifest_path} (pass --write to create)")

    expected_rows, family_bytes = build_rows(
        shard_root, programs_dir, fixture_mod, count, split, auto_split
    )
    on_disk_rows: list[dict[str, Any]] = []
    for line_no, line in enumerate(rows_path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        verify_row_shape(row)
        on_disk_rows.append(row)

    if on_disk_rows != expected_rows:
        raise SystemExit(
            f"{rows_path}: content differs from generator "
            f"(expected {len(expected_rows)} rows, got {len(on_disk_rows)})"
        )
    print(f"OK {ROWS_REL} ({len(on_disk_rows)} rows)")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    for art in manifest.get("artifacts", []):
        rel = art["relative_path"]
        path = shard_root / rel
        if not path.is_file():
            raise SystemExit(f"missing artifact {path}")
        actual = sha256_hex(path.read_bytes())
        if actual != art["sha256"]:
            raise SystemExit(
                f"{path}: sha256 mismatch (manifest {art['sha256']}, actual {actual})"
            )
        print(f"OK {rel}")

    rows_bytes = rows_path.read_bytes()
    idx = manifest.get("training_index", {}).get("rows_jsonl")
    if idx and idx.get("sha256") != sha256_hex(rows_bytes):
        raise SystemExit(f"{rows_path}: sha256 mismatch vs manifest training_index")

    for family, data in family_bytes.items():
        vcir = shard_root / PROGRAMS_PREFIX / f"{family}.vcir"
        if not vcir.is_file():
            raise SystemExit(f"missing {vcir}")
        if vcir.read_bytes() != data:
            raise SystemExit(f"{vcir}: bytes differ from canonical fixture module")

    print(f"OK {MANIFEST_REL}")

    if not skip_eval and not verify_eval(shard_root, expected_rows, strict=True):
        return 1
    return 0


def verify_eval(shard_root: Path, rows: list[dict[str, Any]], *, strict: bool) -> bool:
    root = repo_root()
    suite = root / "benchmarks/vectorbench_v0/suite.json"
    ok_all = True
    for row in rows:
        vcir = shard_root / row["vcir_path"]
        try:
            summary = eval_vcir(root, vcir, suite, row["task_id"])
        except RuntimeError as exc:
            ok_all = False
            msg = str(exc).strip()
            if strict:
                raise SystemExit(msg) from exc
            print(f"warning: vectorc eval skipped/failed: {msg}", file=sys.stderr)
            continue
        if not summary.get("ok"):
            ok_all = False
            detail = json.dumps(summary, ensure_ascii=False)
            if strict:
                raise SystemExit(
                    f"eval not ok for program_id={row['program_id']!r} "
                    f"task={row['task_id']!r}: {detail}"
                )
            print(
                f"warning: eval not ok for {row['program_id']}: {detail}",
                file=sys.stderr,
            )
            continue
        print(
            f"eval OK {row['program_id']} -> {row['task_id']} "
            f"(execute_rate={summary.get('metrics', {}).get('execute_rate')})"
        )
    return ok_all


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--shard-root",
        type=Path,
        default=None,
        help="Dataset shard root (default: benchmarks/datasets/training_slice_v0)",
    )
    parser.add_argument(
        "--programs-dir",
        type=Path,
        default=None,
        help="Source .vcir directory (default: benchmarks/programs)",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Write rows.jsonl, programs/*.vcir, and manifest.json",
    )
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="Verify existing shard (default when --write is absent)",
    )
    parser.add_argument(
        "--count",
        type=int,
        default=len(CANONICAL_FAMILIES),
        help=f"Total training rows (default: {len(CANONICAL_FAMILIES)}; "
        "extras use perturbed program_id, same vcir family)",
    )
    parser.add_argument(
        "--split",
        default=DEFAULT_SPLIT,
        choices=("train", "val", "test"),
        help=f"Split label for every row when --auto-split is off (default: {DEFAULT_SPLIT})",
    )
    parser.add_argument(
        "--auto-split",
        action="store_true",
        help="Assign train/val/test per program_id via SHA256 (80/10/10); ignores --split",
    )
    parser.add_argument(
        "--skip-eval",
        action="store_true",
        help="Do not run vectorc eval (implies warning if cargo missing)",
    )
    args = parser.parse_args()
    if args.verify_only and args.write:
        parser.error("--verify-only and --write are mutually exclusive")
    if args.count < len(CANONICAL_FAMILIES):
        parser.error(f"--count must be >= {len(CANONICAL_FAMILIES)}")

    root = repo_root()
    shard_root = args.shard_root or (root / "benchmarks/datasets/training_slice_v0")
    programs_dir = args.programs_dir or (root / "benchmarks/programs")
    fixture_mod = load_fixture_module()

    skip_eval = args.skip_eval
    if not skip_eval and not cargo_vectorc_available(root):
        print(
            "warning: cargo not available; skipping vectorc eval "
            "(pass --skip-eval to silence)",
            file=sys.stderr,
        )
        skip_eval = True

    if args.write:
        shard_root.mkdir(parents=True, exist_ok=True)
        if not write_shard(
            shard_root,
            programs_dir,
            fixture_mod,
            args.count,
            args.split,
            args.auto_split,
            skip_eval,
        ):
            return 1
    else:
        return verify_shard(
            shard_root,
            programs_dir,
            fixture_mod,
            args.count,
            args.split,
            args.auto_split,
            skip_eval,
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
