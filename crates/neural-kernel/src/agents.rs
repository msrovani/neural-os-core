//! Facade — canônico em hermes_crate::agents (C3).
//! Bin mantém compat `crate::agents::Foo` via re-export, sem duplicação.

pub use hermes_crate::agents::AutoLearnAgent;
pub use hermes_crate::agents::BootSelfHealAgent;
pub use hermes_crate::agents::BootTrustAgent;
pub use hermes_crate::agents::ConsoleAgent;
pub use hermes_crate::agents::CortexAgent;
pub use hermes_crate::agents::DiagnosticSkill;
pub use hermes_crate::agents::FsBridgeAgent;
pub use hermes_crate::agents::GpuDriverAgent;
pub use hermes_crate::agents::HermesAgent;
pub use hermes_crate::agents::HwBridgeAgent;
pub use hermes_crate::agents::HwDetectAgent;
pub use hermes_crate::agents::HwSpecialistAgent;
pub use hermes_crate::agents::InputAgent;
pub use hermes_crate::agents::MemoryAgent;
pub use hermes_crate::agents::MonitorAgent;
pub use hermes_crate::agents::NetAgent;
pub use hermes_crate::agents::NetDriverAgent;
pub use hermes_crate::agents::PlatformAgent;
pub use hermes_crate::agents::SelfEvolveAgent;
pub use hermes_crate::agents::SleepCycleAgent;
pub use hermes_crate::agents::SpecialistAgent;
pub use hermes_crate::agents::UsbDriverAgent;
pub use hermes_crate::agents::TOPIC_KEY_EVENT;
pub use hermes_crate::agents::init_platform_sync;
pub use hermes_crate::agents::register_agency_agents;
pub use hermes_crate::agents::register_hw_agents;
pub use hermes_crate::agents::report_unmatched_intent;
pub use hermes_crate::agents::log_analyst_agent;
pub use hermes_crate::agents::mouse_agent;
pub use hermes_crate::agents::sysinfo_agent;
