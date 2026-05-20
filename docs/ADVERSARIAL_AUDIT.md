# Adversarial audit (codebase review)

This document is an **adversarial-style** review of **current** VectorCompiler paths as implemented in-tree: what a motivated abuser might try, what the code **already mitigates**, what remains **risky**, and what would change if you fork the embedding.

**Scope:** Rust sources for `vc-ir`, `vc-lower-wasm`, `vc-verify`, `vc-bridge`, and `vc-cli` at the time of writing. **Out of scope:** Wasmtime’s own bug taxonomy beyond configuration choices made here.

---

## Summary table

| Area | Risk | Mitigation today | Residual |
|------|------|------------------|----------|
| Malicious `.vcir` | Parser / validation bypass, surprising programs | `serde_json` parse + **`validate_module`** (`MAX_BODY_INSTRS`, **`MAX_PARAMS`**, **`MAX_DECLARED_LOCALS`**); **16 MiB** CLI read cap | JSON parse still allocates within cap; streaming parse not implemented |
| Malicious Wasm in `run` | Host escape via imports / memory / tables | **`WasmPolicy::default`** rejects imports, memories, tables, **`start`** before **`Module::from_binary`** | **`CompiledModule::new_with_policy`** can widen policy — callers must match threat model |
| Fuel-only limits | Long compile / instantiate; guest CPU | Fuel during **`func.call`**; **Wasm bytes capped** (`MAX_WASM_BYTES`) | Cranelift compile + instantiation **outside** guest fuel and **`max_wall_ms`** |
| Bench manifest | Path tricks, huge manifests | **`program_path`** relative to **`benchmarks/`**, no **`..`**; **`canonicalize`** + **`strip_prefix`** guard; case count / arity caps | Untrusted manifests still read local `.vcir` **inside** suite root |
| `synthesize` / **`vc-refine`** | Search burns CPU; misleading “training” story | **`RandomIrRefiner`** + verifier; **`--refiner-seed`**; seed IR validated | Heuristic search only — not latent decoding |
| `vc-bridge` / ONNX | Unsafe model load or bad tensor→IR mapping | Stub **fail-closed**; **`OrtLatentDecoder`** validates **`z`** + **`program_ir_json`** then **`validate_module`** | Integration mistakes remain a **policy** problem |
| Supply chain | Bad dep | Lockfile + **`cargo audit`** (CI, blocking) + **`cargo deny check licenses bans`** | Org SBOM / vendoring not in-repo |

---

## Path 1: `vc-cli compile`

**Flow:** read file (bounded) → `Module::parse_json_slice` → `lower_module` (validates internally) → write Wasm.

### Abuse cases

1. **Huge JSON / deep nesting** — may exhaust memory or CPU in `serde_json` before validation meaningfully runs.
2. **Valid but enormous IR bodies** — validator walks every instruction once; complexity is linear in body length but constants are not “small-program” enforced.
3. **Semantically valid “annoying” programs** — e.g., long linear chains of ops that compile to large Wasm **and** burn fuel at runtime.

### Mitigations

- Structural validation catches **type** and **stack** inconsistencies; empty export name rejected.
- Lowerer emits **straight-line** Wasm matching IR; no hidden control-flow optimizations.

### Gaps

- **File size** capped at **16 MiB** for the compile read path; validator bounds (`MAX_BODY_INSTRS`, etc.) still allow worst-case work proportional to those caps.
- **Compile** does not execute; threat is **DoS against the compiler**, not guest escape.

---

## Path 2: `vc-cli run`

**Flow:** read Wasm bytes → Wasmtime `Module::from_binary` → `Instance::new(&mut store, &module, &[])` → resolve export → `func.call` under fuel.

### Abuse cases

1. **Import-heavy malicious module** — `Instance::new` with `&[]` should **fail** if imports are required; verify error handling in callers (anyhow context) surfaces this clearly to operators.
2. **Memory bombs** — module declares large linear memory or aggressive growth patterns; still within Wasm semantics; **not** additionally capped here. Under **`WasmPolicy::default()`** (used by **`CompiledModule::new`**), modules with a **memory section are rejected before instantiate** — guests have no linear memory unless policy is widened explicitly.
3. **Start / constructors** — if future Wasm features or module shapes run more code at instantiate time, **fuel may not cover** all of that the way it covers the explicit `call` path—this is engine-version dependent.
4. **Export confusion** — wrong `-e` export may call an unintended function **if** the module exports multiple functions with compatible signatures; the CLI only checks name + arity/types at runtime.

### Mitigations

- **Fuel** on the store mitigates **infinite** guest loops **after** invocation starts.
- **Arity and i32-only** restriction reduces accidental broad ABI exposure.
- Modules **produced by this lowerer** are simple; **import count is zero**—good property for first-party artifacts.

### Gaps

- **Default-path `run`** goes through **`CompiledModule::new`** → **`WasmPolicy::default`** rejects imports (and memory/tables/start). Callers using **custom policy** must align docs with their embedding.
- **`run` trusts the path you pass**; swapping binaries on disk between read and load is a **TOCTOU** concern on hostile filesystems.

---

## Path 3: `vc-cli bench`

**Flow:** read manifest → parse **`BenchManifest`** → **`schema_version == 1`** → resolve **`program_path`** relative to **`benchmarks/`** (parent of **`benchmarks/manifests/`**) → **`canonicalize`** program under suite root → read `.vcir` → **`lower_module`** once → each case **`invoke_i32_return`** under manifest **`fuel`**.

### Abuse cases

1. **`program_path` escape** — uses `join` under manifest parent; typical `..` segments could reach sibling directories **if** you allow arbitrary manifests. This is **not** a sandbox escape, but it **is** a **read arbitrary local `.vcir`** footgun.
2. **Many cases / large args vectors** — mitigated by **`MAX_BENCH_CASES`** and **`MAX_PARAMS`** caps in the CLI; still a workload concern at the high bound.
3. **Re-lowers once per manifest** (not per case) — correct today, but if refactored wrongly, behavior could drift.

### Mitigations

- `schema_version` gate prevents naive field drift.
- Execution still uses the same **fuel** / **importless** instantiation as `run`.

### Gaps

- Treat manifests like **code**: run from isolated working directories for untrusted inputs.

---

## Path 3b: `vc-cli synthesize`

**Flow:** read behavioral JSON (**`cases`** required; full bench manifests accepted but **`fuel`** / **`program_path`** are ignored) → optional seed `.vcir` → **`RandomIrRefiner`** (`--refiner-seed`) searches mutations verified against **`vc-verify`**.

### Abuse / misuse

1. **CPU burn** — **`--steps`** controls refinement attempts; keep budgets small for CI.
2. **Spec confusion** — **`cases`** alone define behavior; operators must not assume **`program_path`** constrains synthesis.

---

## Path 4: `vc-bridge` (stub latent decoder)

**Flow:** `StubLatentDecoder::decode_z` checks `z.len() == EMBEDDING_DIM` (256), then **`bail!`** with a message about training / `onnx`.

### Abuse cases

1. **Developer bypass** — future code could “unwrap” stub errors and substitute unsafe defaults—**policy failure**.
2. **`onnx` feature** — currently a **placeholder** with no `ort` dependency; the risk is **future** integration mistakes (loading arbitrary model paths, enabling threads, GPU providers).

### Mitigations

- Stub is **fail-closed**: no partial modules, no best-effort guessing.
- Crate forbids `unsafe_code` at the source level (`#![forbid(unsafe_code)]`).

### Gaps

- **No cryptographic binding** between latent input and IR—not a vulnerability, but **downstream systems** must not assume authenticity.

---

## Path 5: emitted Wasm from `vc-lower-wasm`

**Properties (today):**

- Single function type, single function, single export.
- Instruction mapping is direct; **no imports** section; **no globals**, **no tables**, **no memory** section in the minimal encoder path (verify if you extend the lowerer—adding memory changes everything above).

### Abuse cases (if extended poorly)

1. Introducing **`memory.size` / `memory.grow` patterns** without limits.
2. Adding **host imports** for “convenience” (prints, time) — instantly widens the attack surface.

### Mitigations

- Keeping the lowerer **minimal** is itself a security feature.

---

## Residual risks (honest list)

1. **Host resource usage** is not fully bounded by **fuel** alone (parse, instantiate, module size).
2. **`run` on untrusted Wasm** is only as safe as **Wasmtime + your engine config**; this repository does not enable Wasm proposals beyond defaults you inherit.
3. **IR validation** is **not** a proof of **intent**—only of **internal consistency**.
4. **`vc-bridge` is not integrated**—so latent → IR threats are **mostly latent** today, but **documentation debt** could make future integration rushed.

---

## Recommended hardening backlog (documentation-only)

Prioritize based on your deployment mode (first-party compile vs. user-supplied Wasm):

1. **Operational timeouts** around **`CompiledModule::new`** (compile / instantiate) for adversarial Wasm within **`MAX_WASM_BYTES`**.
2. **`cargo deny` / SBOM** depth beyond CI warnings — tighten **`multiple-versions`** when Wasmtime upgrades converge transitive crates.
3. **Streaming JSON parse** for `.vcir` if memory amplification inside serde becomes a concern.
4. **Pinning** Wasmtime / future ONNX stacks with upgrade policy.
5. **Golden IR tests** asserting lowered Wasm imports nothing (beyond existing **`WasmPolicy`** tests).

Prior drafts mentioned separate WASM verifier passes — **`WasmPolicy`** + Wasmtime cover default **`run`**; keep **`cargo audit`** for advisory churn while **`cargo deny`** focuses licenses/bans.

Use [SECURITY.md](SECURITY.md) for the baseline threat model and operator checklist.

See also **[FULL_LINE_AUDIT.md](FULL_LINE_AUDIT.md)** for a file-by-file adversarial read of all Rust sources.

---

## Follow-up audit (implemented hardening — 2026-05)

Subagent review + remediation pass tightened the following (see git history):

| Finding | Mitigation |
|--------|-------------|
| `bench` manifest `program_path` / absolute paths | Paths must be relative, **`..`-free**, resolved under **`benchmarks/`**, **`canonicalize`** + **`strip_prefix`** guard. |
| Unbounded CLI reads | **`MAX_CLI_FILE_BYTES`** (16 MiB) metadata + byte cap on `.vcir` / `.wasm` / manifest reads. |
| Wasm module size (`run`) | **`MAX_WASM_BYTES`** (16 MiB) enforced before **`Module::from_binary`**. |
| Fuel vs compile/instantiate cost | Documented in **`vc-verify`**: fuel applies to **guest execution** only; **`CompiledModule`** separates one-time compile from per-case **`invoke`** (bench reuses compiled module). |
| `StackUnderflow` mislabel on type mismatch | **`ValidationError::StackTypeMismatch`**. |
| `ort` default `download-binaries` | **`default-features = false`** on optional `ort`; **`OrtLatentDecoder`** no longer calls **`Session::builder`** (no incidental native setup). |
| CI / reproducibility | **`--locked`** on clippy/test/bench smoke; **`vectorc bench`** + **`json-schema`** fixture job + optional **`cargo deny`** |
| Verification tests | **`vc-verify/tests/invoke_checks.rs`** — export/arity/zero-fuel/oversize/reuse/**policy (imports/mem/table/start)**/**wall-clock**. |

**Still not solved by fuel alone:** Cranelift compile + instantiation CPU for adversarial modules within the size cap—operational timeouts / separate workers remain the right tool.
