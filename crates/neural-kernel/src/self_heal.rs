use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use core::sync::atomic::Ordering;
use crate::memory::{GLOBAL_ALLOCATOR, BITMAP_SIZE};
use crate::serial_println;

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
        self.checkpoint.mhi_dram_bytes = crate::mhi::MEMORY_HIERARCHY.lock().as_ref()
            .map_or(0, |m| m.tiers[0].capacity_bytes);
        self.checkpoint.tick = crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed);
        self.checkpoint.valid = true;
        serial_println!("[CHECKPOINT] Salvo @ tick {} — {} frames alocados",
            self.checkpoint.tick, self.checkpoint.allocated_count);
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
