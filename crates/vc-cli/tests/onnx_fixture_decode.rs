//! End-to-end `vectorc decode-z --decoder onnx` against the checked-in fixture.

use std::path::PathBuf;
use std::process::Command;
use vc_bridge::{GoldenLatentDecoder, LatentDecoder};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectorc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_vectorc"))
}

#[test]
fn decode_z_onnx_identity_fixture_round_trips_like_golden() {
    let root = repo_root();
    let model = root.join("benchmarks/fixtures/decoder_identity_z.onnx");
    let zpath = root.join("benchmarks/fixtures/z_zeros.json");
    let vcir =
        std::env::temp_dir().join(format!("vectorc-onnx-decode-{}.vcir", std::process::id()));

    let status = Command::new(vectorc())
        .args([
            "decode-z",
            "-z",
            zpath.to_str().expect("utf-8"),
            "-o",
            vcir.to_str().expect("utf-8"),
            "--decoder",
            "onnx",
            "--onnx-model",
            model.to_str().expect("utf-8"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn decode-z onnx");

    assert!(status.success(), "decode-z onnx failed");

    let raw = std::fs::read_to_string(&vcir).expect("read output vcir");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse vcir json");
    vc_ir::validate_module(&module).expect("validate decoded IR");
    assert_eq!(module.export_name, "run");

    let golden = GoldenLatentDecoder
        .decode_z(&vec![0.0f32; vc_bridge::EMBEDDING_DIM])
        .expect("golden");
    assert_eq!(module, golden);

    let _ = std::fs::remove_file(&vcir);
}

#[test]
fn decode_z_onnx_z_switch_fixture_neg_z0_emits_mul_ir() {
    let root = repo_root();
    let model = root.join("benchmarks/fixtures/decoder_z_switch.onnx");
    let mut z = vec![0.0f32; vc_bridge::EMBEDDING_DIM];
    z[0] = -1.0;
    let zpath = std::env::temp_dir().join(format!("vectorc-z-neg-{}.json", std::process::id()));
    std::fs::write(&zpath, serde_json::to_string(&z).expect("z json")).expect("write z");

    let vcir =
        std::env::temp_dir().join(format!("vectorc-onnx-zswitch-{}.vcir", std::process::id()));

    let status = Command::new(vectorc())
        .args([
            "decode-z",
            "-z",
            zpath.to_str().expect("utf-8"),
            "-o",
            vcir.to_str().expect("utf-8"),
            "--decoder",
            "onnx",
            "--onnx-model",
            model.to_str().expect("utf-8"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn decode-z onnx z_switch");

    assert!(status.success(), "decode-z onnx z_switch failed");

    let mul_bytes = std::fs::read(root.join("benchmarks/programs/mul.vcir")).expect("mul.vcir");
    let expected: vc_ir::Module =
        vc_ir::Module::parse_json_slice(&mul_bytes).expect("parse mul.vcir");
    let raw = std::fs::read_to_string(&vcir).expect("read output vcir");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse decoded vcir");
    vc_ir::validate_module(&module).expect("validate mul decode");
    assert_eq!(module, expected, "expected mul IR from z[0] < 0");

    let _ = std::fs::remove_file(&zpath);
    let _ = std::fs::remove_file(&vcir);
}
