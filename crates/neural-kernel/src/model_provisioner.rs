//! ModelProvisioner (ADR-0086 §3.6A / ADR-0079 §2.8, gap I4).
//! 1º boot Residente: baixa os modelos que faltam do server OTA e registra no
//! ModelHub (`register_bytes` — ponto único de carga). URL vem do UPDATE.CFG
//! (config file), nunca hardcoded. Ordem menor→maior: HwExpert → RustCoder →
//! Reranker → Active (2B) por último — o sistema "fica esperto" barato primeiro.

use alloc::string::String;
use alloc::vec::Vec;
use crate::model_hub::{self, ModelSlot};

/// Ordem de provisionamento (menor → maior; Active por último, só se model_fit OK).
const ORDER: &[ModelSlot] = &[
    ModelSlot::HwExpert,
    ModelSlot::RustCoder,
    ModelSlot::Reranker,
    ModelSlot::Active,
];

fn fetch(url: &str) -> Option<Vec<u8>> {
    // S14: roteia pelo tls::fetch_url (https-capable) — não só HTTP puro do net.
    // Se UPDATE_URL for https, o provision não quebra mais.
    match hermes_crate::tls::fetch_url(url) {
        Ok(data) if !data.is_empty() => Some(data),
        Ok(_) | Err(_) => None,
    }
}

/// Persiste o blob baixado em /models/<FAT_NAME> na NeuralFS do disco (I5 ADR-0086),
/// para o boot seguinte carregar sem re-baixar. Best-effort.
fn persist_slot(name: &str, data: &[u8]) {
    let mut ata_guard = crate::ATA_DRIVER.lock();
    let Some(ata) = ata_guard.as_mut() else { return };
    let parts = crate::fat32::read_mbr(ata);
    for p in &parts {
        if p.type_code != k_nano::neural_fs::volume::MBR_TYPE_NEURALFS {
            continue;
        }
        let dev: &mut dyn k_nano::block_dev::BlockDevice = ata;
        let Some(mut vol) = k_nano::neural_fs::volume::NeuralVolume::mount(dev, p.lba_start as u64)
        else {
            continue;
        };
        // Cria /models/ (se não existir) e grava o arquivo.
        let models = match vol.resolve_path(dev, "models") {
            Some(ino) => ino,
            None => match vol.create_dir(dev, 1, "models") {
                Ok(ino) => ino,
                Err(_) => return,
            },
        };
        let ino = match vol.resolve_path(dev, &alloc::format!("models/{}", name)) {
            Some(ino) => ino,
            None => match vol.create_file(dev, models, name) {
                Ok(ino) => ino,
                Err(_) => return,
            },
        };
        if vol.write_file(dev, ino, data).is_ok() {
            k_nano::slog_bin!("PROV", "info", "persistido /models/{} ({} bytes)", name, data.len());
        }
        // C9 (ora-1): ponte NeuralFS↔SGDB — registra meta do modelo no KV.
        // O boot pode consultar pkg/model/* antes de varrer FAT (integração +
        // verificação de integridade; corta o scan de nomes).
        let meta = alloc::format!(
            "{{\"file\":\"{}\",\"bytes\":{},\"sha256\":\"{}\"}}",
            name,
            data.len(),
            self_update_sha_hex(data)
        );
        let _ = k_ai::sgdb::put_kv(&alloc::format!("pkg/model/{}", name), meta.as_bytes());
        return;
    }
}

/// SHA-256 hex do blob (reuso do self_update — evita duplicar a primitiva).
fn self_update_sha_hex(data: &[u8]) -> String {
    let h = k_nano::tpm::sha256(data);
    let mut s = String::with_capacity(64);
    for b in h {
        s.push_str(&alloc::format!("{:02x}", b));
    }
    s
}

/// Tenta o 1º nome FAT 8.3 candidato do slot contra a base URL.
fn download_slot(base: &str, slot: ModelSlot) -> bool {
    for name in model_hub::fat_names_for(slot) {
        let url = alloc::format!("{}/{}", base.trim_end_matches('/'), name);
        k_nano::slog_bin!("PROV", "info", "GET {} slot={}", url, slot.name());
        if let Some(data) = fetch(&url) {
            k_nano::slog_bin!("PROV", "info", "slot={} bytes={}", slot.name(), data.len());
            if model_hub::register_bytes(slot, &data) {
                persist_slot(name, &data);
                return true;
            }
            k_nano::slog_bin!("PROV", "warn", "slot={} parse falhou, tenta prox nome", slot.name());
        }
    }
    false
}

/// Provisiona os slots vazios. Retorna nº de slots baixados.
/// Base URL = UPDATE.CFG (config); sem config → 0 (não inventa endereço).
pub fn provision_missing() -> usize {
    let Some(base) = crate::self_update::read_update_cfg() else {
        k_nano::slog_bin!("PROV", "info", "sem UPDATE.CFG — skip provision");
        return 0;
    };
    let mut n = 0;
    for &slot in ORDER {
        if model_hub::slot_loaded(slot) {
            continue;
        }
        if download_slot(&base, slot) {
            n += 1;
        } else {
            k_nano::slog_bin!("PROV", "info", "slot={} indisponivel (degradado, retry no prox boot)", slot.name());
        }
    }
    k_nano::slog_bin!("PROV", "info", "provision done: {} slots baixados", n);
    n
}
