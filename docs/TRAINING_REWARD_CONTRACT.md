# Training reward contract

External training repos should treat **this repository** as the **ground-truth oracle**. Rewards must come from **execution**, not from JSON plausibility alone.

---

## Primary metric

**`execute_rate`** from `vectorc eval`:

```bash
vectorc eval -i PROGRAM.vcir \
  --suite benchmarks/vectorbench_v0/suite.json \
  --task TASK_ID \
  --json
```

Parse stdout JSON:

```json
{
  "metrics": {
    "execute_rate": 0.0
  }
}
```

- **`execute_rate`** = (spec cases passed) / (spec cases total) across selected tasks.
- For single-task training, pass `--task add_i32` (or `mul_i32`, `const42_i32`).
- **`validate_rate`** / **`compile_rate`** are auxiliary; optimize **`execute_rate`** for RL.

---

## Python helper (in-repo)

```bash
python3 scripts/rl_reward_execute_rate.py \
  --vcir path/to/out.vcir \
  --task add_i32 \
  [--suite benchmarks/vectorbench_v0/suite.json] \
  [--vectorc /path/to/vectorc]
```

Returns exit code 0 and prints a float in `[0, 1]` on stdout (suitable for TRL/custom reward functions).

Environment:

- `VECTORC` — path to `vectorc` binary (default: `vectorc` on `PATH`)
- `VECTORCOMPILER_ROOT` — repo root if suite path is relative

---

## Behavioral check (hot path)

For decode-time filtering without full suite:

```bash
vectorc check -i PROGRAM.vcir --manifest benchmarks/vectorbench_v0/manifests/add_i32.json --json
```

Structured failures include:

- `validation_code` — e.g. `VCIR_CTL001`
- `failed_case` — `{ args, expect_i32, got_i32?, invoke_error? }`

---

## Dataset splits

Regenerate shard with deterministic splits:

```bash
python3 scripts/gen_training_rows.py --write --count 100 --auto-split
```

Per-row `split` field: `train` (80%), `val` (10%), `test` (10%) from `SHA256(program_id)`.

**Do not** tune on `test` during training; use `val` for early stopping.

---

## GRPO / policy gradient sketch

```python
def reward_fn(completion: str) -> float:
    write_vcir(completion)
    return float(subprocess.check_output([
        "python3", "scripts/rl_reward_execute_rate.py",
        "--vcir", path, "--task", task_id,
    ]))
```

Group-relative methods (Posterior-GRPO, etc.) should use **multiple samples per prompt** ranked by the same `execute_rate`.

---

## Invariants

1. Every reward call runs **`validate_module`** before Wasm (invalid IR → 0 execute).
2. Wasm invocation uses **fuel** and optional **wall_ms** from the suite manifest.
3. Training artifacts are **`.vcir` JSON**, not raw Wasm, until the lowerer accepts them.

See [LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md) and [RESEARCH_IMPLEMENTATION_PLAN.md](RESEARCH_IMPLEMENTATION_PLAN.md).
