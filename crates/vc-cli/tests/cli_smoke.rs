//! Smoke tests for the `vectorc` binary (compile / digest / inspect / run / synthesize / eval).
//!
//! Shell failure dumps (`scripts/dump-eval-failures.sh`, `scripts/batch-eval-training-shard.sh`)
//! are documented in `docs/DEBUGGING_DECODE.md` and not run from `cargo test`.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectorc() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_vectorc"))
}

fn take_sha256_hex_line(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    for line in text.lines() {
        let t = line.trim();
        if t.len() == 64 && t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return t.to_string();
        }
    }
    panic!("expected lowercase SHA-256 hex line in output:\n{text}");
}

#[test]
fn compile_add_fixture() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let out = std::env::temp_dir().join(format!("vectorc-add-{}.wasm", std::process::id()));

    let status = Command::new(vectorc())
        .args([
            "compile",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn vectorc");

    assert!(status.success(), "compile failed");
    let meta = std::fs::metadata(&out).expect("wasm output");
    assert!(meta.len() > 0, "empty wasm");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn digest_vcir_fixture_is_hex64() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let output = Command::new(vectorc())
        .args(["digest", "-i", input.to_str().expect("utf-8 path")])
        .current_dir(&root)
        .output()
        .expect("spawn digest");

    assert!(output.status.success(), "digest failed");
    let line = take_sha256_hex_line(&output.stdout);
    assert_eq!(line.len(), 64);
    assert!(
        line.chars().all(|c| c.is_ascii_hexdigit()),
        "not hex: {line}"
    );
    assert_eq!(line, line.to_ascii_lowercase());
}

#[test]
fn inspect_vcir_fixture_json() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let output = Command::new(vectorc())
        .args([
            "inspect",
            "-i",
            input.to_str().expect("utf-8 path"),
            "--json",
        ])
        .env("RUST_LOG", "warn")
        .current_dir(&root)
        .output()
        .expect("spawn inspect --json");

    assert!(
        output.status.success(),
        "inspect --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let summary: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("parse inspect JSON stdout");
    let obj = summary.as_object().expect("inspect JSON root object");
    for key in [
        "export_name",
        "program_ir_version",
        "params",
        "results",
        "instruction_tree_nodes",
        "max_control_nesting_depth",
        "opcode_histogram",
    ] {
        assert!(obj.contains_key(key), "missing key {key}: {stdout}");
    }
    assert_eq!(summary["export_name"], "run");
    assert!(summary["opcode_histogram"].is_object());
}

#[test]
fn inspect_vcir_fixture_summary() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let output = Command::new(vectorc())
        .args(["inspect", "-i", input.to_str().expect("utf-8 path")])
        .current_dir(&root)
        .output()
        .expect("spawn inspect");

    assert!(output.status.success(), "inspect failed");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("instruction_tree_nodes"),
        "missing metrics: {combined}"
    );
    assert!(
        combined.contains("validate_module: OK"),
        "missing OK marker: {combined}"
    );
}

#[test]
fn compile_print_digest_matches_file_digest() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let wasm = std::env::temp_dir().join(format!("vectorc-digest-{}.wasm", std::process::id()));

    let compiled = Command::new(vectorc())
        .args([
            "compile",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-o",
            wasm.to_str().expect("utf-8 path"),
            "--print-digest",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn compile --print-digest");

    assert!(
        compiled.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let printed = take_sha256_hex_line(&compiled.stdout);

    let direct = Command::new(vectorc())
        .args(["digest", "-i", wasm.to_str().expect("utf-8 path")])
        .current_dir(&root)
        .output()
        .expect("spawn digest wasm");

    assert!(direct.status.success(), "digest wasm failed");
    let from_cmd = take_sha256_hex_line(&direct.stdout);

    assert_eq!(printed, from_cmd);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn check_add_vcir_against_manifest() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let manifest = root.join("benchmarks/manifests/add.json");
    let output = Command::new(vectorc())
        .args([
            "check",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-m",
            manifest.to_str().expect("utf-8 path"),
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn check");

    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse check JSON");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["validate_ok"], true);
    assert_eq!(summary["compile_ok"], true);
    assert_eq!(summary["run_ok"], true);
}

#[test]
fn eval_add_vcir_against_vectorbench_v0() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let suite = root.join("benchmarks/vectorbench_v0/suite.json");
    let output = Command::new(vectorc())
        .args([
            "eval",
            "-i",
            input.to_str().expect("utf-8 path"),
            "--suite",
            suite.to_str().expect("utf-8 path"),
            "--task",
            "add_i32",
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn eval");

    assert!(
        output.status.success(),
        "eval failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse eval JSON");
    assert_eq!(summary["suite_id"], "vectorbench_v0");
    assert_eq!(summary["ok"], true);
    assert_eq!(summary["metrics"]["tasks_passed"], 1);
    assert_eq!(summary["metrics"]["execute_rate"], 1.0);
}

#[test]
fn run_compiled_add() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let out = std::env::temp_dir().join(format!("vectorc-run-{}.wasm", std::process::id()));

    let compile = Command::new(vectorc())
        .args([
            "compile",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn compile");
    assert!(compile.success());

    let run = Command::new(vectorc())
        .args([
            "run",
            "-i",
            out.to_str().expect("utf-8 path"),
            "-e",
            "run",
            "-f",
            "100000",
            "-a",
            "40,2",
            "--expect",
            "42",
        ])
        .current_dir(&root)
        .status()
        .expect("spawn run");
    assert!(run.success(), "run failed");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn run_compiled_add_with_wall_ms() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let out = std::env::temp_dir().join(format!("vectorc-wall-{}.wasm", std::process::id()));

    let compile = Command::new(vectorc())
        .args([
            "compile",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn compile");
    assert!(compile.success());

    let run = Command::new(vectorc())
        .args([
            "run",
            "-i",
            out.to_str().expect("utf-8 path"),
            "-e",
            "run",
            "-f",
            "100000",
            "-a",
            "40,2",
            "--expect",
            "42",
            "--wall-ms",
            "5000",
        ])
        .current_dir(&root)
        .status()
        .expect("spawn run --wall-ms");
    assert!(run.success(), "run with --wall-ms failed");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn run_compiled_add_isolated() {
    let root = repo_root();
    let input = root.join("benchmarks/programs/add.vcir");
    let out = std::env::temp_dir().join(format!("vectorc-isolate-{}.wasm", std::process::id()));

    let compile = Command::new(vectorc())
        .args([
            "compile",
            "-i",
            input.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn compile");
    assert!(compile.success());

    let run = Command::new(vectorc())
        .args([
            "run",
            "--isolate",
            "--isolate-timeout-ms",
            "60000",
            "-i",
            out.to_str().expect("utf-8 path"),
            "-e",
            "run",
            "-f",
            "100000",
            "-a",
            "40,2",
            "--expect",
            "42",
        ])
        .current_dir(&root)
        .status()
        .expect("spawn run --isolate");
    assert!(run.success(), "isolated run failed");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn decode_z_wrong_length_rejected() {
    let root = repo_root();
    let zpath = std::env::temp_dir().join(format!("vectorc-z-bad-len-{}.json", std::process::id()));
    std::fs::write(&zpath, "[1.0, 2.0, 3.0]").expect("write short z JSON");
    let out = std::env::temp_dir().join(format!("vectorc-decode-bad-{}.vcir", std::process::id()));

    let output = Command::new(vectorc())
        .args([
            "decode-z",
            "-z",
            zpath.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
            "--decoder",
            "golden",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn decode-z with wrong z length");

    let _ = std::fs::remove_file(&zpath);
    assert!(
        !output.status.success(),
        "decode-z should reject z.len() != 256: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("256") || combined.contains("Z_CONTRACT"),
        "expected length contract error, got: {combined}"
    );
}

#[test]
fn decode_z_stub_errors() {
    let root = repo_root();
    let zpath = root.join("benchmarks/fixtures/z_zeros.json");
    let out = std::env::temp_dir().join(format!("vectorc-decode-stub-{}.vcir", std::process::id()));

    let output = Command::new(vectorc())
        .args([
            "decode-z",
            "-z",
            zpath.to_str().expect("utf-8 path"),
            "-o",
            out.to_str().expect("utf-8 path"),
            "--decoder",
            "stub",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn decode-z stub");

    assert!(
        !output.status.success(),
        "stub decoder should fail (stdout={}, stderr={})",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("StubLatentDecoder"),
        "expected stub message, got: {combined}"
    );
}

#[test]
fn decode_z_golden_then_run_wasm() {
    let root = repo_root();
    let zpath = root.join("benchmarks/fixtures/z_zeros.json");
    let vcir =
        std::env::temp_dir().join(format!("vectorc-decode-golden-{}.vcir", std::process::id()));
    let wasm =
        std::env::temp_dir().join(format!("vectorc-decode-golden-{}.wasm", std::process::id()));

    let decode = Command::new(vectorc())
        .args([
            "decode-z",
            "-z",
            zpath.to_str().expect("utf-8 path"),
            "-o",
            vcir.to_str().expect("utf-8 path"),
            "--wasm-out",
            wasm.to_str().expect("utf-8 path"),
            "--decoder",
            "golden",
        ])
        .current_dir(&root)
        .status()
        .expect("spawn decode-z golden");
    assert!(decode.success(), "decode-z golden failed");

    let run = Command::new(vectorc())
        .args([
            "run",
            "-i",
            wasm.to_str().expect("utf-8 path"),
            "-e",
            "run",
            "-f",
            "100000",
            "-a",
            "40,2",
            "--expect",
            "42",
        ])
        .current_dir(&root)
        .status()
        .expect("spawn run");
    assert!(run.success(), "run decoded wasm failed");
    let _ = std::fs::remove_file(&vcir);
    let _ = std::fs::remove_file(&wasm);
}

#[test]
fn validate_invalid_fixture_json() {
    let root = repo_root();
    let input = root.join("benchmarks/conformance/invalid/return_inside_block.vcir");
    let output = Command::new(vectorc())
        .args([
            "validate",
            "-i",
            input.to_str().expect("utf-8 path"),
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn validate");

    assert!(
        !output.status.success(),
        "invalid IR should fail validate"
    );
    let report: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse validate JSON");
    assert_eq!(report["ok"], false);
    let diags = report["diagnostics"].as_array().expect("diagnostics array");
    assert_eq!(diags[0]["code"], "VCIR_CTL001");
}

#[test]
fn fix_plan_invalid_fixture() {
    let root = repo_root();
    let input = root.join("benchmarks/conformance/invalid/return_inside_block.vcir");
    let output = Command::new(vectorc())
        .args([
            "fix",
            "-i",
            input.to_str().expect("utf-8 path"),
            "--plan",
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("spawn fix --plan");

    assert!(!output.status.success());
    let report: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse fix JSON");
    assert_eq!(report["ok"], false);
    let plans = report["plans"].as_array().expect("plans");
    assert_eq!(plans[0]["code"], "VCIR_CTL001");
    assert_eq!(plans[0]["repair_id"], "move-return-to-function-end");
}

#[test]
fn explain_ctl001_json() {
    let root = repo_root();
    let output = Command::new(vectorc())
        .args(["explain", "VCIR_CTL001", "--json"])
        .current_dir(&root)
        .output()
        .expect("spawn explain");

    assert!(output.status.success());
    let entry: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse explain JSON");
    assert_eq!(entry["code"], "VCIR_CTL001");
    assert_eq!(entry["repair"]["id"], "move-return-to-function-end");
}

#[test]
fn skills_get_language() {
    let root = repo_root();
    let output = Command::new(vectorc())
        .args(["skills", "get", "language", "--json"])
        .current_dir(&root)
        .output()
        .expect("spawn skills get");

    assert!(output.status.success());
    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("parse skill JSON");
    assert_eq!(doc["name"], "language");
    assert!(doc["body"].as_str().unwrap().contains("program_ir_version"));
}

#[test]
fn synthesize_add_spec() {
    let root = repo_root();
    let spec = root.join("benchmarks/manifests/add.json");
    let out = std::env::temp_dir().join(format!("vectorc-synth-{}.vcir", std::process::id()));

    let status = Command::new(vectorc())
        .args([
            "synthesize",
            "--spec",
            spec.to_str().expect("utf-8 path"),
            "--steps",
            "200",
            "--refiner-seed",
            "1",
            "-o",
            out.to_str().expect("utf-8 path"),
        ])
        .current_dir(&root)
        .status()
        .expect("spawn synthesize");

    assert!(status.success(), "synthesize failed");
    let raw = std::fs::read_to_string(&out).expect("read synthesized IR");
    let module: vc_ir::Module = serde_json::from_str(&raw).expect("parse synthesized IR");
    vc_ir::validate_module(&module).expect("synthesized IR validates");
    let _ = std::fs::remove_file(&out);
}
