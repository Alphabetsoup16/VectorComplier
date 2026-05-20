# Frozen `z` construction (v0)

**Authority for v0:** deterministic `program_id → z` used by training shards, `vectorc decode-z`, and ONNX exports. The reference implementation is [`scripts/gen_training_rows.py`](../scripts/gen_training_rows.py) (`build_z`); this document is the human-readable spec — if they disagree, fix the script to match here.

**Related:** [Z_CONTRACT.md](Z_CONTRACT.md) (Rust/CLI envelope), [DECODER_ROADMAP.md](DECODER_ROADMAP.md) (ONNX I/O), [TRAINING_DATA.md](TRAINING_DATA.md) (shard layout).

---

## v0 constants (do not change mid-run)

| Item | Value |
|------|--------|
| Dimension `D` | **256** (`EMBEDDING_DIM` / `EXPECTED_Z_LEN` in `crates/vc-bridge/src/decoder.rs`) |
| dtype | **`f32`** |
| Normalization | **None** (no L2, no LayerNorm on `z` before the decoder for v0) |
| `program_id` encoding | UTF-8 string (e.g. `add`, `mul`, `const42`, or synthetic `add_dup_0007`) |

**Why no L2 for v0:** coefficients are already in **`[-1.0, 1.0]`** by construction. Adding L2 would change scale and must be baked into ONNX if ever enabled; defer to a versioned bump ([IR_VERSIONING.md](IR_VERSIONING.md) / dataset version), not an silent experiment.

---

## Recipe: `program_id` → `z`

Given `program_id` string `pid` (UTF-8):

1. `block₀ = SHA256(pid.encode("utf-8")).digest()` (32 bytes).
2. Expand with a counter-chained hash: for `k = 1, 2, …` while fewer than 256 values,
   `block_k = SHA256(block_{k-1} || u32_le(k))` where `u32_le(k)` is 4 little-endian bytes.
3. Walk each 4-byte little-endian chunk in `block_k` (pad the final partial chunk with `0x00` on the right).
4. Map `u = uint32(chunk)` → `f32` in **`[-1.0, 1.0]`** via `(u / 2³²) × 2.0 − 1.0`.
5. Take the first **256** mapped values as `z`.

Reference: `build_z()` in [`scripts/gen_training_rows.py`](../scripts/gen_training_rows.py).

Properties:

- **Deterministic** (stdlib `hashlib` only; no NumPy RNG).
- **Bounded** — no L2 or clip step for v0.
- **Not used in v0:** CPython `random.Random` from a seed, or tiling sixteen raw digest `f32` values (those variants are called out in the generator docstring for cross-tool discussion only).

**Later stages:** learned embeddings, opcode histograms — [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md) phase 2b/2c.

---

## JSON envelope (`vectorc decode-z`)

Write a temp file or pipe JSON to the CLI:

- Bare array: `[f32, …]` (length **256**), or
- Object: `{"z": [f32, …]}`

Example (zeros are valid; golden decoder ignores values after length check):

```json
{"z": [0.0, 0.0, "... 254 more ..."]}
```

CLI:

```bash
cargo run -p vc-cli -- decode-z \
  -z /tmp/z.json \
  -o /tmp/out.vcir \
  --decoder golden   # or onnx + --features onnx
```

Parsing and length checks: `crates/vc-cli/src/main.rs` (`parse_z_json`). Full contract: [Z_CONTRACT.md](Z_CONTRACT.md).

---

## ONNX export (same transform)

For v0 **frozen `z` shards**, the ONNX graph should consume **`z` as-is**:

| Training | Export |
|----------|--------|
| Rows already store final `z` | Input tensor name **`z`**, shape **`[1, 256]`**, `f32` |
| No extra normalize in PyTorch | **Do not** add L2 / LayerNorm on `z` in the ONNX graph unless you also regenerate all rows and bump dataset version |

If a later model learns an encoder `features → z`, bake **encoder + any normalize** into the export graph so ORT sees the same tensor the Rust CLI passes to `OrtLatentDecoder`.

Frozen output contract unchanged: **`program_ir_json`** `uint8` UTF-8 `.vcir` bytes — [DECODER_ROADMAP.md](DECODER_ROADMAP.md).

---

## Round-trip tests

### A. `z` construction (no decoder)

After `python3 scripts/gen_training_rows.py --write`:

```bash
python3 scripts/gen_training_rows.py --verify-only
```

`--verify-only` recomputes each row’s `z` with `build_z(program_id)` and compares to `rows.jsonl`. After editing the recipe, run `python3 scripts/gen_training_rows.py --write` and refresh the manifest checksums.

### B. Shard integrity

```bash
bash scripts/verify-dataset-manifest.sh benchmarks/datasets/training_slice_v0 \
  benchmarks/datasets/training_slice_v0/manifest.json
```

### C. Oracle on golden `.vcir` (behavior, not `z`)

Each row’s `task_id` should pass VectorBench on the **reference** program (independent of decode):

```bash
export VECTORC_ROOT="$PWD"
python3 scripts/vectorbench_oracle.py \
  --vcir "$VECTORC_ROOT/benchmarks/datasets/training_slice_v0/programs/add.vcir" \
  --task add_i32 --metric execute_rate
```

### D. Learned decoder (when ONNX exists)

```bash
# Build z JSON from a row, decode, eval
python3 -c "import json; r=json.loads(open('benchmarks/datasets/training_slice_v0/rows.jsonl').readline()); json.dump({'z':r['z']}, open('/tmp/z.json','w'))"
DECODER=onnx ONNX_MODEL=path/to/decoder.onnx \
  VECTORC='cargo run --locked -p vc-cli --features onnx --quiet --' \
  bash scripts/training-oracle.sh --z /tmp/z.json --vcir /tmp/decoded.vcir
```

Expect `execute_rate` to improve as the model trains; v0 frozen `z` alone does not imply a correct decode without supervision.

---

## Changing the recipe

Bump **`dataset_version`** in the shard manifest, regenerate rows, re-export ONNX, and update CI golden vectors. Do not change `D` or normalization without updating [Z_CONTRACT.md](Z_CONTRACT.md) and `EMBEDDING_DIM` in Rust.
