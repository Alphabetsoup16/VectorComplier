# Decoder roadmap: latent → IR (`vc-bridge`)

This document anchors the **latent vector → [`vc_ir::Module`]** bridge without prescribing ML architecture. Code contracts live in `crates/vc-bridge`; MSRV follows the workspace (**1.91**). **Frozen `z` expectations** (length, dtype, JSON envelope) are summarized in [**Z_CONTRACT.md**](Z_CONTRACT.md).

For the full phased plan (data, training, ONNX, evaluation, parallel agent streams), see [**LATENT_FIRST_TRAINING_PLAN.md**](LATENT_FIRST_TRAINING_PLAN.md).

[`vc_ir::Module`]: https://github.com/spencerwolf/VectorComplier/tree/main/crates/vc-ir

## Tensor layout contract (`z`)

| Field | Contract (today) |
|--------|------------------|
| Element type | `f32` |
| Layout | **Flattened** 1-D slice passed to `LatentDecoder::decode_z` — logical shape may be `[batch, D]` with `batch == 1`; Rust API accepts `&[f32]` of length **`EXPECTED_Z_LEN`** (default **`EMBEDDING_DIM`** in `decoder.rs`). |
| Normalization | **TBD** at training/export time (e.g. unit variance); callers must match the exported ONNX preprocessor. |

When ONNX inference is wired, `ort` tensors should match this contract (shape `[1, D]` or `[D]` depending on export — validate against the model, not assumptions).

## ONNX I/O contract (Phase 0 freeze — align exports here)

Training exports should match what [`OrtLatentDecoder`](https://github.com/spencerwolf/VectorComplier/tree/main/crates/vc-bridge/src/onnx.rs) validates at load time.

| Role | **Frozen identifier (today)** | dtype | Notes |
|------|-------------------------------|-------|-------|
| Input | **`z`** (`DECODER_ONNX_INPUT_Z`) | `f32` tensor | Element count **`EXPECTED_Z_LEN`** (256); Rust feeds shape **`[1, D]`** at inference. Dynamic axes in the ONNX model (`-1`) are OK — the concrete tensor must match `D`. |
| Output | **`program_ir_json`** (`DECODER_ONNX_OUTPUT_IR_JSON`) | `uint8` tensor | Rank‑1 tensor: raw bytes of **UTF‑8 JSON** for exactly one [`Module`] (same JSON shape as a `.vcir` file). Parsed via [`Module::parse_json_slice`](https://github.com/spencerwolf/VectorComplier/tree/main/crates/vc-ir) then **`validate_module`**. Additional float/aux outputs are ignored by `OrtLatentDecoder` (e.g. demo **`y`** tensor). |

**CI fixtures** (opset 13, **ONNX IR version 10**):

| File | `program_ir_json` behavior | Regenerate |
|------|---------------------------|------------|
| [`decoder_identity_z.onnx`](../benchmarks/fixtures/decoder_identity_z.onnx) | Constant add IR ([`add.vcir`](../benchmarks/programs/add.vcir)); ignores `z` | [`gen_decoder_identity_fixture_onnx.py`](../scripts/gen_decoder_identity_fixture_onnx.py) |
| [`decoder_z_switch.onnx`](../benchmarks/fixtures/decoder_z_switch.onnx) | **Toy z-dependent decode:** `z[0] >= 0` → add IR, else [`mul.vcir`](../benchmarks/programs/mul.vcir); auxiliary **`y` = Abs(`z`)** | [`gen_decoder_z_switch_fixture_onnx.py`](../scripts/gen_decoder_z_switch_fixture_onnx.py) |

Discovery workflow: load the ONNX in Netron or `ort` metadata APIs and lock names/shapes into code + tests.

## Legacy placeholder wording

Older drafts called inputs “placeholders”; the contract table above is authoritative for contributors today.

## Training → export → ORT (outline)

1. **Train** a decoder (PyTorch / JAX / etc.) that maps `z →` intermediate representation aligned with IR lowering goals.
2. **Export** to ONNX with fixed opset and explicit input/output names consistent with CI fixtures.
3. **Validate** the ONNX outside Rust (shape inference, numerical smoke tests).
4. **Integrate** in Rust: `Session::builder()` → `commit_from_file` / `commit_from_memory`, allocate `Value`s from `z`, run, parse outputs → `vc_ir::Module` builders.
5. **Ship** model path via config (`OrtLatentDecoder::with_model_path`) or embedding bytes behind feature flags — never panic on missing model or bad shape; return `anyhow::Error`.

Heavy Python ML stacks stay **out** of this workspace; only `ort` is pulled when `--features onnx`.

## CLI integration (`vectorc decode-z`)

The **`vectorc`** binary reads bounded UTF-8 JSON (`[f32,…]` or `{"z":[…]}`), dispatches to a [`LatentDecoder`](https://github.com/spencerwolf/VectorComplier/tree/main/crates/vc-bridge/src/decoder.rs), then runs **`validate_module`** and optional Wasm lowering — same validation boundary as **`compile`**.

| Decoder flag | Implementation |
|--------------|------------------|
| `stub` (default) | Fails after length check until a model is wired. |
| `golden` | Deterministic add-two-i32 IR for CI / demos (`benchmarks/fixtures/z_zeros.json`). |
| `onnx` | Requires `cargo ... -p vc-cli --features onnx` and `--onnx-model PATH`; loads ONNX, validates **`z`** + **`program_ir_json`** output contract, runs inference, parses embedded Program IR JSON → **`validate_module`**. |

---

## Failure modes (fail-closed)

| Situation | Intended behavior |
|-----------|-------------------|
| `z.len() != EXPECTED_Z_LEN` | Return `Err` with explicit lengths (`bail!`). |
| ONNX path chosen but no model configured | `Err` describing missing path / wiring (`OrtLatentDecoder`). |
| Session load failure | `Err` with `{:#}` from ONNX Runtime (includes unsupported IR version, missing file, etc.). |
| ONNX inference succeeds but IR JSON invalid / fails validation | `Err` wrapping JSON or `validate_module` diagnostics. |
| EP failure / dtype mismatch / unsupported ONNX IR | Propagate `anyhow`-wrapped `ort` errors after validation steps — avoid panicking in library code. |
