# Latent-first training plan: from vision to `vectorc decode-z --decoder onnx`

This document is the **execution plan** for making VectorCompiler **training-ready** and for validating the thesis: **keep reasoning in shared high-dimensional space; ground to hardware/software only through a narrow, verifiable waist** (validated Program IR → Wasm), not through lossy English or hand-written source as a mandatory middle layer.

**Audience:** contributors and automated agents. **Companion docs:** [ARCHITECTURE.md](ARCHITECTURE.md), [DECODER_ROADMAP.md](DECODER_ROADMAP.md), [Z_CONTRACT.md](Z_CONTRACT.md), [IR_VERSIONING.md](IR_VERSIONING.md), [TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md), [DEVELOPMENT.md](DEVELOPMENT.md), [TRAINING_ON_MAC.md](TRAINING_ON_MAC.md) (economical decoder training on a MacBook Air + optional cloud burst), [CONDITIONS_FOR_OBSOLESCENCE.md](CONDITIONS_FOR_OBSOLESCENCE.md) (long-horizon framing).

---

## 1. North star

**Product thesis.** Internal model computation and multi-step agent coordination should flow through **vectors (or equivalent activations)** wherever possible. **Natural language** remains optional for human interfaces. **“Code” as prose** is optional for ecosystems that need diffs and reviews. **Execution** requires a **finite, typed artifact**: validated **Program IR v2** today ([IR_VERSIONING.md](IR_VERSIONING.md)), then Wasm, then Wasmtime under strict limits.

**Engineering thesis.** The compiler stack is the **grounding head**: continuous `z` → `vc_ir::Module` → `validate_module` → `lower_module` → bounded execution. Training succeeds when this head is **reliable under distribution shift** and **better along task metrics** than serializing intent to English (or to Python) and only then compiling.

**Non-goals for v1 training.** Competing on general neural decompilation (x86 → C) leaderboards, or matching Meta-scale LLVM IR pretraining, unless you explicitly expand IR scope. Those are **baselines**, not phase gates. Replacing all of “normal programming” industry-wide is **out of scope** for v1; see [CONDITIONS_FOR_OBSOLESCENCE.md](CONDITIONS_FOR_OBSOLESCENCE.md) for what that would require in theory.

---

## 2. Design principles (do not violate casually)

1. **Fail closed.** Invalid `z` shapes, failed ONNX loads, or IR that does not validate must return **structured errors**, never panic on adversarial paths (`vc-bridge`, CLI). Keep alignment with [SECURITY.md](SECURITY.md).

2. **Single semantic contract.** `LatentDecoder::decode_z` is the only supported **learned** entry into `vc_ir`. Training and export must target **one** tensor layout and normalization story documented next to the model.

3. **Verifier in the loop.** Correctness is defined by **validation + execution** (and optional behavioral specs), not by token perplexity or “almost right” IR.

4. **Repository split.** Heavy Python/JAX/PyTorch training lives **outside** this Rust workspace; this repo consumes **exported weights** (ONNX first) and golden vectors. Avoid dragging ML frameworks into CI here.

5. **Reproducibility.** Every training run logs: model git hash, dataset version, `EXPECTED_Z_LEN`, normalization, IR schema version (`PROGRAM_IR_VERSION`), and lowerer version assumptions.

---

## 3. Current state (honest inventory)

| Layer | Status |
|-------|--------|
| Program IR v2, validator | **Shipped** (`vc-ir`) |
| IR → Wasm lowering | **Shipped** (`vc-lower-wasm`) |
| Wasmtime + fuel + policy | **Shipped** (`vc-verify`) |
| Verifier-guided IR search | **Shipped** (`vc-refine`, `vectorc synthesize`) |
| `LatentDecoder` trait, stub, golden | **Shipped** (`vc-bridge`) |
| ONNX `OrtLatentDecoder` (`program_ir_json` → IR) | **Done** (fixtures + scoreboard; see [DECODER_ROADMAP.md](DECODER_ROADMAP.md)) |
| Trained model + dataset | **Outside repo** / not started |
| Benchmarks that score `z → behavior` | **Partial** (fixtures exist; learned eval harness needed) |

**Implication:** “Training-ready” means **contracts + export path + eval harness + minimal model** are real—not merely a plan.

---

## 4. Phased roadmap

Each phase has **deliverables**, **owners** (suggested), and **exit criteria**. Phases 0–2 can overlap; Phase 4 depends on a frozen ONNX contract from Phase 0–3.

### Phase 0 — Freeze interfaces (1 short sprint)

**Goal:** No ambiguous tensor or JSON contracts while training starts elsewhere.

**Deliverables**

- [x] **`z` spec sheet** — baseline in [Z_CONTRACT.md](Z_CONTRACT.md); ONNX names/ranks still tracked in [DECODER_ROADMAP.md](DECODER_ROADMAP.md) until export is frozen.
- [ ] **IR version pin:** training data targets exactly one `program_ir_version` and schema; bump process documented ([IR_VERSIONING.md](IR_VERSIONING.md)).
- [ ] **ONNX I/O names and ranks** decided as real identifiers (replace placeholders in DECODER_ROADMAP). Document opset and dynamic vs static batch dim.
- [ ] **CLI matrix** documented: exact flags for `decode-z` with `--decoder onnx` once wired.

**Exit criteria**

- A new contributor can implement `OrtLatentDecoder` without asking Slack what shape `z` is.
- CI still green with stub/golden; no `onnx` required on default CI if policy is feature-gated.

**Suggested owner:** infra / bridge agent.

---

### Phase 1 — Dataset and synthesis pipeline

**Goal:** Deterministic pairs `(spec, z_stub or placeholder, ir_or_generation_procedure)` for supervised learning and for later RL.

**Deliverables**

- [ ] **Canonical task family v1** aligned with current IR expressiveness (e.g. integer straight-line programs, then bounded locals, then control flow when IR grows). Start **smaller** than “full software systems.”
- [ ] **Program generator** (Python recommended) that emits **only** JSON matching `vc_ir` schema and passes `validate_module` when run through `vectorc compile` / Rust tests—or generates AST dicts exportable to JSON.
- [ ] **Behavioral specs** for each program: `cases: [{ args, expect_i32 }, …]` compatible with [bench manifest schema](../schemas/) and `vc-refine` `Spec`.
- [ ] **(Optional) canonical `z` for supervised phase:** early on, `z` can be **encoder(latent_source)** where `latent_source` is structured (e.g. program embedding, synthetic bottleneck, or CFG features)—document that `z` is *not* “English TF-IDF” unless you intend that baseline.
- [ ] **Dataset manifest:** versioning, license, splits (train/val/test), checksum file list — schema [`schemas/dataset_manifest_v1.schema.json`](../schemas/dataset_manifest_v1.schema.json), example [`benchmarks/examples/dataset_manifest.example.json`](../benchmarks/examples/dataset_manifest.example.json); Phase 1 freeze: [`benchmarks/datasets/fixture_slice_v0/manifest.json`](../benchmarks/datasets/fixture_slice_v0/manifest.json) (verify: `bash scripts/verify-dataset-manifest.sh benchmarks benchmarks/datasets/fixture_slice_v0/manifest.json`).

**Exit criteria**

- ≥ N programs (pick N commensurate with team size, e.g. 10k–1M) with **100% validation pass** at generation time.
- Held-out split never seen in generator parameter tuning for tricky families.

**Suggested owner:** data / ML agent (external repo).

---

### Phase 2 — Decoder model architecture (choose and implement once)

**Goal:** A single trainable mapping **`z` → IR** that fits the Phase 0 contract.

**Candidate families** (pick one primary; keep second as ablation):

1. **Structured head:** MLP or transformer **decoder** outputs a **fixed-max-length sequence of discrete op/literal tokens** with constraints + mask; **or** produces a **JSON logits** style prediction parsed into `Module` (only if parser is strict).
2. **Pointer / copy mechanism:** if `z` encodes a library of primitives, attend over a **discrete codebook** (VQ-style). Useful if IR vocabulary stays small.
3. **Diffusion / iterative refinement on IR tokens** (heavier): train denoiser over tokenized IR conditioned on `z`; few-step distilled variant for ONNX export if needed.

**Deliverables**

- [ ] PyTorch (or chosen stack) module with **explicit input shape** `[B, D]` and **explicit output pathway** to serialized IR JSON or to an object that serializes to the same JSON `vc-ir` accepts.
- [ ] **Constraint layer:** post-process that enforces stack discipline *or* training uses validity mask so invalid ops are zero probability (prefer validator-aligned semantics).
- [ ] **Unit tests in ML repo:** round-trip JSON → validate on Rust side (invoke `cargo test -p vc-ir` with fixtures or HTTP-free subprocess).

**Exit criteria**

- Offline val set: **≥ X%** programs validate without edit (set X after baseline; track lift).
- **Execution pass rate** on val: % of programs that compile to Wasm *and* pass all `cases` in spec (stricter than validation alone).

**Suggested owner:** modeling agent (external repo).

---

### Phase 3 — Training objectives

**Goal:** Optimize for **grounded correctness**, not likeness to text.

**Minimum viable objective**

- **Supervised:** cross-entropy between predicted structure and target IR tokenization *or* MSE on intermediate if using continuous relaxation (only if relaxation still maps to discrete valid IR at inference).

**Strongly recommended additions**

- **Validation proxy loss:** differentiable surrogate is optional; **hard** validation in the training loop (every K steps) catches drift early.
- **Execution-aware signal:** batch of programs through Wasm (Python calling `vectorc run` or linking via PyO3 only if justified) for **exact i32 equality** on cases.
- **Refine hook:** for programs that validate but fail specs, run bounded `vc-refine`-style search as **inference-time compute**; optionally distill those successes back into the decoder (advanced).

**Exit criteria**

- Clear plots: **val execute success**, **val validate rate**, **train loss**—do not ship on loss alone.

**Suggested owner:** training agent (external repo).

---

### Phase 4 — Export and Rust integration

**Goal:** `cargo run -p vc-cli --features onnx -- decode-z … --decoder onnx` works on a **checked-in or downloadable** small model.

**Deliverables**

- [x] Export **ONNX** with fixed ops friendly to `ort` (document opset, avoid exotic ops; simplify graph if needed) — checked-in fixtures + generator scripts.
- [x] Implement **`OrtLatentDecoder`** in `vc-bridge`: session build, input name/shape, output fetch, map output tensor(s) → `vc_ir::Module` builder paths.
- [x] **Golden tests:** tiny onnx or session mock if CI cannot load real models; optional **large model download** behind feature flag documented in DEVELOPMENT.md.
- [x] **Fuzz / adversarial:** maintain policy that random `z` never panics (align with existing fuzz culture).

**Exit criteria**

- [x] One end-to-end demo: `z` file → `.vcir` → `.wasm` → `run --expect` passes.
- [x] Error messages include **actionable** hints (wrong length, missing model, dtype mismatch).

**Suggested owner:** Rust / bridge agent (this repo).

---

### Phase 5 — Evaluation and ablations (the “thesis” experiments)

**Goal:** Show latent-first grounding beats mandatory English/code.serialization **on tasks you care about**.

**Required comparisons**

- **Baseline A:** same task spec serialized to English prompt → best available code LM → extract program → compile (may omit if too costly; at least document as future work).
- **Baseline B:** JSON IR generated autoregressively from **text** spec (if IR is small).
- **Baseline C:** random search / `synthesize` budget Matched wall-clock or matched **inference flop** if measurable.

**Metrics**

- **Execute success rate** on held-out specs.
- **Steps / latency** to first correct program.
- **Validator rejection rate** (should drop with training).
- **Safety:** no policy violations in emitted Wasm (existing scans).

**Exit criteria**

- One internal report (markdown) with numbers and failure buckets: *validate failures*, *wrong output*, *timeout*.

**Suggested owner:** eval agent.

---

### Phase 6 — Joint / “shared space” training (later, thesis-critical)

**Goal:** Align **planner or encoder** bottlenecks with the decoder so **`z` is not an afterthought**.

**Deliverables**

- [ ] If `z` comes from another model: **stop-gradient vs fine-tune** decision; LoRA / adapter on producer side.
- [ ] **Contrastive or KL** alignment between producer hidden states and decoder-preferred geometry (only if theory clear).
- [ ] **Multi-turn latent comms** protocol between agents (vector channel + norm bounds)—product-specific.

**Exit criteria**

- Ablations show **joint** training improves execute success vs **frozen** encoder + trained decoder.

**Suggested owner:** research lead agent.

---

## 5. Parallel workstreams (for agent assignment)

Use this table to shard work without merge contention:

| Stream | Repo | Depends on | Primary output |
|--------|------|------------|----------------|
| A. Contract freeze | `.` | — | Phase 0 docs + minor `vc-bridge` API clarity |
| B. Data generator | external ML | Phase 0 | Versioned dataset + checksums |
| C. Model + train | external ML | B | Checkpoints + metrics |
| D. ONNX export | external ML | C | `decoder.onnx` + export report |
| E. `OrtLatentDecoder` | `.` | D, Phase 0 | Green E2E `decode-z` |
| F. Eval harness | `.` + ML | E | Bash or Rust-driven benchmark runner |
| G. Refine / RL | external ML + `.` | E | Optional improvement loop |

**Merge order recommendation:** A → B → C → D → E → F → G.

---

## 6. Risk register (short)

| Risk | Mitigation |
|------|------------|
| IR too weak for interesting `z` | Expand IR in versioned steps; measure when execute success plateaus. |
| ONNX op mismatch on CI | Keep graph tiny; test with `ort` in optional job. |
| Training optimizes validation but fails execution | Spec-driven loss; run Wasm in the loop on a subset. |
| `z` geometry mismatched between two models | Normalize; train adapters; shared contrastive batches. |

---

## 7. Definition of done (“start training” checklist)

Training **starts** when:

- [ ] Phase 0 complete (**frozen `z` + ONNX IO**).
- [ ] Phase 1 generator online and **validated** on Rust side.
- [ ] Phase 2 architecture chosen and **one** forward pass produces parseable IR JSON for a toy batch.

Training is **credible** when:

- [ ] Phase 3 execution metric is **tracked from week one**.
- [ ] Phase 4 prototype runs **`vectorc decode-z --decoder onnx`** on at least one real checkpoint.

The **thesis** is **demonstrated** when Phase 5 shows latent path **beats** agreed baselines on execute success or efficiency—not when the demo merely exists.

---

## 8. Immediate next actions (copy for kickoff)

1. Assign owners to streams A–G and set a single **contract doc** source of truth (link from DECODER_ROADMAP).
2. Create **external** `vectorcompiler-train` (or similar) repo: generator, model, export script, `requirements.txt` / lockfile.
3. Open a tracking issue per phase with exit criteria checkboxes mirroring §4.
4. After first ONNX, add **optional** CI job `cargo test -p vc-bridge --features onnx` with a **tiny fixture** model.

---

*Document version: 1.1 — aligns with Program IR v2 ([IR_VERSIONING.md](IR_VERSIONING.md)), [`z` contract](Z_CONTRACT.md), and `EMBEDDING_DIM` in `crates/vc-bridge/src/decoder.rs`. Update this file when those contracts change.*
