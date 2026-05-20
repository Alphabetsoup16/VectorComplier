use std::path::PathBuf;

#[test]
fn lower_and_invoke_add_fixture() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../benchmarks/programs/add.vcir");
    let raw = std::fs::read_to_string(&path).expect("read fixture");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse JSON IR");
    let wasm = vc_lower_wasm::lower_module(&module).expect("lower");

    let got = vc_verify::invoke_i32_return(
        &wasm,
        "run",
        &[40, 2],
        vc_verify::Limits {
            fuel: 100_000,
            max_wall_ms: None,
        },
    )
    .expect("invoke");
    assert_eq!(got, 42);
}

#[test]
fn lower_and_invoke_mul_fixture() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../benchmarks/programs/mul.vcir");
    let raw = std::fs::read_to_string(&path).expect("read fixture");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse JSON IR");
    let wasm = vc_lower_wasm::lower_module(&module).expect("lower");

    let got = vc_verify::invoke_i32_return(
        &wasm,
        "run",
        &[6, 7],
        vc_verify::Limits {
            fuel: 100_000,
            max_wall_ms: None,
        },
    )
    .expect("invoke");
    assert_eq!(got, 42);
}

#[test]
fn lower_and_invoke_max_f32_fixture() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../benchmarks/programs/max_f32.vcir");
    let raw = std::fs::read_to_string(&path).expect("read fixture");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse JSON IR");
    let wasm = vc_lower_wasm::lower_module(&module).expect("lower");

    let lim = vc_verify::Limits {
        fuel: 100_000,
        max_wall_ms: None,
    };

    match vc_verify::invoke_scalar_return(
        &wasm,
        "run",
        &[
            vc_verify::WasmScalar::F32(3.0),
            vc_verify::WasmScalar::F32(5.0),
        ],
        lim,
    )
    .expect("invoke")
    {
        vc_verify::WasmScalar::F32(v) => assert!((v - 5.0).abs() < 1e-5),
        other => panic!("expected f32 result, got {other:?}"),
    }

    match vc_verify::invoke_scalar_return(
        &wasm,
        "run",
        &[
            vc_verify::WasmScalar::F32(2.0),
            vc_verify::WasmScalar::F32(1.0),
        ],
        lim,
    )
    .expect("invoke")
    {
        vc_verify::WasmScalar::F32(v) => assert!((v - 2.0).abs() < 1e-5),
        other => panic!("expected f32 result, got {other:?}"),
    }
}
