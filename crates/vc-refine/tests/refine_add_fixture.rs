use std::path::PathBuf;

use vc_refine::{ProgramRefiner, RandomIrRefiner, Spec, SpecCase};
use vc_verify::Limits;

#[test]
fn refine_add_manifest_cases_from_wrong_sub() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../benchmarks/manifests/add.json");
    let raw = std::fs::read_to_string(&path).expect("read add manifest");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("parse manifest");
    let cases: Vec<SpecCase> = serde_json::from_value(manifest["cases"].clone()).expect("cases");
    let spec = Spec { cases };

    let vcir_path = manifest_dir.join("../../benchmarks/programs/add.vcir");
    let vcir_raw = std::fs::read_to_string(&vcir_path).expect("read add.vcir");
    let mut initial: vc_ir::Module = serde_json::from_str(&vcir_raw).expect("parse ir");
    initial.func.body[2] = vc_ir::Instr::I32Sub;

    let fuel = manifest["fuel"].as_u64().unwrap_or(50_000);
    let refined = RandomIrRefiner::new(42)
        .refine(&initial, &spec, fuel, 512)
        .expect("refine add program");

    let wasm = vc_lower_wasm::lower_module(&refined).expect("lower");
    let compiled = vc_verify::CompiledModule::new(&wasm).expect("compile");
    for case in &spec.cases {
        let got = compiled
            .invoke_i32_return(
                "run",
                &case.args,
                Limits {
                    fuel,
                    max_wall_ms: None,
                },
            )
            .expect("invoke");
        assert_eq!(got, case.expect_i32);
    }
}
