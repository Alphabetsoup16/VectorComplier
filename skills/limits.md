# Tier-1 limits (Program IR v2)

Enforced by `validate_module` before lowering.

| Limit | Value |
|-------|------:|
| `MAX_PARAMS` | 16 |
| `MAX_DECLARED_LOCALS` | 64 |
| `MAX_BODY_INSTRS` (tree nodes) | 4096 |
| `MAX_CONTROL_DEPTH` | 32 |
| `MAX_EXPORT_NAME_LEN` (UTF-8 bytes) | 128 |
| CLI / Wasm read cap | 16 MiB |

Guest execution: Wasmtime **fuel** + optional **`--wall-ms`** (cooperative). Compile wall-clock default 30s (`vc-verify`).
