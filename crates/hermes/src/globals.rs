//! Globals Hermes — Trust/SelfHeal/Agency/Audit reais via k_ai (sem stubs vazios).

use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use ticket_lock::TicketLock;
use k_ai::conversation::EventLog;
use k_ai::trust::TrustCache;
use k_ai::usage::UsageTracker;
use k_ai::self_heal::SelfHeal;
use k_ai::audit::AuditTrail;
use k_ai::inventory::SystemArchitecture;
use crate::executive::ExecutiveSupervisor;

/// Trinity único no crate cortex (SESSION_273). Não duplicar static vazio.
pub use cortex::trinity::TRINITY;

pub use k_nano::{EVENT_BUS, SKILL_REGISTRY};

/// Re-export módulos reais do Ring 1 (substitui stubs monólito).
pub use k_ai::agency;
pub use k_ai::hw_agents;
pub use k_ai::boot_log_agent;
pub use k_ai::inventory;
pub use k_ai::audit;
pub use k_ai::cognitive::BitNetTrainer;

pub const LOG_SECTOR: u32 = 2048;

#[derive(Clone, Copy)]
pub struct VfsBridge {
    pub read: fn(&str) -> Result<Vec<u8>, &'static str>,
    pub write: fn(&str, &[u8]) -> Result<(), &'static str>,
    pub list: fn(&str) -> Result<Vec<String>, &'static str>,
}

lazy_static! {
    pub static ref APPROVAL_GATE: TicketLock<crate::approval::ApprovalGate> =
        TicketLock::new(crate::approval::ApprovalGate::new());
    pub static ref SKILL_STORAGE: TicketLock<crate::skill_loader::SkillLoader> =
        TicketLock::new(crate::skill_loader::SkillLoader::new());
    pub static ref TRUST_CACHE: TicketLock<TrustCache> = TicketLock::new(TrustCache::new());
    pub static ref USAGE_TRACKER: TicketLock<UsageTracker> = TicketLock::new(UsageTracker::new());
    pub static ref EVENT_LOG: TicketLock<EventLog> = TicketLock::new(EventLog::new());
    pub static ref CONVERSATION_TRACKER: TicketLock<crate::hermes::ConversationTracker> =
        TicketLock::new(crate::hermes::ConversationTracker::new());
    pub static ref PENDING_SKILL: TicketLock<Option<(String, String)>> = TicketLock::new(None);
    pub static ref SELF_HEAL: TicketLock<SelfHeal> = TicketLock::new(SelfHeal::new());
    pub static ref BITNET_TRAINER: TicketLock<BitNetTrainer> =
        TicketLock::new(BitNetTrainer::new());
    pub static ref SYSTEM_ARCH: Mutex<Option<SystemArchitecture>> = Mutex::new(None);
    pub static ref MEMORY_HIERARCHY: Mutex<Option<k_nano::mhi::MemoryHierarchy>> = Mutex::new(None);
    pub static ref AUDIT_TRAIL: Mutex<AuditTrail> = Mutex::new(AuditTrail::new());
    pub static ref EXECUTIVE_SUPERVISOR: Mutex<Option<ExecutiveSupervisor>> = Mutex::new(None);
    static ref VFS_BRIDGE: TicketLock<Option<VfsBridge>> = TicketLock::new(None);
}

/// Instala o VFS do bin de integração sem criar dependência hermes → neural-kernel.
pub fn install_vfs_bridge(bridge: VfsBridge) {
    *VFS_BRIDGE.lock() = Some(bridge);
}

pub fn read_vfs(path: &str) -> Result<Vec<u8>, &'static str> {
    if let Some(bridge) = *VFS_BRIDGE.lock() {
        return (bridge.read)(path);
    }
    k_nano::fs::read_vfs(path)
}

pub fn write_vfs(path: &str, data: &[u8]) -> Result<(), &'static str> {
    if let Some(bridge) = *VFS_BRIDGE.lock() {
        return (bridge.write)(path, data);
    }
    k_nano::fs::write_vfs(path, data)
}

pub fn list_vfs(path: &str) -> Result<Vec<String>, &'static str> {
    if let Some(bridge) = *VFS_BRIDGE.lock() {
        return (bridge.list)(path);
    }
    k_nano::fs::list_vfs(path)
}






