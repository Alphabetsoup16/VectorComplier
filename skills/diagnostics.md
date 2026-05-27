# Validation diagnostic codes (`VCIR_*`)

Stable codes from `vc_ir::ValidationError::code()`. Use `vectorc explain <CODE> --json` for full entries.

| Code | Meaning |
|------|---------|
| `VCIR_VER001` | Wrong `program_ir_version` |
| `VCIR_EXP001` | Empty `export_name` |
| `VCIR_EXP002` | `export_name` too long |
| `VCIR_LIM001` | Too many parameters |
| `VCIR_LIM002` | Too many declared locals |
| `VCIR_LIM003` | Body instruction tree too large |
| `VCIR_SIG001` | Result arity ≠ 1 |
| `VCIR_LOC001` | Bad `local_get` / `local_set` index |
| `VCIR_STK001` | Stack underflow |
| `VCIR_STK002` | Bad stack at `return` |
| `VCIR_STK003` | Stack type mismatch |
| `VCIR_CTL001` | `return` inside nested control |
| `VCIR_CTL002` | Control nesting too deep |
| `VCIR_CTL003` | `if_else` branch stacks differ |
| `VCIR_CTL004` | `block` stack delta wrong |
| `VCIR_CTL005` | Missing trailing `return` |

Each diagnostic includes `repair.id` for `vectorc fix --plan`.
