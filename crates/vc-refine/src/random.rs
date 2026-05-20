use crate::check::ModuleSpecCache;
use crate::refiner::ProgramRefiner;
use crate::spec::Spec;
use anyhow::{bail, Result};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use vc_ir::{validate_module, FuncSig, ValidationError};
use vc_ir::{Instr, Module, ValType, PROGRAM_IR_VERSION};

/// Random mutation search over valid Program IR, verified via Wasm execution.
pub struct RandomIrRefiner {
    seed: u64,
}

impl RandomIrRefiner {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    fn try_mutation(rng: &mut StdRng, module: &mut Module) -> bool {
        match rng.random_range(0..5) {
            0 => Self::mutate_restart_template(rng, module),
            1 => Self::mutate_swap_adjacent(rng, module),
            2 => Self::mutate_replace_binop(rng, module),
            3 => Self::mutate_remove_instr(rng, module),
            _ => Self::mutate_change_i32_const(rng, module),
        }
    }

    fn mutate_restart_template(rng: &mut StdRng, module: &mut Module) -> bool {
        let templates = body_templates(&module.func.sig);
        if templates.is_empty() {
            return false;
        }
        let idx = rng.random_range(0..templates.len());
        module.func.body = templates[idx].clone();
        true
    }

    fn mutate_swap_adjacent(rng: &mut StdRng, module: &mut Module) -> bool {
        let body = &mut module.func.body;
        if body.len() < 3 {
            return false;
        }
        let max_i = body.len() - 2;
        let i = rng.random_range(0..max_i);
        body.swap(i, i + 1);
        true
    }

    fn mutate_replace_binop(rng: &mut StdRng, module: &mut Module) -> bool {
        let binops = [Instr::I32Add, Instr::I32Sub, Instr::I32Mul, Instr::I32Xor];
        let body = &mut module.func.body;
        let candidates: Vec<usize> = body
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                matches!(
                    i,
                    Instr::I32Add | Instr::I32Sub | Instr::I32Mul | Instr::I32Xor
                )
            })
            .map(|(idx, _)| idx)
            .collect();
        if candidates.is_empty() {
            return false;
        }
        let idx = candidates[rng.random_range(0..candidates.len())];
        let mut alts: Vec<Instr> = Vec::new();
        for b in binops.iter() {
            if *b != body[idx] {
                alts.push((*b).clone());
            }
        }
        if alts.is_empty() {
            return false;
        }
        body[idx] = alts[rng.random_range(0..alts.len())].clone();
        true
    }

    fn mutate_remove_instr(rng: &mut StdRng, module: &mut Module) -> bool {
        let body = &mut module.func.body;
        if body.len() <= 2 {
            return false;
        }
        let removable: Vec<usize> = (0..body.len() - 1).collect();
        if removable.is_empty() {
            return false;
        }
        let idx = removable[rng.random_range(0..removable.len())];
        body.remove(idx);
        true
    }

    fn mutate_change_i32_const(rng: &mut StdRng, module: &mut Module) -> bool {
        let body = &mut module.func.body;
        let candidates: Vec<usize> = body
            .iter()
            .enumerate()
            .filter_map(|(idx, i)| matches!(i, Instr::I32Const { .. }).then_some(idx))
            .collect();
        if candidates.is_empty() {
            return false;
        }
        let idx = candidates[rng.random_range(0..candidates.len())];
        let value = rng.random::<i32>();
        body[idx] = Instr::I32Const { value };
        true
    }
}

impl ProgramRefiner for RandomIrRefiner {
    fn refine(&self, initial: &Module, spec: &Spec, fuel: u64, max_steps: usize) -> Result<Module> {
        if spec.cases.is_empty() {
            bail!("spec must contain at least one case");
        }
        validate_module(initial).map_err(|e: ValidationError| anyhow::anyhow!("{e}"))?;

        let mut rng = StdRng::seed_from_u64(self.seed);

        let mut cache = ModuleSpecCache::new();
        let current = initial.clone();
        if cache.satisfies(&current, spec, fuel, None).is_ok() {
            return Ok(current);
        }

        for _ in 0..max_steps {
            let mut trial = current.clone();
            if !Self::try_mutation(&mut rng, &mut trial) {
                continue;
            }
            if validate_module(&trial).is_err() {
                continue;
            }
            if cache.satisfies(&trial, spec, fuel, None).is_ok() {
                return Ok(trial);
            }
        }

        bail!(
            "refinement exhausted {max_steps} steps without satisfying {} case(s)",
            spec.cases.len()
        )
    }
}

fn body_templates(sig: &FuncSig) -> Vec<Vec<Instr>> {
    let mut out = Vec::new();
    if sig.params.len() == 2 && sig.results == [ValType::I32] {
        out.push(vec![
            Instr::LocalGet { index: 0 },
            Instr::LocalGet { index: 1 },
            Instr::I32Add,
            Instr::Return,
        ]);
        out.push(vec![
            Instr::LocalGet { index: 0 },
            Instr::LocalGet { index: 1 },
            Instr::I32Sub,
            Instr::Return,
        ]);
        out.push(vec![
            Instr::LocalGet { index: 0 },
            Instr::LocalGet { index: 1 },
            Instr::I32Mul,
            Instr::Return,
        ]);
        out.push(vec![
            Instr::LocalGet { index: 0 },
            Instr::LocalGet { index: 1 },
            Instr::I32Xor,
            Instr::Return,
        ]);
    } else if sig.params.is_empty() && sig.results == [ValType::I32] {
        out.push(vec![Instr::I32Const { value: 0 }, Instr::Return]);
        out.push(vec![Instr::I32Const { value: 42 }, Instr::Return]);
    } else if sig.params.len() == 1 && sig.results == [ValType::I32] {
        out.push(vec![Instr::LocalGet { index: 0 }, Instr::Return]);
    }
    out
}

/// Minimal wrong module (subtract) for two i32 params — useful in tests.
pub fn wrong_sub_add_signature() -> Module {
    Module {
        program_ir_version: PROGRAM_IR_VERSION,
        export_name: "run".into(),
        func: vc_ir::Func {
            sig: FuncSig {
                params: vec![ValType::I32, ValType::I32],
                results: vec![ValType::I32],
            },
            locals: vec![],
            body: vec![
                Instr::LocalGet { index: 0 },
                Instr::LocalGet { index: 1 },
                Instr::I32Sub,
                Instr::Return,
            ],
        },
    }
}
