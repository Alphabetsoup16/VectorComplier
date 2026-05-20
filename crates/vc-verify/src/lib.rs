//! Execute Wasm modules produced by VectorCompiler under deterministic limits.
//!
//! Compile ([`CompiledModule::new`]) budgets Wasm bytes, runs [`check_wasm_policy`], and
//! optionally enforces [`CompileLimits::max_wall_ms`] on Cranelift compile. [**Fuel**][`Limits::fuel`]
//! and optional guest [**wall-clock**][`Limits::max_wall_ms`] apply per invocation. For many
//! cases against one module, use [`CompiledModule::prepare_invoke`] + [`InvokeSession`].

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{ensure, Context, Result};
use wasmparser::{Encoding, Imports, Parser, Payload};
use wasmtime::{Config, Engine, Instance, Module, Store, Trap, Val};

/// Hard limit on Wasm module bytes accepted for compilation (DoS mitigation for `run`/`bench`).
pub const MAX_WASM_BYTES: usize = 16 * 1024 * 1024;

/// Default wall-clock budget for Cranelift compile + `Module::from_binary` (milliseconds).
pub const DEFAULT_COMPILE_WALL_MS: u64 = 30_000;

/// Cap on detached compile worker threads when a compile wall-clock limit is set.
///
/// On timeout the caller returns immediately but the worker may still run until
/// `Module::from_binary` finishes (bounded by [`MAX_WASM_BYTES`]).
const MAX_ACTIVE_COMPILE_THREADS: usize = 8;

static ACTIVE_COMPILE_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Static capability policy for untrusted Wasm before [`Module::from_binary`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WasmPolicy {
    pub allow_imports: bool,
    pub allow_memory: bool,
    pub allow_tables: bool,
}

/// Scan Wasm bytes and reject modules that declare disallowed imports, memories, tables,
/// component-model payloads, or non-module encodings.
pub fn check_wasm_policy(wasm: &[u8], policy: WasmPolicy) -> Result<()> {
    for payload in Parser::new(0).parse_all(wasm) {
        match payload? {
            Payload::Version { encoding, .. } => {
                if encoding != Encoding::Module {
                    anyhow::bail!(
                        "only core WebAssembly modules supported (encoding={encoding:?})"
                    );
                }
            }
            Payload::ModuleSection { .. } => {
                anyhow::bail!("nested wasm modules not allowed");
            }
            Payload::ComponentSection { .. }
            | Payload::InstanceSection(_)
            | Payload::CoreTypeSection(_)
            | Payload::ComponentInstanceSection(_)
            | Payload::ComponentAliasSection(_)
            | Payload::ComponentTypeSection(_)
            | Payload::ComponentCanonicalSection(_)
            | Payload::ComponentStartSection { .. }
            | Payload::ComponentImportSection(_)
            | Payload::ComponentExportSection(_) => {
                anyhow::bail!("wasm component model sections not allowed");
            }
            Payload::ImportSection(imports) if !policy.allow_imports => {
                for group in imports {
                    match group? {
                        Imports::Single(_, import) => {
                            anyhow::bail!(
                                "wasm imports not allowed (found import `{}` / `{}`)",
                                import.module,
                                import.name
                            );
                        }
                        Imports::Compact1 { module, .. } | Imports::Compact2 { module, .. } => {
                            anyhow::bail!(
                                "wasm imports not allowed (found import module `{module}`)"
                            );
                        }
                    }
                }
            }
            Payload::MemorySection(memories) if !policy.allow_memory => {
                if let Some(memory) = memories.into_iter().next() {
                    let _ = memory?;
                    anyhow::bail!("wasm memories not allowed");
                }
            }
            Payload::TableSection(tables) if !policy.allow_tables => {
                if let Some(table) = tables.into_iter().next() {
                    let _ = table?;
                    anyhow::bail!("wasm tables not allowed");
                }
            }
            Payload::StartSection { .. } => {
                anyhow::bail!("wasm start function not allowed");
            }
            Payload::TagSection(tags) => {
                if let Some(tag) = tags.into_iter().next() {
                    let _ = tag?;
                    anyhow::bail!("wasm tags not allowed");
                }
            }
            Payload::GlobalSection(globals) => {
                if let Some(global) = globals.into_iter().next() {
                    let _ = global?;
                    anyhow::bail!("wasm globals not allowed");
                }
            }
            Payload::ElementSection(elements) => {
                if let Some(el) = elements.into_iter().next() {
                    let _ = el?;
                    anyhow::bail!("wasm element segments not allowed");
                }
            }
            Payload::DataCountSection { .. } => {
                anyhow::bail!("wasm data count section not allowed");
            }
            Payload::DataSection(data) => {
                if let Some(seg) = data.into_iter().next() {
                    let _ = seg?;
                    anyhow::bail!("wasm data segments not allowed");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Supported Wasm scalar values for [`CompiledModule::invoke_scalar_return`].
#[derive(Clone, Debug, PartialEq)]
pub enum WasmScalar {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmScalar {
    fn val_type(&self) -> wasmtime::ValType {
        match self {
            WasmScalar::I32(_) => wasmtime::ValType::I32,
            WasmScalar::I64(_) => wasmtime::ValType::I64,
            WasmScalar::F32(_) => wasmtime::ValType::F32,
            WasmScalar::F64(_) => wasmtime::ValType::F64,
        }
    }

    fn to_val(&self) -> Val {
        match *self {
            WasmScalar::I32(v) => Val::I32(v),
            WasmScalar::I64(v) => Val::I64(v),
            WasmScalar::F32(v) => Val::F32(v.to_bits()),
            WasmScalar::F64(v) => Val::F64(v.to_bits()),
        }
    }

    fn from_val(v: Val) -> Result<Self> {
        Ok(match v {
            Val::I32(x) => WasmScalar::I32(x),
            Val::I64(x) => WasmScalar::I64(x),
            Val::F32(bits) => WasmScalar::F32(f32::from_bits(bits)),
            Val::F64(bits) => WasmScalar::F64(f64::from_bits(bits)),
            other => anyhow::bail!("unsupported wasm value type: {:?}", other),
        })
    }

    fn zero_slot(ty: wasmtime::ValType) -> Val {
        match ty {
            wasmtime::ValType::I32 => Val::I32(0),
            wasmtime::ValType::I64 => Val::I64(0),
            wasmtime::ValType::F32 => Val::F32(0),
            wasmtime::ValType::F64 => Val::F64(0),
            _ => Val::I32(0),
        }
    }
}

/// Wall-clock budget for Wasm compile (`Engine` + `Module::from_binary`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileLimits {
    /// `None` = no host wall-clock cap (trusted first-party artifacts only).
    pub max_wall_ms: Option<u64>,
}

impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            max_wall_ms: Some(DEFAULT_COMPILE_WALL_MS),
        }
    }
}

impl CompileLimits {
    /// No compile wall-clock cap (use only for trusted in-process paths).
    pub const UNBOUNDED: Self = Self { max_wall_ms: None };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub fuel: u64,
    /// Wall-clock cap for guest invocation ([`CompiledModule::invoke_i32_return`],
    /// [`InvokeSession::invoke_i32_return`]).
    ///
    /// Implemented with Wasmtime **epoch interruption**: a watchdog thread calls
    /// [`Engine::increment_epoch`] after the deadline unless the invocation has finished.
    pub max_wall_ms: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 50_000,
            max_wall_ms: None,
        }
    }
}

/// Compiled module bound to one [`Engine`] — reuse for many invocations (e.g. benchmark cases).
pub struct CompiledModule {
    engine: Engine,
    module: Module,
}

impl CompiledModule {
    /// Parse and compile Wasm under [`WasmPolicy::default`] and [`CompileLimits::default`].
    pub fn new(wasm: &[u8]) -> Result<Self> {
        Self::new_with_policy(wasm, WasmPolicy::default(), CompileLimits::default())
    }

    /// Parse and compile Wasm after [`check_wasm_policy`] with explicit compile limits.
    pub fn new_with_policy(
        wasm: &[u8],
        policy: WasmPolicy,
        compile: CompileLimits,
    ) -> Result<Self> {
        ensure!(
            wasm.len() <= MAX_WASM_BYTES,
            "wasm module too large ({} bytes, max {})",
            wasm.len(),
            MAX_WASM_BYTES
        );

        check_wasm_policy(wasm, policy)?;

        let (engine, module) = compile_engine_module(wasm, compile)?;
        Ok(Self { engine, module })
    }

    /// Reuse one store + instance across many cases (resets fuel each call).
    ///
    /// Runs [`Instance::new`] on the current thread (no guest fuel metering). For
    /// first-party modules produced by this repo, instantiation cost scales with
    /// module size and is typically small relative to compile.
    pub fn prepare_invoke(&self, export: &str) -> Result<InvokeSession> {
        let mut store = Store::new(&self.engine, ());
        store.epoch_deadline_trap();
        store.set_epoch_deadline(EPOCH_DEADLINE_FAR_TICKS);

        let instance = Instance::new(&mut store, &self.module, &[])?;
        let func = instance
            .get_func(&mut store, export)
            .with_context(|| format!("missing export `{export}`"))?;

        Ok(InvokeSession {
            engine: self.engine.clone(),
            store,
            func,
        })
    }

    /// Instantiate fresh store + instance, invoke `export(...) -> i32` under [`Limits`].
    pub fn invoke_i32_return(&self, export: &str, args: &[i32], limits: Limits) -> Result<i32> {
        invoke_i32_return_detailed(
            &self.engine,
            &self.module,
            export,
            args,
            limits.fuel,
            limits.max_wall_ms,
        )
    }

    /// Invoke a single-result export with [`WasmScalar`] arguments (any combination of `i32` /
    /// `i64` / `f32` / `f64` supported by Wasm MVP).
    pub fn invoke_scalar_return(
        &self,
        export: &str,
        args: &[WasmScalar],
        limits: Limits,
    ) -> Result<WasmScalar> {
        invoke_scalar_return_detailed(
            &self.engine,
            &self.module,
            export,
            args,
            limits.fuel,
            limits.max_wall_ms,
        )
    }
}

/// Reused store + export func for benchmark / refine loops (one instantiation per module).
pub struct InvokeSession {
    engine: Engine,
    store: Store<()>,
    func: wasmtime::Func,
}

impl InvokeSession {
    /// Invoke `export(...) -> i32` under [`Limits`] (fuel reset; fresh epoch deadline per call).
    pub fn invoke_i32_return(&mut self, args: &[i32], limits: Limits) -> Result<i32> {
        let sargs: Vec<WasmScalar> = args.iter().copied().map(WasmScalar::I32).collect();
        let got = with_wall_watchdog(&self.engine, limits.max_wall_ms, || {
            invoke_scalar_on_func(
                &mut self.store,
                &self.func,
                &sargs,
                limits.fuel,
                limits.max_wall_ms,
            )
        })?;
        match got {
            WasmScalar::I32(v) => Ok(v),
            other => anyhow::bail!("expected i32 return, got {:?}", other),
        }
    }

    /// Invoke a single-result export with [`WasmScalar`] arguments under [`Limits`].
    pub fn invoke_scalar_return(
        &mut self,
        args: &[WasmScalar],
        limits: Limits,
    ) -> Result<WasmScalar> {
        with_wall_watchdog(&self.engine, limits.max_wall_ms, || {
            invoke_scalar_on_func(
                &mut self.store,
                &self.func,
                args,
                limits.fuel,
                limits.max_wall_ms,
            )
        })
    }
}

fn acquire_compile_worker_slot() -> Result<()> {
    loop {
        let active = ACTIVE_COMPILE_THREADS.load(Ordering::Relaxed);
        if active >= MAX_ACTIVE_COMPILE_THREADS {
            anyhow::bail!(
                "too many concurrent wasm compile workers (max {MAX_ACTIVE_COMPILE_THREADS})"
            );
        }
        if ACTIVE_COMPILE_THREADS
            .compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return Ok(());
        }
    }
}

fn release_compile_worker_slot() {
    ACTIVE_COMPILE_THREADS.fetch_sub(1, Ordering::AcqRel);
}

fn build_engine_module(wasm: &[u8]) -> Result<(Engine, Module)> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config)?;
    let module = Module::from_binary(&engine, wasm)?;
    Ok((engine, module))
}

fn compile_engine_module(wasm: &[u8], compile: CompileLimits) -> Result<(Engine, Module)> {
    let Some(ms) = compile.max_wall_ms else {
        return build_engine_module(wasm);
    };

    acquire_compile_worker_slot()?;

    let wasm_owned = wasm.to_vec();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = build_engine_module(&wasm_owned);
        let _ = tx.send(result);
        release_compile_worker_slot();
    });

    match rx.recv_timeout(Duration::from_millis(ms)) {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(e)) => Err(e),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            tracing::warn!(
                wall_ms = ms,
                "wasm compile exceeded wall-clock limit; orphan compile thread may still run \
                 (bounded by MAX_WASM_BYTES and global compile worker cap)"
            );
            anyhow::bail!("wasm compile exceeded wall-clock limit ({ms} ms)")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            anyhow::bail!("wasm compile worker exited before reporting result")
        }
    }
}

/// Enough headroom vs the engine epoch counter for normal runs (epochs move only via
/// `increment_epoch()`, not autonomously). Avoids coupling to Wasmtime epoch APIs.
const EPOCH_DEADLINE_FAR_TICKS: u64 = u64::MAX >> 2;

fn with_wall_watchdog<T>(
    engine: &Engine,
    wall_ms: Option<u64>,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let mut join_watchdog: Option<thread::JoinHandle<()>> = None;
    let cancel = Arc::new(AtomicBool::new(false));

    if let Some(ms) = wall_ms {
        let cancelled = Arc::clone(&cancel);
        let eng = engine.clone();
        join_watchdog = Some(thread::spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            if !cancelled.load(Ordering::Acquire) {
                eng.increment_epoch();
            }
        }));
    }

    let result = f();

    cancel.store(true, Ordering::Release);

    if let Some(h) = join_watchdog {
        let _ = h.join();
    }

    result
}

fn invoke_i32_return_detailed(
    engine: &Engine,
    module: &Module,
    export: &str,
    args: &[i32],
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<i32> {
    with_wall_watchdog(engine, wall_ms, || {
        invoke_i32_return_inner(engine, module, export, args, fuel, wall_ms)
    })
}

fn invoke_scalar_return_detailed(
    engine: &Engine,
    module: &Module,
    export: &str,
    args: &[WasmScalar],
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<WasmScalar> {
    with_wall_watchdog(engine, wall_ms, || {
        invoke_scalar_inner(engine, module, export, args, fuel, wall_ms)
    })
}

fn same_simple_val_type(a: &wasmtime::ValType, b: &wasmtime::ValType) -> bool {
    use wasmtime::ValType::*;
    matches!((a, b), (I32, I32) | (I64, I64) | (F32, F32) | (F64, F64))
}

fn configure_store_for_invoke(
    store: &mut Store<()>,
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<()> {
    store.epoch_deadline_trap();
    match wall_ms {
        None => store.set_epoch_deadline(EPOCH_DEADLINE_FAR_TICKS),
        Some(_) => store.set_epoch_deadline(1),
    }
    store.set_fuel(fuel)?;
    Ok(())
}

fn invoke_scalar_inner(
    engine: &Engine,
    module: &Module,
    export: &str,
    args: &[WasmScalar],
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<WasmScalar> {
    let mut store = Store::new(engine, ());
    let instance = Instance::new(&mut store, module, &[])?;
    let func = instance
        .get_func(&mut store, export)
        .with_context(|| format!("missing export `{export}`"))?;
    invoke_scalar_on_func(&mut store, &func, args, fuel, wall_ms)
}

fn invoke_scalar_on_func(
    store: &mut Store<()>,
    func: &wasmtime::Func,
    args: &[WasmScalar],
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<WasmScalar> {
    configure_store_for_invoke(store, fuel, wall_ms)?;

    let fty = func.ty(&mut *store);
    let param_ty: Vec<wasmtime::ValType> = fty.params().collect();
    let result_ty: Vec<wasmtime::ValType> = fty.results().collect();

    ensure!(
        param_ty.len() == args.len(),
        "arity mismatch: wasm expects {}, got {}",
        param_ty.len(),
        args.len()
    );
    ensure!(
        result_ty.len() == 1,
        "only single-result exports are supported (got {} results)",
        result_ty.len()
    );

    for (i, (pt, arg)) in param_ty.iter().zip(args.iter()).enumerate() {
        let at = arg.val_type();
        ensure!(
            same_simple_val_type(pt, &at),
            "param {i}: wasm expects {:?}, caller passed {:?}",
            pt,
            at
        );
    }

    let rt = result_ty[0].clone();
    ensure!(
        matches!(
            rt,
            wasmtime::ValType::I32
                | wasmtime::ValType::I64
                | wasmtime::ValType::F32
                | wasmtime::ValType::F64
        ),
        "unsupported wasm result type {:?}",
        rt
    );

    let wasm_args: Vec<Val> = args.iter().map(WasmScalar::to_val).collect();
    let mut wasm_results = vec![WasmScalar::zero_slot(rt)];

    match func.call(store, &wasm_args, &mut wasm_results) {
        Ok(()) => WasmScalar::from_val(wasm_results.swap_remove(0)),
        Err(err) => map_invoke_trap(err, wall_ms),
    }
}

fn map_invoke_trap(err: wasmtime::Error, wall_ms: Option<u64>) -> Result<WasmScalar> {
    if let Some(ms) = wall_ms {
        if err
            .root_cause()
            .downcast_ref::<Trap>()
            .is_some_and(|t| *t == Trap::Interrupt)
        {
            anyhow::bail!("wall-clock timeout exceeded ({ms} ms)");
        }
    }
    Err(anyhow::Error::from(err).context("wasm invoke trapped"))
}

fn invoke_i32_return_inner(
    engine: &Engine,
    module: &Module,
    export: &str,
    args: &[i32],
    fuel: u64,
    wall_ms: Option<u64>,
) -> Result<i32> {
    let sargs: Vec<WasmScalar> = args.iter().copied().map(WasmScalar::I32).collect();
    match invoke_scalar_inner(engine, module, export, &sargs, fuel, wall_ms)? {
        WasmScalar::I32(v) => Ok(v),
        other => anyhow::bail!(
            "expected i32 return from wasm export `{export}`, got {:?}",
            other
        ),
    }
}

/// One-shot invoke: compile Wasm then run (fine for sporadic CLI `run`; prefer [`CompiledModule`] in loops).
pub fn invoke_i32_return(wasm: &[u8], export: &str, args: &[i32], limits: Limits) -> Result<i32> {
    CompiledModule::new(wasm)?.invoke_i32_return(export, args, limits)
}

/// One-shot [`WasmScalar`] invoke.
pub fn invoke_scalar_return(
    wasm: &[u8],
    export: &str,
    args: &[WasmScalar],
    limits: Limits,
) -> Result<WasmScalar> {
    CompiledModule::new(wasm)?.invoke_scalar_return(export, args, limits)
}
