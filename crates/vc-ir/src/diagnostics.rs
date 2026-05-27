//! Stable validation diagnostics and repair metadata for agent workflows.
//!
//! Inspired by structured compiler contracts (e.g. zerolang `check --json` / `fix --plan`).

use crate::ast::{Instr, ValType};
use crate::validate::ValidationError;
use serde::Serialize;

/// Machine-stable diagnostic code (pattern-matchable across toolchains).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    VcirVer001,
    VcirExp001,
    VcirExp002,
    VcirLim001,
    VcirLim002,
    VcirLim003,
    VcirSig001,
    VcirLoc001,
    VcirStk001,
    VcirStk002,
    VcirStk003,
    VcirCtl001,
    VcirCtl002,
    VcirCtl003,
    VcirCtl004,
    VcirCtl005,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VcirVer001 => "VCIR_VER001",
            Self::VcirExp001 => "VCIR_EXP001",
            Self::VcirExp002 => "VCIR_EXP002",
            Self::VcirLim001 => "VCIR_LIM001",
            Self::VcirLim002 => "VCIR_LIM002",
            Self::VcirLim003 => "VCIR_LIM003",
            Self::VcirSig001 => "VCIR_SIG001",
            Self::VcirLoc001 => "VCIR_LOC001",
            Self::VcirStk001 => "VCIR_STK001",
            Self::VcirStk002 => "VCIR_STK002",
            Self::VcirStk003 => "VCIR_STK003",
            Self::VcirCtl001 => "VCIR_CTL001",
            Self::VcirCtl002 => "VCIR_CTL002",
            Self::VcirCtl003 => "VCIR_CTL003",
            Self::VcirCtl004 => "VCIR_CTL004",
            Self::VcirCtl005 => "VCIR_CTL005",
        }
    }
}

impl ValidationError {
    pub fn code(&self) -> DiagnosticCode {
        match self {
            ValidationError::UnsupportedVersion(_, _) => DiagnosticCode::VcirVer001,
            ValidationError::EmptyExportName => DiagnosticCode::VcirExp001,
            ValidationError::ExportNameTooLong { .. } => DiagnosticCode::VcirExp002,
            ValidationError::TooManyParams { .. } => DiagnosticCode::VcirLim001,
            ValidationError::TooManyDeclaredLocals { .. } => DiagnosticCode::VcirLim002,
            ValidationError::BodyTooLarge { .. } => DiagnosticCode::VcirLim003,
            ValidationError::BadResultArity => DiagnosticCode::VcirSig001,
            ValidationError::LocalOob { .. } => DiagnosticCode::VcirLoc001,
            ValidationError::StackUnderflow { .. } => DiagnosticCode::VcirStk001,
            ValidationError::BadReturnStack { .. } => DiagnosticCode::VcirStk002,
            ValidationError::StackTypeMismatch { .. } => DiagnosticCode::VcirStk003,
            ValidationError::ReturnInsideNestedControl => DiagnosticCode::VcirCtl001,
            ValidationError::ControlNestingTooDeep { .. } => DiagnosticCode::VcirCtl002,
            ValidationError::BranchStackMismatch => DiagnosticCode::VcirCtl003,
            ValidationError::InvalidBlockStack { .. } => DiagnosticCode::VcirCtl004,
            ValidationError::MissingReturn => DiagnosticCode::VcirCtl005,
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        let code = self.code();
        let (expected, actual, repair) = field_hints(self);
        Diagnostic {
            code: code.as_str().to_string(),
            message: self.to_string(),
            expected,
            actual,
            severity: "error",
            repair,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairHint {
    pub id: &'static str,
    /// `safe` = unlikely to change intended semantics; `heuristic` = may need human review.
    pub safety: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub expected: String,
    pub actual: String,
    pub severity: &'static str,
    pub repair: RepairHint,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplainEntry {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub repair: RepairHint,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixStep {
    pub action: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixPlan {
    pub code: String,
    pub repair_id: String,
    pub safety: String,
    pub steps: Vec<FixStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidateReport {
    pub ok: bool,
    pub program_ir_version: Option<u32>,
    pub export_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,
}

pub fn explain_code(code: &str) -> Option<ExplainEntry> {
    let dc = parse_code(code)?;
    Some(ExplainEntry {
        code: dc.as_str().to_string(),
        title: explain_title(dc),
        detail: explain_detail(dc),
        repair: repair_for_code(dc),
    })
}

pub fn all_explain_entries() -> Vec<ExplainEntry> {
    [
        DiagnosticCode::VcirVer001,
        DiagnosticCode::VcirExp001,
        DiagnosticCode::VcirExp002,
        DiagnosticCode::VcirLim001,
        DiagnosticCode::VcirLim002,
        DiagnosticCode::VcirLim003,
        DiagnosticCode::VcirSig001,
        DiagnosticCode::VcirLoc001,
        DiagnosticCode::VcirStk001,
        DiagnosticCode::VcirStk002,
        DiagnosticCode::VcirStk003,
        DiagnosticCode::VcirCtl001,
        DiagnosticCode::VcirCtl002,
        DiagnosticCode::VcirCtl003,
        DiagnosticCode::VcirCtl004,
        DiagnosticCode::VcirCtl005,
    ]
    .into_iter()
    .map(|dc| ExplainEntry {
        code: dc.as_str().to_string(),
        title: explain_title(dc),
        detail: explain_detail(dc),
        repair: repair_for_code(dc),
    })
    .collect()
}

pub fn fix_plan(err: &ValidationError) -> FixPlan {
    let dc = err.code();
    let repair = repair_for_code(dc);
    FixPlan {
        code: dc.as_str().to_string(),
        repair_id: repair.id.to_string(),
        safety: repair.safety.to_string(),
        steps: fix_steps(err),
    }
}

pub fn validate_report(module: &crate::Module) -> ValidateReport {
    match crate::validate::validate_module(module) {
        Ok(()) => ValidateReport {
            ok: true,
            program_ir_version: Some(module.program_ir_version),
            export_name: Some(module.export_name.clone()),
            diagnostics: None,
        },
        Err(e) => ValidateReport {
            ok: false,
            program_ir_version: Some(module.program_ir_version),
            export_name: Some(module.export_name.clone()),
            diagnostics: Some(vec![e.diagnostic()]),
        },
    }
}

fn parse_code(s: &str) -> Option<DiagnosticCode> {
    Some(match s {
        "VCIR_VER001" => DiagnosticCode::VcirVer001,
        "VCIR_EXP001" => DiagnosticCode::VcirExp001,
        "VCIR_EXP002" => DiagnosticCode::VcirExp002,
        "VCIR_LIM001" => DiagnosticCode::VcirLim001,
        "VCIR_LIM002" => DiagnosticCode::VcirLim002,
        "VCIR_LIM003" => DiagnosticCode::VcirLim003,
        "VCIR_SIG001" => DiagnosticCode::VcirSig001,
        "VCIR_LOC001" => DiagnosticCode::VcirLoc001,
        "VCIR_STK001" => DiagnosticCode::VcirStk001,
        "VCIR_STK002" => DiagnosticCode::VcirStk002,
        "VCIR_STK003" => DiagnosticCode::VcirStk003,
        "VCIR_CTL001" => DiagnosticCode::VcirCtl001,
        "VCIR_CTL002" => DiagnosticCode::VcirCtl002,
        "VCIR_CTL003" => DiagnosticCode::VcirCtl003,
        "VCIR_CTL004" => DiagnosticCode::VcirCtl004,
        "VCIR_CTL005" => DiagnosticCode::VcirCtl005,
        _ => return None,
    })
}

fn repair_for_code(code: DiagnosticCode) -> RepairHint {
    match code {
        DiagnosticCode::VcirVer001 => RepairHint {
            id: "set-program-ir-version",
            safety: "safe",
            summary: "Set program_ir_version to the workspace PROGRAM_IR_VERSION (2).",
        },
        DiagnosticCode::VcirExp001 => RepairHint {
            id: "set-export-name",
            safety: "safe",
            summary: "Provide a non-empty export_name (typically \"run\").",
        },
        DiagnosticCode::VcirExp002 => RepairHint {
            id: "shorten-export-name",
            safety: "safe",
            summary: "Shorten export_name to within MAX_EXPORT_NAME_LEN UTF-8 bytes.",
        },
        DiagnosticCode::VcirLim001 => RepairHint {
            id: "reduce-params",
            safety: "heuristic",
            summary: "Reduce parameter count or split into multiple functions (IR v2 allows one export).",
        },
        DiagnosticCode::VcirLim002 => RepairHint {
            id: "reduce-locals",
            safety: "heuristic",
            summary: "Remove or reuse locals to stay within MAX_DECLARED_LOCALS.",
        },
        DiagnosticCode::VcirLim003 => RepairHint {
            id: "shrink-body",
            safety: "heuristic",
            summary: "Shrink the instruction tree (fewer nodes / less control nesting).",
        },
        DiagnosticCode::VcirSig001 => RepairHint {
            id: "fix-signature-single-result",
            safety: "safe",
            summary: "Program IR v2 requires exactly one scalar result type in func.sig.results.",
        },
        DiagnosticCode::VcirLoc001 => RepairHint {
            id: "fix-local-index",
            safety: "safe",
            summary: "Use local indices in [0, params+declared_locals); fix local_get/local_set.",
        },
        DiagnosticCode::VcirStk001 => RepairHint {
            id: "insert-stack-operands",
            safety: "heuristic",
            summary: "Push required operand types onto the stack before this instruction.",
        },
        DiagnosticCode::VcirStk002 => RepairHint {
            id: "align-return-stack",
            safety: "heuristic",
            summary: "Ensure the stack holds exactly one value of the declared result type before return.",
        },
        DiagnosticCode::VcirStk003 => RepairHint {
            id: "fix-stack-types",
            safety: "heuristic",
            summary: "Insert conversions or change preceding instructions so stack types match.",
        },
        DiagnosticCode::VcirCtl001 => RepairHint {
            id: "move-return-to-function-end",
            safety: "safe",
            summary: "Only the final top-level instruction may be return; use block values instead.",
        },
        DiagnosticCode::VcirCtl002 => RepairHint {
            id: "reduce-control-nesting",
            safety: "heuristic",
            summary: "Flatten block/if_else nesting to stay within MAX_CONTROL_DEPTH.",
        },
        DiagnosticCode::VcirCtl003 => RepairHint {
            id: "balance-if-else-branches",
            safety: "heuristic",
            summary: "Make then/else branches leave identical stack heights and result types.",
        },
        DiagnosticCode::VcirCtl004 => RepairHint {
            id: "fix-block-stack-delta",
            safety: "heuristic",
            summary: "Adjust block body so stack delta matches the block's declared result arity.",
        },
        DiagnosticCode::VcirCtl005 => RepairHint {
            id: "append-trailing-return",
            safety: "safe",
            summary: "End func.body with a single return instruction at top level.",
        },
    }
}

fn field_hints(err: &ValidationError) -> (String, String, RepairHint) {
    let repair = repair_for_code(err.code());
    let (expected, actual) = match err {
        ValidationError::UnsupportedVersion(got, want) => (
            format!("program_ir_version == {want}"),
            format!("program_ir_version == {got}"),
        ),
        ValidationError::EmptyExportName => (
            "non-empty export_name".into(),
            "empty or whitespace export_name".into(),
        ),
        ValidationError::ExportNameTooLong { len, max } => (
            format!("export_name.len() <= {max}"),
            format!("export_name.len() == {len}"),
        ),
        ValidationError::TooManyParams { count, max } => (
            format!("params.len() <= {max}"),
            format!("params.len() == {count}"),
        ),
        ValidationError::TooManyDeclaredLocals { count, max } => (
            format!("locals.len() <= {max}"),
            format!("locals.len() == {count}"),
        ),
        ValidationError::BodyTooLarge { count, max } => (
            format!("instruction_tree_nodes <= {max}"),
            format!("instruction_tree_nodes == {count}"),
        ),
        ValidationError::BadResultArity => (
            "func.sig.results.len() == 1".into(),
            "func.sig.results.len() != 1".into(),
        ),
        ValidationError::LocalOob { index, total } => (
            format!("local index < {total}"),
            format!("local index == {index}"),
        ),
        ValidationError::StackUnderflow { instr } => (
            format!("stack non-empty before {instr:?}"),
            "stack underflow".into(),
        ),
        ValidationError::BadReturnStack { depth, expected } => (
            format!("stack depth == {expected} before return"),
            format!("stack depth == {depth}"),
        ),
        ValidationError::StackTypeMismatch {
            expected,
            actual,
            instr,
        } => (
            format!("stack top == {expected:?} before {instr:?}"),
            format!("stack top == {actual:?}"),
        ),
        ValidationError::ReturnInsideNestedControl => (
            "return only as final top-level instruction".into(),
            "return inside block/if_else".into(),
        ),
        ValidationError::ControlNestingTooDeep { max } => (
            format!("control_nesting_depth <= {max}"),
            "control nesting too deep".into(),
        ),
        ValidationError::BranchStackMismatch => (
            "if_else branches leave identical stacks".into(),
            "branch stack mismatch".into(),
        ),
        ValidationError::InvalidBlockStack {
            enter_len,
            leave_len,
            expected_delta,
        } => (
            format!("block stack delta == {expected_delta}"),
            format!("entered at {enter_len}, left at {leave_len}"),
        ),
        ValidationError::MissingReturn => (
            "func.body ends with return".into(),
            "missing or misplaced return".into(),
        ),
    };
    (expected, actual, repair)
}

fn explain_title(code: DiagnosticCode) -> String {
    match code {
        DiagnosticCode::VcirVer001 => "Unsupported program_ir_version".into(),
        DiagnosticCode::VcirExp001 => "Empty export_name".into(),
        DiagnosticCode::VcirExp002 => "export_name too long".into(),
        DiagnosticCode::VcirLim001 => "Too many parameters".into(),
        DiagnosticCode::VcirLim002 => "Too many declared locals".into(),
        DiagnosticCode::VcirLim003 => "Function body too large".into(),
        DiagnosticCode::VcirSig001 => "Invalid result arity".into(),
        DiagnosticCode::VcirLoc001 => "Local index out of range".into(),
        DiagnosticCode::VcirStk001 => "Stack underflow".into(),
        DiagnosticCode::VcirStk002 => "Bad stack at return".into(),
        DiagnosticCode::VcirStk003 => "Stack type mismatch".into(),
        DiagnosticCode::VcirCtl001 => "return inside nested control".into(),
        DiagnosticCode::VcirCtl002 => "Control nesting too deep".into(),
        DiagnosticCode::VcirCtl003 => "if_else branch stack mismatch".into(),
        DiagnosticCode::VcirCtl004 => "block stack mismatch".into(),
        DiagnosticCode::VcirCtl005 => "Missing trailing return".into(),
    }
}

fn explain_detail(code: DiagnosticCode) -> String {
    match code {
        DiagnosticCode::VcirVer001 => {
            "Program IR files must declare program_ir_version matching the workspace validator."
        }
        DiagnosticCode::VcirExp001 => "Wasm export name must be non-empty for lowering.",
        DiagnosticCode::VcirExp002 => {
            "export_name is bounded to prevent abuse; shorten the UTF-8 name."
        }
        DiagnosticCode::VcirLim001 => "Tier-1 cap on parameters per function.",
        DiagnosticCode::VcirLim002 => "Tier-1 cap on declared locals (excluding parameters).",
        DiagnosticCode::VcirLim003 => "Instruction tree node count exceeds MAX_BODY_INSTRS.",
        DiagnosticCode::VcirSig001 => {
            "v2 modules expose one function with exactly one scalar return type."
        }
        DiagnosticCode::VcirLoc001 => {
            "local_get/local_set index must refer to a parameter or declared local slot."
        }
        DiagnosticCode::VcirStk001 => {
            "An instruction consumed more stack values than were available."
        }
        DiagnosticCode::VcirStk002 => {
            "return must see exactly one value of the declared result type on the stack."
        }
        DiagnosticCode::VcirStk003 => {
            "Stack types must match each instruction's Wasm-like typing rules."
        }
        DiagnosticCode::VcirCtl001 => {
            "Structured control uses block/if_else; return is only allowed at function end."
        }
        DiagnosticCode::VcirCtl002 => "block/if_else nesting exceeds MAX_CONTROL_DEPTH.",
        DiagnosticCode::VcirCtl003 => {
            "Both branches of if_else must leave the same stack shape."
        }
        DiagnosticCode::VcirCtl004 => {
            "A block's body must leave the stack in the state promised by the block type."
        }
        DiagnosticCode::VcirCtl005 => "The function body must end with a top-level return.",
    }
    .to_string()
}

fn fix_steps(err: &ValidationError) -> Vec<FixStep> {
    let repair = repair_for_code(err.code());
    let mut steps = vec![FixStep {
        action: repair.id,
        detail: repair.summary.to_string(),
    }];
    match err {
        ValidationError::UnsupportedVersion(_, want) => steps.push(FixStep {
            action: "edit-json-field",
            detail: format!("Set \"program_ir_version\": {want} at module root."),
        }),
        ValidationError::StackUnderflow { instr } => steps.push(FixStep {
            action: "inspect-predecessors",
            detail: format!("Add producers for operands required before {instr:?}."),
        }),
        ValidationError::StackTypeMismatch {
            expected,
            actual,
            instr,
        } => steps.push(FixStep {
            action: "insert-conversion-or-op",
            detail: format!(
                "Need {expected:?} on stack before {instr:?}; found {actual:?}."
            ),
        }),
        ValidationError::ReturnInsideNestedControl => steps.push(FixStep {
            action: "restructure-control",
            detail: "Hoist computation into block bodies; keep a single trailing return.".into(),
        }),
        ValidationError::MissingReturn => steps.push(FixStep {
            action: "append-return",
            detail: "Add { \"return\": null } as the last element of func.body.".into(),
        }),
        _ => {}
    }
    steps
}

fn valtype_label(t: ValType) -> &'static str {
    match t {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
    }
}

/// Stable parse summary (syntax only — does not imply validation success).
#[derive(Debug, Clone, Serialize)]
pub struct ParseSummary {
    pub export_name: String,
    pub program_ir_version: u32,
    pub params: Vec<String>,
    pub results: Vec<String>,
    pub declared_locals: usize,
    pub top_level_body_kinds: Vec<String>,
}

pub fn parse_summary(module: &crate::Module) -> ParseSummary {
    let sig = &module.func.sig;
    ParseSummary {
        export_name: module.export_name.clone(),
        program_ir_version: module.program_ir_version,
        params: sig.params.iter().copied().map(valtype_label).map(str::to_string).collect(),
        results: sig
            .results
            .iter()
            .copied()
            .map(valtype_label)
            .map(str::to_string)
            .collect(),
        declared_locals: module.func.locals.len(),
        top_level_body_kinds: module
            .func
            .body
            .iter()
            .map(instr_kind_label)
            .collect(),
    }
}

fn instr_kind_label(instr: &Instr) -> String {
    match instr {
        Instr::I32Const { .. } => "i32_const",
        Instr::I64Const { .. } => "i64_const",
        Instr::F32Const { .. } => "f32_const",
        Instr::F64Const { .. } => "f64_const",
        Instr::I32Add => "i32_add",
        Instr::I32Sub => "i32_sub",
        Instr::I32Mul => "i32_mul",
        Instr::I32Xor => "i32_xor",
        Instr::I32Eq => "i32_eq",
        Instr::I32Eqz => "i32_eqz",
        Instr::I32TruncF32S => "i32_trunc_f32_s",
        Instr::F32ConvertI32S => "f32_convert_i32_s",
        Instr::F32Add => "f32_add",
        Instr::F32Sub => "f32_sub",
        Instr::F32Mul => "f32_mul",
        Instr::F32Div => "f32_div",
        Instr::F32Gt => "f32_gt",
        Instr::F64Add => "f64_add",
        Instr::F64Sub => "f64_sub",
        Instr::F64Mul => "f64_mul",
        Instr::F64Div => "f64_div",
        Instr::Drop => "drop",
        Instr::LocalGet { .. } => "local_get",
        Instr::LocalSet { .. } => "local_set",
        Instr::Block { .. } => "block",
        Instr::IfElse { .. } => "if_else",
        Instr::Return => "return",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Func, FuncSig, Instr, Module, PROGRAM_IR_VERSION};

    #[test]
    fn codes_are_stable() {
        let err = ValidationError::EmptyExportName;
        assert_eq!(err.code().as_str(), "VCIR_EXP001");
        assert_eq!(
            explain_code("VCIR_EXP001").unwrap().title,
            "Empty export_name"
        );
    }

    #[test]
    fn validate_report_ok_add_shape() {
        let module = Module {
            program_ir_version: PROGRAM_IR_VERSION,
            export_name: "run".into(),
            func: Func {
                sig: FuncSig {
                    params: vec![ValType::I32, ValType::I32],
                    results: vec![ValType::I32],
                },
                locals: vec![],
                body: vec![
                    Instr::LocalGet { index: 0 },
                    Instr::LocalGet { index: 1 },
                    Instr::I32Add,
                    Instr::Return,
                ],
            },
        };
        let report = validate_report(&module);
        assert!(report.ok);
        assert!(report.diagnostics.is_none());
    }
}
