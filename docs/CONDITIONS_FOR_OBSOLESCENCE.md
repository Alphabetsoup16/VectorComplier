# Conditions for obsoleting “normal programming”

This note sharpens what **obsolete** would mean in theory, what must become true for that to happen, and how VectorCompiler’s direction fits—without conflating **v1 research scope** with a claim about the whole industry.

**Companion docs:** [LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md) (near-term execution), [ARCHITECTURE.md](ARCHITECTURE.md) (current narrow waist), [Z_CONTRACT.md](Z_CONTRACT.md) (`z` tensor + JSON envelope), [IR_VERSIONING.md](IR_VERSIONING.md) (Program IR pins), [TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md) (digests, manifests, authority boundaries).

---

## 1. What “obsolete” does and does not mean

**Does mean:** Hand-authored, UTF-8 **high-level source** (Python, Rust, Go, …) is **no longer the canonical medium** for specifying, storing, reviewing, and shipping most software changes. Models and tooling **author and refine** behavior in **shared high-dimensional state**; **discrete, checked artifacts** ground execution.

**Does not mean:**

- **No formal objects.** Hardware still consumes finite instruction streams. Something must become **opcodes, wire formats, or checked IR** before it runs.
- **No accountability trail.** Organizations still need **who approved what, when**, under what policy—even if the artifact is not prose.
- **No human-readable layer.** People still need **explanations and summaries**; those layers need not be **authoritative** or **diff-granularity canonical**.

**Precise slogan.** The target is not “never decode,” but **avoid mandatory serialization of intent through natural language or Algol-shaped source** when a **shorter path exists**: latent or structured internal state → **verifiable** program form → execution.

---

## 2. Six jobs of source code today

Textual programs are one packaging for six different functions:

| Job | Examples | If you remove text, you must still… |
|-----|----------|--------------------------------------|
| **Semantic specification** | Logic, data layout, APIs | Hold truth in a **semantics-carrying** representation |
| **Human consensus** | Design review, style guides | Agree on **objects** (graphs, properties, budgets), not only lines |
| **Diffable history** | Blame, revert, archaeology | **Version canonical artifacts** (IR, manifests, proofs), not casual chat |
| **Interop** | Libraries, OS ABI, standards | **Stable interfaces** others can target—by spec, not vibe |
| **Legal / certification** | Safety cases, contracts | **Auditable artifacts** courts and insurers accept |
| **Execution bridge** | Compilers, linkers, loaders | **Trusted lowering** to deployable units |

Latent-first stacks attack **specification** and the **execution bridge** most directly. **Obsolescence** additionally requires parity on **consensus, history, interop, and law**—or text survives as the **liability interface** even when models never “think” in it.

---

## 3. Three theory paths

### Path A — Canonical artifacts stop being “source in a repo”

**Claim.** The **system of record** is not `.rs` / `.py` files but **typed, machine-checkable** layers: rich IR, capability manifests, test obligations, optional proof bundles. Humans use **views**; authority lives in **what validates and reproduces**.

**Needs:** Industry- or domain-level **semantic standards** (like LLVM IR or the JVM spec, but for your chosen waists), **tooling parity** (debug, profile, analyze, migrate), and **upgrade** paths when the canonical language evolves.

**Role of vectors.** **Authoring and search** stay continuous; **commits** record **checked discrete** state plus metadata.

### Path B — Shared latent protocols between agents and runtimes

**Claim.** Teams of agents coordinate in **vector channels + structured memory + tool traces**; **compilation is one tool** among many (`latent → IR → Wasm` here; elsewhere: solvers, DBs, kernels).

**Needs:** **Stable `z` contracts** (normalization, signing), **grounding** whenever reality is touched (network, money, hardware), and **budgets** (fuel, time, capability) enforced at tool boundaries—see [SECURITY.md](SECURITY.md) for this repo’s posture.

### Path C — Social and legal acceptance

**Claim.** Liability moves from “we read the C file after the crash” to “we certify **pipeline + artifact class + approval graph**.”

**Needs:** **Reproducible builds** from signed IR, **formal specs** where stakes demand them, and **human-in-the-loop** approval on **constraint graphs** rather than on every line of syntax.

Until Path C advances, **normal programming remains the default legal convenience** even if technically redundant.

---

## 4. End-state reference model

| Layer | What replaces “typing programs” |
|-------|----------------------------------|
| **Intent** | Objectives, constraints, tests, economics—**not necessarily** English |
| **Authoring** | Search in latent space + **verifier / compiler feedback** |
| **Canonical form** | Rich IR, Wasm (+ future extensions), manifests—not mandatory HL source |
| **Collaboration** | Diffs on **graphs / properties / policies** |
| **Human read** | **Non-authoritative** explanations and projections |
| **Execution** | Sandboxed, metered, capability-scoped hosts |

**Programming migrates:** from “expressing thought in Algol-shaped text” to **steering formal objects under budgets**—still a discipline, different surface.

---

## 5. Minimal conditions (all must trend true)

For **hand-written high-level source** to shrink from default practice to niche:

1. **Dominant canonical layer(s)** with **clear operational semantics**, not N incompatible private compilers.
2. **Authoring economics:** latent (or assisted) paths beat text on **reliability, latency, and team cost** on **real** systems—not demos alone.
3. **Tooling and education:** professionals are trained on **canonical artifacts**, not only languages.
4. **Trust bundle:** reproducible builds, signing, policy approval, and **postmortem** workflows that work **without** every maintainer parsing idiomatic C.

Latent models **alone** do not obsolete programming. **Verified, shared, diffable semantics plus a toolchain society trusts** do. Continuous representation is the **engine** for *producing* those artifacts faster; VectorCompiler instantiates **one narrow waist** (validated Program IR → Wasm) suitable for **grounded, bounded execution** early in that stack.

---

## 6. Scope honesty for this repository

**This workspace today** delivers the **execution-facing waist** and **training plan** toward `z → IR → Wasm`. It does **not** yet deliver Path A’s full canonical language for all software, Path B’s multi-agent protocols, or Path C’s legal stack.

Use this document to **align narratives**: v1 success is **measured execution and verifier metrics** on bounded tasks ([LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md)); **obsolescence** is a **multi-year, industry-scale** conjunction of §5, not a single model release. Operational pins for this repo live under [IR_VERSIONING.md](IR_VERSIONING.md), [Z_CONTRACT.md](Z_CONTRACT.md), and [TRUST_AND_CANONICAL_ARTIFACTS.md](TRUST_AND_CANONICAL_ARTIFACTS.md).

---

*Version 1.0 — conceptual framing for contributors; not a product commitment.*
