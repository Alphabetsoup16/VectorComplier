//! Agent-facing compiler commands (structured diagnostics, skills, repair plans).

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use vc_ir::{
    explain_code, fix_plan, parse_summary, validate_module, validate_report, ExplainEntry,
    FixPlan, Module, ParseSummary, ValidateReport,
};

const SKILL_LANGUAGE: &str = include_str!("../../../skills/language.md");
const SKILL_DIAGNOSTICS: &str = include_str!("../../../skills/diagnostics.md");
const SKILL_LIMITS: &str = include_str!("../../../skills/limits.md");
const SKILL_DECODER: &str = include_str!("../../../skills/decoder.md");

pub const SKILL_NAMES: &[&str] = &["language", "diagnostics", "limits", "decoder"];

fn skill_body(name: &str) -> Option<&'static str> {
    match name {
        "language" => Some(SKILL_LANGUAGE),
        "diagnostics" => Some(SKILL_DIAGNOSTICS),
        "limits" => Some(SKILL_LIMITS),
        "decoder" => Some(SKILL_DECODER),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct SkillsList {
    pub skills: Vec<SkillMeta>,
}

#[derive(Serialize)]
pub struct SkillMeta {
    pub name: String,
    pub summary: String,
}

#[derive(Serialize)]
pub struct SkillDocument {
    pub name: String,
    pub body: String,
}

#[derive(Serialize)]
pub struct FixPlanReport {
    pub ok: bool,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<ValidateReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plans: Option<Vec<FixPlan>>,
}

pub fn run_skills_list(json: bool) -> Result<()> {
    let skills = SkillsList {
        skills: SKILL_NAMES
            .iter()
            .map(|name| SkillMeta {
                name: (*name).to_string(),
                summary: skill_summary(name),
            })
            .collect(),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&skills).context("serialize skills list")?
        );
    } else {
        for s in &skills.skills {
            println!("{} — {}", s.name, s.summary);
        }
    }
    Ok(())
}

pub fn run_skills_get(name: &str, json: bool) -> Result<()> {
    let body = skill_body(name)
        .with_context(|| format!("unknown skill `{name}` (try: {})", SKILL_NAMES.join(", ")))?;
    if json {
        let doc = SkillDocument {
            name: name.to_string(),
            body: body.to_string(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).context("serialize skill document")?
        );
    } else {
        println!("=== skill: {name} ===\n{body}");
    }
    Ok(())
}

pub fn run_explain(code: &str, json: bool) -> Result<()> {
    let entry = explain_code(code).with_context(|| format!("unknown diagnostic code `{code}`"))?;
    print_explain(&entry, json)
}

pub fn run_explain_all(json: bool) -> Result<()> {
    if json {
        let entries: Vec<ExplainEntry> = vc_ir::all_explain_entries();
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).context("serialize explain entries")?
        );
        return Ok(());
    }
    for entry in vc_ir::all_explain_entries() {
        print_explain(&entry, false)?;
        println!();
    }
    Ok(())
}

pub fn run_validate(path: &Path, bytes: &[u8], json: bool) -> Result<()> {
    let module = match Module::parse_json_slice(bytes) {
        Ok(m) => m,
        Err(e) => {
            if json {
                let report = serde_json::json!({
                    "ok": false,
                    "parse_ok": false,
                    "parse_error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                eprintln!("parse error: {e:#}");
            }
            std::process::exit(1);
        }
    };
    let report = validate_report(&module);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serialize validate report")?
        );
    } else if report.ok {
        println!("validate: OK ({})", path.display());
    } else if let Some(diags) = &report.diagnostics {
        for d in diags {
            println!("{}: {}", d.code, d.message);
            println!("  expected: {}", d.expected);
            println!("  actual: {}", d.actual);
            println!("  repair: {} ({})", d.repair.id, d.repair.safety);
        }
    }
    if !report.ok {
        std::process::exit(1);
    }
    Ok(())
}

pub fn run_parse(path: &Path, bytes: &[u8], json: bool) -> Result<()> {
    let module = Module::parse_json_slice(bytes)
        .with_context(|| format!("parse Program IR {}", path.display()))?;
    let summary = parse_summary(&module);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).context("serialize parse summary")?
        );
    } else {
        print_parse_human(&summary);
    }
    Ok(())
}

pub fn run_fix_plan(path: &Path, bytes: &[u8], json: bool) -> Result<()> {
    let report = match Module::parse_json_slice(bytes) {
        Ok(module) => {
            let validate = validate_report(&module);
            let plans = match validate_module(&module) {
                Ok(()) => None,
                Err(e) => Some(vec![fix_plan(&e)]),
            };
            FixPlanReport {
                ok: validate.ok,
                source: path.display().to_string(),
                parse_error: None,
                validate: Some(validate),
                plans,
            }
        }
        Err(e) => FixPlanReport {
            ok: false,
            source: path.display().to_string(),
            parse_error: Some(e.to_string()),
            validate: None,
            plans: None,
        },
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serialize fix plan report")?
        );
    } else if let Some(ref validate) = report.validate {
        if validate.ok {
            println!("fix --plan: nothing to repair ({})", path.display());
        } else if let Some(plans) = &report.plans {
            for p in plans {
                println!("{} ({})", p.code, p.repair_id);
                for step in &p.steps {
                    println!("  - {}: {}", step.action, step.detail);
                }
            }
        }
    } else {
        println!("parse error: {}", report.parse_error.as_deref().unwrap_or("?"));
    }
    if !report.ok {
        std::process::exit(1);
    }
    Ok(())
}

fn print_explain(entry: &ExplainEntry, json: bool) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(entry).context("serialize explain entry")?
        );
    } else {
        println!("{} — {}", entry.code, entry.title);
        println!("{}", entry.detail);
        println!(
            "repair: {} ({}) — {}",
            entry.repair.id, entry.repair.safety, entry.repair.summary
        );
    }
    Ok(())
}

fn print_parse_human(s: &ParseSummary) {
    println!("export_name: {}", s.export_name);
    println!("program_ir_version: {}", s.program_ir_version);
    println!("params: {}", s.params.join(", "));
    println!("results: {}", s.results.join(", "));
    println!("declared_locals: {}", s.declared_locals);
    println!("top_level_body_kinds: {}", s.top_level_body_kinds.join(", "));
}

fn skill_summary(name: &str) -> String {
    match name {
        "language" => "Program IR v2 syntax and agent workflow".into(),
        "diagnostics" => "VCIR_* validation codes and repair ids".into(),
        "limits" => "Tier-1 caps and execution budgets".into(),
        "decoder" => "Latent z and ONNX I/O contract".into(),
        _ => String::new(),
    }
}
