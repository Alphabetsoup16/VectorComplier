# Adversarial audit (codebase review)

Adversarial-style review of **current** VectorCompiler paths: what a motivated abuser might try, what the code mitigates, what remains risky, and operator mitigations.

**Scope:** `vc-ir`, `vc-lower-wasm`, `vc-verify`, `vc-bridge`, `vc-cli`, agent interface (`validate` / `explain` / `fix` / `skills`). **Out of scope:** Wasmtime/Cranelift internals beyond configuration here.

**Companion:** [AGENT_COMPILER_INTERFACE.md](AGENT_COMPILER_INTERFACE.md), [SECURITY.md](SECURITY.md), [FULL_LINE_AUDIT.md](FULL_LINE_AUDIT.md).

---

## Summary table

| Area | Risk | Mitigation today | Residual |
|------|------|------------------|----------|
| Malicious `.vcir` | Parser / validation bypass | `serde_json` + **`validate_module`** caps; **16 MiB** CLI read | JSON parse allocates within cap; no streaming parse |
| Malicious Wasm in `run` | Host escape via imports / memory | **`WasmPolicy::default`** preflight | Custom policy widens surface; compile/instantiate not fuel-metered |
| Bench / eval manifests | Path escape, huge manifests | Relative paths, no `..`, **`canonicalize`** + **`strip_prefix`** under repo / `benchmarks/` | Untrusted suite JSON still a **trust** input |
| `synthesize` / `vc-refine` | CPU burn | Step budget + fuel per case | Heuristic search only |
| `vc-bridge` / ONNX | Untrusted model / bad IR bytes | Shape checks + **`validate_module`** fail-closed | ORT runtime = trusted code; model path is user-supplied |
| Agent CLI (`skills`, `explain`) | Oversized tokens, path tricks | Skill **allowlist** + name charset/length; diagnostic code format gate | `explain --all` prints full catalog (bounded, 16 codes) |
| `agent-repair` | CPU burn via `--synthesize` | **`max_steps`** cap; synthesize only when parse+validate OK but run fails | Overwrites `--input` unless `-o`; temp IR in system temp |
| `fix --plan` | Misleading auto-repair | **Plan only** — no file writes | Agents must not treat plans as applied fixes |
| Supply chain | Bad dependency | Lockfile + **`cargo audit`** + **`cargo deny`** in CI | Org SBOM / vendoring not in-repo |

---

## Path 1: `vectorc compile`

**Flow:** bounded read → `parse_json_slice` → `lower_module` (re-validates) → write Wasm.

### Abuse

- Huge / deeply nested JSON within 16 MiB cap.
- Valid IR at tier-1 limits still produces large Wasm and slow validation walks.

### Mitigations

- Structural validation; empty export rejected; emit size cap.

### Residual

- **Compiler DoS** on host CPU/memory within documented caps—not guest escape.

---

## Path 2: `vectorc run`

**Flow:** bounded read → policy → compile → fuel-metered `call`.

### Abuse

- Untrusted Wasm with custom policy.
- TOCTOU if path swapped between read and load on hostile FS.
- Instantiate/compile cost outside guest fuel.

### Mitigations

- Default policy denies imports, memory, tables, start.
- **`--isolate`** subprocess + timeout for untrusted blobs.
- Optional **`--wall-ms`** on guest execution.

### Residual

- Treat non-repo Wasm as **untrusted**; use isolation + wall-clock.

---

## Path 3: `bench` / `check` / `eval`

**Flow:** manifest or spec → resolve paths → lower once (where applicable) → invoke under fuel.

### Abuse

- `program_path` or suite `manifest` escape to read arbitrary `.vcir` / JSON.
- Huge `cases` arrays (mitigated by **`MAX_BENCH_CASES`**).

### Mitigations

- **`bench`:** `program_path` relative to suite root, no `..`, canonicalize under `benchmarks/`.
- **`eval`:** suite `manifest` entries must be **relative**, no `..`, canonicalize under **repository root** (2026-05 hardening).
- **`check`:** user-supplied spec path is explicit trust boundary (bounded read).

### Residual

- Do not point **`eval`** at untrusted `suite.json` without reviewing manifest paths.

---

## Path 4: `vc-bridge` / ONNX

**Flow:** `decode_z` → optional ONNX → UTF-8 JSON → **`validate_module`**.

### Abuse

- Malicious ONNX model (native code in ORT).
- Adversarial `program_ir_json` bytes (oversized UTF-8 within tensor limits).

### Mitigations

- Stub/golden fail-closed; ONNX path validates tensor shapes then IR.
- **`--onnx-model`** is an explicit trusted path (like loading a shared library).

### Residual

- No authenticity binding between `z` and IR; training must not trust decode without **`check`/`eval`**.

---

## Path 5: emitted Wasm (`vc-lower-wasm`)

First-party modules: no imports, single export, policy-tested in CI.

**Risk if extended:** memory sections, host imports—would invalidate current threat model.

---

## Path 6: Agent compiler interface

**Flow:** `validate` / `parse` / `explain` / `fix --plan` / `skills` — structured stdout for automation.

### Abuse

1. **`skills get ../../../etc/passwd`** — rejected: fixed allowlist (`language`, `diagnostics`, `limits`, `decoder`) + charset/length on name before lookup.
2. **`explain` with megabyte code string** — rejected: max length + `VCIR_*` token shape before catalog lookup.
3. **`parse` on untrusted giant JSON** — same **16 MiB** read cap as other CLI paths (caller passes bytes from `read_bounded`).
4. **Agents trust `fix --plan` as applied** — policy: plans are hints only; no file mutation.
5. **`validate` exit code 1** — intentional for CI; stderr may still have `RUST_LOG` noise—use `--json` on stdout.

### Mitigations

- Bundled skills via `include_str!` (no runtime path to repo `skills/`).
- Stable **`VCIR_*`** codes in `vc_ir::diagnostics`.
- Conformance fixture: `benchmarks/conformance/invalid/return_inside_block.vcir` → **`VCIR_CTL001`**.

### Residual

- Repair plans are **heuristic** (safe vs heuristic `repair.safety`); wrong automated edits remain possible if an agent applies plans blindly.
- `parse` does **not** validate—agents must call `validate` before `compile`.

---

## Path 7: `agent-repair`

**Flow:** bounded `check` → optional `fix_plan` metadata → optional in-process **`synthesize`** (`vc-refine`) → re-check.

### Abuse

1. **`--max-steps` huge + `--synthesize`** — multiplies refiner work (fuel per case × `synthesize_steps` × steps).
2. **Overwrite `--input`** — default writes successful repair back to input path (use `-o` for sandbox copies).
3. **Trusting repair JSON as fixed** — plans are hints; synthesize output is heuristic.

### Mitigations

- Default **`max_steps=3`**; synthesize runs only when IR already validates but behavioral check fails.
- Same **16 MiB** read cap and fuel/wall limits as `check`.
- Structured **`validation_code`** / **`failed_case`** for CEGIS-style external agents (no silent success).

### Residual

- Not a proof of correctness—only passes manifest/spec cases within budgets.
- Temp files in `$TMPDIR` may leak partial IR on shared hosts (use `-o` + private temp).

---

## Residual risks (honest)

1. Host CPU for parse / Cranelift compile / instantiate not fully bounded by guest fuel.
2. IR validation ≠ proof of intent—only internal consistency.
3. ONNX + untrusted Wasm require explicit trust boundaries and ops discipline.
4. Pre-1.0 diagnostic codes may gain new variants—pin `vectorc` version in agent loops (use `skills get`).

---

## Hardening backlog

| Priority | Item |
|----------|------|
| High | Operational timeouts on compile for untrusted Wasm (`run --isolate`, external job limits). |
| Medium | Streaming JSON parse for `.vcir` if serde amplification becomes measurable. |
| Medium | Span/offset in diagnostics if IR gets a text surface. |
| Low | `fix --apply` behind explicit flag (never default). |

---

## Implemented hardening (changelog)

| When | Finding | Mitigation |
|------|---------|------------|
| 2026-05 | Bench `program_path` escape | Relative, no `..`, under `benchmarks/` |
| 2026-05 | Unbounded CLI reads | `MAX_CLI_FILE_BYTES` |
| 2026-05 | Wasm size | `MAX_WASM_BYTES` |
| 2026-05 | `StackUnderflow` on type mismatch | `StackTypeMismatch` |
| 2026-05 | Eval absolute suite manifests | Relative only + repo-root canonicalize |
| 2026-05 | Agent skill / explain injection | Allowlist + token format limits |
| 2026-05 | Agent interface | `VCIR_*` diagnostics, `fix --plan` (no writes), bundled skills |
| 2026-05 | Structured `check` / CEGIS loop | `validation_code`, `failed_case`; `agent-repair` bounded synthesize |
| 2026-05 | RL training hooks | `scripts/rl_reward_execute_rate.py`, `--auto-split` in `gen_training_rows.py` |

**Still not solved by fuel alone:** Cranelift compile + instantiation CPU within size caps.
