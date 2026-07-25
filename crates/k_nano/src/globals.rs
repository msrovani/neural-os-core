// Global driver instances shared across K²CHJ crates.
// Initialized at boot by neural-kernel's main.rs.
use spin::Mutex;
use lazy_static::lazy_static;

pub static ATA_DRIVER: Mutex<Option<crate::ata::AtaDriver>> = Mutex::new(None); // ponytail: Option::None is const
pub static USB_MSC: Mutex<Option<crate::usb_msc::UsbMassStorage>> = Mutex::new(None);

lazy_static! {
    /// Shared event bus for all K²CHJ crates.
    pub static ref EVENT_BUS: event_bus::EventBus = event_bus::EventBus::new();
    /// ADR-0047 LatentBus — hidden-state channel (parallel to EventBus).
    pub static ref LATENT_BUS: event_bus::LatentBus = event_bus::LatentBus::new();
    /// ADR-0068 MessageBus — ponto-a-ponto entre agentes (complementa EventBus).
    pub static ref MESSAGE_BUS: event_bus::MessageBus = event_bus::MessageBus::new();
}

// ponytail: SKILL_REGISTRY stub for cross-crate access.
// neural-kernel's main.rs initializes the real one (with pre-loaded skills).
lazy_static! {
    pub static ref SKILL_REGISTRY: ticket_lock::TicketLock<skill_registry::SkillRegistry> =
        ticket_lock::TicketLock::new(skill_registry::SkillRegistry::new());
}


