use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use alloc::format;
use alloc::collections::BTreeMap;
use core::sync::atomic::Ordering;
use k_nano::memory::{GLOBAL_ALLOCATOR, BITMAP_SIZE};
use crate::chunker;
use event_bus::{Event, CapabilityToken};

pub struct BudgetedRecovery {
    budget: u64,
    tick: u64,
}

impl BudgetedRecovery {
    pub fn new(_max_ops: usize, max_budget: u64) -> Self {
        Self { budget: max_budget, tick: 0 }
    }
    
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }
    
    pub fn can_execute(&self) -> bool {
        true
    }
}

pub struct SilentFailureDetector {
    failures: BTreeMap<String, u64>,
    threshold: u64,
    tick: u64,
}

impl SilentFailureDetector {
    pub fn new(threshold: u64) -> Self {
        Self { 
            failures: BTreeMap::new(), 
            threshold, 
            tick: 0 
        }
    }
    
    pub fn set_tick(&mut self, tick: u64) {
        self.tick = tick;
    }
    
    pub fn record_failure(&mut self, agent: &str, tick: u64) {
        let entry = self.failures.entry(agent.to_string()).or_insert(tick);
        *entry = tick;
    }
    
    pub fn heartbeat(&mut self, agent: &str) {
        let entry = self.failures.entry(agent.to_string()).or_insert(self.tick);
        *entry = self.tick;
    }
    
    pub fn detect_silent(&self) -> Vec<String> {
        let mut silent = Vec::new();
        for (agent, last_tick) in &self.failures {
            if self.tick - last_tick > self.threshold {
                silent.push(agent.clone());
            }
        }
        silent
    }
}


#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub valid: bool,
    pub bitmap: [u8; k_nano::memory::BITMAP_SIZE],
    pub next_free_bit: usize,
    pub total_frames: usize,
    pub usable_frames: usize,
    pub allocated_count: usize,
    pub mhi_dram_bytes: u64,
    pub tick: u64,
    pub heap_start: u64,             // heap region start address (0 = unknown)
    pub heap_size: u64,               // heap region size in bytes
    pub page_table_pml4_addr: u64,    // CR3 / PML4 physical address (0 = unknown)
    pub driver_state_hash: u64,       // FNV-1a hash of driver init flags (0 = not captured)
    pub checkpoint_version: u8,       // serialization format version (v2=10 u64s, v3=+save_count)
    pub save_count: u64,              // incremented on each save_checkpoint() call
}

impl Checkpoint {
    pub const fn empty() -> Self {
        Checkpoint {
            valid: false, bitmap: [0; k_nano::memory::BITMAP_SIZE],
            next_free_bit: 0, total_frames: 0,
            usable_frames: 0, allocated_count: 0,
            mhi_dram_bytes: 0, tick: 0,
            heap_start: 0, heap_size: 0,
            page_table_pml4_addr: 0, driver_state_hash: 0,
            checkpoint_version: 0,
            save_count: 0,
        }
    }

    /// Serialize checkpoint to binary blob for SGDB storage.
    fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + BITMAP_SIZE + 11 * 8 + 1);
        buf.push(if self.valid { 1 } else { 0 });
        buf.extend_from_slice(&self.bitmap);
        buf.extend_from_slice(&(self.next_free_bit as u64).to_le_bytes());
        buf.extend_from_slice(&(self.total_frames as u64).to_le_bytes());
        buf.extend_from_slice(&(self.usable_frames as u64).to_le_bytes());
        buf.extend_from_slice(&(self.allocated_count as u64).to_le_bytes());
        buf.extend_from_slice(&self.mhi_dram_bytes.to_le_bytes());
        buf.extend_from_slice(&self.tick.to_le_bytes());
        buf.extend_from_slice(&self.heap_start.to_le_bytes());
        buf.extend_from_slice(&self.heap_size.to_le_bytes());
        buf.extend_from_slice(&self.page_table_pml4_addr.to_le_bytes());
        buf.extend_from_slice(&self.driver_state_hash.to_le_bytes());
        buf.push(self.checkpoint_version);
        buf.extend_from_slice(&self.save_count.to_le_bytes());
        buf
    }

    /// Deserialize checkpoint from binary blob.
    /// Supports v2 (10 u64s) and v3 (10 u64s + save_count) formats.
    fn deserialize(data: &[u8]) -> Option<Self> {
        // minimum: valid(1) + bitmap(BITMAP_SIZE) + 10*u64(80) + version(1) = BITMAP_SIZE + 82
        // v3 adds save_count(8) = BITMAP_SIZE + 90
        if data.len() < 1 + BITMAP_SIZE + 80 + 1 {
            return None;
        }
        let mut off = 0;
        let valid = data[off] != 0;
        off += 1;
        let mut bitmap = [0u8; BITMAP_SIZE];
        bitmap.copy_from_slice(&data[off..off + BITMAP_SIZE]);
        off += BITMAP_SIZE;
        let next_free_bit = u64::from_le_bytes(data[off..off + 8].try_into().ok()?) as usize;
        off += 8;
        let total_frames = u64::from_le_bytes(data[off..off + 8].try_into().ok()?) as usize;
        off += 8;
        let usable_frames = u64::from_le_bytes(data[off..off + 8].try_into().ok()?) as usize;
        off += 8;
        let allocated_count = u64::from_le_bytes(data[off..off + 8].try_into().ok()?) as usize;
        off += 8;
        let mhi_dram_bytes = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let tick = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let heap_start = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let heap_size = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let page_table_pml4_addr = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let driver_state_hash = u64::from_le_bytes(data[off..off + 8].try_into().ok()?);
        off += 8;
        let checkpoint_version = data[off];
        off += 1;
        // v3+: save_count appended after checkpoint_version
        let save_count = if checkpoint_version >= 3 && data.len() >= off + 8 {
            u64::from_le_bytes(data[off..off + 8].try_into().ok()?)
        } else {
            0
        };
        Some(Checkpoint {
            valid,
            bitmap,
            next_free_bit,
            total_frames,
            usable_frames,
            allocated_count,
            mhi_dram_bytes,
            tick,
            heap_start,
            heap_size,
            page_table_pml4_addr,
            driver_state_hash,
            checkpoint_version,
            save_count,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ErrorContext {
    pub kind: &'static str,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub ring: u8,
    pub daemon: String,
    pub tick: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FailureClass {
    MemoryFault,
    ExecutionFault,
    ResourceFault,
    LogicFault,
    ExternalFault,
    UnknownFault,
}

impl FailureClass {
    pub fn classify(kind: &str, msg: &str) -> Self {
        if kind.contains("PageFault") || msg.contains("OOM") || msg.contains("memory") {
            FailureClass::MemoryFault
        } else if kind.contains("DoubleFault") || kind.contains("GeneralProtection") || msg.contains("GPF") {
            FailureClass::ExecutionFault
        } else if msg.contains("skill") || msg.contains("not found") || msg.contains("timeout") {
            FailureClass::ResourceFault
        } else if msg.contains("assert") || msg.contains("Assertion") {
            FailureClass::LogicFault
        } else if msg.contains("network") || msg.contains("device") {
            FailureClass::ExternalFault
        } else {
            FailureClass::UnknownFault
        }
    }

    pub fn default_recovery(&self) -> &'static str {
        match self {
            FailureClass::MemoryFault => "Compactar heap, verificar page table, reiniciar daemon",
            FailureClass::ExecutionFault => "Verificar IST stack, reiniciar core AP, restaurar checkpoint",
            FailureClass::ResourceFault => "Registrar recurso faltante, criar skill sob demanda",
            FailureClass::LogicFault => "Logar contexto, tentar continuar ignorando assert",
            FailureClass::ExternalFault => "Retentar operacao, timeout maior, fallback offline",
            FailureClass::UnknownFault => "Logar para analise do LLM, halt seguro",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FailedStrategy {
    pub error_msg: String,
    pub attempted_action: String,
    pub tick: u64,
}

#[derive(Debug)]
pub enum RecoveryAction {
    LogAndContinue,
    RestartDaemon(String, Option<fn() -> bool>),
    CreateSkill(String, String, Option<fn() -> bool>),
    CheckpointRestore,
    AwaitLLM(String),
}

pub struct SelfHeal {
    pub pending_fixes: Vec<(String, String)>,
    pub lessons: Vec<FailedStrategy>,
    pub checkpoint: Checkpoint,
}

/// Invariantes de hardware: I3 (firmware) e I4 (skill) — só VIDs conhecidos (N2 gated).
pub const FW_KNOWN_VIDS: &[(u16, u16, &str, &str)] = &[
    (0x10DE, 0x03, "nvidia/gp108", "FECS+GPCCS"),
    (0x8086, 0x03, "i915/skl+kbl", "GuC+HuC+DMC"),
    (0x10EC, 0x02, "rtl_nic", "rtl8168*"),
    (0x10EC, 0x0D, "rtl_nic", "rtl8168*"),
    (0x8086, 0x02, "intel/iwlwifi", "AX200/AX210"),
    (0x8086, 0x0D, "intel/iwlwifi", "AX200/AX210"),
];

/// Relatório do scan VID-gated (ADR-0042 N2).
#[derive(Clone, Debug, Default)]
pub struct VidGateReport {
    pub scanned: u32,
    pub noop: u32,
    pub heal_issues: u32,
    pub health_published: u32,
}

impl SelfHeal {
    pub const fn new() -> Self {
        SelfHeal { pending_fixes: Vec::new(), lessons: Vec::new(), checkpoint: Checkpoint::empty() }
    }

    /// True se (VID, class) está na tabela de FW conhecidos (gate N2).
    pub fn vid_class_needs_fw(vid: u16, class: u8) -> bool {
        FW_KNOWN_VIDS.iter().any(|(v, c, _, _)| *v == vid && *c == class as u16)
    }

    /// Gate fino: Intel net 8086 02/0D só se não for Ethernet nativo.
    /// - `subclass == 0x00` → Ethernet (e1000) ≠ iwlwifi
    /// - DIDs conhecidos 100E/100F/10D3/1502/1503 → never iwlwifi (sem retrain)
    pub fn device_needs_fw(vid: u16, did: u16, class: u8, subclass: u8) -> bool {
        if !Self::vid_class_needs_fw(vid, class) {
            return false;
        }
        if vid == 0x8086
            && matches!(did, 0x100E | 0x100F | 0x10D3 | 0x1502 | 0x1503)
        {
            return false;
        }
        if vid == 0x8086 && (class == 0x02 || class == 0x0D) && subclass == 0x00 {
            return false;
        }
        true
    }

    /// ADR-0042 N2: percorre inventário PCI; só age em VID conhecidos.
    /// - `noop`: VID fora da tabela ou FW/skill OK
    /// - `heal`: publica HEALTH_ISSUE (I3/I4) — Ring 1 não carrega FW de Ring 2
    /// `devices`: (VID, DID, class, subclass).
    pub fn run_vid_gated_scan(
        &mut self,
        devices: &[(u16, u16, u8, u8)],
    ) -> VidGateReport {
        let mut report = VidGateReport::default();
        for &(vid, did, class, subclass) in devices {
            report.scanned = report.scanned.saturating_add(1);
            if !Self::device_needs_fw(vid, did, class, subclass) {
                report.noop = report.noop.saturating_add(1);
                continue;
            }
            let desc = alloc::format!("{:04X}:{:04X}", vid, did);
            let fw_ok = self.check_device_firmware(vid, did, class);
            let skill_ok = self.check_device_skill(vid, did, class, &desc);
            if fw_ok && skill_ok {
                report.noop = report.noop.saturating_add(1);
                k_nano::slog_kai!("Gate", "n2", "noop VID={:04X}:{:04X} class={:02X}", vid, did, class);
            } else {
                report.heal_issues = report.heal_issues.saturating_add(1);
                if !fw_ok {
                    report.health_published = report.health_published.saturating_add(1);
                }
                if !skill_ok {
                    report.health_published = report.health_published.saturating_add(1);
                }
                k_nano::slog_kai!("Gate", "n2", "heal VID={:04X}:{:04X} class={:02X} fw_ok={} skill_ok={}", vid, did, class, fw_ok, skill_ok);
            }
        }
        k_nano::slog_kai!("Gate", "n2", "done scanned={} noop={} heal={} HEALTH_ISSUE={}",
            report.scanned, report.noop, report.heal_issues, report.health_published);
        report
    }

    /// I3: Verifica se um dispositivo conhecido tem firmware carregado.
    /// Se nao tiver, registra pendencia e publica HEALTH_ISSUE.
    pub fn check_device_firmware(&mut self, vid: u16, did: u16, class: u8) -> bool {
        let needs_fw = Self::vid_class_needs_fw(vid, class);
        if !needs_fw { return true; }
        // Ring 1 (k-ai): nunca carrega ACR NVIDIA — só sinaliza HEALTH_ISSUE.
        let loaded = false;
        if !loaded {
            let dev = alloc::format!("{:04X}:{:04X} class={}", vid, did, class);
            self.pending_fixes.push((dev.clone(),
                alloc::format!("firmware ausente para VID={:04X} DID={:04X}", vid, did)));
            let msg = alloc::format!("HEALTH_ISSUE:I3:{}:firmware_ausente", dev);
            let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                id: 0, topic: alloc::string::String::from("HEALTH_ISSUE"),
                payload: msg.into_bytes(), token: event_bus::CapabilityToken::Legacy(1),
            });
            k_nano::slog_kai!("SelfHeal", "info", "I3: {} precisa de firmware", dev);
            return false;
        }
        true
    }

    /// I4: Verifica se existe skill para um dispositivo sem driver.
    pub fn check_device_skill(&mut self, vid: u16, did: u16, class: u8, desc: &str) -> bool {
        let skill_name = alloc::format!("driver_{:04X}_{:04X}", vid, did);
        let has_skill = k_nano::SKILL_REGISTRY.lock().has_skill(&skill_name);
        if !has_skill && class != 0x03 && class != 0x06 {
            self.pending_fixes.push((skill_name.clone(),
                alloc::format!("skill ausente para {} ({:04X}:{:04X}:{:02X})", desc, vid, did, class)));
            let msg = alloc::format!("HEALTH_ISSUE:I4:{}:skill_ausente:{:04X}:{:04X}", desc, vid, did);
            let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                id: 0, topic: alloc::string::String::from("HEALTH_ISSUE"),
                payload: msg.into_bytes(), token: event_bus::CapabilityToken::Legacy(1),
            });
            k_nano::slog_kai!("SelfHeal", "info", "I4: {} sem skill '{}'", desc, skill_name);
            return false;
        }
        true
    }

    fn get_mhi_dram_bytes() -> u64 {
        // MHI vive em hermes/neural-kernel — Ring 1 lê via k_nano se disponível
        0
    }

    pub fn save_checkpoint(&mut self) {
        k_nano::slog_kai!("CHECKPOINT", "info", "Salvando estado do kernel...");
        let guard = GLOBAL_ALLOCATOR.lock();
        if let Some(ref alloc) = *guard {
            self.checkpoint.bitmap = alloc.bitmap;
            self.checkpoint.next_free_bit = alloc.next_free_bit;
            self.checkpoint.total_frames = alloc.total_frames;
            self.checkpoint.usable_frames = alloc.usable_frames;
            self.checkpoint.allocated_count = alloc.allocated_count;
        }
        drop(guard);
        self.checkpoint.mhi_dram_bytes = Self::get_mhi_dram_bytes();
        self.checkpoint.tick = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.checkpoint.heap_start = 0x_4000_0000_0000; // ponytail: fixed heap addr from AGENTS.md
        self.checkpoint.heap_size = 512 * 1024 * 1024;  // 512MB heap
        self.checkpoint.page_table_pml4_addr = unsafe {
            x86_64::registers::control::Cr3::read().0.start_address().as_u64()
        };
        // ponytail: driver state hash — FNV-1a over ATA + E1000 init flags (1 if Some, 0 if None)
        let driver_flags: [u8; 2] = [
            k_nano::globals::ATA_DRIVER.lock().is_some() as u8,
            k_nano::nic_globals::E1000.lock().is_some() as u8,
        ];
        let mut hash: u64 = 0xcbf29ce484222325;
        for &byte in &driver_flags {
            hash = hash.wrapping_mul(0x100000001b3) ^ (byte as u64);
        }
        self.checkpoint.driver_state_hash = hash;
        self.checkpoint.save_count = self.checkpoint.save_count.wrapping_add(1);
        self.checkpoint.checkpoint_version = 3; // v3 = save_count field
        self.checkpoint.valid = true;
        k_nano::slog_kai!("CHECKPOINT", "info", "Salvo #{} @ tick {} — {} frames alocados ({} KB bitmap)",
            self.checkpoint.save_count, self.checkpoint.tick, self.checkpoint.allocated_count, BITMAP_SIZE / 1024);
        // Persist to SGDB
        let blob = self.checkpoint.serialize();
        match crate::sgdb::put_kv("sys/checkpoint", &blob) {
            Ok(()) => k_nano::slog_kai!("CHECKPOINT", "info", "SGDB persist OK bytes={}", blob.len()),
            Err(e) => k_nano::slog_kai!("CHECKPOINT", "info", "SGDB persist SKIP {:?}", e),
        }
    }

    /// Snapshot semântico: aplica CDC Rabin no bitmap para chunking.
    /// Retorna (chunks_completos, chunks_delta_modificados).
    /// `prev_bitmap` = bitmap anterior (vazio [] se primeiro snapshot).
    pub fn semantic_snapshot(&mut self, prev_bitmap: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        use cortex::delta::xor_buffers;
        use k_nano::memory::BITMAP_SIZE;

        // Copia o bitmap atual para análise fora do lock
        let current_bmp = {
            let guard = GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map_or([0u8; BITMAP_SIZE], |a| a.bitmap)
        };

        if prev_bitmap.is_empty() || prev_bitmap.len() != BITMAP_SIZE {
            let chunks = chunker::chunk_data(&current_bmp);
            k_nano::slog_kai!("SNAPSHOT", "info", "Primeiro: {} chunks CDC", chunks.len());
            return (chunks, Vec::new());
        }

        let delta_data = xor_buffers(&current_bmp, prev_bitmap);
        let delta_chunks = chunker::chunk_data(&delta_data);
        let nonzero: Vec<Vec<u8>> = delta_chunks.into_iter()
            .filter(|c| c.iter().any(|&b| b != 0))
            .collect();

        k_nano::slog_kai!("SNAPSHOT", "info", "Delta: {}/{} chunks modificados", nonzero.len(), chunker::chunk_data(&current_bmp).len());

        (chunker::chunk_data(&current_bmp), nonzero)
    }

    /// Restore checkpoint state.
    ///
    /// # Best-effort semantics
    ///
    /// This is a **best-effort** restore. Only the frame allocator bitmap is
    /// actually written back to the running allocator. The following state is
    /// **saved but NOT restored** (until subsystems become checkpoint-aware):
    ///
    /// | State                  | Saved? | Restored? | Reason |
    /// |------------------------|--------|-----------|-------|
    /// | Frame allocator bitmap | ✅     | ✅        | Written back to `GLOBAL_ALLOCATOR` |
    /// | Frame allocator cursor | ✅     | ✅        | `next_free_bit`, totals |
    /// | Heap region (start/sz) | ✅     | ❌        | `talc` heap not snapshot-aware |
    /// | Page tables (PML4/CR3) | ✅     | ❌        | P09 — would need full PML4 walk |
    /// | Driver init state      | ✅     | ❌        | Driver structs not snapshot-aware |
    /// | MHI DRAM bytes         | ✅     | ❌        | MHI state lives in hermes crate |
    /// | Timestamp (tick)       | ✅     | ❌        | Informational only |
    /// | Save count (#)         | ✅     | ❌        | Informational only |
    ///
    /// To add restore for a new subsystem:
    ///   1. Add save/load fields to `Checkpoint`
    ///   2. Implement a `checkpoint_restore(&self)` method on the subsystem
    ///   3. Call it here after the bitmap restore
    pub fn restore_checkpoint(&mut self) -> bool {
        // Try SGDB first
        if crate::sgdb::ready() {
            match crate::sgdb::get_kv("sys/checkpoint") {
                Ok(Some(data)) => {
                    if let Some(cp) = Checkpoint::deserialize(&data) {
                        self.checkpoint = cp;
                        k_nano::slog_kai!("CHECKPOINT", "info", "SGDB load OK @ tick {}", self.checkpoint.tick);
                    }
                }
                Ok(None) => k_nano::slog_kai!("CHECKPOINT", "info", "SGDB miss"),
                Err(e) => k_nano::slog_kai!("CHECKPOINT", "info", "SGDB load error {:?}", e),
            }
        }
        if !self.checkpoint.valid {
            k_nano::slog_kai!("CHECKPOINT", "info", "Nenhum checkpoint valido para restaurar.");
            return false;
        }
        k_nano::slog_kai!("CHECKPOINT", "info", "Restaurando checkpoint #{} v{} @ tick {}...",
            self.checkpoint.save_count, self.checkpoint.checkpoint_version, self.checkpoint.tick);
        let mut guard = GLOBAL_ALLOCATOR.lock();
        if let Some(ref mut alloc) = *guard {
            alloc.bitmap = self.checkpoint.bitmap;
            alloc.next_free_bit = self.checkpoint.next_free_bit;
            alloc.total_frames = self.checkpoint.total_frames;
            alloc.usable_frames = self.checkpoint.usable_frames;
            alloc.allocated_count = self.checkpoint.allocated_count;
        }
        drop(guard);
        k_nano::slog_kai!("CHECKPOINT", "info",
            "RESTORED bitmap={}/{} frames allocated_count={} heap={:#x}+{}MB",
            self.checkpoint.next_free_bit, self.checkpoint.total_frames,
            self.checkpoint.allocated_count,
            self.checkpoint.heap_start,
            self.checkpoint.heap_size / (1024 * 1024));
        k_nano::slog_kai!("CHECKPOINT", "warn",
            "BEST-EFFORT: page_tables(pml4={:#x}) heap_talc drivers(mhi={},hash={:#x}) NOT restored — subsystems not checkpoint-aware (P09)",
            self.checkpoint.page_table_pml4_addr,
            self.checkpoint.mhi_dram_bytes,
            self.checkpoint.driver_state_hash);
        k_nano::slog_kai!("SELF-HEAL", "info",
            "checkpoint loaded: saved={} version={} heap={}",
            self.checkpoint.save_count,
            self.checkpoint.checkpoint_version,
            self.checkpoint.heap_size);
        true
    }

    fn already_tried(&self, msg: &str, action: &str) -> bool {
        self.lessons.iter().any(|l| l.error_msg == msg && l.attempted_action == action)
    }

    pub fn record_failure(&mut self, msg: String, action: String, tick: u64) {
        k_nano::slog_kai!("SELF", "HEAL", "Falha registrada: '{}' + '{}'", msg, action);
        self.lessons.push(FailedStrategy { error_msg: msg, attempted_action: action, tick });
    }

    pub fn analyze(&mut self, ctx: &ErrorContext, recover: bool) -> RecoveryAction {
        let class = FailureClass::classify(ctx.kind, &ctx.message);
        k_nano::slog_kai!("SELF", "HEAL", "{:?}: {} daemon '{}' ({} lessons)", class, ctx.kind, ctx.daemon, self.lessons.len());

        if !recover { return RecoveryAction::LogAndContinue; }

        if class == FailureClass::MemoryFault && !self.already_tried(&ctx.message, "restart") {
            self.lessons.push(FailedStrategy { error_msg: ctx.message.clone(), attempted_action: String::from("restart"), tick: ctx.tick });
            return RecoveryAction::RestartDaemon(ctx.daemon.clone(), None);
        }
        if class == FailureClass::ResourceFault && !self.already_tried(&ctx.message, "create") {
            let fix = format!("Criar: {}", ctx.message);
            self.pending_fixes.push((ctx.daemon.clone(), fix.clone()));
            self.lessons.push(FailedStrategy { error_msg: ctx.message.clone(), attempted_action: String::from("create"), tick: ctx.tick });
            
            // Corrective Prompting: publish LLM_REQUEST with error context
            let prompt = alloc::format!(
                "Error '{}' in '{}'. Context: daemon={}, ring={}, tick={}. History: {}. Generate minimal recovery skill or fix.",
                ctx.message, ctx.file, ctx.daemon, ctx.ring, ctx.tick,
                {
                    let mut s = alloc::string::String::new();
                    for (i, l) in self.lessons.iter().enumerate() {
                        if i > 0 { s.push_str("; "); }
                        s.push_str(&l.error_msg);
                    }
                    s
                }
            );
            let _ = k_nano::EVENT_BUS.publish(Event {
                id: 0,
                topic: "LLM_REQUEST".into(),
                payload: prompt.into_bytes(),
                token: CapabilityToken::Legacy(1),
            });
            
            return RecoveryAction::CreateSkill(ctx.daemon.clone(), fix, None);
        }
        RecoveryAction::LogAndContinue
    }

    pub fn list_pending(&self) -> Vec<String> {
        self.pending_fixes.iter().map(|(d, f)| format!("[{}] {}", d, f)).collect()
    }
}

impl ErrorContext {
    pub fn from_event_bytes(payload: &[u8]) -> Result<Self, &'static str> {
        let s = core::str::from_utf8(payload).map_err(|_| "invalid utf8")?;
        let kind = if s.starts_with("#PF") { "PageFault" } else if s.starts_with("#GP") { "GeneralProtection" } else { "Unknown" };
        Ok(ErrorContext {
            kind,
            message: s.to_string(),
            file: "exception_handler".into(),
            line: 0,
            ring: 0,
            daemon: "kernel".into(),
            tick: k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64,
        })
    }
}

/// M6: Classify failure by error code (for exception handlers)
pub fn classify_by_code(code: u32) -> FailureClass {
    match code {
        0x00..=0x0F => FailureClass::MemoryFault,
        0x10..=0x1F => FailureClass::ExecutionFault,
        0x20..=0x2F => FailureClass::ResourceFault,
        0x40..=0x4F => FailureClass::LogicFault,
        _ => FailureClass::ExternalFault,
    }
}


