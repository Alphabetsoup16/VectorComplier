# Full-repo vetting report

Consolidated from parallel read-only audits (security, IR/lower alignment, refine/CLI, docs/CI, test coverage, dependencies, `vc-bridge`). Post-audit fixes are tracked in git history after this document was added.

## Verdict

| Area | Status |
|------|--------|
| **Core pipeline** (IR → Wasm → verify) | Sound for v1 instruction set; validated IR lowers consistently |
| **Security** | Strong defaults (no imports/memory/tables in policy); residual compile/parse DoS within caps |
| **Synthesis** | Functional (`vc-refine` + `synthesize`); heuristic, not latent-ML |
| **Tests** | Full **`cargo test`** suite + **`vectorc`** CLI smoke + Wasm policy integration tests |
| **Docs** | SECURITY / ADVERSARIAL_AUDIT / ARCHITECTURE aligned with **`WasmPolicy`**, **`synthesize`**, epoch wall-clock |

## Automated checks (run locally)

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
./scripts/validate-schemas.sh   # pip install check-jsonschema && jq
cargo deny check licenses bans   # cargo install cargo-deny --locked --version 0.18.3
for m in benchmarks/manifests/*.json; do cargo run --locked -q -p vc-cli -- bench -m "$m"; done
```

## Open risks (accepted or backlog)

1. **Wasmtime / deps** — workspace pins **Wasmtime 43.x**; **`cargo audit`** fails CI on new RustSec advisories.
2. **Compile/instantiate CPU** — not fuel-metered; use process timeouts for untrusted Wasm.
3. **`max_wall_ms`** — cooperative epoch interruption (not real-time scheduling); combine with **fuel** for hostile loops.
4. **Latent path** — **`OrtLatentDecoder`** + golden/stub decoders ship in-tree; production story still needs a **trained** model and dataset-scale eval.
5. **JSON parse before IR limits** — 16 MiB cap bounds worst case; streaming parse is future work.

## CI extras

- **`scripts/validate-schemas.sh`** — Program IR fixtures + **`{cases}`** slices from bench manifests vs **`schemas/`** (requires **`pip install check-jsonschema`** and **`jq`**).
- **`cargo deny check licenses bans`** — licenses + duplicate crate bans (pinned **`cargo-deny 0.18.3`** in CI for MSRV **1.91**); advisories use **`cargo audit`**.

## Review scope

Scoped passes covered: security, IR/schema alignment, refine/CLI, documentation drift, test matrix, dependency hygiene, and `vc-bridge` ONNX integration.
