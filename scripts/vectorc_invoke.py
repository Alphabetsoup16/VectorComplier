"""Shared helpers for invoking `vectorc` from Python scripts (stdlib only)."""
from __future__ import annotations

import os
import re
import shlex
import shutil
import subprocess
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def rust_toolchain_channel(root: Path) -> str | None:
    path = root / "rust-toolchain.toml"
    if not path.is_file():
        return None
    match = re.search(r'channel\s*=\s*"([^"]+)"', path.read_text(encoding="utf-8"))
    return match.group(1) if match else None


def vectorc_argv(root: Path | None = None) -> list[str]:
    """Command argv prefix for `vectorc` subcommands (excludes subcommand args).

    Honors ``VECTORC`` when set (same string as ``scripts/vectorc-prefix.sh``).
    Otherwise uses ``rustup run <channel> cargo run -p vc-cli --quiet --``.
    """
    raw = os.environ.get("VECTORC")
    if raw:
        return shlex.split(raw)

    root = root or repo_root()
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


def run_vectorc(
    args: list[str],
    *,
    root: Path | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    root = root or Path(os.environ.get("VECTORCOMPILER_ROOT", repo_root()))
    cmd = vectorc_argv(root) + args
    return subprocess.run(
        cmd,
        cwd=root,
        capture_output=True,
        text=True,
        check=check,
        env={**os.environ, "RUST_LOG": os.environ.get("RUST_LOG", "warn")},
    )
