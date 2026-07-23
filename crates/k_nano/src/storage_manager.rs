//! Storage Manager — interface para gerenciamento de armazenamento.
//! Integrado com DiskIntelligenceAgent, MHI, ARC cache, VFS + StorageBus (ADR-0062 P2).

use alloc::string::String;

pub fn storage_report() -> String {
    let mut s = String::from("=== Gerenciador de Armazenamento ===\n");
    if !crate::disk_agent::DISK_AGENT_INIT.load(core::sync::atomic::Ordering::Relaxed) {
        s.push_str("DiskAgent: nao inicializado\n");
    } else {
        s.push_str("DiskAgent: OK\n");
    }
    s.push_str(&alloc::format!(
        "MHI: {} alocacoes\n",
        crate::mhi::MHI_REGISTRY.lock().len()
    ));

    s.push_str("\n");
    s.push_str(&crate::storage_bus::bus_report());

    s.push_str("\nMontagens VFS:\n");
    if let Some(ref vfs) = *crate::vfs::VFS.lock() {
        for mount in vfs.mount_table() {
            s.push_str(&alloc::format!(
                "  {} -> {}\n",
                mount.mount_point,
                mount.agent_name
            ));
        }
    } else {
        s.push_str("  (VFS ausente)\n");
    }
    s.push_str("\nComandos: /storage, /smart, /mount <dev> <path>, /format <dev>\n");
    s
}

pub fn smart_report(tick: u64) -> String {
    let mut s = String::from("=== SMART ===\n");
    s.push_str(&alloc::format!("Tick atual: {}\n", tick));
    s.push_str("Use o DiskIntelligenceAgent para leitura SMART.\n");
    s
}
