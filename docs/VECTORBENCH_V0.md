# VectorBench v0 (training validation oracle)

**VectorBench** is the frozen behavioral benchmark for decoder training: score a candidate `.vcir` against multiple I/O manifests in **one** `vectorc` process and get rates you can log every epoch.

**Suite file:** [`benchmarks/vectorbench_v0/suite.json`](../benchmarks/vectorbench_v0/suite.json)

---

## Metrics (optimize these, not byte CE alone)

| Metric | Meaning |
|--------|---------|
| `validate_rate` | Fraction of tasks where `validate_module` passed |
| `compile_rate` | Fraction of tasks where lowering + Wasm compile succeeded |
| `execute_rate` | **All spec cases passed / all cases** across tasks (primary) |
| `tasks_passed` | Tasks with full pass on that task’s manifest |

Byte accuracy on JSON is a proxy; **execute_rate** is the oracle.

---

## CLI (in-process, training hot path)

```bash
# Training: one decoded program → one task (matches your label / manifest)
cargo run -p vc-cli --quiet -- eval \
  -i /tmp/decoded.vcir \
  --suite benchmarks/vectorbench_v0/suite.json \
  --task add_i32 \
  --json

# Regression: run all tasks (each task’s manifest against the same `.vcir` — only valid for multi-program eval)
cargo run -p vc-cli --quiet -- eval -i /tmp/decoded.vcir --suite benchmarks/vectorbench_v0/suite.json --json
```

Stdout is one JSON object (`suite_id`, `tasks`, `metrics`, `ok`). Exit `0` only if every **selected** task passes.

Per-task detail includes `parse_ok`, `validate_ok`, `compile_ok`, `run_ok`, `cases_passed`, `cases_total`, `errors`.

---

## Shell wrapper (decode + eval)

```bash
bash scripts/training-oracle.sh --vcir /tmp/out.vcir
bash scripts/training-oracle.sh --z path/to/z.json --vcir /tmp/out.vcir
```

Set `VECTORC_ROOT` to this repo when calling from a sibling training project.

---

## Python (training repo)

```bash
python3 scripts/vectorbench_oracle.py --vcir /tmp/out.vcir --metric execute_rate
```

Use in PyTorch validation (set `TASK` / `--task` to match the sample, e.g. `mul_i32`):

```python
import subprocess
out = subprocess.check_output([
    "python3", f"{VECTORC_ROOT}/scripts/vectorbench_oracle.py",
    "--vcir", str(vcir_path),
    "--task", sample["task_id"],  # e.g. add_i32, mul_i32, const42_i32
    "--metric", "execute_rate",
], text=True)
execute_rate = float(out.strip())
```

Prefer **`vectorc eval`** directly if you batch many samples and want one fewer subprocess layer.

---

## v0 tasks

| `task_id` | Manifest | Behavior |
|-----------|------------|----------|
| `add_i32` | `benchmarks/manifests/add.json` | `a + b` |
| `mul_i32` | `benchmarks/manifests/mul.json` | `a * b` |
| `const42_i32` | `benchmarks/manifests/const42.json` | constant 42 |

`max_f32` is intentionally excluded until `vc-verify` exposes f32 return on the eval path.

---

## Extending the suite

1. Add a benchmark manifest under `benchmarks/manifests/`.
2. Append a `tasks[]` entry in `suite.json`.
3. Run `vectorc eval` on golden `.vcir` to confirm pass.
4. Bump suite version or `suite_id` when publishing a new leaderboard freeze.

See also: [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md), [Z_CONTRACT.md](Z_CONTRACT.md), [DEBUGGING_DECODE.md](DEBUGGING_DECODE.md) (failure dumps + batch shard eval), [scripts/eval-decode-scoreboard.sh](../scripts/eval-decode-scoreboard.sh) (full decode→compile→run CI path).
