# Latent vector contract (`z`)

Single place for **machine-facing** expectations on the continuous input to [`LatentDecoder::decode_z`](https://github.com/spencerwolf/VectorComplier/tree/main/crates/vc-bridge/src/decoder.rs). ONNX input naming is frozen as **`z`** (see [DECODER_ROADMAP.md](DECODER_ROADMAP.md)); output tensors remain training-defined until mapped in `vc-bridge`.

**Companion:** [IR_VERSIONING.md](IR_VERSIONING.md) (Program IR pin), [TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md) (digests and manifests).

---

## Rust API

| Field | Contract |
|-------|----------|
| Element type | `f32` |
| Slice length | Exactly **`EXPECTED_Z_LEN`** (today equals **`EMBEDDING_DIM`**, **256**, in `decoder.rs`). |
| Logical layout | Row-major; batch **1** is assumed when flattening `[1, D]` → `&[f32]`. |
| Normalization | **v0:** none — see [Z_BUILD.md](Z_BUILD.md). Later exports must match any ONNX preprocessor. |

Violations return **`Err`** (fail closed); never panic on adversarial lengths.

---

## CLI JSON envelope (`vectorc decode-z`)

Input file is bounded UTF-8 JSON, either:

- A bare JSON array: `[f32, …]`, or  
- An object: `{ "z": [f32, …] }`

The flattened length must equal **`EXPECTED_Z_LEN`**.

---

## Phase 0 checklist (training readiness)

Lock these before treating a decoder as reproducible across machines:

1. **Dimension** — explicit `D` (today 256) or a versioned bump with migration notes.  
2. **Normalization** — documented and test-fixed (golden vectors + ONNX preprocessing).  
3. **dtype** — `f32` unless the workspace explicitly widens.  
4. **IR target** — single `program_ir_version` / schema (see [IR_VERSIONING.md](IR_VERSIONING.md)).
5. **ONNX packaging** — **`ModelProto.ir_version` ≤ 10** for ORT 1.22 (see [`benchmarks/fixtures/decoder_identity_z.onnx`](../benchmarks/fixtures/decoder_identity_z.onnx) + generator script).
6. **IR sidecar tensor** — ONNX export must include **`program_ir_json`** (`uint8`, UTF‑8 JSON identical in shape to a `.vcir` file). Details: [DECODER_ROADMAP.md](DECODER_ROADMAP.md).

Auxiliary float heads (e.g. logits) may coexist; **`OrtLatentDecoder`** only reads **`program_ir_json`** for IR materialization today.
