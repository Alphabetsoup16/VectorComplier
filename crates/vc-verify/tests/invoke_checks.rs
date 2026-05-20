//! Integration checks for Wasm loading, arity/export, fuel traps, policy, and wall-clock limits.

use vc_verify::{
    invoke_i32_return, CompileLimits, CompiledModule, Limits, WasmPolicy, MAX_WASM_BYTES,
};

fn limits(fuel: u64) -> Limits {
    Limits {
        fuel,
        max_wall_ms: None,
    }
}

fn wasm_run_returns_i32() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
          (func $run (export "run") (param i32 i32) (result i32)
            (local.get 0)
            (local.get 1)
            (i32.add))
        )"#,
    )
    .expect("wat")
}

#[test]
fn invoke_session_matches_one_shot() {
    let wasm = wasm_run_returns_i32();
    let cmp = CompiledModule::new(&wasm).expect("compile");
    let limits = limits(100_000);
    let mut session = cmp.prepare_invoke("run").expect("prepare");
    let a = session
        .invoke_i32_return(&[2, 40], limits)
        .expect("session invoke");
    let b = invoke_i32_return(&wasm, "run", &[2, 40], limits).expect("one shot");
    assert_eq!(a, b);
}

#[test]
fn compile_wall_clock_zero_ms_rejects() {
    let wasm = wasm_run_returns_i32();
    let err = CompiledModule::new_with_policy(
        &wasm,
        WasmPolicy::default(),
        CompileLimits {
            max_wall_ms: Some(0),
        },
    )
    .err()
    .expect("zero-ms compile budget should time out");
    assert!(
        format!("{err}").contains("compile exceeded wall-clock"),
        "{err}"
    );
}

#[test]
fn compile_default_wall_ms_accepts_small_module() {
    let wasm = wasm_run_returns_i32();
    CompiledModule::new(&wasm).expect("default 30s compile budget should accept small module");
}

#[test]
fn compiled_reuse_matches_one_shot() {
    let wasm = wasm_run_returns_i32();
    let cmp = CompiledModule::new(&wasm).expect("compile");
    let a = cmp
        .invoke_i32_return("run", &[2, 40], limits(100_000))
        .expect("invoke reuse");
    let b = invoke_i32_return(&wasm, "run", &[2, 40], limits(100_000)).expect("one shot");
    assert_eq!(a, b);
}

#[test]
fn missing_export_errors() {
    let wasm = wat::parse_str("(module (func (export \"other\") (result i32) (i32.const 0)))")
        .expect("wat");
    let err = invoke_i32_return(&wasm, "run", &[], limits(10_000)).unwrap_err();
    assert!(format!("{}", err).contains("missing export"), "{}", err);
}

#[test]
fn arity_mismatch_errors() {
    let wasm = wasm_run_returns_i32();
    let err = invoke_i32_return(&wasm, "run", &[1], limits(10_000)).unwrap_err();
    assert!(format!("{}", err).contains("arity"), "{}", err);
}

#[test]
fn zero_fuel_returns_error() {
    let wasm = wat::parse_str(r#"(module (func (export "run") (result i32) (i32.const 7)))"#)
        .expect("wat");
    assert!(
        invoke_i32_return(&wasm, "run", &[], limits(0)).is_err(),
        "zero fuel should prevent successful execution"
    );
}

#[test]
fn oversize_module_rejected() {
    let big = vec![0u8; MAX_WASM_BYTES + 1];
    assert!(
        CompiledModule::new(&big).is_err(),
        "oversized wasm accepted"
    );
}

#[test]
fn module_with_import_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "env" "noop" (func))
          (func (export "run") (result i32) (i32.const 0))
        )"#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("import module should be rejected"),
    };
    assert!(
        format!("{err}").contains("imports not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_memory_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "run") (result i32) (i32.const 0))
        )"#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("memory module should be rejected"),
    };
    assert!(
        format!("{err}").contains("memories not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_table_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (table 1 funcref)
          (func (export "run") (result i32) (i32.const 0))
        )"#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("table module should be rejected"),
    };
    assert!(
        format!("{err}").contains("tables not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_start_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (func $start)
          (start $start)
          (func (export "run") (result i32) (i32.const 0))
        )"#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("start module should be rejected"),
    };
    assert!(
        format!("{err}").contains("start function not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_global_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (global i32 (i32.const 0))
          (func (export "run") (result i32)
            (i32.const 0)))
        "#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("global module should be rejected"),
    };
    assert!(
        format!("{err}").contains("globals not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_element_segment_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (table 1 funcref)
          (elem (i32.const 0) 0)
          (func (export "run") (result i32) (i32.const 0))
        )"#,
    )
    .expect("wat");

    let err = match CompiledModule::new_with_policy(
        &wasm,
        WasmPolicy {
            allow_tables: true,
            ..WasmPolicy::default()
        },
        CompileLimits::default(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("element segment module should be rejected"),
    };
    assert!(
        format!("{err}").contains("element segments not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn module_with_data_segment_rejected() {
    let wasm = wat::parse_str(
        r#"
        (module
          (data "x")
          (func (export "run") (result i32)
            (i32.const 0)))
        "#,
    )
    .expect("wat");

    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("data segment module should be rejected"),
    };
    assert!(
        format!("{err}").contains("data segments not allowed"),
        "unexpected error: {err}"
    );
}

#[test]
fn wasm_component_encoding_rejected() {
    let wasm: Vec<u8> = vec![
        0x00, 0x61, 0x73, 0x6d, // `\0asm`
        0x0d, 0x00, // component version
        0x01, 0x00, // component kind
    ];
    let err = match CompiledModule::new(&wasm) {
        Err(e) => e,
        Ok(_) => panic!("component wasm should be rejected"),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("only core WebAssembly") || msg.contains("encoding"),
        "unexpected error: {msg}"
    );
}

#[test]
fn invoke_session_wall_clock_timeout_on_infinite_loop() {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (export "run") (result i32)
            (loop (br 0))
            unreachable)
        )"#,
    )
    .expect("wat");

    let cmp = CompiledModule::new(&wasm).expect("compile");
    let mut session = cmp.prepare_invoke("run").expect("prepare");
    let err = session
        .invoke_i32_return(
            &[],
            Limits {
                fuel: u64::MAX,
                max_wall_ms: Some(200),
            },
        )
        .unwrap_err();

    assert!(
        format!("{err}").contains("wall-clock timeout"),
        "unexpected error: {err}"
    );
}

#[test]
fn wall_clock_timeout_on_infinite_loop() {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (export "run") (result i32)
            (loop (br 0))
            unreachable)
        )"#,
    )
    .expect("wat");

    let cmp = CompiledModule::new(&wasm).expect("compile");
    let err = cmp
        .invoke_i32_return(
            "run",
            &[],
            Limits {
                fuel: u64::MAX,
                max_wall_ms: Some(200),
            },
        )
        .unwrap_err();

    assert!(
        format!("{err}").contains("wall-clock timeout"),
        "unexpected error: {err}"
    );
}
