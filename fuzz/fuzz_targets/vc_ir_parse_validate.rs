#![no_main]

// Exercise JSON parse + IR validation — denial of service / panic paths on hostile `.vcir`.
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if let Ok(module) = vc_ir::Module::parse_json_slice(data) {
        let _ = vc_ir::validate_module(&module);
    }
});
