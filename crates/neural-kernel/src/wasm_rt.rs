//! WASM Runtime — Skill execution engine (#309a-c, #103-104, M31-M36, M42-M45).
//! Fuel metering, linear memory pool, WASI→Skill bridge, hybrid kernel/WASM agents.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

// ─── #104: Linear Memory Pool (256 KB per skill) ──────────────────────────

pub struct MemoryPool {
    pools: BTreeMap<String, Vec<u8>>,
    max_size: usize,
}
impl MemoryPool {
    pub fn new(max_size: usize) -> Self { MemoryPool { pools: BTreeMap::new(), max_size } }
    pub fn alloc(&mut self, skill: &str) -> &mut [u8] {
        self.pools.entry(String::from(skill)).or_insert_with(|| vec![0u8; self.max_size]);
        self.pools.get_mut(skill).unwrap().as_mut_slice()
    }
    pub fn free(&mut self, skill: &str) { self.pools.remove(skill); }
    pub fn status(&self) -> String { alloc::format!("[WASM-MEM] {} skills, {} KB each", self.pools.len(), self.max_size / 1024) }
}

// ─── M32: Developer Contract ──────────────────────────────────────────────

#[derive(Clone)]
pub struct WasmSkillManifest { pub name: String, pub kind: String, pub description: String, pub version: String, pub author: String, pub required_tokens: Vec<u64> }

// ─── M33: 15 WASI→Skill mappings ──────────────────────────────────────────

pub fn wasi_to_skill(wasi: &str) -> &'static str {
    match wasi {
        "fd_read" => "FileAgent.read", "fd_write" => "FileAgent.write", "fd_close" => "FileAgent.close",
        "path_open" => "FileAgent.open", "path_stat" => "FileAgent.stat", "path_readlink" => "FileAgent.readlink",
        "clock_time_get" => "TimeAgent.now", "clock_res_get" => "TimeAgent.resolution",
        "random_get" => "SystemAgent.random", "proc_exit" => "SystemAgent.exit",
        "environ_get" => "SystemAgent.env", "args_get" => "SystemAgent.args",
        "poll_oneoff" => "EventBusAgent.poll", "sched_yield" => "SchedulerAgent.yield",
        _ => "Unknown"
    }
}

// ─── M45: Capability Token ────────────────────────────────────────────────

#[derive(Clone)]
pub struct WasmCapability { pub token: u64, pub skills_allowed: Vec<String>, pub max_memory: usize, pub max_fuel: u64 }

// ─── M31: AgentManifest JSON ──────────────────────────────────────────────

pub fn manifest_to_json(m: &WasmSkillManifest) -> String {
    alloc::format!("{{\"name\":\"{}\",\"kind\":\"{}\",\"version\":\"{}\",\"author\":\"{}\"}}", m.name, m.kind, m.version, m.author)
}

// ─── #309c: Hybrid Agents ─────────────────────────────────────────────────

pub enum AgentOrigin { Kernel, Wasm(Vec<u8>), ExternalMCP(String) }
pub struct HybridAgent { pub name: String, pub origin: AgentOrigin, pub fuel: u64 }

pub struct HybridRegistry { agents: Vec<HybridAgent> }
impl HybridRegistry {
    pub fn new() -> Self { HybridRegistry { agents: Vec::new() } }
    pub fn register(&mut self, name: &str, origin: AgentOrigin) { self.agents.push(HybridAgent { name: String::from(name), origin, fuel: 100_000 }); }
    pub fn status(&self) -> String { alloc::format!("[HYBRID] {} agents", self.agents.len()) }
}

// ─── #309b: BitNet IDE ────────────────────────────────────────────────────

pub struct BitNetIde { pub buffer: String }
impl BitNetIde {
    pub fn new() -> Self { BitNetIde { buffer: String::new() } }
    pub fn generate_skill(&mut self, description: &str) -> String {
        alloc::format!("(skill generated for: {})", description)
    }
    pub fn status(&self) -> String { String::from("[IDE] BitNet IDE ativo") }
}

// ─── #309a: WASM Skill Runtime ────────────────────────────────────────────

pub struct WasmSkillRuntime {
    pub pool: MemoryPool,
    pub registry: HybridRegistry,
    pub ide: BitNetIde,
    pub manifests: BTreeMap<String, WasmSkillManifest>,
}

impl WasmSkillRuntime {
    pub fn new() -> Self {
        WasmSkillRuntime { pool: MemoryPool::new(256 * 1024), registry: HybridRegistry::new(), ide: BitNetIde::new(), manifests: BTreeMap::new() }
    }
    pub fn load_skill(&mut self, name: &str, bytecode: Vec<u8>, manifest: WasmSkillManifest) {
        self.pool.alloc(name);
        self.registry.register(name, AgentOrigin::Wasm(bytecode));
        self.manifests.insert(String::from(name), manifest);
    }
    pub fn status(&self) -> String {
        alloc::format!("[WASM-RT] {} skills, {}", self.manifests.len(), self.pool.status())
    }
}
