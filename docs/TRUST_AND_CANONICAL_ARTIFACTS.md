# Trust and canonical artifacts

VectorCompiler’s direction (see [CONDITIONS_FOR_OBSOLESCENCE.md](CONDITIONS_FOR_OBSOLESCENCE.md)) assumes **machine-checkable** layers—not prose—as the execution-facing system of record. This repo implements a thin slice: validated Program IR → Wasm under strict host policy.

---

## What is authoritative

| Stage | Authority |
|-------|-----------|
| Program IR JSON | **`validate_module`** + JSON Schema (`schemas/program_ir_v2.schema.json`). |
| Wasm bytes | **`lower_module`** after validation; **`vc-verify`** preflight before execution. |
| Behavioral specs | Bench manifests / synthesize specs validated against `schemas/bench_spec_v1.schema.json`. |
| Latent `z` | Length and dtype enforced at decode boundary; full normalization contract in [Z_CONTRACT.md](Z_CONTRACT.md). |

Human-readable CLI output is intentionally **non-authoritative**: it summarizes what passed validation.

---

## Logging vs stdout (`vectorc`)

`vectorc` configures **`tracing`** to write **`stderr`** (human-readable logs). Commands intended for scripting print machine-readable output to **`stdout`**:

| Command | stdout | stderr (`tracing`, default `info`) |
|---------|--------|-------------------------------------|
| **`digest -i PATH`** | One lowercase SHA-256 hex line | Optional logs |
| **`compile … --print-digest`** | SHA-256 hex after write | Optional logs |
| **`inspect -i file.vcir --json`** | Single JSON object (see below) | Optional logs |
| **`inspect -i file.vcir`** (no `--json`) | Human summary | Optional logs |
| **`run`** | Result `i32` line | Optional logs |

Use **`RUST_LOG`** to tune stderr without changing stdout. Examples:

```bash
RUST_LOG=warn vectorc digest -i benchmarks/programs/add.vcir
RUST_LOG=vectorc=debug vectorc inspect -i benchmarks/programs/add.vcir --json
```

If you capture stdout in scripts (`$(vectorc digest …)`), set **`RUST_LOG=warn`** (or `error`) when the default **`info`** lines on stderr clutter CI logs. stdout remains the contract; stderr is diagnostic only.

---

## Digests (`vectorc`)

| Command | Meaning |
|---------|---------|
| **`digest -i PATH`** | Lowercase SHA-256 hex of **raw file bytes** (`.vcir`, `.wasm`, etc.), bounded by CLI read limits. |
| **`compile … --print-digest`** | After writing Wasm, prints SHA-256 hex of the **emitted Wasm**. |

Use digests to pin artifacts in manifests, CI caches, and offline reproduction logs. They do **not** replace semantic validation: two different IR sources could theoretically lower to identical Wasm only if the lowerer is deterministic (today treat digest equality as best-effort pinning, not a semantic equivalence proof).

---

## Inspect (`vectorc inspect`)

`inspect -i file.vcir` runs **`validate_module`**, then prints a structured summary (signature, metrics, opcode histogram). With **`--json`**, stdout is a single JSON object: `export_name`, `program_ir_version`, `params`, `results`, `instruction_tree_nodes`, `max_control_nesting_depth`, `opcode_histogram` (object). Treat either view as **debugging and review aid**; diffs for governance should target canonical JSON + validator, not inspect output.

---

## Dataset manifests

[`schemas/dataset_manifest_v1.schema.json`](../schemas/dataset_manifest_v1.schema.json) describes optional indexing metadata: dataset id/version, optional splits, and a list of `{ relative_path, sha256 }` entries for reproducibility. Example: [`benchmarks/examples/dataset_manifest.example.json`](../benchmarks/examples/dataset_manifest.example.json).

Training runs should still log model hash, generator commit, IR version, and normalization per [LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md).

---

## Reproducibility limits

Wasmtime fuel and engine configuration narrow **resource** nondeterminism; they do not by themselves prove bitwise-stable Wasm across compiler upgrades. For strongest pins, record **`vectorc` / crate versions** with artifact digests.
