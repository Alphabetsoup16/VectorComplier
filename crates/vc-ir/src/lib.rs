//! VectorCompiler intermediate representation (Program IR).
//!
//! **v2:** single exported function, Wasm-aligned types including floats, structured control flow.

mod ast;
pub mod diagnostics;
mod limits;
mod metrics;
mod validate;

pub use ast::{Func, FuncSig, Instr, Module, ValType, PROGRAM_IR_VERSION};
pub use limits::{
    MAX_BODY_INSTRS, MAX_CONTROL_DEPTH, MAX_DECLARED_LOCALS, MAX_EXPORT_NAME_LEN, MAX_PARAMS,
};
pub use metrics::{instr_tree_node_count, max_control_nesting_depth};
pub use serde_json;
pub use diagnostics::{
    all_explain_entries, explain_code, fix_plan, parse_summary, validate_report, Diagnostic,
    DiagnosticCode, ExplainEntry, FixPlan, ParseSummary, RepairHint, ValidateReport,
};
pub use validate::{validate_module, ValidationError};
