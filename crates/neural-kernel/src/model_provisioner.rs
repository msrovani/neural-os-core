//! ModelProvisioner (ADR-0086 §3.6A / ADR-0079 §2.8, gap I4).
//! 1º boot Residente: baixa os modelos que faltam do server OTA e registra no
//! ModelHub (`register_bytes` — ponto único de carga). URL vem do UPDATE.CFG
//! (config file), nunca hardcoded. Ordem menor→maior: HwExpert → RustCoder →
//! Reranker → Active (2B) por último — o sistema "fica esperto" barato primeiro.

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
    match crate::net::resolve_and_http_get_safe(url) {
        Ok(data) if !data.is_empty() => Some(data),
        Ok(_) | Err(_) => None,
    }
}

/// Tenta o 1º nome FAT 8.3 candidato do slot contra a base URL.
fn download_slot(base: &str, slot: ModelSlot) -> bool {
    for name in model_hub::fat_names_for(slot) {
        let url = alloc::format!("{}/{}", base.trim_end_matches('/'), name);
        k_nano::slog_bin!("PROV", "info", "GET {} slot={}", url, slot.name());
        if let Some(data) = fetch(&url) {
            k_nano::slog_bin!("PROV", "info", "slot={} bytes={}", slot.name(), data.len());
            if model_hub::register_bytes(slot, &data) {
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
