# Training on a MacBook Air (economical decoder plan)

Practical guide for training a **`z` → Program IR** decoder that plugs into VectorCompiler’s existing ONNX path. Heavy ML stays **outside** this Rust workspace; this repo supplies **contracts**, **oracles**, and **eval scripts**.

**Companion docs:** [LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md) (full phased roadmap), [DECODER_ROADMAP.md](DECODER_ROADMAP.md) (frozen ONNX I/O), [Z_CONTRACT.md](Z_CONTRACT.md) (`z` layout), [Z_BUILD.md](Z_BUILD.md) (v0 `program_id` → `z` recipe), [TRAINING_DATA.md](TRAINING_DATA.md) (shard / `rows.jsonl`), [PREFLIGHT_BEFORE_TRAINING.md](PREFLIGHT_BEFORE_TRAINING.md) (gate before training), [DEVELOPMENT.md](DEVELOPMENT.md) (scoreboard commands).

---

## Executive summary

| Phase | Where | Typical time | Spend |
|-------|--------|--------------|--------|
| 0 — Contracts & toy | Mac (CPU) | 1–2 days | $0 |
| 1 — Data (1k–10k programs) | Mac (CPU) | 3–7 days | $0 |
| 2 — Decoder v0 (byte CE) | Mac (MPS/CPU) | ~1 week | $0 |
| 3 — Execution metrics | Mac + `vectorc` subprocess | 2–3 days | $0 |
| 4 — Scale data + train | **Cloud burst** | 4–12 h GPU | **~$5–25** |
| 5 — Export ONNX + scoreboard | Mac | 1 day | $0 |

**Strategy for MacBook Air users:** use the Air for **data**, **architecture**, **export plumbing**, and **Rust validation**; rent a **small GPU for a few hours** only when local training is too slow or you need tens of thousands of examples and hyperparameter sweeps.

---

## MacBook Air constraints

Typical limits:

- **RAM:** 8 GB is tight for PyTorch + large batches; **16 GB+** is workable for sub‑1M‑parameter models.
- **GPU:** Apple **MPS** can beat CPU but is slower than a datacenter T4/L4 and occasionally fails on ops (fall back to CPU).
- **Thermals:** Long runs throttle; prefer short epochs with checkpoints over overnight marathons.
- **Disk:** Keep datasets and checkpoints outside this repo (`target/` is already large).

**Local is enough for:** synthetic data, tiny models (256 → ~512 byte sequences), ONNX export smoke tests, `vectorc` / scoreboard eval.

**Rent compute when:** an epoch exceeds ~30–45 minutes, you need **≥50k** programs, or you want many hyperparameter trials.

---

## Frozen training target (match Rust today)

Do not change these per experiment without bumping docs and tests.

| Item | Value |
|------|--------|
| Input name | `z` (`DECODER_ONNX_INPUT_Z`) |
| Input shape | `[1, 256]` `f32` |
| Output name | `program_ir_json` (`DECODER_ONNX_OUTPUT_IR_JSON`) |
| Output | `uint8` rank‑1, UTF‑8 bytes of one `.vcir` `Module` JSON |
| ONNX | opset **13**, **`ModelProto.ir_version ≤ 10`** (ORT 1.22 / `ort` 2.0.0-rc.10) |
| Post-train eval | [`scripts/eval-decode-scoreboard.sh`](../scripts/eval-decode-scoreboard.sh) |

**v0 frozen recipe:** no L2 on `z`; build vectors with [Z_BUILD.md](Z_BUILD.md) / [`gen_training_rows.py`](../scripts/gen_training_rows.py). If you add L2 or a new construction later, bump dataset version and bake the same op into ONNX.

**Rust integration today:** [`OrtLatentDecoder`](../crates/vc-bridge/src/onnx.rs) loads the graph, runs inference, reads **`program_ir_json`**, parses JSON, calls **`validate_module`**. Auxiliary outputs (e.g. float `y`) are ignored.

**Toy baselines in-repo:** [`decoder_identity_z.onnx`](../benchmarks/fixtures/decoder_identity_z.onnx) (constant add IR), [`decoder_z_switch.onnx`](../benchmarks/fixtures/decoder_z_switch.onnx) (`z[0] >= 0` → add else mul).

---

## Recommended repo layout (training — separate from VectorCompiler)

```
vectorcompiler-train/          # new repo (sibling or separate remote)
  pyproject.toml               # torch, onnx, onnxruntime, numpy
  data/
    raw/                       # .vcir + per-program spec JSON
    processed/                 # npz/parquet: z, json_bytes, len, program_id, split
  scripts/
    gen_programs.py
    build_z.py
    train_decoder.py
    export_onnx.py
    eval_rust.py               # invokes VectorCompiler scoreboard
  checkpoints/
  exports/decoder_v0.onnx
```

Point evaluation at this workspace:

```bash
export VECTORC_ROOT=/path/to/VectorComplier
cd "$VECTORC_ROOT"

# Training validation oracle (one decoded program → one task_id):
cargo run -p vc-cli --quiet -- eval \
  -i /tmp/decoded.vcir \
  --suite benchmarks/vectorbench_v0/suite.json \
  --task add_i32 \
  --json

# Or shell / Python wrappers:
bash scripts/training-oracle.sh --vcir /tmp/decoded.vcir
python3 scripts/vectorbench_oracle.py --vcir /tmp/decoded.vcir --metric execute_rate

# Full CI-style decode → compile → run (slower):
DECODER=onnx ONNX_MODEL=exports/decoder_v0.onnx \
  VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
  bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json
```

See **[VECTORBENCH_V0.md](VECTORBENCH_V0.md)** for metrics (`execute_rate`, `validate_rate`, …).

---

## Phase 0 — Prove the loop ($0, 1–2 days)

**Goal:** One exported ONNX from your training repo loads in Rust and passes the scoreboard on at least one manifest.

1. Use the three **VectorBench v0** programs (`add`, `mul`, `const42` under [`benchmarks/programs/`](../benchmarks/programs/)) or a tiny memorization set (~50 rows, program-id embeddings). A fourth file (`max_f32.vcir`) exists for lowering tests but is **not** in VectorBench v0.
2. Model: `z (256) → MLP → logits over max_len × 256` bytes (`max_len` 512–768).
3. Train on CPU (50–200 epochs, batch 32–64).
4. Export ONNX with names `z`, `program_ir_json` (materialize `uint8` bytes in the graph or in a thin export wrapper).
5. Verify:

```bash
cargo test -p vc-bridge --features onnx
cargo test -p vc-cli --features onnx --test onnx_fixture_decode
```

**Exit:** `vectorc eval` on VectorBench v0 with `metrics.execute_rate > 0` on memorization; then `decode_ok` / scoreboard on `add.json`.

---

## Phase 1 — Synthetic data ($0, ~1 week)

**Goal:** 1k–10k programs that **already pass** `validate_module`, each with behavioral **spec** cases.

**Generation (cheapest first):**

1. **Template mutations** from add/mul/const42/max_f32 (stay within Program IR v2).
2. **`vectorc synthesize`** on random specs (slower; uses in-repo search).
3. **Python generator** emitting JSON validated by [`schemas/program_ir_v2.schema.json`](../schemas/program_ir_v2.schema.json) and `vectorc compile`.

**Per sample store:** `program_id`, `.vcir`, `spec_json`, canonical `json_bytes`, `json_len`, `split` (train/val/test 80/10/10 by program, not by opcode only).

**Manifest:** follow [`schemas/dataset_manifest_v1.schema.json`](../schemas/dataset_manifest_v1.schema.json); freeze example: [`benchmarks/datasets/fixture_slice_v0/manifest.json`](../benchmarks/datasets/fixture_slice_v0/manifest.json).

```bash
bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/datasets/fixture_slice_v0/manifest.json
python3 scripts/generate_program_ir_fixtures.py --verify-only
```

**Exit:** ≥1k programs, 100% valid at generation time; held-out val/test.

---

## Phase 2 — Define `z` and train decoder v0 (Mac MPS/CPU, $0)

**Goal:** Supervised `z → program_ir_json` that generalizes beyond hand-written fixtures.

### `z` progression

| Stage | Construction | Purpose |
|-------|----------------|---------|
| **2a** | Learned `E[program_id]` (256-d) | Memorization / plumbing |
| **2b** | MLP on opcode histogram, arity, `json_len`, hash bits → 256-d | Cheap structured encoder |
| **2c** | Autoencoder: `json_bytes` → encoder → `z`, decoder `z` → bytes | `z` carries IR information |

Defer “agent latent” until **2c** reaches high val byte accuracy and acceptable val execute rate.

### Model size

- Target **&lt; 500k–1M** parameters.
- Example decoder: `LayerNorm(256) → Linear(512) → GELU → Dropout → Linear(512) → GELU → Linear(max_len×256)`.
- Loss: byte cross-entropy with padding mask; AdamW ~1e-3.

### Mac settings

```python
device = "mps" if torch.backends.mps.is_available() else "cpu"
batch_size = 16 if device == "mps" else 32  # reduce if OOM
num_workers = 0  # avoids macOS multiprocessing issues
```

Use `PYTORCH_ENABLE_MPS_FALLBACK=1` if an op fails on MPS. Checkpoint every epoch.

**Exit:** Val byte accuracy ≥ 95%; val **validate rate** (Rust) ≥ 70%.

---

## Phase 3 — Rust as oracle ($0)

**Goal:** Optimize for metrics you care about, not loss alone.

Every N epochs on validation:

1. Decode to bytes → write temp `.vcir`.
2. Run **`vectorc check -i /tmp/out.vcir -m benchmarks/manifests/add.json --json`** (one process, compile once) for frequent val loops; use [`scripts/eval-decode-scoreboard.sh`](../scripts/eval-decode-scoreboard.sh) for full decode→compile→run regression (CI parity).

**Log:** `val_byte_acc`, `val_validate_rate`, `val_compile_rate`, `val_execute_rate` (cases passed / total).

Do **not** backprop through Wasm. Optional inference-time repair: `vectorc synthesize` on failure (measure decode vs decode+repair separately).

**Skip decode for compile-only baselines:**

```bash
USE_EXISTING_VCIR=1 VECTORC='cargo run --locked -p vc-cli --quiet --' \
  bash scripts/eval-decode-scoreboard.sh
```

**Exit:** Held-out execute success clearly above `decoder_z_switch` on programs outside the add/mul toy family.

---

## Phase 4 — Cloud burst (~$5–25)

**Trigger when:**

- Local epoch &gt; 30–45 minutes
- Need **≥50k** programs
- Need many hyperparameter trials

| Option | Typical cost | Notes |
|--------|----------------|-------|
| Google Colab (free T4) | $0 | Queues, session limits |
| Colab Pro | ~$10/mo | More stable GPU |
| Vast.ai / RunPod spot | ~$0.2–0.6/hr | Good for 4–12 h runs |
| Lambda / similar | ~$0.5–1/hr | Faster batches |

**Pattern:** develop on Mac → upload `data/processed/` tarball → one spot job:

```bash
python train_decoder.py --epochs 30 --batch-size 128 --device cuda
python export_onnx.py --checkpoint best.pt
python eval_rust.py --onnx exports/decoder_v0.onnx
```

Download `decoder_v0.onnx` + `metrics.json` only.

| Task | Mac | Cloud |
|------|-----|-------|
| Data generation | ✓ | optional |
| Architecture debug | ✓ | |
| Full-data training | slow | ✓ |
| Hyperparameter sweep | ✗ | ✓ |
| ONNX + ORT smoke | ✓ | either |
| `vectorc` scoreboard | ✓ | optional |

---

## Phase 5 — Export discipline

1. Prefer **static shapes:** `z` `[1,256]`, `program_ir_json` `[max_len]` with pad.
2. **`ir_version = 10`** when saving ONNX (see [`scripts/gen_decoder_identity_fixture_onnx.py`](../scripts/gen_decoder_identity_fixture_onnx.py)).
3. Names: `z`, `program_ir_json`.
4. Verify on Mac:

```bash
python3 -c "import onnx; onnx.checker.check_model('exports/decoder_v0.onnx')"
cargo test -p vc-bridge --features onnx
```

---

## Phase 6 — Later (not v0)

- Joint training with upstream encoder (agent hidden → `z`).
- Larger `D` or IR version bumps ([IR_VERSIONING.md](IR_VERSIONING.md)).
- Logits head without `program_ir_json` (requires new Rust mapping).
- English/code-LM baselines ([LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md) Phase 5).

---

## Budget tiers

### Tier 0 — $0, Air only (~2 weeks part-time)

1k programs, program-id or opcode-hash `z`, tiny MLP, CPU/MPS. Scoreboard on 2–3 manifests. Beat toy `z_switch` on at least one held-out family.

### Tier 1 — $0 Mac + ~$10 cloud (~3 weeks)

10k–50k programs, stage **2b/2c** `z`, one 6–8 h spot GPU run. Full scoreboard on all [`benchmarks/manifests/`](../benchmarks/manifests/).

### Tier 2 — ~$50–100 cloud

100k+ programs, small hyperparameter sweep, optional text→IR baseline on a subset.

---

## Mac pitfalls

1. **8 GB RAM:** `max_len` ≤ 512–768; batch 8–16; stream dataset from disk.
2. **Sleep:** `caffeinate -i python train_decoder.py` for long local runs.
3. **MPS:** Test one batch before a long run; keep a CPU code path.
4. **Rust + train:** Avoid full `cargo build` while training (RAM contention).
5. **Thermals:** Prefer cloud burst over throttled 12 h local jobs.

---

## Success criteria

| Milestone | Target |
|-----------|--------|
| Plumbing | ONNX passes `cargo test -p vc-bridge --features onnx` |
| Validation | ≥90% val decode passes `validate_module` |
| Execution | ≥70% val case pass rate on held-out manifests |
| vs toy | Beat `decoder_z_switch` outside add/mul-only tasks |
| Cost | Cloud spend **&lt; $25** for decoder v0 |

---

## Already in this repo (reuse)

| Asset | Role |
|-------|------|
| [`OrtLatentDecoder`](../crates/vc-bridge/src/onnx.rs) | Load + `program_ir_json` → `Module` |
| [`eval-decode-scoreboard.sh`](../scripts/eval-decode-scoreboard.sh) | JSON metrics: decode/compile/run |
| [`eval-decode-pipeline.sh`](../scripts/eval-decode-pipeline.sh) | Single-path smoke |
| [`fixture_slice_v0` manifest](../benchmarks/datasets/fixture_slice_v0/manifest.json) | Checksum’d dataset index |
| [`generate_program_ir_fixtures.py`](../scripts/generate_program_ir_fixtures.py) | Verify/write canonical four programs |
| ONNX fixture generators | [`gen_decoder_identity_fixture_onnx.py`](../scripts/gen_decoder_identity_fixture_onnx.py), [`gen_decoder_z_switch_fixture_onnx.py`](../scripts/gen_decoder_z_switch_fixture_onnx.py) |

---

## Suggested first week on a MacBook Air

| Day | Task |
|-----|------|
| 1 | Create `vectorcompiler-train`; wire `eval_rust.py` → scoreboard |
| 2 | Generate ~500 valid `.vcir` + specs; train/val/test split |
| 3 | Program-id `z` + tiny MLP; train CPU; export ONNX v0 |
| 4 | Scoreboard on add + mul; fix export names/shapes |
| 5 | Opcode-histogram `z`; retrain |
| 6 | Scale to 2k–5k programs; log val execute metrics |
| 7 | If slow: one Colab or Vast spot run; download best ONNX |

---

## What not to do (saves time and money)

- Train a large code LM as v0.
- RL from random `z` without supervised bytes first.
- Change ONNX output contract without updating `vc-bridge` and CI.
- Optimize on training loss without `eval-decode-scoreboard.sh` execution metrics.

---

*Document version: 1.0 — MacBook Air economical path; aligns with Program IR v2 and `EMBEDDING_DIM = 256`.*
