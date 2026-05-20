# Program IR versioning

Program IR is the **typed discrete waist** between latent space and Wasm. Versioning keeps training data, validators, and tooling aligned.

---

## Source of truth

| Artifact | Role |
|----------|------|
| `vc_ir::PROGRAM_IR_VERSION` (`crates/vc-ir/src/ast.rs`) | Must match JSON field `program_ir_version` on every `Module`. |
| `vc_ir::validate_module` | Rejects unsupported versions before lowering or execution. |
| `schemas/program_ir_v2.schema.json` | JSON Schema for **v2** fixtures and generators. |
| `schemas/program_ir_v1.schema.json` | Historical schema; retained for migration references only. |

**Today:** the workspace targets **Program IR v2** (see [ARCHITECTURE.md](ARCHITECTURE.md) §Program IR v2 snapshot).

---

## Bump checklist

When introducing **v3** (or breaking validation rules within a version):

1. Increment `PROGRAM_IR_VERSION` and document semantic delta in this file and [ARCHITECTURE.md](ARCHITECTURE.md).  
2. Add `schemas/program_ir_v3.schema.json` (or extend policy if non-breaking).  
3. Update `scripts/validate-schemas.sh` and CI **json-schema** job inputs.  
4. Refresh golden fixtures and any dataset manifests that declare `program_ir_version`.  
5. Note decoder / ONNX assumptions: training exports must target the same pin ([LATENT_FIRST_TRAINING_PLAN.md](LATENT_FIRST_TRAINING_PLAN.md)).

Non-breaking additions (new opcodes with backward-compatible parsing) still deserve a **minor** narrative in CHANGELOG-style notes even if the integer version stays put—prefer validator + schema updates in the same PR.
