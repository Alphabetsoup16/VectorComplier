use serde::{Deserialize, Serialize};

/// Program IR schema version (must match JSON `program_ir_version`).
///
/// **v2** adds `f32`/`f64`, structured control (`block`, `if_else`), and richer arithmetic.
pub const PROGRAM_IR_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuncSig {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Instr {
    /// Push constant i32.
    I32Const {
        value: i32,
    },
    /// Push constant i64.
    I64Const {
        value: i64,
    },
    /// IEEE-754 `f32` constant (`serde_json` number).
    F32Const {
        value: f32,
    },
    /// IEEE-754 `f64` constant.
    F64Const {
        value: f64,
    },
    I32Add,
    I32Sub,
    I32Mul,
    /// Bitwise XOR on two i32 values.
    I32Xor,
    /// Equal (`i32,i32` → `i32`, Wasm `i32.eq`): result is 1 or 0.
    I32Eq,
    /// Equals zero (`i32` → `i32`, Wasm `i32.eqz`).
    I32Eqz,
    I32TruncF32S,
    F32ConvertI32S,
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    /// Float greater-than (`f32`,`f32` → `i32`, Wasm `f32.gt`).
    F32Gt,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    /// Drop top stack value.
    Drop,
    LocalGet {
        index: u32,
    },
    LocalSet {
        index: u32,
    },
    /// Structured block (`block … end`). Stack depth before vs after body follows Wasm block typing.
    Block {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<ValType>,
        body: Vec<Instr>,
    },
    /// Conditional (`if … else … end`). Pops `i32` condition (0 = else branch).
    IfElse {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<ValType>,
        then_body: Vec<Instr>,
        else_body: Vec<Instr>,
    },
    /// Unconditional return (must leave exactly `results.len()` values on the stack).
    ///
    /// Allowed only as the **last** instruction of the function body, never inside `block` /
    /// `if_else`.
    Return,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Func {
    pub sig: FuncSig,
    /// Locals declared after parameters (Wasm ordering).
    pub locals: Vec<ValType>,
    pub body: Vec<Instr>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    /// Schema version for forwards-compatible tooling.
    pub program_ir_version: u32,
    /// Exported Wasm function name (single export).
    pub export_name: String,
    pub func: Func,
}

impl Module {
    pub fn parse_json_slice(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }
}
