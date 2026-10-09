//! T0: tools as WASI p2 command components, run in-process with wasmtime (ADR 0028 §1,
//! ADR 0013). This is the default tier for any tool that needs code but not a computer.
//!
//! A tool reads its input on stdin and answers on stdout. It gets nothing else: no files, no
//! environment, no sockets, no clock beyond what WASI gives every component. Each run has
//! **fuel** (instructions), a **wall-clock deadline** (epoch interruption and async timeout) and a **memory
//! cap** (a store limiter, inside a pooling allocator sized for the node). The component's
//! bytes must hash to what the publisher signed in the manifest.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use bytes::Bytes;
use sha2::{Digest, Sha256};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig, Store};
use wasmtime_wasi::p2::bindings::Command;
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::manifest::{ManifestError, Needs, SignedManifest};
use crate::sandbox::MAX_OUTPUT;
use ed25519_dalek::VerifyingKey;

/// How often the deadline clock ticks.
const EPOCH_TICK: Duration = Duration::from_millis(10);

#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("this tool doesn't run in the WASM tier")]
    NotWasm,
    #[error("the component isn't the one the publisher signed")]
    WrongComponent,
    #[error("not a valid component: {0}")]
    Invalid(String),
    #[error("wasm runtime: {0}")]
    Runtime(String),
}

/// Per-run ceilings.
#[derive(Clone, Copy, Debug)]
pub struct WasmLimits {
    /// Instructions, roughly (wasmtime fuel units).
    pub fuel: u64,
    pub wall: Duration,
    pub memory_bytes: usize,
}

impl Default for WasmLimits {
    fn default() -> Self {
        WasmLimits {
            fuel: 2_000_000_000,
            wall: Duration::from_secs(5),
            memory_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Why a run stopped before finishing by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stopped {
    OutOfFuel,
    OutOfTime,
    OutOfMemory,
    /// The tool crashed (a trap) or exited with an error.
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct WasmOutput {
    pub stdout: Bytes,
    pub stderr: Bytes,
    /// `None` when the tool finished on its own and succeeded.
    pub stopped: Option<Stopped>,
    pub fuel_used: u64,
    pub elapsed: Duration,
}

impl WasmOutput {
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
}

/// A verified, compiled tool, ready to run many times.
#[derive(Clone)]
pub struct WasmTool {
    pub id: String,
    pub sha256: String,
    component: Component,
}

struct RunState {
    wasi: WasiCtx,
    table: ResourceTable,
    limiter: Limiter,
}

impl WasiView for RunState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// Caps linear memory and remembers when it said no.
struct Limiter {
    memory_bytes: usize,
    memory_used: usize,
    pending_growth: usize,
    denied: bool,
}

impl Limiter {
    fn new(memory_bytes: usize) -> Self {
        Self {
            memory_bytes,
            memory_used: 0,
            pending_growth: 0,
            denied: false,
        }
    }
}

impl wasmtime::ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.pending_growth = 0;
        let growth = desired.saturating_sub(current);
        let total = self.memory_used.checked_add(growth);
        let ok = maximum.is_none_or(|maximum| desired <= maximum)
            && total.is_some_and(|total| total <= self.memory_bytes);
        self.denied |= !ok;
        if ok {
            self.memory_used += growth;
            self.pending_growth = growth;
        }
        Ok(ok)
    }

    fn memory_grow_failed(&mut self, _error: wasmtime::Error) -> wasmtime::Result<()> {
        self.memory_used -= std::mem::take(&mut self.pending_growth);
        Ok(())
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= 100_000)
    }
}

/// The WASM tier of one agent node: one engine, one linker, a deadline clock.
pub struct WasmTier {
    engine: Engine,
    linker: Linker<RunState>,
    trusted: Vec<VerifyingKey>,
    max_memory: usize,
    stop: Arc<AtomicBool>,
}

impl Drop for WasmTier {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl WasmTier {
    /// `max_concurrent` runs at once on this node, each with at most `max_memory` bytes.
    pub fn new(
        trusted: Vec<VerifyingKey>,
        max_concurrent: u32,
        max_memory: usize,
    ) -> Result<Self, WasmError> {
        let rt = |e: wasmtime::Error| WasmError::Runtime(e.to_string());
        let mut pool = PoolingAllocationConfig::default();
        pool.total_component_instances(max_concurrent)
            .total_core_instances(max_concurrent * 4)
            .total_memories(max_concurrent * 2)
            .total_tables(max_concurrent * 4)
            .max_memory_size(max_memory);
        let mut cfg = Config::new();
        cfg.consume_fuel(true)
            .epoch_interruption(true)
            .wasm_component_model(true)
            .allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
        let engine = Engine::new(&cfg).map_err(rt)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(rt)?;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let (engine, stop) = (engine.clone(), stop.clone());
            std::thread::Builder::new()
                .name("zoen-wasm-epoch".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(EPOCH_TICK);
                        engine.increment_epoch();
                    }
                })
                .map_err(|e| WasmError::Runtime(e.to_string()))?;
        }
        Ok(WasmTier {
            engine,
            linker,
            trusted,
            max_memory,
            stop,
        })
    }

    /// Checks the signed manifest (trusted publisher, `needs: wasm`, component hash) and
    /// compiles the component.
    pub fn load(&self, manifest: &SignedManifest, bytes: &[u8]) -> Result<WasmTool, WasmError> {
        let m = manifest.verify(&self.trusted)?;
        if m.needs != Needs::Wasm {
            return Err(WasmError::NotWasm);
        }
        let sha = hex::encode(Sha256::digest(bytes));
        if m.component_sha256.as_deref() != Some(sha.as_str()) {
            return Err(WasmError::WrongComponent);
        }
        let component =
            Component::new(&self.engine, bytes).map_err(|e| WasmError::Invalid(e.to_string()))?;
        Ok(WasmTool {
            id: m.id.clone(),
            sha256: sha,
            component,
        })
    }

    /// Runs `tool` once with `input` on stdin.
    pub async fn run(
        &self,
        tool: &WasmTool,
        input: Bytes,
        limits: WasmLimits,
    ) -> Result<WasmOutput, WasmError> {
        let started = Instant::now();
        let stdout = MemoryOutputPipe::new(MAX_OUTPUT);
        let stderr = MemoryOutputPipe::new(64 * 1024);
        // Nothing inherited: no env, no args beyond the tool's name, no preopened dirs, no
        // sockets or name lookups.
        let wasi = WasiCtx::builder()
            .stdin(MemoryInputPipe::new(input))
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .args(&[tool.id.as_str()])
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            .socket_addr_check(|_, _| Box::pin(async { false }))
            .build();
        let mut store = Store::new(
            &self.engine,
            RunState {
                wasi,
                table: ResourceTable::new(),
                limiter: Limiter::new(limits.memory_bytes.min(self.max_memory)),
            },
        );
        store.limiter(|s| &mut s.limiter);
        store
            .set_fuel(limits.fuel)
            .map_err(|e| WasmError::Runtime(e.to_string()))?;
        // Yield to the executor now and then, so one tool never hogs a thread.
        store
            .fuel_async_yield_interval(Some(1_000_000))
            .map_err(|e| WasmError::Runtime(e.to_string()))?;
        let ticks = (limits.wall.as_millis() / EPOCH_TICK.as_millis()).max(1) as u64;
        store.set_epoch_deadline(ticks);
        store.epoch_deadline_trap();

        let result = tokio::time::timeout(limits.wall, async {
            let cmd = Command::instantiate_async(&mut store, &tool.component, &self.linker).await?;
            cmd.wasi_cli_run().call_run(&mut store).await
        })
        .await;
        let fuel_used = limits.fuel - store.get_fuel().unwrap_or(0);
        let denied = store.data().limiter.denied;
        let stopped = match result {
            Err(_) => Some(Stopped::OutOfTime),
            Ok(Ok(Ok(()))) => None,
            Ok(Ok(Err(()))) => Some(Stopped::Failed("exited with an error".into())),
            Ok(Err(e)) => Some(match e.downcast_ref::<wasmtime::Trap>() {
                Some(wasmtime::Trap::OutOfFuel) => Stopped::OutOfFuel,
                Some(wasmtime::Trap::Interrupt) => Stopped::OutOfTime,
                _ if denied => Stopped::OutOfMemory,
                _ => Stopped::Failed(format!("{e:#}").lines().next().unwrap_or("").to_string()),
            }),
        };
        let stopped = match stopped {
            Some(Stopped::Failed(_)) if denied => Some(Stopped::OutOfMemory),
            other => other,
        };
        Ok(WasmOutput {
            stdout: stdout.contents(),
            stderr: stderr.contents(),
            stopped,
            fuel_used,
            elapsed: started.elapsed(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{Memory, MemoryType, ResourceLimiter};

    fn memory_store(bytes: usize) -> Store<Limiter> {
        let mut store = Store::new(&Engine::default(), Limiter::new(bytes));
        store.limiter(|limiter| limiter);
        store
    }

    #[test]
    fn linear_memories_share_one_run_budget() {
        let mut store = memory_store(64 * 1024 * 1024);
        Memory::new(&mut store, MemoryType::new(640, None)).unwrap();
        assert!(Memory::new(&mut store, MemoryType::new(640, None)).is_err());
        Memory::new(&mut store, MemoryType::new(384, None)).unwrap();
        assert_eq!(store.data().memory_used, 64 * 1024 * 1024);
    }

    #[test]
    fn rejected_growth_does_not_consume_another_memorys_budget() {
        let mut store = memory_store(4 * 64 * 1024);
        let memory = Memory::new(&mut store, MemoryType::new(1, Some(1))).unwrap();
        assert!(memory.grow(&mut store, 1).is_err());
        Memory::new(&mut store, MemoryType::new(3, None)).unwrap();
        assert_eq!(store.data().memory_used, 4 * 64 * 1024);
    }

    #[test]
    fn failed_allocation_returns_reserved_growth_to_the_run_budget() {
        let mut limiter = Limiter::new(4 * 64 * 1024);
        assert!(limiter.memory_growing(0, 64 * 1024, None).unwrap());
        assert!(limiter
            .memory_growing(64 * 1024, 3 * 64 * 1024, None)
            .unwrap());
        limiter
            .memory_grow_failed(wasmtime::Error::msg("allocation failed"))
            .unwrap();
        assert!(limiter.memory_growing(0, 3 * 64 * 1024, None).unwrap());
        assert!(!limiter.memory_growing(0, 64 * 1024, None).unwrap());
    }
}
