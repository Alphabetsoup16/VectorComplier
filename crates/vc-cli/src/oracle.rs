//! In-process Program IR oracle: parse → validate → lower → Wasm execute (spec cases).

use std::path::Path;

/// Hard cap on files read via CLI (manifest / `.vcir` / `.wasm`).
pub const MAX_CLI_FILE_BYTES: usize = 16 * 1024 * 1024;

use serde::Serialize;
use vc_ir::{validate_module, Module, ValidationError};
use vc_lower_wasm::lower_module;
use vc_verify::{CompiledModule, Limits, MAX_WASM_BYTES};


#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct BenchCase {
    pub args: Vec<i32>,
    pub expect_i32: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckSummary {
    pub parse_ok: bool,
    pub validate_ok: bool,
    pub compile_ok: bool,
    pub run_ok: bool,
    pub cases_passed: usize,
    pub cases_total: usize,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_case_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_case: Option<FailedCaseDetail>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailedCaseDetail {
    pub args: Vec<i32>,
    pub expect_i32: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub got_i32: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke_error: Option<String>,
}

pub fn read_bounded(path: &Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;

    let f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    f.take((MAX_CLI_FILE_BYTES as u64) + 1)
        .read_to_end(&mut buf)?;
    anyhow::ensure!(
        buf.len() <= MAX_CLI_FILE_BYTES,
        "file {} exceeds max size ({} bytes, limit {})",
        path.display(),
        buf.len(),
        MAX_CLI_FILE_BYTES
    );
    Ok(buf)
}

pub fn evaluate_vcir_bytes(
    bytes: &[u8],
    source_label: &str,
    export: &str,
    fuel: u64,
    max_wall_ms: Option<u64>,
    cases: &[BenchCase],
) -> CheckSummary {
    let cases_total = cases.len();
    let mut summary = CheckSummary {
        parse_ok: false,
        validate_ok: false,
        compile_ok: false,
        run_ok: false,
        cases_passed: 0,
        cases_total,
        ok: false,
        validation_code: None,
        failed_case_index: None,
        failed_case: None,
        errors: Vec::new(),
    };

    let module = match Module::parse_json_slice(bytes) {
        Ok(m) => m,
        Err(e) => {
            summary
                .errors
                .push(format!("parse Program IR {source_label}: {e}"));
            return summary;
        }
    };
    summary.parse_ok = true;

    if let Err(e) = validate_module(&module) {
        record_validation_failure(&mut summary, &e);
        return summary;
    }
    summary.validate_ok = true;

    let wasm = match lower_module(&module) {
        Ok(w) => w,
        Err(e) => {
            if let vc_lower_wasm::LowerError::Validation(ve) = &e {
                record_validation_failure(&mut summary, ve);
            } else {
                summary.errors.push(format!("lower: {e:#}"));
            }
            return summary;
        }
    };

    if wasm.len() > MAX_WASM_BYTES {
        summary
            .errors
            .push(format!("wasm size {} exceeds max {}", wasm.len(), MAX_WASM_BYTES));
        return summary;
    }
    summary.compile_ok = true;

    let compiled = match CompiledModule::new(&wasm) {
        Ok(c) => c,
        Err(e) => {
            summary.errors.push(format!("instantiate: {e:#}"));
            return summary;
        }
    };

    let limits = Limits { fuel, max_wall_ms };

    let mut session = match compiled.prepare_invoke(export) {
        Ok(s) => s,
        Err(e) => {
            summary.errors.push(format!("prepare invoke: {e:#}"));
            return summary;
        }
    };

    for (i, case) in cases.iter().enumerate() {
        match session.invoke_i32_return(&case.args, limits) {
            Ok(got) if got == case.expect_i32 => summary.cases_passed += 1,
            Ok(got) => {
                summary.failed_case_index = Some(i);
                summary.failed_case = Some(FailedCaseDetail {
                    args: case.args.clone(),
                    expect_i32: case.expect_i32,
                    got_i32: Some(got),
                    invoke_error: None,
                });
                summary.errors.push(format!(
                    "case {i}: expected {}, got {got}",
                    case.expect_i32
                ));
            }
            Err(e) => {
                summary.failed_case_index = Some(i);
                summary.failed_case = Some(FailedCaseDetail {
                    args: case.args.clone(),
                    expect_i32: case.expect_i32,
                    got_i32: None,
                    invoke_error: Some(format!("{e:#}")),
                });
                summary.errors.push(format!("case {i}: {e:#}"));
            }
        }
    }

    summary.run_ok = summary.cases_passed == summary.cases_total && cases_total > 0;
    summary.ok = summary.validate_ok && summary.compile_ok && summary.run_ok;
    summary
}

pub fn evaluate_vcir_path(
    input: &Path,
    export: &str,
    fuel: u64,
    max_wall_ms: Option<u64>,
    cases: &[BenchCase],
) -> CheckSummary {
    match read_bounded(input) {
        Ok(bytes) => evaluate_vcir_bytes(
            &bytes,
            &input.display().to_string(),
            export,
            fuel,
            max_wall_ms,
            cases,
        ),
        Err(e) => CheckSummary {
            parse_ok: false,
            validate_ok: false,
            compile_ok: false,
            run_ok: false,
            cases_passed: 0,
            cases_total: cases.len(),
            ok: false,
            validation_code: None,
            failed_case_index: None,
            failed_case: None,
            errors: vec![format!("read {}: {e:#}", input.display())],
        },
    }
}

fn record_validation_failure(summary: &mut CheckSummary, e: &ValidationError) {
    summary.validation_code = Some(e.code().as_str().to_string());
    summary.errors.push(format!("validate_module: {e}"));
}
