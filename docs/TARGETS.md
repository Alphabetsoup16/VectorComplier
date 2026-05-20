# Targets and portability

VectorCompiler is a **Rust workspace** that builds native tools (`vectorc`) and emits **Wasm modules** as data artifacts. This page clarifies **host triples** you may use for development and CI, and how **Wasm** fits the cross-platform story.

---

## Host platforms (Rust triples)

### `aarch64-apple-darwin` (Apple Silicon)

**Primary recommended developer machine** for this repository on macOS. Rust 1.91 (see `rust-toolchain.toml`) and Wasmtime 43.x (workspace dependency) support Apple Silicon natively.

Typical workflow:

```bash
rustup target add aarch64-apple-darwin   # usually default on Apple Silicon
cargo build -p vc-cli
```

### `x86_64-apple-darwin` (Intel macOS)

**Optional** but supported: build and run the same CLI and tests on Intel Macs. No crate in this workspace is arm64-specific; performance characteristics differ but semantics should match.

```bash
rustup target add x86_64-apple-darwin
cargo build -p vc-cli --target x86_64-apple-darwin
```

Use this when you need to verify behaviors on older hardware or CI images that still use Intel macOS runners.

### `riscv64gc-unknown-linux-gnu` (64-bit RISC-V Linux)

**Cross-compilation** to RISC-V is a plausible deployment target for edge or research hardware. The workspace does not commit CI config for RISC-V here, but nothing in the documented IR lowering path is architecture-specific—**Wasmtime and Cranelift** must support your chosen host triple for `vc-verify` and CLI runs on that machine.

High-level steps (illustrative—toolchain names vary by distro):

1. Install a RISC-V Rust target: `rustup target add riscv64gc-unknown-linux-gnu`
2. Provide a linker / sysroot for `riscv64gc-unknown-linux-gnu` (often via `CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_LINKER` and `CC_*` environment variables).
3. `cargo build -p vc-cli --target riscv64gc-unknown-linux-gnu`

**Caveat:** third-party native dependencies (here: Wasmtime) must successfully build or provide prebuilt artifacts for your exact RISC-V Linux environment; when in doubt, build on the target hardware.

---

## Wasm as the portable execution target

VectorCompiler’s **primary distributable program form** is a **Wasm module** produced by `vc-lower-wasm`, not a `.dylib` or `.so` derived from the latent model. That choice implies:

| Concern | How Wasm helps |
|--------|----------------|
| Same bitcode everywhere | One `.wasm` can be stored, signed, and replayed across OS/arch combinations. |
| Host-specific runtimes | Wasmtime (this repo), V8, Wasm3, browser engines, etc., become interchangeable **embeddings** as long as imports match. |
| Capability gates | Hosts decide whether to expose WASI, clocks, or sockets; VectorCompiler’s emitted modules today aim for **zero imports**—see [SECURITY.md](SECURITY.md). |

**Important distinction:** “portableWasm” does **not** mean “safe by default.” It means **deployment surface shrinks**: you ship a compact binary with explicit entry points instead of rebuilding native artifacts per latent revision on every platform.

---

## Where triples matter vs. where they do not

- **IR JSON (`.vcir`)** — platform-agnostic text.
- **Lowering** — deterministic mapping from IR to Wasm; **not** tuned per host ISA in v1.
- **`vc-verify` / CLI `run` and `bench`** — depend on **Wasmtime** for the **host** you run on; fuel and traps behave per Wasmtime’s implementation on that triple.

---

## Suggested CI matrix (documentation-only)

When you add CI, a practical starting matrix is:

- `aarch64-apple-darwin` (or Linux `aarch64` runner if available)
- `x86_64-unknown-linux-gnu`
- Optionally `riscv64gc-unknown-linux-gnu` once toolchain support is confirmed in your environment

Wasm modules produced in CI on one host should **bit-identically** match another host’s lowering for the same IR input (assuming the same versions of `vc-lower-wasm` and dependencies)—any drift is a bug worth tracking.
