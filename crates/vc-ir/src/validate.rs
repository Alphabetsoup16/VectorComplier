use crate::ast::{FuncSig, Instr, Module, ValType, PROGRAM_IR_VERSION};
use crate::limits::{
    MAX_BODY_INSTRS, MAX_CONTROL_DEPTH, MAX_DECLARED_LOCALS, MAX_EXPORT_NAME_LEN, MAX_PARAMS,
};
use crate::metrics::instr_tree_node_count;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("unsupported program_ir_version: {0} (expected {1})")]
    UnsupportedVersion(u32, u32),
    #[error("export_name must be non-empty")]
    EmptyExportName,
    #[error("export_name length {len} exceeds maximum {max}")]
    ExportNameTooLong { len: usize, max: usize },
    #[error("parameter count {count} exceeds maximum {max}")]
    TooManyParams { count: usize, max: usize },
    #[error("declared local count {count} exceeds maximum {max}")]
    TooManyDeclaredLocals { count: usize, max: usize },
    #[error("function body instruction count {count} exceeds maximum {max}")]
    BodyTooLarge { count: usize, max: usize },
    #[error("v2 requires exactly one exported result type")]
    BadResultArity,
    #[error("local index {index} out of range (locals {total})")]
    LocalOob { index: u32, total: u32 },
    #[error("stack underflow before {instr:?}")]
    StackUnderflow { instr: Instr },
    #[error("stack has {depth} values before return, expected {expected}")]
    BadReturnStack { depth: usize, expected: usize },
    #[error("stack type mismatch before {instr:?}: expected {expected:?}, found {actual:?}")]
    StackTypeMismatch {
        expected: ValType,
        actual: ValType,
        instr: Instr,
    },
    #[error("`return` is only allowed as the final function instruction")]
    ReturnInsideNestedControl,
    #[error("control nesting exceeds maximum depth ({max})")]
    ControlNestingTooDeep { max: usize },
    #[error("`if_else` branches leave mismatched stacks")]
    BranchStackMismatch,
    #[error(
        "`block` stack mismatch: entered at height {enter_len}, left at {leave_len}, expected delta {expected_delta}"
    )]
    InvalidBlockStack {
        enter_len: usize,
        leave_len: usize,
        expected_delta: usize,
    },
    #[error("function body must end with return")]
    MissingReturn,
}

fn stack_pop(
    stack: &mut Vec<ValType>,
    want: ValType,
    instr: &Instr,
) -> Result<(), ValidationError> {
    let got = stack.pop().ok_or_else(|| ValidationError::StackUnderflow {
        instr: instr.clone(),
    })?;
    if got != want {
        return Err(ValidationError::StackTypeMismatch {
            expected: want,
            actual: got,
            instr: instr.clone(),
        });
    }
    Ok(())
}

pub fn validate_module(module: &Module) -> Result<(), ValidationError> {
    if module.program_ir_version != PROGRAM_IR_VERSION {
        return Err(ValidationError::UnsupportedVersion(
            module.program_ir_version,
            PROGRAM_IR_VERSION,
        ));
    }
    if module.export_name.trim().is_empty() {
        return Err(ValidationError::EmptyExportName);
    }
    if module.export_name.len() > MAX_EXPORT_NAME_LEN {
        return Err(ValidationError::ExportNameTooLong {
            len: module.export_name.len(),
            max: MAX_EXPORT_NAME_LEN,
        });
    }

    let n = instr_tree_node_count(&module.func.body);
    if n > MAX_BODY_INSTRS {
        return Err(ValidationError::BodyTooLarge {
            count: n,
            max: MAX_BODY_INSTRS,
        });
    }

    validate_func(&module.func.sig, &module.func.locals, &module.func.body)?;

    Ok(())
}

pub fn validate_func(
    sig: &FuncSig,
    locals_decl: &[ValType],
    body: &[Instr],
) -> Result<(), ValidationError> {
    if sig.params.len() > MAX_PARAMS {
        return Err(ValidationError::TooManyParams {
            count: sig.params.len(),
            max: MAX_PARAMS,
        });
    }
    if locals_decl.len() > MAX_DECLARED_LOCALS {
        return Err(ValidationError::TooManyDeclaredLocals {
            count: locals_decl.len(),
            max: MAX_DECLARED_LOCALS,
        });
    }

    if sig.results.len() != 1 {
        return Err(ValidationError::BadResultArity);
    }

    let total_locals = sig.params.len() as u32 + locals_decl.len() as u32;

    let Some(last_instr) = body.last() else {
        return Err(ValidationError::MissingReturn);
    };
    if !matches!(last_instr, Instr::Return) {
        return Err(ValidationError::MissingReturn);
    }

    let mut stack: Vec<ValType> = Vec::new();
    validate_instr_list(
        &body[..body.len() - 1],
        &mut stack,
        sig,
        locals_decl,
        total_locals,
        0,
    )?;

    let ret_ty = sig.results[0];
    if stack.len() != 1 {
        return Err(ValidationError::BadReturnStack {
            depth: stack.len(),
            expected: 1,
        });
    }
    stack_pop(&mut stack, ret_ty, last_instr)?;
    if !stack.is_empty() {
        return Err(ValidationError::BadReturnStack {
            depth: stack.len(),
            expected: 0,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_instr_list(
    instrs: &[Instr],
    stack: &mut Vec<ValType>,
    sig: &FuncSig,
    locals_decl: &[ValType],
    total_locals: u32,
    depth: usize,
) -> Result<(), ValidationError> {
    if depth > MAX_CONTROL_DEPTH {
        return Err(ValidationError::ControlNestingTooDeep {
            max: MAX_CONTROL_DEPTH,
        });
    }

    for instr in instrs {
        match instr {
            Instr::Return => return Err(ValidationError::ReturnInsideNestedControl),
            Instr::Block { result, body } => {
                if depth >= MAX_CONTROL_DEPTH {
                    return Err(ValidationError::ControlNestingTooDeep {
                        max: MAX_CONTROL_DEPTH,
                    });
                }
                let enter_len = stack.len();
                validate_instr_list(body, stack, sig, locals_decl, total_locals, depth + 1)?;
                let leave_len = stack.len();
                let expected_delta = usize::from(result.is_some());
                if leave_len != enter_len + expected_delta {
                    return Err(ValidationError::InvalidBlockStack {
                        enter_len,
                        leave_len,
                        expected_delta,
                    });
                }
                if let Some(t) = result {
                    match stack.last() {
                        Some(top) if top == t => {}
                        Some(actual) => {
                            return Err(ValidationError::StackTypeMismatch {
                                expected: *t,
                                actual: *actual,
                                instr: instr.clone(),
                            });
                        }
                        None => {
                            return Err(ValidationError::StackUnderflow {
                                instr: instr.clone(),
                            });
                        }
                    }
                }
            }
            Instr::IfElse {
                result,
                then_body,
                else_body,
            } => {
                if depth >= MAX_CONTROL_DEPTH {
                    return Err(ValidationError::ControlNestingTooDeep {
                        max: MAX_CONTROL_DEPTH,
                    });
                }
                stack_pop(stack, ValType::I32, instr)?;
                let base_len = stack.len();
                let mut then_stack = stack.clone();
                validate_instr_list(
                    then_body,
                    &mut then_stack,
                    sig,
                    locals_decl,
                    total_locals,
                    depth + 1,
                )?;
                let mut else_stack = stack.clone();
                validate_instr_list(
                    else_body,
                    &mut else_stack,
                    sig,
                    locals_decl,
                    total_locals,
                    depth + 1,
                )?;
                if then_stack != else_stack {
                    return Err(ValidationError::BranchStackMismatch);
                }
                let expected_len = base_len + usize::from(result.is_some());
                if then_stack.len() != expected_len {
                    return Err(ValidationError::BranchStackMismatch);
                }
                if let Some(t) = result {
                    match then_stack.last() {
                        Some(top) if top == t => {}
                        Some(actual) => {
                            return Err(ValidationError::StackTypeMismatch {
                                expected: *t,
                                actual: *actual,
                                instr: instr.clone(),
                            });
                        }
                        None => return Err(ValidationError::BranchStackMismatch),
                    }
                }
                *stack = then_stack;
            }
            Instr::I32Const { .. } => stack.push(ValType::I32),
            Instr::I64Const { .. } => stack.push(ValType::I64),
            Instr::F32Const { .. } => stack.push(ValType::F32),
            Instr::F64Const { .. } => stack.push(ValType::F64),
            Instr::I32Add | Instr::I32Sub | Instr::I32Mul | Instr::I32Xor | Instr::I32Eq => {
                stack_pop(stack, ValType::I32, instr)?;
                stack_pop(stack, ValType::I32, instr)?;
                stack.push(ValType::I32);
            }
            Instr::I32Eqz => {
                stack_pop(stack, ValType::I32, instr)?;
                stack.push(ValType::I32);
            }
            Instr::I32TruncF32S => {
                stack_pop(stack, ValType::F32, instr)?;
                stack.push(ValType::I32);
            }
            Instr::F32ConvertI32S => {
                stack_pop(stack, ValType::I32, instr)?;
                stack.push(ValType::F32);
            }
            Instr::F32Add | Instr::F32Sub | Instr::F32Mul | Instr::F32Div => {
                stack_pop(stack, ValType::F32, instr)?;
                stack_pop(stack, ValType::F32, instr)?;
                stack.push(ValType::F32);
            }
            Instr::F32Gt => {
                stack_pop(stack, ValType::F32, instr)?;
                stack_pop(stack, ValType::F32, instr)?;
                stack.push(ValType::I32);
            }
            Instr::F64Add | Instr::F64Sub | Instr::F64Mul | Instr::F64Div => {
                stack_pop(stack, ValType::F64, instr)?;
                stack_pop(stack, ValType::F64, instr)?;
                stack.push(ValType::F64);
            }
            Instr::Drop => {
                stack.pop().ok_or_else(|| ValidationError::StackUnderflow {
                    instr: instr.clone(),
                })?;
            }
            Instr::LocalGet { index } => {
                if *index >= total_locals {
                    return Err(ValidationError::LocalOob {
                        index: *index,
                        total: total_locals,
                    });
                }
                let ty = local_type(sig, locals_decl, *index)?;
                stack.push(ty);
            }
            Instr::LocalSet { index } => {
                if *index >= total_locals {
                    return Err(ValidationError::LocalOob {
                        index: *index,
                        total: total_locals,
                    });
                }
                let ty = local_type(sig, locals_decl, *index)?;
                stack_pop(stack, ty, instr)?;
            }
        }
    }
    Ok(())
}

fn local_type(
    sig: &FuncSig,
    locals_decl: &[ValType],
    index: u32,
) -> Result<ValType, ValidationError> {
    let pi = index as usize;
    if pi < sig.params.len() {
        return Ok(sig.params[pi]);
    }
    let li = pi - sig.params.len();
    locals_decl
        .get(li)
        .copied()
        .ok_or(ValidationError::LocalOob {
            index,
            total: sig.params.len() as u32 + locals_decl.len() as u32,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Func, FuncSig, Module};

    #[test]
    fn validates_add_two_params() {
        let module = Module {
            program_ir_version: 2,
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
        validate_module(&module).unwrap();
    }

    #[test]
    fn if_else_float_max() {
        // max(a,b) on f32 via compare + branches; returns f32.
        let module = Module {
            program_ir_version: 2,
            export_name: "run".into(),
            func: Func {
                sig: FuncSig {
                    params: vec![ValType::F32, ValType::F32],
                    results: vec![ValType::F32],
                },
                locals: vec![],
                body: vec![
                    Instr::LocalGet { index: 0 },
                    Instr::LocalGet { index: 1 },
                    Instr::F32Gt,
                    Instr::IfElse {
                        result: Some(ValType::F32),
                        then_body: vec![Instr::LocalGet { index: 0 }],
                        else_body: vec![Instr::LocalGet { index: 1 }],
                    },
                    Instr::Return,
                ],
            },
        };
        validate_module(&module).unwrap();
    }

    #[test]
    fn rejects_missing_return() {
        let module = Module {
            program_ir_version: 2,
            export_name: "run".into(),
            func: Func {
                sig: FuncSig {
                    params: vec![ValType::I32],
                    results: vec![ValType::I32],
                },
                locals: vec![],
                body: vec![Instr::LocalGet { index: 0 }],
            },
        };
        assert!(matches!(
            validate_module(&module),
            Err(ValidationError::MissingReturn)
        ));
    }

    fn minimal_module(
        export_name: String,
        sig: FuncSig,
        locals: Vec<ValType>,
        body: Vec<Instr>,
    ) -> Module {
        Module {
            program_ir_version: 2,
            export_name,
            func: Func { sig, locals, body },
        }
    }

    #[test]
    fn rejects_export_name_too_long() {
        let name = "a".repeat(MAX_EXPORT_NAME_LEN + 1);
        let module = minimal_module(
            name,
            FuncSig {
                params: vec![],
                results: vec![ValType::I32],
            },
            vec![],
            vec![Instr::I32Const { value: 0 }, Instr::Return],
        );
        assert_eq!(
            validate_module(&module),
            Err(ValidationError::ExportNameTooLong {
                len: MAX_EXPORT_NAME_LEN + 1,
                max: MAX_EXPORT_NAME_LEN,
            })
        );
    }

    #[test]
    fn rejects_too_many_params() {
        let module = minimal_module(
            "run".into(),
            FuncSig {
                params: vec![ValType::I32; MAX_PARAMS + 1],
                results: vec![ValType::I32],
            },
            vec![],
            vec![Instr::I32Const { value: 0 }, Instr::Return],
        );
        assert_eq!(
            validate_module(&module),
            Err(ValidationError::TooManyParams {
                count: MAX_PARAMS + 1,
                max: MAX_PARAMS,
            })
        );
    }

    #[test]
    fn rejects_too_many_declared_locals() {
        let module = minimal_module(
            "run".into(),
            FuncSig {
                params: vec![],
                results: vec![ValType::I32],
            },
            vec![ValType::I32; MAX_DECLARED_LOCALS + 1],
            vec![Instr::I32Const { value: 0 }, Instr::Return],
        );
        assert_eq!(
            validate_module(&module),
            Err(ValidationError::TooManyDeclaredLocals {
                count: MAX_DECLARED_LOCALS + 1,
                max: MAX_DECLARED_LOCALS,
            })
        );
    }

    #[test]
    fn rejects_body_too_large_flat() {
        let mut body: Vec<Instr> = (0..MAX_BODY_INSTRS)
            .map(|i| Instr::I32Const { value: i as i32 })
            .collect();
        body.push(Instr::Drop);
        body.push(Instr::Return);
        let module = minimal_module(
            "run".into(),
            FuncSig {
                params: vec![],
                results: vec![ValType::I32],
            },
            vec![],
            body,
        );
        assert_eq!(
            validate_module(&module),
            Err(ValidationError::BodyTooLarge {
                count: MAX_BODY_INSTRS + 2,
                max: MAX_BODY_INSTRS,
            })
        );
    }

    #[test]
    fn accepts_limits_at_boundary() {
        let name = "a".repeat(MAX_EXPORT_NAME_LEN);
        let mut body = Vec::with_capacity(MAX_BODY_INSTRS);
        for _ in 0..(MAX_BODY_INSTRS - 2) / 2 {
            body.push(Instr::I64Const { value: 0 });
            body.push(Instr::Drop);
        }
        body.push(Instr::I32Const { value: 0 });
        body.push(Instr::Return);
        assert_eq!(body.len(), MAX_BODY_INSTRS);
        let module = minimal_module(
            name,
            FuncSig {
                params: vec![ValType::I32; MAX_PARAMS],
                results: vec![ValType::I32],
            },
            vec![ValType::I32; MAX_DECLARED_LOCALS],
            body,
        );
        validate_module(&module).unwrap();
    }
}
