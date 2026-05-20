//! Property tests against `validate_module` / instruction sequence rules.

use proptest::prelude::*;
use vc_ir::{
    validate_module, Func, FuncSig, Instr, Module, ValType, ValidationError, MAX_DECLARED_LOCALS,
    MAX_PARAMS, PROGRAM_IR_VERSION,
};

fn arb_nonempty_export() -> impl Strategy<Value = String> {
    // Stay well under `MAX_EXPORT_NAME_LEN` (UTF-8 ASCII here).
    "[a-zA-Z][a-zA-Z0-9_]{0,12}"
}

fn arb_i32_binop() -> impl Strategy<Value = Instr> {
    prop_oneof![
        Just(Instr::I32Add),
        Just(Instr::I32Sub),
        Just(Instr::I32Mul),
        Just(Instr::I32Xor),
    ]
}

fn arb_local_type() -> impl Strategy<Value = ValType> {
    prop_oneof![Just(ValType::I32), Just(ValType::I64)]
}

fn fold_params_to_single_i32(n_params: usize) -> impl Strategy<Value = Vec<Instr>> {
    let mut prefix: Vec<Instr> = Vec::with_capacity(n_params.saturating_mul(2));
    for i in 0..n_params as u32 {
        prefix.push(Instr::LocalGet { index: i });
    }
    if n_params <= 1 {
        return Just(prefix).boxed();
    }
    // n_params - 1 binary ops, each op kind chosen independently
    let k = n_params - 1;
    prop::collection::vec(arb_i32_binop(), k)
        .prop_map(move |ops| {
            let mut v = prefix.clone();
            v.extend(ops);
            v
        })
        .boxed()
}

/// Body that leaves exactly one `i32` on the stack, without the final `return`.
fn arb_valid_prefix(n_params: usize) -> impl Strategy<Value = Vec<Instr>> {
    if n_params == 0 {
        return any::<i32>()
            .prop_map(|value| vec![Instr::I32Const { value }])
            .boxed();
    }
    let base = fold_params_to_single_i32(n_params);
    // Optional stack-neutral "touch-ups": still exactly one i32 on stack.
    let touch = prop_oneof![
        (any::<i32>(), arb_i32_binop()).prop_map(|(c, op)| vec![Instr::I32Const { value: c }, op]),
        any::<i64>().prop_map(|c| vec![Instr::I64Const { value: c }, Instr::Drop]),
    ];
    // Keep generated bodies far below `MAX_BODY_INSTRS`.
    let n_touch = 0usize..=4usize;
    (base, prop::collection::vec(touch, n_touch))
        .prop_map(|(mut body, patches)| {
            for patch in patches {
                body.extend(patch);
            }
            body
        })
        .boxed()
}

fn arb_valid_body(n_params: usize) -> impl Strategy<Value = Vec<Instr>> {
    arb_valid_prefix(n_params).prop_map(|mut prefix| {
        prefix.push(Instr::Return);
        prefix
    })
}

fn arb_valid_module() -> impl Strategy<Value = Module> {
    (
        arb_nonempty_export(),
        prop::collection::vec(Just(ValType::I32), 0usize..=MAX_PARAMS.min(5)),
        prop::collection::vec(arb_local_type(), 0usize..=MAX_DECLARED_LOCALS.min(4)),
    )
        .prop_flat_map(|(export_name, params, locals)| {
            let n = params.len();
            arb_valid_body(n).prop_map(move |body| Module {
                program_ir_version: PROGRAM_IR_VERSION,
                export_name: export_name.clone(),
                func: Func {
                    sig: FuncSig {
                        params: params.clone(),
                        results: vec![ValType::I32],
                    },
                    locals: locals.clone(),
                    body,
                },
            })
        })
}

proptest! {
    #[test]
    fn valid_random_modules_validate(m in arb_valid_module()) {
        validate_module(&m).unwrap();
    }

    #[test]
    fn bad_version_rejected(version in any::<u32>().prop_filter("not v2", |&v| v != PROGRAM_IR_VERSION)) {
        let m = Module {
            program_ir_version: version,
            export_name: "run".into(),
            func: Func {
                sig: FuncSig {
                    params: vec![],
                    results: vec![ValType::I32],
                },
                locals: vec![],
                body: vec![Instr::I32Const { value: 0 }, Instr::Return],
            },
        };
        assert_eq!(
            validate_module(&m),
            Err(ValidationError::UnsupportedVersion(version, PROGRAM_IR_VERSION))
        );
    }

    #[test]
    fn empty_export_rejected(ws in "[ \t\r\n]{1,8}") {
        let m = Module {
            program_ir_version: PROGRAM_IR_VERSION,
            export_name: ws,
            func: Func {
                sig: FuncSig {
                    params: vec![],
                    results: vec![ValType::I32],
                },
                locals: vec![],
                body: vec![Instr::I32Const { value: 0 }, Instr::Return],
            },
        };
        assert_eq!(
            validate_module(&m),
            Err(ValidationError::EmptyExportName)
        );
    }
}

#[test]
fn return_inside_nested_control_rejected() {
    let m = Module {
        program_ir_version: PROGRAM_IR_VERSION,
        export_name: "run".into(),
        func: Func {
            sig: FuncSig {
                params: vec![],
                results: vec![ValType::I32],
            },
            locals: vec![],
            body: vec![
                Instr::Block {
                    result: None,
                    body: vec![Instr::I32Const { value: 1 }, Instr::Return],
                },
                Instr::Return,
            ],
        },
    };
    assert!(matches!(
        validate_module(&m),
        Err(ValidationError::ReturnInsideNestedControl)
    ));
}
