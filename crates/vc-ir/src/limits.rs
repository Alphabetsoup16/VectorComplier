//! Tier-1 resource limits for Program IR v2 (validation only).

/// Maximum instructions in a function body (including the final `return`).
pub const MAX_BODY_INSTRS: usize = 4096;
/// Maximum function parameters.
pub const MAX_PARAMS: usize = 16;
/// Maximum locals declared after parameters (Wasm `locals` section).
pub const MAX_DECLARED_LOCALS: usize = 64;
/// Maximum byte length of `Module::export_name` (UTF-8).
pub const MAX_EXPORT_NAME_LEN: usize = 128;

/// Maximum nesting depth for `block` / `if_else` trees (`Instr` tree depth).
pub const MAX_CONTROL_DEPTH: usize = 32;
