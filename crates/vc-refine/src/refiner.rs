use crate::spec::Spec;
use anyhow::Result;
use vc_ir::Module;

/// Search for a Program IR module that satisfies a behavioral spec.
pub trait ProgramRefiner {
    fn refine(&self, initial: &Module, spec: &Spec, fuel: u64, max_steps: usize) -> Result<Module>;
}
