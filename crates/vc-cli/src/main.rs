mod agent;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use vc_bridge::{GoldenLatentDecoder, LatentDecoder, StubLatentDecoder, EMBEDDING_DIM};
use vc_ir::{
    instr_tree_node_count, max_control_nesting_depth, Func, FuncSig, Instr, Module, ValType,
    PROGRAM_IR_VERSION,
};
use vc_refine::{ProgramRefiner, RandomIrRefiner, Spec};
use vc_verify::{CompiledModule, MAX_WASM_BYTES};

/// Hard cap on files read via CLI (manifest / `.vcir` / `.wasm`).
const MAX_CLI_FILE_BYTES: usize = 16 * 1024 * 1024;

const MAX_BENCH_CASES: usize = 10_000;

/// Guest wall-clock budget for Wasm invocation (shared by `run`, `check`, `bench`).
#[derive(Args, Clone, Copy)]
struct GuestWallClockArgs {
    /// Guest wall-clock budget per Wasm invocation (milliseconds). Omitted = no cap
    /// (backward compatible; use for trusted first-party Wasm only).
    #[arg(long)]
    wall_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum DecodeBackendCli {
    /// Fail after validating `z` length until a learned decoder is wired.
    #[default]
    Stub,
    /// Deterministic add IR for pipeline tests (`GoldenLatentDecoder`).
    Golden,
    #[cfg(feature = "onnx")]
    /// ONNX Runtime (`ort`); `program_ir_json` → validated Program IR.
    Onnx,
}

#[derive(Args)]
struct DecodeZArgs {
    /// JSON file: `[f32, …]` or `{"z":[f32,…]}` (`z.len()` must match the decoder contract).
    #[arg(short = 'z', long)]
    z_json: PathBuf,
    /// Output Program IR JSON path.
    #[arg(short = 'o', long)]
    output: PathBuf,
    /// If set, also emit Wasm here (same lowering as `compile`).
    #[arg(long)]
    wasm_out: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = DecodeBackendCli::Stub)]
    decoder: DecodeBackendCli,
}

#[cfg(feature = "onnx")]
#[derive(Args)]
struct DecodeZOnnxArgs {
    /// Required when `--decoder onnx`.
    #[arg(long)]
    onnx_model: Option<PathBuf>,
}

#[derive(Parser)]
#[command(
    name = "vectorc",
    about = "VectorCompiler — latent / Program IR → Wasm → verify",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decode latent vector JSON → Program IR (optional Wasm); see `vc-bridge::LatentDecoder`.
    DecodeZ {
        #[command(flatten)]
        decode: DecodeZArgs,
        #[cfg(feature = "onnx")]
        #[command(flatten)]
        onnx: DecodeZOnnxArgs,
    },
    /// Validate Program IR JSON and emit a Wasm module.
    Compile {
        #[arg(short = 'i', long)]
        input: PathBuf,
        #[arg(short = 'o', long)]
        output: PathBuf,
        /// Print SHA-256 (hex) of emitted Wasm to stdout after writing.
        #[arg(long)]
        print_digest: bool,
    },
    /// Print lowercase SHA-256 hex digest of a bounded file (`.vcir`, `.wasm`, etc.).
    Digest {
        #[arg(short = 'i', long)]
        input: PathBuf,
    },
    /// Validate `.vcir` and print a structured summary for humans (authority remains `validate_module` + schema).
    Inspect {
        #[arg(short = 'i', long)]
        input: PathBuf,
        /// Emit machine-readable JSON summary on stdout (after validation).
        #[arg(long)]
        json: bool,
    },
    /// Validate Program IR only; emit structured `VCIR_*` diagnostics (`--json`).
    Validate {
        #[arg(short = 'i', long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Parse `.vcir` and summarize structure (syntax only; does not validate).
    Parse {
        #[arg(short = 'i', long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Explain a `VCIR_*` validation code, or `--all` for the full catalog.
    Explain {
        /// Diagnostic code (e.g. `VCIR_CTL001`).
        code: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Typed repair plans from validation failures (`--plan` only; does not edit files).
    Fix {
        #[arg(short = 'i', long)]
        input: PathBuf,
        /// Required today: only plan mode is implemented.
        #[arg(long)]
        plan: bool,
        #[arg(long)]
        json: bool,
    },
    /// Version-matched agent guidance bundled with the `vectorc` binary.
    Skills {
        #[command(subcommand)]
        action: SkillsCommand,
    },
    /// Run a Wasm module with fuel metering (single `i32` return).
    Run {
        #[arg(short = 'i', long)]
        wasm: PathBuf,
        #[arg(short = 'e', long, default_value = "run")]
        export: String,
        #[arg(short = 'f', long, default_value_t = 50_000)]
        fuel: u64,
        #[arg(short = 'a', long, value_delimiter = ',', allow_hyphen_values = true)]
        args: Vec<i32>,
        /// If set, exit non-zero when the result differs.
        #[arg(long)]
        expect: Option<i32>,
        /// Run Wasm compile+invoke in a subprocess via the same `vectorc` binary (host isolation).
        ///
        /// **Unix-first:** the child is placed in its own process group so a wall-clock timeout can
        /// terminate the whole group. Non-Unix hosts still subprocess but without group semantics.
        #[arg(long)]
        isolate: bool,
        /// Wall-clock budget (milliseconds) for the isolated subprocess (Wasm compile + invoke).
        #[arg(long, default_value_t = 30_000)]
        isolate_timeout_ms: u64,
        #[command(flatten)]
        guest_wall: GuestWallClockArgs,
    },
    /// Run an I/O benchmark manifest against a `.vcir` program.
    Bench {
        #[arg(short = 'm', long)]
        manifest: PathBuf,
        #[command(flatten)]
        guest_wall: GuestWallClockArgs,
    },
    /// Score a `.vcir` against a VectorBench suite (training validation oracle).
    Eval {
        #[arg(short = 'i', long)]
        input: PathBuf,
        /// Suite index JSON (`benchmarks/vectorbench_v0/suite.json`).
        #[arg(long)]
        suite: PathBuf,
        /// Run only this `task_id` from the suite (typical training path: one program, one spec).
        #[arg(long)]
        task: Option<String>,
        /// Machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        guest_wall: GuestWallClockArgs,
    },
    /// Validate, compile once, and run spec cases in-process (training / eval hot path).
    Check {
        #[arg(short = 'i', long)]
        input: PathBuf,
        /// Behavioral spec JSON (`cases` only; same shape as `synthesize --spec`).
        #[arg(long, conflicts_with = "manifest")]
        spec: Option<PathBuf>,
        /// Benchmark manifest (uses `cases`, `export`, `fuel`; ignores `program_path`).
        #[arg(short = 'm', long, conflicts_with = "spec")]
        manifest: Option<PathBuf>,
        #[arg(long)]
        export: Option<String>,
        #[arg(long)]
        fuel: Option<u64>,
        /// Machine-readable result on stdout (errors still on stderr via tracing).
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        guest_wall: GuestWallClockArgs,
    },
    /// Search for Program IR that satisfies a behavioral spec (verifier-driven).
    Synthesize {
        /// JSON spec: `{ "cases": [ { "args": [...], "expect_i32": N }, ... ] }`
        #[arg(long)]
        spec: PathBuf,
        /// Initial Program IR JSON (default: `benchmarks/programs/add.vcir` or built-in add).
        #[arg(long)]
        seed: Option<PathBuf>,
        /// Maximum refinement attempts.
        #[arg(long, default_value_t = 500)]
        steps: usize,
        /// Wasm fuel per spec case during verification.
        #[arg(long, default_value_t = 100_000)]
        fuel: u64,
        /// RNG seed for `RandomIrRefiner` (IR `--seed` is separate).
        #[arg(long, default_value_t = 0)]
        refiner_seed: u64,
        /// Write synthesized Program IR JSON here (otherwise print JSON to stdout).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum SkillsCommand {
    /// List bundled skill documents.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Print one skill (`language`, `diagnostics`, `limits`, `decoder`).
    Get {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Deserialize)]
struct BenchManifest {
    schema_version: u32,
    program_path: PathBuf,
    #[serde(default = "default_export")]
    export: String,
    #[serde(default = "default_fuel")]
    fuel: u64,
    cases: Vec<BenchCase>,
}

fn default_export() -> String {
    "run".into()
}

fn default_fuel() -> u64 {
    50_000
}

#[derive(Debug, Deserialize)]
struct BenchCase {
    args: Vec<i32>,
    expect_i32: i32,
}

/// Spec file for `synthesize` (optional `schema_version`, required `cases`).
#[derive(Debug, Deserialize)]
struct SynthesizeSpecFile {
    #[serde(default)]
    schema_version: Option<u32>,
    cases: Vec<BenchCase>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ZFile {
    Bare(Vec<f32>),
    Wrapped { z: Vec<f32> },
}

fn resolve_run_args(mut cli_args: Vec<i32>) -> Result<Vec<i32>> {
    if cli_args.is_empty() {
        if let Ok(raw) = std::env::var("VECTORC_RUN_ARGS_JSON") {
            if !raw.is_empty() {
                cli_args = serde_json::from_str(&raw)
                    .with_context(|| "parse VECTORC_RUN_ARGS_JSON (expect `[i32, …]`)")?;
            }
        }
    }
    Ok(cli_args)
}

fn parse_z_json(raw: &str) -> Result<Vec<f32>> {
    let parsed: ZFile = serde_json::from_str(raw)
        .with_context(|| "parse z JSON (expect `[f32,…]` or `{\"z\":[…]}`)")?;
    let z = match parsed {
        ZFile::Bare(v) => v,
        ZFile::Wrapped { z } => z,
    };
    anyhow::ensure!(
        z.len() == EMBEDDING_DIM,
        "z length must be exactly {EMBEDDING_DIM} (got {}) — see Z_CONTRACT",
        z.len()
    );
    Ok(z)
}

fn run_decode_z(
    args: DecodeZArgs,
    #[allow(unused_variables)] onnx_model: Option<PathBuf>,
) -> Result<()> {
    let raw = read_bounded_string(&args.z_json)?;
    let z = parse_z_json(&raw)?;
    let module = match args.decoder {
        DecodeBackendCli::Stub => StubLatentDecoder.decode_z(&z),
        DecodeBackendCli::Golden => GoldenLatentDecoder.decode_z(&z),
        #[cfg(feature = "onnx")]
        DecodeBackendCli::Onnx => {
            let path = onnx_model.context("--onnx-model is required when --decoder onnx")?;
            vc_bridge::OrtLatentDecoder::with_model_path(path).decode_z(&z)
        }
    }
    .context("latent decode_z")?;
    let json = serde_json::to_string_pretty(&module).context("serialize Program IR")?;
    fs::write(&args.output, format!("{json}\n"))
        .with_context(|| format!("write {}", args.output.display()))?;
    if let Some(wasm_path) = args.wasm_out.as_ref() {
        let wasm = vc_lower_wasm::lower_module(&module)?;
        ensure_wasm_emit_size(&wasm)?;
        fs::write(wasm_path, &wasm).with_context(|| format!("write {}", wasm_path.display()))?;
        tracing::info!(
            ir = %args.output.display(),
            wasm = %wasm_path.display(),
            "decode-z wrote IR and Wasm"
        );
    } else {
        tracing::info!(out = %args.output.display(), "decode-z wrote IR");
    }
    Ok(())
}

const DEFAULT_ADD_VCIR: &str = "benchmarks/programs/add.vcir";

fn minimal_add_seed() -> Module {
    Module {
        program_ir_version: PROGRAM_IR_VERSION,
        export_name: "run".into(),
        func: Func {
            sig: FuncSig {
                params: vec![ValType::I32, ValType::I32],
                results: vec![ValType::I32],
            },
            locals: vec![],
            body: vec![
                Instr::LocalGet { index: 0 },
                Instr::LocalGet { index: 1 },
                Instr::I32Add,
                Instr::Return,
            ],
        },
    }
}

fn load_seed_module(seed: Option<&Path>) -> Result<Module> {
    if let Some(path) = seed {
        let bytes = read_bounded(path)?;
        return Module::parse_json_slice(&bytes)
            .with_context(|| format!("parse Program IR seed {}", path.display()));
    }

    let default_path = Path::new(DEFAULT_ADD_VCIR);
    if default_path.is_file() {
        let bytes = read_bounded(default_path)?;
        match Module::parse_json_slice(&bytes) {
            Ok(module) => return Ok(module),
            Err(e) => {
                tracing::warn!(
                    path = %default_path.display(),
                    error = %e,
                    "default seed IR invalid; using built-in add template"
                );
            }
        }
    }

    Ok(minimal_add_seed())
}

#[derive(Debug, Clone, Serialize)]
struct CheckSummary {
    parse_ok: bool,
    validate_ok: bool,
    compile_ok: bool,
    run_ok: bool,
    cases_passed: usize,
    cases_total: usize,
    ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VectorbenchSuite {
    schema_version: u32,
    suite_id: String,
    tasks: Vec<VectorbenchTask>,
}

#[derive(Debug, Deserialize)]
struct VectorbenchTask {
    task_id: String,
    manifest: PathBuf,
}

#[derive(Debug, Serialize)]
struct EvalMetrics {
    tasks_total: usize,
    tasks_passed: usize,
    validate_rate: f64,
    compile_rate: f64,
    execute_rate: f64,
}

#[derive(Debug, Serialize)]
struct EvalSummary {
    suite_id: String,
    vcir: String,
    tasks: BTreeMap<String, CheckSummary>,
    metrics: EvalMetrics,
    ok: bool,
}

fn ensure_wasm_emit_size(wasm: &[u8]) -> Result<()> {
    anyhow::ensure!(
        wasm.len() <= MAX_WASM_BYTES,
        "lowered Wasm exceeds limit ({} bytes, max {})",
        wasm.len(),
        MAX_WASM_BYTES
    );
    Ok(())
}

fn load_bench_manifest(path: &Path) -> Result<BenchManifest> {
    let raw = read_bounded_string(path)?;
    let m: BenchManifest =
        serde_json::from_str(&raw).with_context(|| "parse benchmark manifest JSON")?;
    anyhow::ensure!(
        m.schema_version == 1,
        "unsupported benchmark schema_version {}",
        m.schema_version
    );
    Ok(m)
}

fn load_check_cases(
    spec: Option<&Path>,
    manifest: Option<&Path>,
) -> Result<(String, u64, Vec<BenchCase>)> {
    match (spec, manifest) {
        (Some(spec_path), None) => {
            let behavioral = load_synthesize_spec(spec_path)?;
            ensure_spec_limits(&behavioral)?;
            let cases = behavioral
                .cases
                .into_iter()
                .map(|c| BenchCase {
                    args: c.args,
                    expect_i32: c.expect_i32,
                })
                .collect();
            Ok(("run".into(), 100_000, cases))
        }
        (None, Some(manifest_path)) => {
            let m = load_bench_manifest(manifest_path)?;
            ensure_case_limits(&m.cases)?;
            Ok((m.export, m.fuel, m.cases))
        }
        (Some(_), Some(_)) => anyhow::bail!("provide only one of --spec or --manifest"),
        (None, None) => anyhow::bail!("check requires --spec or --manifest"),
    }
}

fn evaluate_vcir(
    input: &Path,
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
        errors: Vec::new(),
    };

    let bytes = match read_bounded(input) {
        Ok(b) => b,
        Err(e) => {
            summary
                .errors
                .push(format!("read {}: {e:#}", input.display()));
            return summary;
        }
    };

    let module = match vc_ir::Module::parse_json_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            summary
                .errors
                .push(format!("parse Program IR {}: {e}", input.display()));
            return summary;
        }
    };
    summary.parse_ok = true;

    if let Err(e) = vc_ir::validate_module(&module) {
        summary.errors.push(format!("validate_module: {e}"));
        return summary;
    }
    summary.validate_ok = true;

    let wasm = match vc_lower_wasm::lower_module(&module) {
        Ok(w) => w,
        Err(e) => {
            summary.errors.push(format!("lower: {e:#}"));
            return summary;
        }
    };

    if let Err(e) = ensure_wasm_emit_size(&wasm) {
        summary.errors.push(format!("{e:#}"));
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

    let limits = vc_verify::Limits { fuel, max_wall_ms };

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
            Ok(got) => summary
                .errors
                .push(format!("case {i}: expected {}, got {got}", case.expect_i32)),
            Err(e) => summary.errors.push(format!("case {i}: {e:#}")),
        }
    }

    summary.run_ok = summary.cases_passed == summary.cases_total && cases_total > 0;
    summary.ok = summary.validate_ok && summary.compile_ok && summary.run_ok;
    summary
}

fn finish_check(summary: &CheckSummary, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(summary)?);
    } else if summary.ok {
        println!(
            "{} / {} cases passed",
            summary.cases_passed, summary.cases_total
        );
    }
    if summary.ok {
        Ok(())
    } else {
        anyhow::bail!("check failed")
    }
}

fn run_check(
    input: &Path,
    spec: Option<&Path>,
    manifest: Option<&Path>,
    export_override: Option<&str>,
    fuel_override: Option<u64>,
    max_wall_ms: Option<u64>,
    json: bool,
) -> Result<()> {
    let (default_export, default_fuel, cases) = load_check_cases(spec, manifest)?;
    let export = export_override.unwrap_or(&default_export);
    let fuel = fuel_override.unwrap_or(default_fuel);
    let summary = evaluate_vcir(input, export, fuel, max_wall_ms, &cases);
    if summary.ok {
        tracing::info!(export, fuel, cases_total = summary.cases_total, "check OK");
    } else {
        for err in &summary.errors {
            tracing::warn!(%err, "check case error");
        }
    }
    finish_check(&summary, json)
}

fn load_vectorbench_suite(path: &Path) -> Result<VectorbenchSuite> {
    let raw = read_bounded_string(path)?;
    let suite: VectorbenchSuite =
        serde_json::from_str(&raw).with_context(|| format!("parse suite {}", path.display()))?;
    anyhow::ensure!(
        suite.schema_version == 1,
        "unsupported vectorbench schema_version {}",
        suite.schema_version
    );
    anyhow::ensure!(!suite.tasks.is_empty(), "suite has no tasks");
    Ok(suite)
}

fn resolve_suite_manifest(suite_path: &Path, manifest: &Path) -> Result<PathBuf> {
    let suite_dir = suite_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("suite path must live in a directory")?;
    let repo_root = suite_dir
        .parent()
        .and_then(|p| p.parent())
        .context("suite path must be under benchmarks/<name>/suite.json")?;
    if manifest.is_absolute() {
        return Ok(manifest.to_path_buf());
    }
    let candidate = repo_root.join(manifest);
    if candidate.is_file() {
        return Ok(candidate);
    }
    let alt = suite_dir.join(manifest);
    anyhow::ensure!(
        alt.is_file(),
        "manifest not found: {} (also tried {})",
        candidate.display(),
        alt.display()
    );
    Ok(alt)
}

fn run_eval(
    input: &Path,
    suite_path: &Path,
    task_filter: Option<&str>,
    max_wall_ms: Option<u64>,
    json: bool,
) -> Result<()> {
    let suite = load_vectorbench_suite(suite_path)?;
    let selected: Vec<&VectorbenchTask> = if let Some(id) = task_filter {
        let found: Vec<_> = suite.tasks.iter().filter(|t| t.task_id == id).collect();
        anyhow::ensure!(
            !found.is_empty(),
            "suite `{}` has no task_id `{id}`",
            suite.suite_id
        );
        found
    } else {
        suite.tasks.iter().collect()
    };

    let mut tasks_out = BTreeMap::new();
    let mut tasks_passed = 0usize;
    let mut validate_ok_n = 0usize;
    let mut compile_ok_n = 0usize;
    let mut cases_passed = 0usize;
    let mut cases_total = 0usize;

    for task in selected {
        let manifest_path = resolve_suite_manifest(suite_path, &task.manifest)?;
        let m = load_bench_manifest(&manifest_path)?;
        ensure_case_limits(&m.cases)?;
        let summary = evaluate_vcir(input, &m.export, m.fuel, max_wall_ms, &m.cases);
        if summary.ok {
            tasks_passed += 1;
        }
        if summary.validate_ok {
            validate_ok_n += 1;
        }
        if summary.compile_ok {
            compile_ok_n += 1;
        }
        cases_passed += summary.cases_passed;
        cases_total += summary.cases_total;
        tasks_out.insert(task.task_id.clone(), summary);
    }

    let n = tasks_out.len();
    let rate = |num: usize, den: usize| {
        if den == 0 {
            0.0
        } else {
            num as f64 / den as f64
        }
    };

    let eval = EvalSummary {
        suite_id: suite.suite_id.clone(),
        vcir: input.display().to_string(),
        tasks: tasks_out,
        metrics: EvalMetrics {
            tasks_total: n,
            tasks_passed,
            validate_rate: rate(validate_ok_n, n),
            compile_rate: rate(compile_ok_n, n),
            execute_rate: rate(cases_passed, cases_total),
        },
        ok: tasks_passed == n,
    };

    if json {
        println!("{}", serde_json::to_string(&eval)?);
    } else {
        println!(
            "vectorbench {}: {}/{} tasks, execute_rate {:.3} ({}/{})",
            eval.suite_id,
            eval.metrics.tasks_passed,
            eval.metrics.tasks_total,
            eval.metrics.execute_rate,
            cases_passed,
            cases_total
        );
    }

    if eval.ok {
        Ok(())
    } else {
        anyhow::bail!("eval failed")
    }
}

fn load_synthesize_spec(path: &Path) -> Result<Spec> {
    let raw = read_bounded_string(path)?;
    let file: SynthesizeSpecFile =
        serde_json::from_str(&raw).with_context(|| format!("parse spec {}", path.display()))?;
    if let Some(v) = file.schema_version {
        anyhow::ensure!(
            v == 1,
            "unsupported synthesize spec schema_version {v} (only 1 supported)"
        );
    }
    Ok(Spec {
        cases: file
            .cases
            .into_iter()
            .map(|c| vc_refine::SpecCase {
                args: c.args,
                expect_i32: c.expect_i32,
            })
            .collect(),
    })
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = Vec::new();
    file.take((MAX_CLI_FILE_BYTES as u64) + 1)
        .read_to_end(&mut buf)
        .with_context(|| format!("read {}", path.display()))?;
    if buf.len() > MAX_CLI_FILE_BYTES {
        anyhow::bail!(
            "file exceeds max size ({}, limit {})",
            path.display(),
            MAX_CLI_FILE_BYTES
        );
    }
    Ok(buf)
}

fn read_bounded_string(path: &Path) -> Result<String> {
    let bytes = read_bounded(path)?;
    String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", path.display()))
}

/// `program_path` must stay within the benchmark suite root (typically `benchmarks/`).
fn assert_benchmark_program_safe(program_path: &Path) -> Result<()> {
    if program_path.is_absolute() {
        anyhow::bail!("program_path must be relative");
    }
    for c in program_path.components() {
        match c {
            Component::ParentDir => anyhow::bail!("program_path must not contain `..`"),
            Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("program_path contains invalid components")
            }
            _ => {}
        }
    }
    Ok(())
}

fn ensure_case_limits(cases: &[BenchCase]) -> Result<()> {
    anyhow::ensure!(!cases.is_empty(), "spec must contain at least one case");
    anyhow::ensure!(
        cases.len() <= MAX_BENCH_CASES,
        "too many cases ({} > {})",
        cases.len(),
        MAX_BENCH_CASES
    );
    for (i, case) in cases.iter().enumerate() {
        anyhow::ensure!(
            case.args.len() <= vc_ir::MAX_PARAMS,
            "case {i}: too many args ({} > {})",
            case.args.len(),
            vc_ir::MAX_PARAMS
        );
    }
    Ok(())
}

/// Copy Wasm into a fresh temp file after bounded read in the parent, then spawn `vectorc run`
/// without `--isolate` so compilation and invocation stay out-of-process.
fn run_isolated_subprocess(
    wasm: &Path,
    export: &str,
    fuel: u64,
    args: &[i32],
    expect: Option<i32>,
    isolate_timeout_ms: u64,
    max_wall_ms: Option<u64>,
) -> Result<()> {
    let bytes = read_bounded(wasm)?;
    let tmp_path = std::env::temp_dir().join(format!(
        "vectorc-isolate-{}-{}.wasm",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp_path, &bytes)
        .with_context(|| format!("write isolated wasm {}", tmp_path.display()))?;

    struct TmpFile(PathBuf);
    impl Drop for TmpFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _tmp = TmpFile(tmp_path.clone());

    let exe = std::env::current_exe().context("resolve current executable for isolate")?;
    let mut cmd = ProcessCommand::new(&exe);
    cmd.arg("run");
    cmd.arg("-i").arg(&tmp_path);
    cmd.arg("-e").arg(export);
    cmd.arg("-f").arg(fuel.to_string());
    if !args.is_empty() {
        cmd.arg("-a").arg(
            args.iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if let Some(e) = expect {
        cmd.arg("--expect").arg(e.to_string());
    }
    if let Some(ms) = max_wall_ms {
        cmd.arg("--wall-ms").arg(ms.to_string());
    }
    cmd.env_remove("VECTORC_RUN_ARGS_JSON");
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn `{}` run (isolate)", exe.display()))?;

    wait_child_wall_clock(&mut child, Duration::from_millis(isolate_timeout_ms))
}

fn wait_child_wall_clock(child: &mut std::process::Child, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        match child.try_wait().context("poll isolated subprocess")? {
            Some(status) => {
                if status.success() {
                    return Ok(());
                }
                std::process::exit(status.code().unwrap_or(1));
            }
            None => {
                if start.elapsed() >= timeout {
                    terminate_isolated_child(child)?;
                    anyhow::bail!(
                        "isolated subprocess exceeded wall-clock timeout ({timeout:?}); child was terminated"
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn terminate_isolated_child(child: &mut std::process::Child) -> Result<()> {
    let pid = child.id();
    #[cfg(unix)]
    {
        unsafe {
            let rc = libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            if rc != 0 {
                let err = std::io::Error::last_os_error();
                tracing::warn!(%pid, %err, "kill(-pid, SIGKILL) failed; falling back to child.kill()");
                child
                    .kill()
                    .context("kill isolated subprocess after timeout")?;
            }
        }
    }
    #[cfg(not(unix))]
    {
        child
            .kill()
            .context("kill isolated subprocess after timeout")?;
    }
    let _ = child.wait();
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn valtype_label(t: ValType) -> &'static str {
    match t {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
    }
}

fn walk_op_histogram(instrs: &[Instr], counts: &mut BTreeMap<&'static str, usize>) {
    for instr in instrs {
        let tag = match instr {
            Instr::I32Const { .. } => "i32_const",
            Instr::I64Const { .. } => "i64_const",
            Instr::F32Const { .. } => "f32_const",
            Instr::F64Const { .. } => "f64_const",
            Instr::I32Add => "i32_add",
            Instr::I32Sub => "i32_sub",
            Instr::I32Mul => "i32_mul",
            Instr::I32Xor => "i32_xor",
            Instr::I32Eq => "i32_eq",
            Instr::I32Eqz => "i32_eqz",
            Instr::I32TruncF32S => "i32_trunc_f32_s",
            Instr::F32ConvertI32S => "f32_convert_i32_s",
            Instr::F32Add => "f32_add",
            Instr::F32Sub => "f32_sub",
            Instr::F32Mul => "f32_mul",
            Instr::F32Div => "f32_div",
            Instr::F32Gt => "f32_gt",
            Instr::F64Add => "f64_add",
            Instr::F64Sub => "f64_sub",
            Instr::F64Mul => "f64_mul",
            Instr::F64Div => "f64_div",
            Instr::Drop => "drop",
            Instr::LocalGet { .. } => "local_get",
            Instr::LocalSet { .. } => "local_set",
            Instr::Block { body, .. } => {
                walk_op_histogram(body, counts);
                "block"
            }
            Instr::IfElse {
                then_body,
                else_body,
                ..
            } => {
                walk_op_histogram(then_body, counts);
                walk_op_histogram(else_body, counts);
                "if_else"
            }
            Instr::Return => "return",
        };
        *counts.entry(tag).or_insert(0) += 1;
    }
}

#[derive(Serialize)]
struct InspectJsonSummary {
    export_name: String,
    program_ir_version: u32,
    params: Vec<String>,
    results: Vec<String>,
    instruction_tree_nodes: usize,
    max_control_nesting_depth: usize,
    opcode_histogram: BTreeMap<String, usize>,
}

fn inspect_summary(module: &Module) -> InspectJsonSummary {
    let sig = &module.func.sig;
    let params: Vec<String> = sig
        .params
        .iter()
        .copied()
        .map(|t| valtype_label(t).to_string())
        .collect();
    let results: Vec<String> = sig
        .results
        .iter()
        .copied()
        .map(|t| valtype_label(t).to_string())
        .collect();
    let mut hist = BTreeMap::new();
    walk_op_histogram(&module.func.body, &mut hist);
    let opcode_histogram = hist.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    InspectJsonSummary {
        export_name: module.export_name.clone(),
        program_ir_version: module.program_ir_version,
        params,
        results,
        instruction_tree_nodes: instr_tree_node_count(&module.func.body),
        max_control_nesting_depth: max_control_nesting_depth(&module.func.body),
        opcode_histogram,
    }
}

fn run_program_ir_inspect(path: &Path, json: bool) -> Result<()> {
    let bytes = read_bounded(path)?;
    let module = Module::parse_json_slice(&bytes)
        .with_context(|| format!("parse Program IR {}", path.display()))?;
    vc_ir::validate_module(&module).context("validate_module failed")?;

    if json {
        let summary = inspect_summary(&module);
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).context("serialize inspect JSON")?
        );
        tracing::info!(path = %path.display(), "inspect OK (json)");
        return Ok(());
    }

    let sig = &module.func.sig;
    let params: Vec<&str> = sig.params.iter().copied().map(valtype_label).collect();
    let results: Vec<&str> = sig.results.iter().copied().map(valtype_label).collect();
    let locals: Vec<&str> = module
        .func
        .locals
        .iter()
        .copied()
        .map(valtype_label)
        .collect();

    println!("=== Program IR inspect (human summary — not authoritative) ===");
    println!(
        "source_file: {}",
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
    );
    println!("program_ir_version (file): {}", module.program_ir_version);
    println!("program_ir_version (crate): {PROGRAM_IR_VERSION}");
    println!("export_name: {}", module.export_name);
    println!("params: {}", params.join(", "));
    println!("results: {}", results.join(", "));
    println!(
        "declared_locals ({}): {}",
        locals.len(),
        if locals.is_empty() {
            "(none)".into()
        } else {
            locals.join(", ")
        }
    );
    println!(
        "instruction_tree_nodes: {}",
        instr_tree_node_count(&module.func.body)
    );
    println!(
        "max_control_nesting_depth: {}",
        max_control_nesting_depth(&module.func.body)
    );

    let mut hist = BTreeMap::new();
    walk_op_histogram(&module.func.body, &mut hist);
    println!("opcode_histogram:");
    for (k, v) in &hist {
        println!("  {k}: {v}");
    }
    println!("validate_module: OK");
    tracing::info!(path = %path.display(), "inspect OK");
    Ok(())
}

fn ensure_spec_limits(spec: &Spec) -> Result<()> {
    anyhow::ensure!(
        !spec.cases.is_empty(),
        "spec must contain at least one case"
    );
    anyhow::ensure!(
        spec.cases.len() <= MAX_BENCH_CASES,
        "too many cases ({} > {})",
        spec.cases.len(),
        MAX_BENCH_CASES
    );
    for (i, case) in spec.cases.iter().enumerate() {
        anyhow::ensure!(
            case.args.len() <= vc_ir::MAX_PARAMS,
            "case {i}: too many args ({} > {})",
            case.args.len(),
            vc_ir::MAX_PARAMS
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::DecodeZ {
            decode,
            #[cfg(feature = "onnx")]
            onnx,
        } => {
            #[cfg(feature = "onnx")]
            let onnx_model = onnx.onnx_model;
            #[cfg(not(feature = "onnx"))]
            let onnx_model: Option<PathBuf> = None;
            run_decode_z(decode, onnx_model)?;
        }
        Command::Compile {
            input,
            output,
            print_digest,
        } => {
            let bytes = read_bounded(&input)?;
            let module = vc_ir::Module::parse_json_slice(&bytes)
                .with_context(|| format!("parse Program IR {}", input.display()))?;
            // `lower_module` validates internally; keep single validation boundary there.
            let wasm = vc_lower_wasm::lower_module(&module)?;
            ensure_wasm_emit_size(&wasm)?;
            fs::write(&output, &wasm).with_context(|| format!("write {}", output.display()))?;
            tracing::info!(out = %output.display(), "wrote Wasm module");
            if print_digest {
                println!("{}", sha256_hex(&wasm));
            }
        }
        Command::Digest { input } => {
            let bytes = read_bounded(&input)?;
            println!("{}", sha256_hex(&bytes));
        }
        Command::Inspect { input, json } => run_program_ir_inspect(&input, json)?,
        Command::Validate { input, json } => {
            let bytes = read_bounded(&input)?;
            agent::run_validate(&input, &bytes, json)?;
        }
        Command::Parse { input, json } => {
            let bytes = read_bounded(&input)?;
            agent::run_parse(&input, &bytes, json)?;
        }
        Command::Explain { code, all, json } => {
            if all {
                agent::run_explain_all(json)?;
            } else {
                let code = code.context("explain requires CODE or pass --all")?;
                agent::run_explain(&code, json)?;
            }
        }
        Command::Fix { input, plan, json } => {
            anyhow::ensure!(
                plan,
                "fix requires --plan (file edits are not implemented; use synthesize or an external agent)"
            );
            let bytes = read_bounded(&input)?;
            agent::run_fix_plan(&input, &bytes, json)?;
        }
        Command::Skills { action } => match action {
            SkillsCommand::List { json } => agent::run_skills_list(json)?,
            SkillsCommand::Get { name, json } => agent::run_skills_get(&name, json)?,
        },
        Command::Run {
            wasm,
            export,
            fuel,
            args,
            expect,
            isolate,
            isolate_timeout_ms,
            guest_wall,
        } => {
            let args = resolve_run_args(args)?;
            let max_wall_ms = guest_wall.wall_ms;
            if isolate {
                run_isolated_subprocess(
                    &wasm,
                    &export,
                    fuel,
                    &args,
                    expect,
                    isolate_timeout_ms,
                    max_wall_ms,
                )?;
                tracing::info!(export, fuel, isolate_timeout_ms, "isolated wasm finished");
            } else {
                let bytes = read_bounded(&wasm)?;
                let got = vc_verify::invoke_i32_return(
                    &bytes,
                    &export,
                    &args,
                    vc_verify::Limits { fuel, max_wall_ms },
                )?;
                tracing::info!(got, export, fuel, "wasm finished");
                if let Some(exp) = expect {
                    anyhow::ensure!(got == exp, "expected {exp}, got {got}");
                }
                println!("{got}");
            }
        }
        Command::Bench {
            manifest,
            guest_wall,
        } => {
            let mdir = manifest
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .with_context(|| "manifest path must live in a directory")?;
            // Parent of manifests/ → suite root (`benchmarks/`).
            let suite_root = mdir.parent().with_context(|| "manifest path too shallow")?;

            let raw = read_bounded_string(&manifest)?;
            let m: BenchManifest =
                serde_json::from_str(&raw).with_context(|| "parse benchmark manifest JSON")?;
            anyhow::ensure!(
                m.schema_version == 1,
                "unsupported benchmark schema_version {}",
                m.schema_version
            );

            assert_benchmark_program_safe(&m.program_path)?;

            let prog_path = suite_root.join(&m.program_path);
            let suite_canon = fs::canonicalize(suite_root)
                .with_context(|| format!("canonicalize {}", suite_root.display()))?;
            let prog_canon = fs::canonicalize(&prog_path)
                .with_context(|| format!("canonicalize {}", prog_path.display()))?;
            if prog_canon.strip_prefix(&suite_canon).is_err() {
                anyhow::bail!(
                    "program_path `{}` escapes benchmark suite `{}` after resolution",
                    prog_path.display(),
                    suite_canon.display()
                );
            }

            let prog_bytes = read_bounded(&prog_canon)?;
            let module = vc_ir::Module::parse_json_slice(&prog_bytes)
                .with_context(|| format!("parse {}", prog_canon.display()))?;
            let wasm = vc_lower_wasm::lower_module(&module)?;

            ensure_case_limits(&m.cases)?;

            let compiled = CompiledModule::new(&wasm)?;
            let limits = vc_verify::Limits {
                fuel: m.fuel,
                max_wall_ms: guest_wall.wall_ms,
            };
            let mut session = compiled
                .prepare_invoke(&m.export)
                .context("prepare benchmark invoke session")?;
            let mut passed = 0usize;
            for (i, case) in m.cases.iter().enumerate() {
                let got = session
                    .invoke_i32_return(&case.args, limits)
                    .with_context(|| format!("case {i} execute"))?;
                anyhow::ensure!(
                    got == case.expect_i32,
                    "case {i}: expected {}, got {}",
                    case.expect_i32,
                    got
                );
                passed += 1;
            }
            tracing::info!(passed, total = m.cases.len(), "benchmark OK");
            println!("{} / {} cases passed", passed, m.cases.len());
        }
        Command::Eval {
            input,
            suite,
            task,
            json,
            guest_wall,
        } => run_eval(&input, &suite, task.as_deref(), guest_wall.wall_ms, json)?,
        Command::Check {
            input,
            spec,
            manifest,
            export,
            fuel,
            json,
            guest_wall,
        } => run_check(
            &input,
            spec.as_deref(),
            manifest.as_deref(),
            export.as_deref(),
            fuel,
            guest_wall.wall_ms,
            json,
        )?,
        Command::Synthesize {
            spec,
            seed,
            steps,
            fuel,
            refiner_seed,
            output,
        } => {
            let behavioral = load_synthesize_spec(&spec)?;
            ensure_spec_limits(&behavioral)?;

            let initial = load_seed_module(seed.as_deref())?;
            vc_ir::validate_module(&initial).context("initial seed IR failed validation")?;
            let refiner = RandomIrRefiner::new(refiner_seed);
            let refined = refiner
                .refine(&initial, &behavioral, fuel, steps)
                .context("synthesis failed: no module satisfied spec within step budget")?;

            let json = serde_json::to_string_pretty(&refined)
                .context("serialize synthesized Program IR")?;

            if let Some(out) = output {
                fs::write(&out, format!("{json}\n"))
                    .with_context(|| format!("write {}", out.display()))?;
                println!("{}", out.display());
            } else {
                println!("{json}");
            }
            tracing::info!(cases = behavioral.cases.len(), steps, fuel, "synthesis OK");
        }
    }

    Ok(())
}
