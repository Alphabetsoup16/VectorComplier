//! Bounded CEGIS-style repair loop: check → diagnostic/fix-plan → optional synthesize.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use vc_ir::{fix_plan, validate_module, Module, ValidateReport};
use vc_refine::{ProgramRefiner, RandomIrRefiner, Spec};

use crate::oracle::{evaluate_vcir_path, CheckSummary, FailedCaseDetail};

#[derive(Debug, Clone, Serialize)]
pub struct RepairStep {
    pub step: usize,
    pub check: CheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_report: Option<ValidateReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_plan: Option<vc_ir::FixPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<FailedCaseDetail>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesize_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRepairReport {
    pub ok: bool,
    pub source: String,
    pub steps_taken: usize,
    pub max_steps: usize,
    pub steps: Vec<RepairStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
}

pub struct AgentRepairOptions {
    pub max_steps: usize,
    pub synthesize: bool,
    pub synthesize_steps: usize,
    pub refiner_seed: u64,
    pub fuel: u64,
    pub max_wall_ms: Option<u64>,
    pub json: bool,
    pub write_output: Option<PathBuf>,
}

pub fn run_agent_repair(
    input: &Path,
    export: &str,
    cases: &[crate::oracle::BenchCase],
    behavioral: &Spec,
    opts: AgentRepairOptions,
) -> Result<()> {
    anyhow::ensure!(opts.max_steps > 0, "max_steps must be at least 1");
    anyhow::ensure!(
        !cases.is_empty(),
        "repair requires at least one spec case (--spec or --manifest)"
    );

    let mut working = input.to_path_buf();
    let mut steps = Vec::new();
    let mut ok = false;

    for step_idx in 0..opts.max_steps {
        let check = evaluate_vcir_path(
            &working,
            export,
            opts.fuel,
            opts.max_wall_ms,
            cases,
        );

        if check.ok {
            ok = true;
            steps.push(RepairStep {
                step: step_idx,
                check,
                validation_code: None,
                validate_report: None,
                fix_plan: None,
                counterexample: None,
                action: "pass".into(),
                synthesize_ok: None,
            });
            break;
        }

        let validation_code = check.validation_code.clone();
        let counterexample = check.failed_case.clone();
        let mut action = String::from("report");
        let mut validate_report = None;
        let mut plan = None;
        let mut synthesize_ok = None;

        let bytes = fs::read(&working)
            .with_context(|| format!("read working IR {}", working.display()))?;
        if let Ok(module) = Module::parse_json_slice(&bytes) {
            validate_report = Some(vc_ir::validate_report(&module));
            if let Err(e) = validate_module(&module) {
                plan = Some(fix_plan(&e));
            }
        }

        if opts.synthesize
            && check.parse_ok
            && check.validate_ok
            && !check.run_ok
            && step_idx + 1 < opts.max_steps
        {
            action = "synthesize".into();
            let seed_bytes = fs::read(&working)?;
            let initial = Module::parse_json_slice(&seed_bytes)
                .context("parse seed for synthesize step")?;
            validate_module(&initial).context("seed IR invalid before synthesize")?;
            let refiner = RandomIrRefiner::new(opts.refiner_seed.wrapping_add(step_idx as u64));
            match refiner.refine(&initial, behavioral, opts.fuel, opts.synthesize_steps) {
                Ok(refined) => {
                    let json = serde_json::to_string_pretty(&refined)?;
                    let tmp = std::env::temp_dir().join(format!(
                        "vectorc-repair-{}-{}.vcir",
                        std::process::id(),
                        step_idx
                    ));
                    fs::write(&tmp, format!("{json}\n"))?;
                    working = tmp;
                    synthesize_ok = Some(true);
                }
                Err(e) => {
                    tracing::warn!(error = %e, step = step_idx, "synthesize step failed");
                    synthesize_ok = Some(false);
                }
            }
        }

        steps.push(RepairStep {
            step: step_idx,
            check,
            validation_code,
            validate_report,
            fix_plan: plan,
            counterexample,
            action,
            synthesize_ok,
        });
    }

    let output_path = if ok {
        if let Some(out) = opts.write_output {
            fs::copy(&working, &out).with_context(|| format!("write {}", out.display()))?;
            Some(out.display().to_string())
        } else if working != *input {
            fs::copy(&working, input).with_context(|| format!("update {}", input.display()))?;
            Some(input.display().to_string())
        } else {
            None
        }
    } else {
        None
    };

    let report = AgentRepairReport {
        ok,
        source: input.display().to_string(),
        steps_taken: steps.len(),
        max_steps: opts.max_steps,
        steps,
        output_path,
    };

    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serialize agent-repair report")?
        );
    } else if ok {
        println!(
            "agent-repair: OK in {} step(s) ({})",
            report.steps_taken,
            input.display()
        );
    } else {
        println!(
            "agent-repair: failed after {} step(s) ({})",
            report.steps_taken,
            input.display()
        );
        if let Some(last) = report.steps.last() {
            if let Some(code) = &last.validation_code {
                println!("  validation: {code}");
            }
            if let Some(cx) = &last.counterexample {
                println!(
                    "  counterexample: args={:?} expect={}",
                    cx.args, cx.expect_i32
                );
            }
        }
    }

    if ok {
        Ok(())
    } else {
        anyhow::bail!("agent-repair did not satisfy spec within step budget")
    }
}
