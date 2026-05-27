# Quickstart (5 minutes)

This path shows **real value** today: a program is only “correct” if it **validates, compiles, and passes I/O cases** under fuel—not if JSON looks plausible.

## 1. Bootstrap

```bash
git clone https://github.com/Alphabetsoup16/VectorComplier.git
cd VectorComplier
bash scripts/bootstrap-dev.sh
bash scripts/preflight-lite.sh
```

If `cargo test` fails with an MSRV error, run `rustup override unset` in this directory or install the channel from `rust-toolchain.toml` (`1.91.0`).

## 2. Score a program (training oracle)

```bash
source scripts/vectorc-prefix.sh
$VECTORC eval -i benchmarks/programs/add.vcir \
  --suite benchmarks/vectorbench_v0/suite.json \
  --task add_i32 --json
```

Look at `metrics.execute_rate` — **1.0** means all spec cases passed.

Python reward hook (same metric):

```bash
python3 scripts/rl_reward_execute_rate.py \
  --vcir benchmarks/programs/add.vcir --task add_i32
```

## 3. See structured failure (agent / repair)

```bash
$VECTORC validate -i benchmarks/conformance/invalid/return_inside_block.vcir --json
$VECTORC explain VCIR_CTL001 --json
$VECTORC check -i benchmarks/conformance/invalid/return_inside_block.vcir \
  -m benchmarks/manifests/add.json --json
```

`check --json` includes `validation_code` and, on behavioral failure, a `failed_case` counterexample.

## 4. Latent → IR → run (pipeline demo)

```bash
$VECTORC decode-z -z benchmarks/fixtures/z_zeros.json -o /tmp/out.vcir \
  --wasm-out /tmp/out.wasm --decoder golden
$VECTORC run -i /tmp/out.wasm -e run -f 100000 -a 40,2 --expect 42
```

`--decoder stub` fails by design until you attach a trained ONNX model ([DECODER_ROADMAP.md](DECODER_ROADMAP.md)).

## 5. Install `vectorc` for external training repos

```bash
cargo install --path crates/vc-cli --locked
export VECTORC=vectorc
python3 scripts/rl_reward_execute_rate.py --vcir my_decoder_out.vcir --task add_i32
```

## What to read next

| Goal | Doc |
|------|-----|
| Agent JSON contracts | [AGENT_COMPILER_INTERFACE.md](AGENT_COMPILER_INTERFACE.md) |
| Training rewards | [TRAINING_REWARD_CONTRACT.md](TRAINING_REWARD_CONTRACT.md) |
| Full dev setup | [DEVELOPMENT.md](DEVELOPMENT.md) |
| Research roadmap | [RESEARCH_IMPLEMENTATION_PLAN.md](RESEARCH_IMPLEMENTATION_PLAN.md) |
