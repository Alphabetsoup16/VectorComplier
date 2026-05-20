# Full line-by-line adversarial audit

**Scope:** Every Rust source file under `crates/*/src/**/*.rs`, integration tests, and CLI tests as of this revision. **Out of scope:** Wasmtime/Cranelift internals, `target/`, generated lockfile semantics beyond dependency posture.

**Method:** Manual read for abuse cases (confused deputy, DoS, sandbox escape, logic bugs, panic paths). **Verdict:** Default pipeline is **fit for trusted dev / bounded benchmarks**. Untrusted Wasm still warrants **`run --isolate`** + tight fuel + size caps.

---

## Executive summary

| Crate | Risk level | Notes |
|-------|------------|--------|
| `vc-ir` | Low | Parse allocates before validation; limits enforced in `validate_module`. |
| `vc-lower-wasm` | Low | Straight mapping; **`End`** opcode closes Wasm function after validated body. |
| `vc-verify` | Medium | **`WasmPolicy`** scan (imports, memory, tables, start, globals, tags, elements, data; **component encoding**, nested modules, component-model sections **always** rejected); compile/instantiate **not** fuel-metered; epoch wall-clock is **cooperative**. |
| `vc-cli` | Medium | Bench path guards; **`run --isolate`** (Unix PGID + SIGKILL on timeout). |
| `vc-refine` | Low–Med | CPU-bound search; binop mutation avoids infinite RNG loops. |
| `vc-bridge` | Low | Fail-closed stubs / ONNX placeholder. |

---

## `vc-ir`

**Files:** `lib.rs`, `ast.rs`, `limits.rs`, `validate.rs`

- **`Module::parse_json_slice`:** Full JSON parse with serde; bounded only by caller-supplied slice length (CLI caps at 16 MiB). No streaming parse — large allocations possible within cap.
- **`export_name.trim()`:** Unicode whitespace can yield a non-empty export; intentional but operators should expect unusual names.
- **Stack validator:** Errors clone **`Instr`** — host-side only.

**Residual:** Allocator pressure within **`serde_json`** bounded by file cap, not instruction count alone at parse time.

---

## `vc-lower-wasm`

**Files:** `lib.rs`, `lower.rs`

- **`lower_module`:** Always **`validate_module`** first.
- **`wasm_local_groups`:** Correct contiguous local encoding.

---

## `vc-verify`

**Files:** `lib.rs`, `tests/invoke_checks.rs`

- **`check_wasm_policy`:** Imports/mem/tables respect **`WasmPolicy`** flags; **start**, **tags**, **globals**, **elements**, **data count**, **data**, **`Encoding::Component`**, **`ModuleSection`**, and **component-model section payloads** are **always** rejected (not toggleable in code today).
- **`CompiledModule::new`:** Size cap + policy + **`Module::from_binary`** — Cranelift compile not fuel-metered.
- **`invoke_i32_return`:** Fuel + epoch deadline; watchdog respects **`cancel`** after invoke returns.

**Residual:** Core Wasm **code/type/export** sections still arbitrary within engine limits — policy focuses on imports/memory/tables and stripping risky sections, not full bytecode minimization.

---

## `vc-cli`

**Files:** `main.rs`, `tests/cli_smoke.rs`

- **`read_bounded`:** Metadata + byte length checks — mitigates common TOCTOU shrink races.
- **Bench:** **`canonicalize`** + **`strip_prefix`** on suite root.
- **`run --isolate`:** Temp Wasm copy; subprocess **`vectorc run`**; Unix process group + **`SIGKILL`** on timeout; **`libc::pre_exec`** sets **`setpgid`**.

**Residual:** **`wait_child_wall_clock`** calls **`process::exit`** on child failure — whole CLI exits with child status (CLI-only semantics).

---

## `vc-refine` / `vc-bridge`

- **`module_satisfies_spec`:** Invoke export name from IR matches spec harness.
- **Bridge:** See **`docs/DECODER_ROADMAP.md`**.

---

## Recommended next actions

1. Bump **Rust/Wasmtime** when feasible; broaden **`cargo deny`** advisories with newer tooling.
2. Extend **isolate** pattern to **`compile`** if adversarial **`.vcir`** becomes part of your threat model.
