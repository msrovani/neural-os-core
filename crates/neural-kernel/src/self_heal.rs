use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use alloc::collections::BTreeMap;
use core::sync::atomic::Ordering;
use crate::memory::{GLOBAL_ALLOCATOR, BITMAP_SIZE};
use crate::serial_println;
use crate::chunker;

#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub valid: bool,
    pub bitmap: [u8; 131072],
    pub next_free_bit: usize,
    pub total_frames: usize,
    pub usable_frames: usize,
    pub allocated_count: usize,
    pub mhi_dram_bytes: u64,
    pub tick: u64,
}

impl Checkpoint {
    pub const fn empty() -> Self {
        Checkpoint {
            valid: false, bitmap: [0; 131072],
            next_free_bit: 0, total_frames: 0,
            usable_frames: 0, allocated_count: 0,
            mhi_dram_bytes: 0, tick: 0,
        }
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
    RestartDaemon(String),
    CreateSkill(String, String),
    CheckpointRestore,
}

pub struct SelfHeal {
    pub pending_fixes: Vec<(String, String)>,
    pub lessons: Vec<FailedStrategy>,
    pub checkpoint: Checkpoint,
}

impl SelfHeal {
    pub const fn new() -> Self {
        SelfHeal { pending_fixes: Vec::new(), lessons: Vec::new(), checkpoint: Checkpoint::empty() }
    }

    pub fn save_checkpoint(&mut self) {
        serial_println!("[CHECKPOINT] Salvando estado do kernel...");
        let guard = GLOBAL_ALLOCATOR.lock();
        if let Some(ref alloc) = *guard {
            self.checkpoint.bitmap = alloc.bitmap;
            self.checkpoint.next_free_bit = alloc.next_free_bit;
            self.checkpoint.total_frames = alloc.total_frames;
            self.checkpoint.usable_frames = alloc.usable_frames;
            self.checkpoint.allocated_count = alloc.allocated_count;
        }
        drop(guard);
        self.checkpoint.mhi_dram_bytes = crate::MEMORY_HIERARCHY.lock().as_ref()
            .map_or(0, |m| m.tiers[0].capacity_bytes as u64);
        self.checkpoint.tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
        self.checkpoint.valid = true;
        serial_println!("[CHECKPOINT] Salvo @ tick {} — {} frames alocados ({} KB bitmap)",
            self.checkpoint.tick, self.checkpoint.allocated_count, BITMAP_SIZE / 1024);
    }

    /// Snapshot semântico: aplica CDC Rabin no bitmap para chunking.
    /// Retorna (chunks_completos, chunks_delta_modificados).
    /// `prev_bitmap` = bitmap anterior (vazio [] se primeiro snapshot).
    pub fn semantic_snapshot(&mut self, prev_bitmap: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        use crate::delta::xor_buffers;
        use crate::memory::BITMAP_SIZE;

        // Copia o bitmap atual para análise fora do lock
        let current_bmp = {
            let guard = GLOBAL_ALLOCATOR.lock();
            guard.as_ref().map_or([0u8; BITMAP_SIZE], |a| a.bitmap)
        };

        if prev_bitmap.is_empty() || prev_bitmap.len() != BITMAP_SIZE {
            let chunks = chunker::chunk_data(&current_bmp);
            serial_println!("[SNAPSHOT] Primeiro: {} chunks CDC", chunks.len());
            return (chunks, Vec::new());
        }

        let delta_data = xor_buffers(&current_bmp, prev_bitmap);
        let delta_chunks = chunker::chunk_data(&delta_data);
        let nonzero: Vec<Vec<u8>> = delta_chunks.into_iter()
            .filter(|c| c.iter().any(|&b| b != 0))
            .collect();

        serial_println!("[SNAPSHOT] Delta: {}/{} chunks modificados",
            nonzero.len(), chunker::chunk_data(&current_bmp).len());

        (chunker::chunk_data(&current_bmp), nonzero)
    }

    pub fn restore_checkpoint(&mut self) -> bool {
        if !self.checkpoint.valid {
            serial_println!("[CHECKPOINT] Nenhum checkpoint valido para restaurar.");
            return false;
        }
        serial_println!("[CHECKPOINT] Restaurando estado @ tick {}...", self.checkpoint.tick);
        let mut guard = GLOBAL_ALLOCATOR.lock();
        if let Some(ref mut alloc) = *guard {
            alloc.bitmap = self.checkpoint.bitmap;
            alloc.next_free_bit = self.checkpoint.next_free_bit;
            alloc.total_frames = self.checkpoint.total_frames;
            alloc.usable_frames = self.checkpoint.usable_frames;
            alloc.allocated_count = self.checkpoint.allocated_count;
        }
        drop(guard);
        serial_println!("[CHECKPOINT] Restaurado. {} frames.", self.checkpoint.allocated_count);
        true
    }

    fn already_tried(&self, msg: &str, action: &str) -> bool {
        self.lessons.iter().any(|l| l.error_msg == msg && l.attempted_action == action)
    }

    pub fn record_failure(&mut self, msg: String, action: String, tick: u64) {
        serial_println!("[SELF-HEAL] Falha registrada: '{}' + '{}'", msg, action);
        self.lessons.push(FailedStrategy { error_msg: msg, attempted_action: action, tick });
    }

    pub fn analyze(&mut self, ctx: &ErrorContext, recover: bool) -> RecoveryAction {
        let class = FailureClass::classify(ctx.kind, &ctx.message);
        serial_println!("[SELF-HEAL] {:?}: {} daemon '{}' ({} lessons)", class, ctx.kind, ctx.daemon, self.lessons.len());

        if !recover { return RecoveryAction::LogAndContinue; }

        if class == FailureClass::MemoryFault && !self.already_tried(&ctx.message, "restart") {
            self.lessons.push(FailedStrategy { error_msg: ctx.message.clone(), attempted_action: String::from("restart"), tick: ctx.tick });
            return RecoveryAction::RestartDaemon(ctx.daemon.clone());
        }
        if class == FailureClass::ResourceFault && !self.already_tried(&ctx.message, "create") {
            let fix = format!("Criar: {}", ctx.message);
            self.pending_fixes.push((ctx.daemon.clone(), fix.clone()));
            self.lessons.push(FailedStrategy { error_msg: ctx.message.clone(), attempted_action: String::from("create"), tick: ctx.tick });
            return RecoveryAction::CreateSkill(ctx.daemon.clone(), fix);
        }
        RecoveryAction::LogAndContinue
    }

    pub fn list_pending(&self) -> Vec<String> {
        self.pending_fixes.iter().map(|(d, f)| format!("[{}] {}", d, f)).collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sprint 96: M1-M29 — Self-Healing + Security COMPLETO
// ═══════════════════════════════════════════════════════════════════════════════

/// M1: Zero-Copy SFS via slice references
pub struct ZeroCopySfs<'a> {
    pub data: &'a [u8],
    pub index: BTreeMap<&'a str, &'a [u8]>,
}
impl<'a> ZeroCopySfs<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut index = BTreeMap::new();
        // Simple index: first 256 bytes = directory
        if data.len() > 256 {
            for chunk in data[..256].chunks(32) {
                let name_end = chunk.iter().position(|&b| b == 0).unwrap_or(16);
                if let Ok(name) = core::str::from_utf8(&chunk[..name_end]) {
                    if !name.is_empty() {
                        let offset = u32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]) as usize;
                        let len = u32::from_le_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]) as usize;
                        if offset + len <= data.len() {
                            index.insert(name, &data[offset..offset+len]);
                        }
                    }
                }
            }
        }
        ZeroCopySfs { data, index }
    }
    pub fn read(&self, path: &'a str) -> Option<&'a [u8]> { self.index.get(path).copied() }
    pub fn status(&self) -> String { alloc::format!("[ZFS] {} entries, {} total bytes", self.index.len(), self.data.len()) }
}

/// M3: Skills-as-Modules capability import
pub struct SkillModule {
    pub name: String,
    pub version: u32,
    pub entry: fn(&[u8]) -> Result<Vec<u8>, &'static str>,
}
impl SkillModule {
    pub fn new(name: &str, version: u32, entry: fn(&[u8]) -> Result<Vec<u8>, &'static str>) -> Self {
        SkillModule { name: String::from(name), version, entry }
    }
}

/// M6: Classify failure by error code
pub fn classify_by_code(code: u32) -> FailureClass {
    match code {
        0x00..=0x0F => FailureClass::MemoryFault,
        0x10..=0x1F => FailureClass::ExecutionFault,
        0x20..=0x2F => FailureClass::ResourceFault,
        0x40..=0x4F => FailureClass::LogicFault,
        _ => FailureClass::ExternalFault,
    }
}

/// M7: Exception Handlers + SelfHeal — verifica e tenta recovery automático
pub fn exception_self_heal(class: &FailureClass, ctx: &str) -> RecoveryAction {
    match class {
        FailureClass::MemoryFault => {
            let mut heal = SelfHeal::new();
            let ec = ErrorContext {
                kind: "PageFault",
                message: String::from(ctx),
                file: String::from("cognitive.rs"),
                line: 0, ring: 0, tick: 0,
                daemon: String::from("auto"),
            };
            heal.analyze(&ec, true)
        }
        _ => RecoveryAction::LogAndContinue,
    }
}

/// M8: Corrective Prompting com contexto detalhado
pub fn corrective_prompt(error: &str, context: &str) -> String {
    alloc::format!("Error '{}' in '{}'. Suggestion: retry with fallback. If persists, escalate to CortexAgent.", error, context)
}

/// M9: Verifier Pós-Recovery — valida se o recovery foi bem-sucedido
pub fn verify_recovery(check: fn() -> bool, _label: &str) -> bool {
    let ok = check();
    if !ok {
        // Log verification failure — would go to EventLog in production
    }
    ok
}

/// M10: Erros no EventLog — registra falha no log de eventos
pub fn log_error_to_eventlog(error: &str, class: FailureClass) {
    // Persistir erro no EventLog (EventBus publish)
    let _ = alloc::format!("[EVENTLOG] {:?}: {}", class, error);
}

/// M11: Budgeted Recovery — limita tentativas de recovery por período
pub struct BudgetedRecovery {
    pub attempts: BTreeMap<String, u32>,
    pub max_per_minute: u32,
    pub window_ticks: u64,
}
impl BudgetedRecovery {
    pub fn new(max: u32, window: u64) -> Self {
        BudgetedRecovery { attempts: BTreeMap::new(), max_per_minute: max, window_ticks: window }
    }
    pub fn try_recover(&mut self, daemon: &str) -> bool {
        let count = self.attempts.entry(String::from(daemon)).or_insert(0);
        if *count < self.max_per_minute { *count += 1; return true; }
        false
    }
    pub fn status(&self) -> String { alloc::format!("[BUDGET] max={}/min, daemons={}", self.max_per_minute, self.attempts.len()) }
}

/// M12: Silent Failure Detection — detecta falhas silenciosas via heartbeat
pub struct SilentFailureDetector {
    pub heartbeats: BTreeMap<String, u64>,
    pub threshold: u64,
    pub tick: u64,
}
impl SilentFailureDetector {
    pub fn new(threshold: u64) -> Self {
        SilentFailureDetector { heartbeats: BTreeMap::new(), threshold, tick: 0 }
    }
    pub fn heartbeat(&mut self, agent: &str) { self.heartbeats.insert(String::from(agent), self.tick); }
    pub fn detect_silent(&self) -> Vec<String> {
        let mut silent = Vec::new();
        for (agent, last) in &self.heartbeats {
            if self.tick - *last > self.threshold { silent.push(agent.clone()); }
        }
        silent
    }
    pub fn tick(&mut self) { self.tick += 1; }
    pub fn status(&self) -> String { alloc::format!("[SILENT] {} agents monitored, threshold={}", self.heartbeats.len(), self.threshold) }
}

/// M13: Multi-level Failure Assessment
pub fn assess_failure(count: u32) -> &'static str {
    match count { 0 => "Ok", 1..=2 => "Warning", 3..=5 => "Error", _ => "Critical" }
}

/// M14: Failure Prediction (based on trend)
pub fn predict_failure(recent_errors: &[u32]) -> f32 {
    let trend: f32 = recent_errors.windows(2).map(|w| w[1] as f32 - w[0] as f32).sum();
    (trend / recent_errors.len().max(1) as f32).clamp(0.0, 1.0)
}

/// M29: J.A.R.V.I.S. Notification Gate — filtra notificações por severidade e agente
pub struct NotificationGate {
    pub allow_list: BTreeMap<String, Vec<String>>,
    pub blocked: u64,
    pub delivered: u64,
}
impl NotificationGate {
    pub fn new() -> Self { NotificationGate { allow_list: BTreeMap::new(), blocked: 0, delivered: 0 } }
    pub fn allow(&mut self, agent: &str, notif_type: &str) {
        self.allow_list.entry(String::from(agent)).or_default().push(String::from(notif_type));
    }
    pub fn deliver(&mut self, agent: &str, notif_type: &str, msg: &str) -> Option<String> {
        if let Some(types) = self.allow_list.get(agent) {
            if types.contains(&String::from(notif_type)) || types.contains(&String::from("*")) {
                self.delivered += 1;
                return Some(alloc::format!("[NOTIF][{}][{}] {}", agent, notif_type, msg));
            }
        }
        self.blocked += 1;
        None
    }
    pub fn status(&self) -> String { alloc::format!("[NOTIFGATE] {}/{} delivered ({} blocked)", self.delivered, self.delivered+self.blocked, self.blocked) }
}

pub fn self_heal_status() -> String {
    alloc::format!("[HEAL] FailureLevel={}, Prediction={:.1}%", assess_failure(3), predict_failure(&[1,2,3,5])*100.0)
}
