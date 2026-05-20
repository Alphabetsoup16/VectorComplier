//! First-party lowered Wasm must satisfy the default untrusted policy.

use std::path::PathBuf;

use vc_ir::Module;
use vc_lower_wasm::lower_module;
use vc_verify::{check_wasm_policy, CompiledModule, WasmPolicy};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_vcir(rel: &str) -> Module {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path).expect("read vcir");
    Module::parse_json_slice(&bytes).expect("parse vcir")
}

#[test]
fn lowered_add_passes_default_policy_and_compiles() {
    let wasm = lower_module(&load_vcir("benchmarks/programs/add.vcir")).expect("lower add");
    check_wasm_policy(&wasm, WasmPolicy::default()).expect("policy scan");
    CompiledModule::new(&wasm).expect("compile");
}

#[test]
fn lowered_mul_passes_default_policy_and_compiles() {
    let wasm = lower_module(&load_vcir("benchmarks/programs/mul.vcir")).expect("lower mul");
    check_wasm_policy(&wasm, WasmPolicy::default()).expect("policy scan");
    CompiledModule::new(&wasm).expect("compile");
}

#[test]
fn lowered_max_f32_passes_default_policy_and_compiles() {
    let wasm = lower_module(&load_vcir("benchmarks/programs/max_f32.vcir")).expect("lower max_f32");
    check_wasm_policy(&wasm, WasmPolicy::default()).expect("policy scan");
    CompiledModule::new(&wasm).expect("compile");
}
