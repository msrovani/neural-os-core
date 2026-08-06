//! AutoInstallerAgent — Agente EventDriven que orquestra instalação (ADR-0079 M2).
//! Escuta tópico SYS_INSTALL, executa HwProfiler + SysInstaller,
//! publica progresso via EventBus.

use alloc::string::String;
use alloc::format;
use core::sync::atomic::{AtomicBool, Ordering};
use agent_core::{Agent, AgentKind, AgentManifest, AgentTickResult, ScheduleKind};
use crate::block_dev::BlockDevice;
use crate::hw_profiler::{self, HwProfile};
use crate::sys_installer::SysInstaller;
use crate::neural_fs::volume::NeuralVolume;

/// Tópico do EventBus para disparar instalação.
pub const TOPIC_SYS_INSTALL: &str = "SYS_INSTALL";
/// Tópico para notificar progresso (Jarbas card escuta).
pub const TOPIC_INSTALL_PROGRESS: &str = "INSTALL_PROGRESS";

pub static INSTALLER_BUSY: AtomicBool = AtomicBool::new(false);

const INSTALL_MANIFEST: AgentManifest = AgentManifest {
    name: "auto-installer",
    kind: AgentKind::System,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: false,
};

pub struct AutoInstallerAgent {
    manifest: AgentManifest,
    receiver: event_bus::Receiver,
}

impl AutoInstallerAgent {
    pub fn new() -> Self {
        let rx = crate::EVENT_BUS.subscribe(TOPIC_SYS_INSTALL);
        Self {
            manifest: INSTALL_MANIFEST,
            receiver: rx,
        }
    }

    /// Executa instalação completa no target BlockDevice.
    /// `source` — boot device com ESP contendo bootloader.
    /// `target` — disco alvo (HD/SSD/NVMe).
    /// `kernel_elf` — bytes do kernel.elf.
    pub fn run_install(
        &self,
        source: &mut dyn BlockDevice,
        target: &mut dyn BlockDevice,
        kernel_elf: &[u8],
    ) -> Result<String, &'static str> {
        if INSTALLER_BUSY.load(Ordering::Relaxed) {
            return Err("installer already running");
        }
        INSTALLER_BUSY.store(true, Ordering::Relaxed);

        // 1. Perfil de HW
        Self::publish_progress("Detectando hardware...", 5);
        let profile = hw_profiler::profile_hardware();
        let hw_summary = profile.summary();

        // 2. Valida target
        let total_lba = target.total_sectors();
        if total_lba < 2048 + 512 + 64 {
            INSTALLER_BUSY.store(false, Ordering::Relaxed);
            return Err("target too small");
        }

        // 3. Executa instalação (cria GPT dual, copia ESP, cria NeuralFS, copia kernel)
        Self::publish_progress(&format!("Criando GPT..."), 10);
        let mut installer = SysInstaller::new();
        installer.install(source, target, kernel_elf)?;

        Self::publish_progress("NeuralFS formatado, kernel copiado", 70);

        // 4. Copia catálogo de skills para o target
        Self::publish_progress("Copiando skills...", 75);
        let _ = Self::copy_skill_manifest(target, profile);

        // 5. Finaliza
        Self::publish_progress("Instalação completa!", 100);
        INSTALLER_BUSY.store(false, Ordering::Relaxed);

        Ok(hw_summary)
    }

    /// Copia manifesto das skills builtin para o NeuralFS do target.
    fn copy_skill_manifest(target: &mut dyn BlockDevice, profile: HwProfile) -> Result<(), &'static str> {
        // Encontra partição NeuralFS no target (assumimos LBA neural_start ~= total - data - 34)
        let total_lba = target.total_sectors();
        let esp_size = 1_048_576u64.min(total_lba.saturating_sub(2048 + 64) / 2);
        let neural_start = 2048u64 + esp_size;

        let mut vol = NeuralVolume::mount(target, neural_start)
            .ok_or("NeuralFS mount for skills failed")?;

        // Cria /skills/ directory
        let skills_ino = vol.create_file(target, 1, "skills")
            .map_err(|_| "create /skills dir failed")?;

        // Serializa catálogo de skills do registry
        let reg = crate::SKILL_REGISTRY.lock();
        let skill_list = reg.list_skills();
        let mut catalog = alloc::string::String::from("# Builtin Skills Catalog\n\n");
        for (name, policy) in &skill_list {
            catalog.push_str(&alloc::format!("- {} (enabled={})\n", name, policy.enabled));
        }
        let catalog_bytes = catalog.as_bytes();
        let sk_ino = vol.create_file(target, skills_ino, "CATALOG.MD")
            .map_err(|_| "create CATALOG.MD failed")?;
        vol.write_file(target, sk_ino, catalog_bytes)
            .map_err(|_| "write CATALOG.MD failed")?;

        // Salva perfil HW como /config/hw_profile.txt
        let config_ino = vol.create_file(target, 1, "config")
            .map_err(|_| "create /config dir failed")?;
        let hw_bytes = profile.summary();
        let hprof_ino = vol.create_file(target, config_ino, "hw_profile.txt")
            .map_err(|_| "create hw_profile.txt failed")?;
        vol.write_file(target, hprof_ino, hw_bytes.as_bytes())
            .map_err(|_| "write hw_profile.txt failed")?;

        // ponytail: skills WASM do pendrive serão copiadas na Fase 1 (SmartFileCopier)
        Ok(())
    }

    fn publish_progress(step: &str, percent: u8) {
        let _ = crate::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: String::from(TOPIC_INSTALL_PROGRESS),
            payload: format!("{{step:{},progress:{}}}", step, percent).into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
        crate::slog_nano!("INSTALL", "info", "{} ({}%)", step, percent);
    }
}

impl Agent for AutoInstallerAgent {
    fn manifest(&self) -> &AgentManifest {
        &self.manifest
    }

    fn tick(&mut self, _tick: u64, _tick_count: u64) -> AgentTickResult {
        if !self.receiver.has_pending() {
            return AgentTickResult::Pending;
        }
        // SYS_INSTALL recebido (shell `install` / intent) → executa a instalação:
        // source = boot device (ATA), target = 1º disco da StorageBus (ADR-0086 I2/I3).
        while let Some(ev) = self.receiver.try_receive() {
            crate::slog_nano!("INSTALL", "info", "SYS_INSTALL event received: {:?}", ev.topic);
            let result = self.run_install_from_bus();
            crate::slog_nano!(
                "INSTALL",
                "info",
                "instalacao: {}",
                result.as_deref().unwrap_or("FAILED")
            );
        }
        AgentTickResult::Pending
    }
}

impl AutoInstallerAgent {
    /// Dispara a instalação com source=boot (ATA) e target=1º disco não-boot.
    /// ponytail: target = AHCI → NVMe → USB (globals vivos); sem UI de seleção
    /// (menu de disco fica como refinamento — I3 fase 1.8).
    fn run_install_from_bus(&self) -> Result<String, &'static str> {
        // source: boot device (ATA) — lê o kernel.elf da ESP
        let mut ata_guard = crate::globals::ATA_DRIVER.lock();
        let Some(ata) = ata_guard.as_mut() else {
            return Err("sem ATA (boot device ausente)");
        };
        let parts = unsafe { crate::fat32::read_mbr(ata) };
        let mut kernel_elf: Option<alloc::vec::Vec<u8>> = None;
        for p in &parts {
            if matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
                if let Some(fs) = unsafe { crate::fat32::Fat32Reader::new(ata, p) } {
                    if let Some(k) = unsafe { fs.read_file("kernel.elf") } {
                        kernel_elf = Some(k);
                        break;
                    }
                }
            }
        }
        let kernel = kernel_elf.ok_or("kernel.elf nao encontrado no boot")?;
        crate::slog_nano!("INSTALL", "info", "kernel.elf lido: {} bytes", kernel.len());

        // target: 1º disco não-boot (AHCI → NVMe → USB) via globals
        let mut inst = SysInstaller::new();
        if let Some(g) = crate::globals::AHCI_DRIVER.lock().as_mut() {
            let dev: &mut dyn BlockDevice = g;
            inst.install(ata, dev, &kernel)?;
            return Ok(alloc::format!("instalado em AHCI ({} bytes)", inst.bytes_copied));
        }
        if let Some(g) = crate::disk_agent::nvme::NVME_DRIVER.lock().as_mut() {
            let dev: &mut dyn BlockDevice = g;
            inst.install(ata, dev, &kernel)?;
            return Ok(alloc::format!("instalado em NVMe ({} bytes)", inst.bytes_copied));
        }
        if let Some(g) = crate::globals::USB_MSC.lock().as_mut() {
            let dev: &mut dyn BlockDevice = g;
            inst.install(ata, dev, &kernel)?;
            return Ok(alloc::format!("instalado em USB ({} bytes)", inst.bytes_copied));
        }
        Err("nenhum disco alvo (AHCI/NVMe/USB) disponivel")
    }
}
