# Research implementation plan

Actionable synthesis from frontier work on **execution-guided RL**, **latent→IR**, **neurosymbolic repair**, and **agent-first compilers**—mapped onto VectorCompiler’s existing **latent → Program IR → Wasm → oracle** stack.

This document is the **engineering contract** for what is implemented in-repo vs what stays in the external training repo (`vectorcompiler-train`).

---

## Principles (unchanged)

| Principle | Decision |
|-----------|----------|
| Semantic waist | Program IR v2 + `validate_module` — not LLVM, not Python as ground truth |
| Training oracle | `vectorc eval` / `check` — primary metric **`execute_rate`** |
| Agent surface | Structured `VCIR_*` diagnostics + `fix --plan` + skills (zerolang-style contracts) |
| Training compute | External PyTorch; this repo = compiler + benchmarks + reward scripts |

---

## Phase map

### Phase A — Agent oracle contracts (done)

- [x] `vc_ir::diagnostics` — stable `VCIR_*` codes, `FixPlan`, `validate_report`
- [x] `vectorc validate | parse | explain | fix --plan | skills`
- [x] `docs/AGENT_COMPILER_INTERFACE.md` + bundled `skills/*.md`
- [x] Conformance fixture `benchmarks/conformance/invalid/return_inside_block.vcir`

### Phase B — Structured behavioral failures (done)

- [x] `CheckSummary` fields: `validation_code`, `failed_case_index`, `failed_case`
- [x] Shared oracle module: `crates/vc-cli/src/oracle.rs`
- [x] `vectorc agent-repair` — bounded check → fix-plan → optional in-process `synthesize`

### Phase C — Training / RL plumbing (done in-repo)

- [x] `scripts/rl_reward_execute_rate.py` — TRL/HF-friendly reward from `vectorc eval --json`
- [x] `scripts/decode_rank_eval.sh` — multi-candidate decode + rank by `execute_rate`
- [x] `scripts/gen_training_rows.py --auto-split` — deterministic 80/10/10 train/val/test
- [x] `docs/TRAINING_REWARD_CONTRACT.md`

### Phase D — External training (out of repo)

- [ ] PyTorch decoder head + GRPO/RL with `execute_rate` reward
- [ ] ONNX export wired to `vectorc decode-z --decoder onnx`
- [ ] Scale VectorBench beyond v0 canonical families

### Phase E — Research extensions (future)

- [ ] E-graph / refinement search behind `vc-refine` (Prism/EggMind-style)
- [ ] Multi-task suite expansion + curriculum from `execute_rate` gradients
- [ ] Optional NL→IR sketch layer (not replacing IR waist)

---

## Command reference (new)

```bash
# Structured check (behavioral)
vectorc check -i prog.vcir -m benchmarks/vectorbench_v0/manifests/add_i32.json --json

# Bounded CEGIS-style loop
vectorc agent-repair -i prog.vcir --spec spec.json --max-steps 3 --synthesize --json

# RL reward (training repo)
python3 scripts/rl_reward_execute_rate.py --vcir out.vcir --task add_i32

# Multi-candidate ranking after decode
./scripts/decode_rank_eval.sh z.json candidates/ ranked/
```

---

## Paper themes → repo hooks

| Theme | Papers / ideas | VectorCompiler hook |
|-------|----------------|---------------------|
| Execution RL | RLEF, StepCoder, EG-CFG, Posterior-GRPO | `execute_rate` in `eval`; `rl_reward_execute_rate.py` |
| Neurosymbolic | LLM-CEGIS, FunSearch | `agent-repair` + `synthesize` + counterexample in `CheckSummary` |
| Latent→IR | NLI, SEMREP, AST-T5 | JSON Program IR decode; not LLVM waist |
| Agent compilers | zerolang | `validate`/`fix --plan`/`skills`; Wasm remains oracle |

**Not adopted:** SWE-bench as primary metric; full in-repo PyTorch; zerolang as IR replacement.

---

## Success criteria

1. **Agents** can close the loop with JSON-only tools (`validate` → `explain` → `fix` → `check` / `agent-repair`).
2. **Training** can call a single Python reward function aligned with VectorBench.
3. **Splits** are reproducible and documented for val/holdout without leakage.
4. **CI** stays green on Rust 1.91 with smoke tests for new CLI paths.

See also: [RESEARCH_READING_LIST.md](RESEARCH_READING_LIST.md), [TRAINING_REWARD_CONTRACT.md](TRAINING_REWARD_CONTRACT.md), [AGENT_COMPILER_INTERFACE.md](AGENT_COMPILER_INTERFACE.md).
