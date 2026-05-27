# Scripts

Operator and training helpers for VectorCompiler. Most training scripts use **stdlib Python only**; bash scripts source [`vectorc-prefix.sh`](vectorc-prefix.sh) so `cargo` uses the pinned Rust toolchain.

## Getting started

```bash
bash scripts/bootstrap-dev.sh    # check toolchain + optional tools
bash scripts/preflight-lite.sh   # fast gate (~fmt/clippy/test/smokes)
bash scripts/preflight.sh        # full gate before training (audit, ONNX optional)
```

Set **`VECTORC`** (exported by `vectorc-prefix.sh`) or **`VECTORCOMPILER_ROOT`** for Python helpers.

## Script index

| Script | Purpose | Deps |
|--------|---------|------|
| [`bootstrap-dev.sh`](bootstrap-dev.sh) | Check rustup channel, jq, python, optional audit tools | bash |
| [`preflight-lite.sh`](preflight-lite.sh) | Fast CI-like gate for PRs | Rust, optional jq + check-jsonschema |
| [`preflight.sh`](preflight.sh) | Full local gate (scoreboards, audit, deny, ONNX default on) | + cargo-audit, cargo-deny |
| [`vectorc-prefix.sh`](vectorc-prefix.sh) | Sets `VECTORC` to `rustup run <channel> cargo run -p vc-cli --` | rustup |
| [`vectorc_invoke.py`](vectorc_invoke.py) | Python `vectorc_argv()` / `run_vectorc()` | stdlib |
| [`gen_training_rows.py`](gen_training_rows.py) | Build/verify training shard + `z` rows | stdlib; eval uses `vectorc` |
| [`rl_reward_execute_rate.py`](rl_reward_execute_rate.py) | Print `execute_rate` for RL rewards | stdlib + `vectorc_invoke` |
| [`training-oracle.sh`](training-oracle.sh) | Shell wrapper for `vectorc eval` | bash |
| [`vectorbench_oracle.py`](vectorbench_oracle.py) | Python eval wrapper | stdlib |
| [`decode_rank_eval.sh`](decode_rank_eval.sh) | Rank `.vcir` candidates by `execute_rate` | bash + python3 |
| [`eval-decode-scoreboard.sh`](eval-decode-scoreboard.sh) | Decode + score manifest cases | bash |
| [`batch-eval-training-shard.sh`](batch-eval-training-shard.sh) | Eval all rows in a dataset shard | bash |
| [`validate-schemas.sh`](validate-schemas.sh) | JSON Schema checks | jq, check-jsonschema |
| [`verify-dataset-manifest.sh`](verify-dataset-manifest.sh) | Artifact checksums vs manifest | bash |
| [`target-size.sh`](target-size.sh) / [`clean.sh`](clean.sh) | Disk hygiene | bash |

Fixture generators (`gen_decoder_*_onnx.py`, `generate_program_ir_fixtures.py`) need optional `pip install onnx numpy` — maintainer-only.
