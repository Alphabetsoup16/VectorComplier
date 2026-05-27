# Research reading list

Curated pointers for VectorCompiler’s research directions. Full implementation mapping: [RESEARCH_IMPLEMENTATION_PLAN.md](RESEARCH_IMPLEMENTATION_PLAN.md).

---

## Execution-guided RL & code generation

- **RLEF** — Reinforcement learning with execution feedback for code models.
- **StepCoder** — Step-wise execution signals for program synthesis.
- **EG-CFG** — Execution-guided control-flow for structured generation.
- **Posterior-GRPO** — Group-relative policy optimization with verifiable rewards.

**Use here:** `vectorc eval --json` → `metrics.execute_rate` as the primary RL reward; optional repair loops at inference.

---

## Latent / neural → structured IR

- **NLI** — Neural latent intermediates for program representation.
- **SEMREP** — Semantic program representations from models.
- **AST-T5** — Structured AST generation with seq2seq.
- **Meta LLM Compiler** — LLM-assisted compilation stacks (borrow contracts, not waist).

**Use here:** Decoder outputs **Program IR JSON**, validated before Wasm; no LLVM as mandatory waist.

---

## Neurosymbolic & search

- **LLM-CEGIS** — Counterexample-guided inductive synthesis with LLM proposals.
- **FunSearch** — Evolution + verifier for function discovery.
- **E-graphs (Prism, EggMind)** — Equality saturation for rewrite/search.

**Use here:** `vectorc agent-repair` (counterexample in `CheckSummary`); `vectorc synthesize` (`vc-refine`); future e-graph backend in `vc-refine`.

---

## Agent-first compilers & Wasm

- **[zerolang](https://github.com/vercel-labs/zerolang)** — `check` / `explain` / `fix --plan` / skills for agents.
- **Agentless** — Minimal agent surface for SWE tasks (contrast: we optimize for IR+Wasm oracle).
- **WASI-NN / WebNN** — On-device inference (future decoder deployment paths).

**Use here:** Agent CLI + skills; Wasmtime sandbox + fuel as the behavioral ground truth.

---

## When to read vs when to ship

| Read for… | Ship in VectorCompiler as… |
|-----------|------------------------------|
| Reward design | `execute_rate`, `scripts/rl_reward_execute_rate.py` |
| Repair loops | `agent-repair`, `fix --plan`, `synthesize` |
| Structured decode | ONNX + `validate_module` gate |
| Benchmark scale | VectorBench v0 → v1 (separate milestone) |
