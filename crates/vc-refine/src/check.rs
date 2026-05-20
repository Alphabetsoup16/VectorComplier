use crate::spec::Spec;
use anyhow::{ensure, Context, Result};
use vc_ir::Module;
use vc_lower_wasm::lower_module;
use vc_verify::{CompiledModule, InvokeSession, Limits};

/// Reuse Wasm compile + invoke session when lowered bytes are unchanged (refine hot path).
pub struct ModuleSpecCache {
    cached_wasm: Option<Vec<u8>>,
    session: Option<InvokeSession>,
}

impl ModuleSpecCache {
    pub fn new() -> Self {
        Self {
            cached_wasm: None,
            session: None,
        }
    }

    /// Lower `module`, run every spec case under `fuel` / `max_wall_ms`, return `Ok(())` only if all match.
    pub fn satisfies(
        &mut self,
        module: &Module,
        spec: &Spec,
        fuel: u64,
        max_wall_ms: Option<u64>,
    ) -> Result<()> {
        let wasm = lower_module(module).context("lower module to wasm")?;
        if self.cached_wasm.as_deref() != Some(wasm.as_slice()) {
            let compiled = CompiledModule::new(&wasm).context("compile wasm")?;
            let session = compiled
                .prepare_invoke(&module.export_name)
                .context("instantiate wasm export")?;
            self.cached_wasm = Some(wasm);
            self.session = Some(session);
        }

        let limits = Limits { fuel, max_wall_ms };
        let session = self
            .session
            .as_mut()
            .expect("session present after cache refresh");
        for (i, case) in spec.cases.iter().enumerate() {
            let got = session
                .invoke_i32_return(&case.args, limits)
                .with_context(|| format!("invoke case {i}"))?;
            ensure!(
                got == case.expect_i32,
                "case {i}: expected {}, got {}",
                case.expect_i32,
                got
            );
        }
        Ok(())
    }
}

impl Default for ModuleSpecCache {
    fn default() -> Self {
        Self::new()
    }
}
