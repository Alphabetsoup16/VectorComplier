# Codebase audit (May 2026)

Parallel audit of Rust crates, docs, scripts/setup, and tests/CI. **Status after remediation** in the same release.

## Executive summary

| Area | Verdict |
|------|---------|
| Rust workspace | **Solid** — clippy `-D warnings`, 66+ unit/integration tests, Wasm policy + fuel |
| Security | **Good** for v0 scope — bounded reads, path canonicalization, agent allowlists |
| Docs | **Fixed** — wrong manifest paths, script usage, architecture/agent gaps |
| Setup UX | **Improved** — `bootstrap-dev.sh`, `preflight-lite.sh`, `Makefile`, `docs/QUICKSTART.md`, devcontainer |
| Real value | **Oracle + agent contracts** shippable; **learned decoder** still external |

## Findings addressed

1. **`agent-repair` unbounded reads** → `oracle::read_bounded` in `repair.rs`
2. **Wrong docs paths** (`vectorbench_v0/manifests/…`) → `benchmarks/manifests/add.json`
3. **`decode_rank_eval.sh` usage** in research plan → `CANDIDATE_DIR TASK_ID`
4. **`rl_reward_execute_rate.py`** → uses `scripts/vectorc_invoke.py` (rustup-pinned `cargo run`)
5. **Contributor path** → Tier A/B in `CONTRIBUTING.md`, `preflight-lite` for PRs
6. **README / index** → `QUICKSTART.md`, scripts index, agent + training hooks

## Open backlog (not blocking)

| Priority | Item |
|----------|------|
| Medium | Default `run --wall-ms` for untrusted Wasm |
| Medium | ONNX model file size cap |
| Medium | `bench` vs `oracle::evaluate_vcir_path` parity test |
| Low | Unit tests for `oracle.rs` edge cases |
| Low | `agent-repair --synthesize` smoke in CI |
| Product | Trained decoder + public VectorBench numbers (Phase D) |

## Audit agents

Rust/CI, documentation, setup/scripts, and tests/benchmarks were reviewed read-only; fixes applied in-tree per lists above.

See [ADVERSARIAL_AUDIT.md](ADVERSARIAL_AUDIT.md) for threat-model detail.
