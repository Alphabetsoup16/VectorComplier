use thiserror::Error;
use vc_ir::{Instr as IrInstr, Module as IrModule, ValType};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection,
    Instruction as WasmInstr, Module as WasmModule, TypeSection, ValType as WasmValType,
};

#[derive(Debug, Error)]
pub enum LowerError {
    #[error(transparent)]
    Validation(#[from] vc_ir::ValidationError),
    #[error("lower internal error: function body must end with return")]
    MissingReturn,
}

fn map_valtype(t: ValType) -> WasmValType {
    match t {
        ValType::I32 => WasmValType::I32,
        ValType::I64 => WasmValType::I64,
        ValType::F32 => WasmValType::F32,
        ValType::F64 => WasmValType::F64,
    }
}

fn map_block_type(result: Option<ValType>) -> BlockType {
    match result {
        None => BlockType::Empty,
        Some(t) => BlockType::Result(map_valtype(t)),
    }
}

fn wasm_local_groups(locals: &[ValType]) -> Vec<(u32, WasmValType)> {
    if locals.is_empty() {
        return vec![];
    }

    let mut out: Vec<(u32, WasmValType)> = vec![];
    let mut cur_ty = locals[0];
    let mut cur_count = 1u32;
    for &ty in &locals[1..] {
        if ty == cur_ty {
            cur_count += 1;
        } else {
            out.push((cur_count, map_valtype(cur_ty)));
            cur_ty = ty;
            cur_count = 1;
        }
    }
    out.push((cur_count, map_valtype(cur_ty)));
    out
}

fn lower_instr_list(instrs: &[IrInstr], func: &mut Function) -> Result<(), LowerError> {
    for instr in instrs {
        match instr {
            IrInstr::I32Const { value } => {
                func.instruction(&WasmInstr::I32Const(*value));
            }
            IrInstr::I64Const { value } => {
                func.instruction(&WasmInstr::I64Const(*value));
            }
            IrInstr::F32Const { value } => {
                func.instruction(&WasmInstr::F32Const((*value).into()));
            }
            IrInstr::F64Const { value } => {
                func.instruction(&WasmInstr::F64Const((*value).into()));
            }
            IrInstr::I32Add => {
                func.instruction(&WasmInstr::I32Add);
            }
            IrInstr::I32Sub => {
                func.instruction(&WasmInstr::I32Sub);
            }
            IrInstr::I32Mul => {
                func.instruction(&WasmInstr::I32Mul);
            }
            IrInstr::I32Xor => {
                func.instruction(&WasmInstr::I32Xor);
            }
            IrInstr::I32Eq => {
                func.instruction(&WasmInstr::I32Eq);
            }
            IrInstr::I32Eqz => {
                func.instruction(&WasmInstr::I32Eqz);
            }
            IrInstr::I32TruncF32S => {
                func.instruction(&WasmInstr::I32TruncF32S);
            }
            IrInstr::F32ConvertI32S => {
                func.instruction(&WasmInstr::F32ConvertI32S);
            }
            IrInstr::F32Add => {
                func.instruction(&WasmInstr::F32Add);
            }
            IrInstr::F32Sub => {
                func.instruction(&WasmInstr::F32Sub);
            }
            IrInstr::F32Mul => {
                func.instruction(&WasmInstr::F32Mul);
            }
            IrInstr::F32Div => {
                func.instruction(&WasmInstr::F32Div);
            }
            IrInstr::F32Gt => {
                func.instruction(&WasmInstr::F32Gt);
            }
            IrInstr::F64Add => {
                func.instruction(&WasmInstr::F64Add);
            }
            IrInstr::F64Sub => {
                func.instruction(&WasmInstr::F64Sub);
            }
            IrInstr::F64Mul => {
                func.instruction(&WasmInstr::F64Mul);
            }
            IrInstr::F64Div => {
                func.instruction(&WasmInstr::F64Div);
            }
            IrInstr::Drop => {
                func.instruction(&WasmInstr::Drop);
            }
            IrInstr::LocalGet { index } => {
                func.instruction(&WasmInstr::LocalGet(*index));
            }
            IrInstr::LocalSet { index } => {
                func.instruction(&WasmInstr::LocalSet(*index));
            }
            IrInstr::Block { result, body } => {
                func.instruction(&WasmInstr::Block(map_block_type(*result)));
                lower_instr_list(body, func)?;
                func.instruction(&WasmInstr::End);
            }
            IrInstr::IfElse {
                result,
                then_body,
                else_body,
            } => {
                func.instruction(&WasmInstr::If(map_block_type(*result)));
                lower_instr_list(then_body, func)?;
                func.instruction(&WasmInstr::Else);
                lower_instr_list(else_body, func)?;
                func.instruction(&WasmInstr::End);
            }
            IrInstr::Return => {
                return Err(LowerError::MissingReturn);
            }
        }
    }
    Ok(())
}

pub fn lower_module(module: &IrModule) -> Result<Vec<u8>, LowerError> {
    vc_ir::validate_module(module)?;

    let sig = &module.func.sig;

    let mut wasm_mod = WasmModule::new();

    let mut types = TypeSection::new();
    let params_wasm: Vec<WasmValType> = sig.params.iter().copied().map(map_valtype).collect();
    let results_wasm: Vec<WasmValType> = sig.results.iter().copied().map(map_valtype).collect();
    types.ty().function(params_wasm, results_wasm);
    wasm_mod.section(&types);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    wasm_mod.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export(module.export_name.as_str(), ExportKind::Func, 0);
    wasm_mod.section(&exports);

    let mut codes = CodeSection::new();

    let local_groups = wasm_local_groups(&module.func.locals);
    let mut func = Function::new(local_groups);

    let body = &module.func.body;
    let (ret, main) = body.split_last().ok_or(LowerError::MissingReturn)?;
    if !matches!(ret, IrInstr::Return) {
        return Err(LowerError::MissingReturn);
    }

    lower_instr_list(main, &mut func)?;
    func.instruction(&WasmInstr::Return);
    func.instruction(&WasmInstr::End);
    codes.function(&func);

    wasm_mod.section(&codes);

    Ok(wasm_mod.finish())
}
