# Contributing

Thanks for your interest in VectorCompiler.

## Contributor tiers

### Tier A — Rust changes (most PRs)

```bash
bash scripts/bootstrap-dev.sh    # optional: verify rustup channel
make preflight-lite              # fmt, clippy, tests, smokes (~few minutes)
```

If `cargo test` reports an MSRV mismatch, check `rustup show` for a **directory override** older than `rust-toolchain.toml` and run `rustup override unset`, or use `source scripts/vectorc-prefix.sh` before `cargo run`.

### Tier B — Training / release / ONNX

```bash
bash scripts/preflight.sh
# Skip ONNX locally:
RUN_ONNX=0 bash scripts/preflight.sh
```

Full preflight also needs `cargo-audit`, `cargo-deny`, and `pip install check-jsonschema` + `jq` for schemas.

## Before you open a PR

1. Run **Tier A** at minimum.
2. For ONNX-related changes, run **Tier B** with `RUN_ONNX=1` (default in `preflight.sh`).

## Pull requests

- Keep changes focused; match existing crate boundaries ([docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).
- Update docs and JSON schemas when you change contracts (`z`, Program IR, manifests).
- Add or extend tests for behavior you change.

## Training / ML work

Decoder training lives in a **separate Python repository** (see [docs/TRAINING_ON_MAC.md](docs/TRAINING_ON_MAC.md)). This repo accepts **exported ONNX** and oracle/eval tooling—not PyTorch dependencies in the Rust workspace.

Install `vectorc` for external repos:

```bash
cargo install --path crates/vc-cli --locked
```

## License

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](LICENSE).
