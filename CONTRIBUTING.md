# Contributing

Thanks for your interest in VectorCompiler.

## Before you open a PR

1. Run the preflight gate from the repo root:
   ```bash
   bash scripts/preflight.sh
   ```
2. For ONNX-related changes, also run with ONNX enabled:
   ```bash
   RUN_ONNX=1 bash scripts/preflight.sh
   ```

## Pull requests

- Keep changes focused; match existing crate boundaries ([docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)).
- Update docs and JSON schemas when you change contracts (`z`, Program IR, manifests).
- Add or extend tests for behavior you change.

## Training / ML work

Decoder training lives in a **separate Python repository** (see [docs/TRAINING_ON_MAC.md](docs/TRAINING_ON_MAC.md)). This repo accepts **exported ONNX** and oracle/eval tooling—not PyTorch dependencies in the Rust workspace.

## License

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](LICENSE).
