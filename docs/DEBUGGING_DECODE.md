# Debugging decode / eval failures

When a training sample fails VectorBench (`ok=false`), capture structured artifacts before re-running the full decode pipeline.

**Authority:** `vectorc eval` (see [VECTORBENCH_V0.md](VECTORBENCH_V0.md)).

---

## Single-sample dump

[`scripts/dump-eval-failures.sh`](../scripts/dump-eval-failures.sh) runs eval (and optionally inspect) for one `.vcir` + `task_id` and writes a directory you can attach to an issue or diff in a notebook.

```bash
# From repo root (build vectorc first, or set VECTORC=vectorc if on PATH)
bash scripts/dump-eval-failures.sh \
  --vcir /tmp/decoded.vcir \
  --task add_i32 \
  --out /tmp/fail-001

# Include validated IR summary
bash scripts/dump-eval-failures.sh \
  --vcir /tmp/decoded.vcir \
  --task add_i32 \
  --out /tmp/fail-001 \
  --inspect
```

**Output layout**

| File | Contents |
|------|----------|
| `summary.json` | Full `vectorc eval --json` object (or error envelope if eval did not print JSON) |
| `errors.txt` | Human-readable lines: stderr, per-case errors, metric snapshot |
| `program.vcir` | Copy of the scored program |
| `inspect.json` | Optional: `vectorc inspect --json` |

Exit code: non-zero if eval failed or `summary.ok` is false.

Environment: `VECTORC_ROOT`, `VECTORC`, `SUITE` (same as [`scripts/training-oracle.sh`](../scripts/training-oracle.sh)).

---

## Batch over a training shard

[`scripts/batch-eval-training-shard.sh`](../scripts/batch-eval-training-shard.sh) reads `rows.jsonl` under a shard directory (default: [`benchmarks/datasets/training_slice_v0/`](../benchmarks/datasets/training_slice_v0/)).

**Row schema** (one JSON object per line; see [`training_slice_v0/rows.jsonl`](../benchmarks/datasets/training_slice_v0/rows.jsonl)):

```json
{"program_id":"add","task_id":"add_i32","vcir_path":"programs/add.vcir","split":"train","z":[...]}
```

| Field | Required | Notes |
|-------|----------|-------|
| `vcir` or `vcir_path` | yes | Path relative to the shard dir or absolute |
| `task_id` | yes | VectorBench v0 task, e.g. `add_i32`, `mul_i32` |
| `row_id` or `program_id` | no | Stable id for failure dumps (default: `line-N`) |
| `suite` | no | Override suite JSON for this row |
| `z`, `manifest`, `split` | no | Ignored by batch eval (used by training pipelines) |

```bash
# Aggregate report on stdout; exit 1 if any row fails
bash scripts/batch-eval-training-shard.sh

# Custom shard + write report file
bash scripts/batch-eval-training-shard.sh \
  benchmarks/datasets/training_slice_v0 \
  --out /tmp/shard-report.json

# Also run dump-eval-failures.sh per failed row
bash scripts/batch-eval-training-shard.sh \
  benchmarks/datasets/training_slice_v0 \
  --dump-failures /tmp/fail-dumps \
  --out /tmp/shard-report.json
```

Aggregate JSON includes `rows_total`, `rows_passed`, `pass_rate`, `execute_rate_mean`, per-row summaries, and `failures` (list of `.vcir` paths where `ok=false`).

The checked-in [`training_slice_v0/`](../benchmarks/datasets/training_slice_v0/) shard has three golden rows (all should pass eval).

---

## Rust tests

Heavy shell integration is not run in `cargo test`. Use:

- `cargo test -p vc-cli eval_add_vcir_against_vectorbench_v0` — golden eval JSON shape
- The scripts above locally or in CI after `cargo build -p vc-cli`

To add a script smoke step in CI later, run batch eval with `--limit 3` and expect exit 1 on the fixture shard (one bad row).

---

## Related

- [VECTORBENCH_V0.md](VECTORBENCH_V0.md) — metrics and tasks
- [scripts/training-oracle.sh](../scripts/training-oracle.sh) — decode + eval one-liner
- [scripts/eval-decode-scoreboard.sh](../scripts/eval-decode-scoreboard.sh) — full decode→compile→run CI path
