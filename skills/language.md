# Program IR v2 (agent language guide)

VectorCompiler’s executable waist is **Program IR v2** (`.vcir` JSON). Authority: `validate_module` in `vc-ir` + `schemas/program_ir_v2.schema.json`.

## Module shape

```json
{
  "program_ir_version": 2,
  "export_name": "run",
  "func": {
    "sig": { "params": ["i32", "i32"], "results": ["i32"] },
    "locals": [],
    "body": [ /* instructions */ ]
  }
}
```

- **One** exported function; **one** scalar result in `sig.results`.
- `body` must end with a top-level **`return`** (not nested inside `block` / `if_else`).
- Types: `i32`, `i64`, `f32`, `f64` (Wasm scalars).

## Common instructions

| Kind | JSON | Stack effect (summary) |
|------|------|------------------------|
| `local_get` | `{"local_get":{"index":0}}` | → value |
| `local_set` | `{"local_set":{"index":0}}` | value → |
| `i32_add` | `"i32_add"` | i32 i32 → i32 |
| `return` | `"return"` | (ends function) |
| `block` | `{"block":{"body":[...]}}` | structured control |
| `if_else` | `{"if_else":{"then_body":[],"else_body":[]}}` | branches must match stacks |

## Agent workflow

1. Emit or edit `.vcir` JSON.
2. `vectorc validate -i prog.vcir --json` — structured diagnostics (`VCIR_*` codes).
3. `vectorc explain VCIR_STK001 --json` / `vectorc fix --plan -i prog.vcir --json`.
4. `vectorc compile` → Wasm; `vectorc check` / `vectorc eval` for behavioral oracle.

Latent path: fixed `z` (len **256** f32) → `vectorc decode-z` → same validation boundary.
