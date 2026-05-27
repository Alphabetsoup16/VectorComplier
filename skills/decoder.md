# Latent decoder contract (v0)

| Item | Contract |
|------|----------|
| Input | `z`: `f32[256]` |
| ONNX input name | `z` shape `[1, 256]` |
| ONNX output | `program_ir_json`: UTF-8 bytes of one `.vcir` `Module` |
| Post-decode | `validate_module` (fail closed) |

Training: build `z` with `Z_BUILD` / `scripts/gen_training_rows.py` — do not invent ad hoc vectors.

Score with `vectorc eval --task <id> --json` (`execute_rate` is the primary metric).
