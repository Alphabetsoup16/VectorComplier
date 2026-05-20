# Preflight before training

Run this gate **before** you spend time on decoder training or large eval sweeps. It separates **correctness** (must pass) from **performance** (optimize only after the gate is green).

**One command:** from the repo root:

```bash
bash scripts/preflight.sh
```

Optional: skip ONNX steps if you have not built with `--features onnx`:

```bash
RUN_ONNX=0 bash scripts/preflight.sh
```

---

## What “rock solid” means here

| Layer | Guarantee |
|-------|-----------|
| **Contracts** | Frozen `z` / ONNX I/O ([Z_CONTRACT.md](Z_CONTRACT.md), [DECODER_ROADMAP.md](DECODER_ROADMAP.md)) |
| **IR** | `validate_module` on every accepted `.vcir`; lowering re-validates |
| **Wasm** | Emitted modules ≤ **16 MiB** (`MAX_WASM_BYTES`); guest **fuel** on invoke |
| **Trust** | Canonical digests / inspect on stderr vs stdout ([TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md)) |
| **Pipeline** | `eval-decode-scoreboard.sh` = decode → compile → run per dataset manifest |
| **CI parity** | `preflight.sh` ≈ `.github/workflows/ci.yml` (fmt, clippy, test, schemas, scoreboards) |

Known **accepted** limits (do not block v1 training; document in eval):

- **Cranelift compile / instantiate** are not fuel-metered (see [ADVERSARIAL_AUDIT.md](ADVERSARIAL_AUDIT.md)).
- **`CompiledModule`** reuses engine + module; each invoke uses a **new store** (by design).
- **Supply chain** — `cargo audit` and `cargo deny check licenses bans` run in CI and in `preflight.sh` (install tools locally if missing).

---

## Correctness checklist (do first)

1. **Green preflight** — `bash scripts/preflight.sh`
2. **Frozen export** — ONNX `z` + `program_ir_json`; opset 13; IR v10
3. **Scoreboard on your manifest** — same cases you will use in training:
   ```bash
   VECTORC='cargo run -p vc-cli --features onnx --quiet --' \
     DECODER=onnx ONNX_MODEL=exports/decoder_v0.onnx \
     bash scripts/eval-decode-scoreboard.sh benchmarks/manifests/add.json
   ```
4. **Dataset checksums** — if you add programs under `benchmarks/datasets/`, extend manifest + `verify-dataset-manifest.sh`
5. **Pick `z` normalization once** — document in the training repo and bake into ONNX

---

## Performance: what to optimize (and what not to) before training

### Do **not** optimize yet

- Cranelift / Wasmtime version bumps (track separately; audit-driven)
- Micro-optimizing IR validation or opcode histograms
- Parallel scoreboard shells across manifests (fine for CI; training uses one manifest at a time)

### **Do** use the in-process eval path

Subprocess-per-step (`decode-z` → `compile` → `run` per case) is correct for **CI and integration** but too slow for **tight training loops** (thousands of samples on a MacBook Air).

| Tool | Use when |
|------|----------|
| [`scripts/eval-decode-scoreboard.sh`](../scripts/eval-decode-scoreboard.sh) | End-to-end ONNX + manifest regression; CI |
| **`vectorc check`** | After you have a `.vcir` (decoded or golden): **one process**, compile once, run all cases |
| **`vectorc bench`** | Reference `.vcir` on disk under `benchmarks/programs/` |

**Training loop pattern (recommended):**

```bash
# 1) ONNX decode (subprocess or ORT in Python — your choice)
vectorc decode-z -z z.json -o /tmp/out.vcir --decoder onnx --onnx-model model.onnx

# 2) Fast oracle (in-process)
vectorc check -i /tmp/out.vcir -m benchmarks/manifests/add.json --json
```

From Python, call `vectorc check … --json` once per sample (one subprocess) instead of `compile` + N×`run`.

Later, for maximum throughput, link against `vc_ir` / `vc_lower_wasm` / `vc_verify` in-process (separate training crate or PyO3) — not required for v0.

### Optional next perf steps (after preflight is green)

1. Batch eval driver in the training repo (amortize process spawn)
2. Cache `CompiledModule` per **canonical IR digest** when re-evaluating the same program
3. Cloud GPU burst only for scale — see [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md)

---

## Related docs

- [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md) — economical Mac + cloud plan
- [LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md) — phased roadmap
- [VETTING_REPORT.md](VETTING_REPORT.md) — static review notes
- [ADVERSARIAL_AUDIT.md](ADVERSARIAL_AUDIT.md) — threat model and caps
