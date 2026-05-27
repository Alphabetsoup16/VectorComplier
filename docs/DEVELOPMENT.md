# Development guide

## Toolchain

Install [rustup](https://rustup.rs/) and open a **new shell** so `~/.cargo/bin` is on `PATH` (e.g. `source ~/.cargo/env` after install).

Run `bash scripts/bootstrap-dev.sh` to verify the pinned toolchain (`rust-toolchain.toml`, currently **1.91.0**). If `rustup override` pins an older compiler in this directory, run `rustup override unset` or use `scripts/vectorc-prefix.sh` / `make preflight-lite`.

```bash
cd /path/to/VectorComplier
rustc --version   # 1.91.0 via rust-toolchain.toml
cargo --version
```

`cargo clean` is **built into Cargo** — there is nothing extra to install.

## Build artifacts and disk hygiene

| Path | Git | Typical size |
|------|-----|----------------|
| `target/` | ignored | ~0.5–1.5 GiB after a full `cargo test` (Wasmtime + dev deps) |
| `.local/` | ignored | Hygiene log only (kilobytes) |

Wasmtime and Cranelift dominate `target/` size. That is normal for a debug workspace build.

### When to run `cargo clean`

- After switching **Rust toolchain** versions (`rust-toolchain.toml` changed).
- When builds behave oddly (stale incremental artifacts).
- Before archiving or syncing the repo to save disk.
- When `target/` grows past ~2 GiB without a good reason (run `scripts/target-size.sh`).

### Helper scripts

```bash
# Show current target size (+ last clean from log)
./scripts/target-size.sh

# Remove artifacts; append entry to .local/target-hygiene.log
./scripts/clean.sh

# Size only, no deletion
./scripts/clean.sh --dry-run
```

Equivalent manual command:

```bash
cargo clean
```

### Custom target directory

To keep artifacts outside the repo (e.g. on a fast external SSD):

```bash
export CARGO_TARGET_DIR="$HOME/.cache/vectorcompiler-target"
```

Scripts respect `CARGO_TARGET_DIR`. Add that export to your shell profile if you use it permanently.

### CI vs local

GitHub Actions uses [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) — runners cache dependencies; **your laptop’s `target/` is unrelated**. Cleaning locally does not affect CI.

## Standard checks

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p vc-cli -- bench -m benchmarks/manifests/add.json
```

## Fuzzing (`fuzz/`)

[`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html) targets JSON parse + IR validation (`vc_ir_parse_validate`). **LibFuzzer requires a nightly toolchain** for `cargo fuzz build` / `cargo fuzz run`:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
cd fuzz
cargo fuzz run vc_ir_parse_validate -- -runs=1000
```

CI builds this target on **nightly** (sanitizer flags); the standalone `fuzz/` crate also builds with stable via plain `cargo build` for quick sanity checks.

## Optional features (ONNX)

```bash
# onnx pulls ort with download-binaries (native ONNX Runtime); needs network on first build for FetchContent/download steps.
cargo test -p vc-bridge --features onnx --locked
cargo test -p vc-cli --features onnx --locked --test onnx_fixture_decode
cargo check -p vc-cli --features onnx
```

See [crates/vc-bridge/README.md](../crates/vc-bridge/README.md) for the **`DECODER_ONNX_INPUT_Z`** contract and CI ONNX fixture.

**Training a learned decoder** (separate Python repo, MacBook Air + optional cloud GPU): [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md). Frozen v0 `z` recipe and shard layout: [Z_BUILD.md](Z_BUILD.md), [TRAINING_DATA.md](TRAINING_DATA.md).

## Preflight

| Gate | When |
|------|------|
| `bash scripts/preflight-lite.sh` or `make preflight-lite` | PRs / daily dev (~minutes) |
| `bash scripts/preflight.sh` | Before decoder training or release |

Script index: [`scripts/README.md`](../scripts/README.md). RL reward smoke: `python3 scripts/rl_reward_execute_rate.py --vcir benchmarks/programs/add.vcir --task add_i32`. Rank decode candidates: `bash scripts/decode_rank_eval.sh /tmp/cands add_i32`.

Full local gate (audit, deny, ONNX scoreboards): see [PREFLIGHT_BEFORE_TRAINING.md](PREFLIGHT_BEFORE_TRAINING.md).

```bash
bash scripts/preflight.sh
RUN_ONNX=0 bash scripts/preflight.sh   # skip ONNX
```

**In-process spec check** (compile once, run all manifest cases — use after `decode-z` in training loops):

```bash
cargo run --locked -p vc-cli --quiet -- check \
  -i /tmp/decoded.vcir \
  -m benchmarks/manifests/add.json \
  --json
```

## Dataset manifests & pipeline smoke

```bash
# Training slice (regenerate: python3 scripts/gen_training_rows.py --write)
bash scripts/verify-dataset-manifest.sh benchmarks/datasets/training_slice_v0 \
  benchmarks/datasets/training_slice_v0/manifest.json

# Batch VectorBench eval over rows.jsonl (see docs/DEBUGGING_DECODE.md)
bash scripts/batch-eval-training-shard.sh benchmarks/datasets/training_slice_v0 --out /tmp/shard-report.json

# Checksums vs benchmarks/examples/dataset_manifest.example.json
bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/examples/dataset_manifest.example.json

# decode-z (golden) → Wasm → run, plus compile-from-.vcir path
VECTORC="cargo run --locked -p vc-cli --quiet --" bash scripts/eval-decode-pipeline.sh

# ONNX decoder (requires vc-cli `onnx` feature; uses checked-in fixture by default)
DECODER=onnx bash scripts/eval-decode-pipeline.sh
DECODER=onnx ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx bash scripts/eval-decode-pipeline.sh

# Decoder eval scoreboard (JSON on stdout; exit 1 if any stage fails)
VECTORC="cargo run --locked -p vc-cli --quiet --" \
  bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json

DECODER=onnx \
  ONNX_MODEL=benchmarks/fixtures/decoder_identity_z.onnx \
  VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
  bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json

# Reference `.vcir` per manifest (skip decode-z; all manifests should pass)
USE_EXISTING_VCIR=1 VECTORC="cargo run --locked -p vc-cli --quiet --" \
  bash scripts/eval-decode-scoreboard.sh
```

`vectorc` logs via **`tracing`** on **stderr**; scriptable stdout is documented in [TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md) (`digest`, `inspect --json`, etc.). Use **`RUST_LOG=warn`** when capturing stdout in shell.

## JSON Schema validation

Machine-readable schemas live under [`schemas/`](../schemas/). They describe JSON **shape** (field names, `op`-tagged instructions, tier-1 size caps). Stack discipline and Wasm semantics are still enforced by `vc_ir::validate_module` after parse.

**CI-aligned wrapper** (Python — same as the **`json-schema`** GitHub Actions job):

```bash
pip install check-jsonschema
./scripts/validate-schemas.sh
```

**`cargo deny`** (licenses + duplicate crate versions; advisories use **`cargo audit`** in CI):

```bash
cargo install cargo-deny --locked --version 0.18.3
cargo deny check licenses bans
```

| File | Validates |
|------|-----------|
| [`schemas/program_ir_v2.schema.json`](../schemas/program_ir_v2.schema.json) | `.vcir` Program IR v2 fixtures ([`scripts/validate-schemas.sh`](../scripts/validate-schemas.sh)) |
| [`schemas/program_ir_v1.schema.json`](../schemas/program_ir_v1.schema.json) | Historical Program IR v1 (migration reference) |
| [`schemas/bench_spec_v1.schema.json`](../schemas/bench_spec_v1.schema.json) | Synthesize spec `{ "cases": [ … ] }` (slice taken from full bench manifests via `jq`) |
| [`schemas/dataset_manifest_v1.schema.json`](../schemas/dataset_manifest_v1.schema.json) | Dataset manifest example ([`benchmarks/examples/`](../benchmarks/examples/)) |

After editing manifests with real checksums, run **`bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/examples/dataset_manifest.example.json`** (see optional scripts section).

**Node ([ajv-cli](https://github.com/ajv-validator/ajv-cli)):**

```bash
npm install -g ajv-cli ajv-formats
ajv validate -s schemas/program_ir_v1.schema.json -d benchmarks/programs/add.vcir --spec=draft2020
ajv validate -s schemas/bench_spec_v1.schema.json -d <(jq '{cases: .cases}' benchmarks/manifests/add.json) --spec=draft2020
```

**Python ([check-jsonschema](https://github.com/python-jsonschema/check-jsonschema)):** individual files — prefer **`./scripts/validate-schemas.sh`** for the full fixture set.

Passing schema validation does not replace `cargo test -p vc-ir` or compiling through `vectorc compile`.
