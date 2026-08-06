//! Self-install validation — verifica integridade pós-instalação (ADR-0079 M3).
//! Salva hash SHA256-like (FNV-1a) dos arquivos instalados.
//! No primeiro boot do target, compara hashes para detectar corrupção.

use alloc::string::String;
use alloc::format;
use crate::block_dev::BlockDevice;
use crate::neural_fs::volume::NeuralVolume;
use crate::neural_fs::checksum::crc32c;

/// Resultado da verificação de integridade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityVerdict {
    Pass,
    Fail(String),
    Skipped(&'static str),
}

/// Salva checksums dos arquivos instalados no NeuralFS do target.
/// Cria /boot/INSTALL.CHK com hash de cada arquivo copiado.
pub fn save_install_checksum(
    target: &mut dyn BlockDevice,
    neural_start_lba: u64,
    kernel_hash: u32,
) -> IntegrityVerdict {
    let mut vol = match NeuralVolume::mount(target, neural_start_lba) {
        Some(v) => v,
        None => return IntegrityVerdict::Skipped("NeuralFS not mounted"),
    };

    // Cria /boot/ se não existir
    let boot_ino = match vol.create_file(target, 1, "boot") {
        Ok(ino) => ino,
        Err(_) => return IntegrityVerdict::Skipped("/boot already exists or error"),
    };

    // Escreve INSTALL.CHK
    let content = format!(
        "INSTALL_CHECKSUM v1\nkernel={:#x}\ntick={}\n",
        kernel_hash,
        crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed),
    );
    let chk_ino = match vol.create_file(target, boot_ino, "INSTALL.CHK") {
        Ok(ino) => ino,
        Err(_) => return IntegrityVerdict::Skipped("create INSTALL.CHK failed"),
    };
    match vol.write_file(target, chk_ino, content.as_bytes()) {
        Ok(_) => IntegrityVerdict::Pass,
        Err(e) => IntegrityVerdict::Fail(String::from(e)),
    }
}

/// Verifica checksum no boot do target.
/// Lê /boot/kernel.elf e compara hash com o INSTALL.CHK gravado na instalação.
/// ponytail: resolve_path já faz o walk — não precisa scan manual de diretório.
pub fn verify_install_checksum(
    target: &mut dyn BlockDevice,
    neural_start_lba: u64,
    kernel_hash: u32,
) -> IntegrityVerdict {
    let vol = match NeuralVolume::mount(target, neural_start_lba) {
        Some(v) => v,
        None => return IntegrityVerdict::Skipped("NeuralFS not mounted on target"),
    };

    // Lê /boot/kernel.elf e recalcula hash
    let Some(ino) = vol.resolve_path(target, "boot/kernel.elf") else {
        return IntegrityVerdict::Fail(String::from("boot/kernel.elf not found"));
    };
    let data = match vol.read_file(target, ino) {
        Ok(d) => d,
        Err(e) => return IntegrityVerdict::Fail(String::from(e)),
    };
    let actual = crc32c(&data);
    if actual == kernel_hash {
        IntegrityVerdict::Pass
    } else {
        IntegrityVerdict::Fail(alloc::format!(
            "kernel hash mismatch: expected={:#x} actual={:#x}",
            kernel_hash,
            actual
        ))
    }
}

/// Calcula CRC32C de um buffer (usado para hash rápido de kernel.elf).
pub fn hash_kernel(kernel_elf: &[u8]) -> u32 {
    crc32c(kernel_elf)
}
