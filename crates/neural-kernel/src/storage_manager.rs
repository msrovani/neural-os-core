//! Storage Manager — interface para gerenciamento de armazenamento.
//! Integrado com DiskIntelligenceAgent, MHI, ARC cache, VFS.
//! Fornece dados para o App Storage no Settings.

use alloc::string::String;

pub fn storage_report() -> String {
    let mut s = String::from("=== Gerenciador de Armazenamento ===\n");
    if !crate::disk_agent::DISK_AGENT_INIT.load(core::sync::atomic::Ordering::Relaxed) {
        s.push_str("DiskAgent: nao inicializado\n"); return s;
    }
    s.push_str(&alloc::format!("MHI: {} alocacoes\n", crate::mhi::MHI_REGISTRY.lock().len()));

    // Montagens VFS
    s.push_str("\nMontagens:\n");
    if let Some(ref vfs) = *crate::vfs::VFS.lock() {
        for mount in vfs.mount_table() {
            s.push_str(&alloc::format!("  {} -> {}\n", mount.mount_point, mount.agent_name));
        }
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
