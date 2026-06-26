use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use crate::serial_println;

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

#[derive(Clone, Debug)]
pub struct FailedStrategy {
    pub error_msg: String,
    pub attempted_action: String,
    pub tick: u64,
}

pub enum RecoveryAction {
    LogAndContinue,
    RestartDaemon(String),
    CreateSkill(String, String),
    CheckpointRestore,
}

pub struct SelfHeal {
    pub pending_fixes: Vec<(String, String)>,
    pub lessons: Vec<FailedStrategy>,
}

impl SelfHeal {
    pub const fn new() -> Self {
        SelfHeal { pending_fixes: Vec::new(), lessons: Vec::new() }
    }

    fn already_tried(&self, msg: &str, action: &str) -> bool {
        self.lessons.iter().any(|l| l.error_msg == msg && l.attempted_action == action)
    }

    pub fn record_failure(&mut self, msg: String, action: String, tick: u64) {
        serial_println!("[SELF-HEAL] Registrando falha: '{}' com '{}' falhou. Aprendendo...", msg, action);
        self.lessons.push(FailedStrategy { error_msg: msg, attempted_action: action, tick });
    }

    pub fn analyze(&mut self, ctx: &ErrorContext) -> RecoveryAction {
        let msg_slice = &ctx.message;
        serial_println!("[SELF-HEAL] Erro: {} | {}:{} | ring {} | daemon '{}' | tick {}",
            ctx.kind, ctx.file, ctx.line, ctx.ring, ctx.daemon, ctx.tick);

        // Check for OOM
        if msg_slice.contains("OOM") || msg_slice.contains("out of memory") {
            if !self.already_tried(msg_slice, "compact_heap") {
                self.lessons.push(FailedStrategy {
                    error_msg: ctx.message.clone(), attempted_action: String::from("compact_heap"), tick: ctx.tick,
                });
                serial_println!("[SELF-HEAL] OOM: compactar heap. (aprendizado: se falhar, tentar swap no MHI)");
                return RecoveryAction::LogAndContinue;
            }
            serial_println!("[SELF-HEAL] OOM: compactar ja tentado. Sugerindo: matar skill menos prioritarias.");
            return RecoveryAction::LogAndContinue;
        }

        // Check for Page Fault
        if ctx.kind.contains("PageFault") {
            if !self.already_tried(msg_slice, "restart_daemon") {
                self.lessons.push(FailedStrategy {
                    error_msg: ctx.message.clone(), attempted_action: String::from("restart_daemon"), tick: ctx.tick,
                });
                serial_println!("[SELF-HEAL] Page Fault: reiniciando daemon '{}'.", ctx.daemon);
                return RecoveryAction::RestartDaemon(ctx.daemon.clone());
            }
            serial_println!("[SELF-HEAL] Page Fault: restart ja tentado. Sugerindo: desabilitar modulo problematico.");
            return RecoveryAction::LogAndContinue;
        }

        // Check for missing skill
        if msg_slice.contains("skill nao encontrada") || msg_slice.contains("not found") {
            let key = format!("create_skill:{}", ctx.daemon);
            if !self.already_tried(msg_slice, &key) {
                let fix = format!("Registrar skill faltante: {} (sugerida pelo LLM)", ctx.message);
                self.pending_fixes.push((ctx.daemon.clone(), fix.clone()));
                self.lessons.push(FailedStrategy {
                    error_msg: ctx.message.clone(), attempted_action: key, tick: ctx.tick,
                });
                serial_println!("[SELF-HEAL] Pendencia registrada: {}", fix);
                return RecoveryAction::CreateSkill(ctx.daemon.clone(), fix);
            }
            serial_println!("[SELF-HEAL] Skill ja solicitada antes. Sugerindo: implementar manualmente.");
            return RecoveryAction::LogAndContinue;
        }

        serial_println!("[SELF-HEAL] Erro desconhecido. Logando. Lições aprendidas: {}", self.lessons.len());
        RecoveryAction::LogAndContinue
    }

    pub fn list_pending(&self) -> Vec<String> {
        self.pending_fixes.iter().map(|(d, f)| format!("[{}] {}", d, f)).collect()
    }

    pub fn list_lessons(&self) -> Vec<String> {
        self.lessons.iter().map(|l| format!("[tick {}] '{}' -> '{}' FALHOU", l.tick, l.error_msg, l.attempted_action)).collect()
    }
}
