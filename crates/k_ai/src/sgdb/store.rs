//! ADR-0063 — SgdbStore facade: contrato único KV/doc para consumidores AIOS.
//! Namespaces: hanr/ md/ pkg/ skill/ audit/ vdb/ sys/
//! Preferir esta API a `put_blob` cru (exceto bridge legado RAG).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::engine::{with_engine, AiosDatabaseEngine};
use super::layers::ensure_ready;
use super::memory_doc::{MemoryDoc, MemoryLayer};

/// Prefixo canônico de namespace.
pub mod ns {
    pub const HANR: &str = "hanr/";
    pub const MD: &str = "md/";
    pub const PKG: &str = "pkg/";
    pub const SKILL: &str = "skill/";
    pub const AUDIT: &str = "audit/";
    pub const VDB: &str = "vdb/";
    pub const SYS: &str = "sys/";
}

pub fn ready() -> bool {
    k_nano::storage::is_ready()
}

pub fn backend() -> &'static str {
    if ready() {
        k_nano::storage::backend_name()
    } else {
        "none"
    }
}

/// Boot / init: Hamming dispatch + engine + rebuild ART/BQ.
pub fn boot_init() {
    super::hamming_dispatch::select_best_hamming_kernel();
    ensure_ready();
    if k_nano::storage::is_ready() {
        let n = with_engine(|e| e.rebuild_indices_from_tickv()).unwrap_or(0);
        let _ = n;
    }
}

/// Varre PCI devices encontrados e escreve predições do HW Expert v4 no SGDB /hw/pci/.
/// Chamado após carregar HWEXPRT4.BIN. Cada device vira chave:
///   /hw/pci/<idx>/vid, /hw/pci/<idx>/did, /hw/pci/<idx>/family, ...
pub fn predict_all_pci() {
    if !cortex::cortex::hwexpert_v4_is_loaded() {
        k_nano::slog_kai!("SGDB", "hw_predict", "HW Expert v4 not loaded — skip PCI predict");
        return;
    }
    let devices = unsafe { k_nano::pci::scan_pci() };
    let mut count = 0usize;
    for (idx, dev) in devices.iter().enumerate() {
        let prefix = alloc::format!("/hw/pci/{}", idx);
        // Write raw PCI info
        let _ = put_kv(&alloc::format!("{}/vid", prefix), &dev.vendor_id.to_le_bytes());
        let _ = put_kv(&alloc::format!("{}/did", prefix), &dev.device_id.to_le_bytes());
        let _ = put_kv(&alloc::format!("{}/class", prefix), &[dev.class, dev.subclass]);

        // HW Expert v4 prediction
        if let Some(pred) = cortex::cortex::hwexpert_v4_predict(dev.vendor_id, dev.device_id) {
            let _ = put_kv(&alloc::format!("{}/family", prefix), &[pred.family_id]);
            let _ = put_kv(&alloc::format!("{}/fw_id", prefix), &[pred.fw_id]);
            let _ = put_kv(&alloc::format!("{}/agent_id", prefix), &[pred.agent_id]);
            let _ = put_kv(&alloc::format!("{}/caps_bits", prefix), &pred.caps_bits.to_le_bytes());
            let _ = put_kv(&alloc::format!("{}/next_action", prefix), &[pred.next_action]);
            count += 1;
        }
    }
    k_nano::slog_kai!("SGDB", "hw_predict", "{} PCI devices written to /hw/pci/ (HW Expert v4)", count);
}

/// KV cru sob key absoluta (ex. `hanr/user`, `pkg/foo`, `audit/head`).
pub fn put_kv(key: &str, data: &[u8]) -> Result<(), &'static str> {
    ensure_ready();
    if !k_nano::storage::is_ready() {
        // honesty: sem TickvLite só indexa se for MemoryDoc path; KV puro exige flash
        return Err("tickv not ready");
    }
    k_nano::storage::put_blob(key, data)
}

pub fn get_kv(key: &str) -> Result<Option<Vec<u8>>, &'static str> {
    if !k_nano::storage::is_ready() {
        return Ok(None);
    }
    match k_nano::storage::get_blob(key) {
        Ok(v) => Ok(Some(v)),
        Err("missing") => Ok(None),
        Err(e) => Err(e),
    }
}

/// MemoryDoc via engine (também indexa ART/BQ).
pub fn put_doc(doc: MemoryDoc) -> Result<u64, &'static str> {
    ensure_ready();
    with_engine(|e| e.put(doc)).unwrap_or(Err("engine down"))
}

pub fn get_doc(layer: MemoryLayer, key: &str) -> Result<Option<MemoryDoc>, &'static str> {
    ensure_ready();
    with_engine(|e| e.get(layer, key)).unwrap_or(Err("engine down"))
}

/// SleepCycle CONSOLIDATE: flush L0/L1 RAM → Tickv (+ compact best-effort).
pub fn checkpoint_working() -> Result<usize, &'static str> {
    ensure_ready();
    let n = with_engine(|e| e.checkpoint_l0l1()).unwrap_or(Err("engine down"))?;
    if ready() {
        let _ = k_nano::storage::with_tickv(|kv| kv.compact());
    }
    Ok(n)
}

/// SleepCycle PRUNE: limpa arena L0/L1 já persistida (get cai no Tickv).
pub fn prune_working_ram() -> usize {
    ensure_ready();
    with_engine(|e| e.prune_ram_l0l1()).unwrap_or(0)
}

/// Texto HANR L7 (identity): keys lógicas user|memory|soul|persona → `hanr/{name}` + md/L7.
pub fn put_hanr(name: &str, text: &str) -> Result<(), &'static str> {
    ensure_ready();
    let kv_key = format!("{}{}", ns::HANR, name);
    put_kv(&kv_key, text.as_bytes())?;
    let doc = MemoryDoc::new(MemoryLayer::L7Identity, name, text.as_bytes().to_vec());
    let _ = put_doc(doc);
    Ok(())
}

pub fn get_hanr(name: &str) -> Result<Option<String>, &'static str> {
    let kv_key = format!("{}{}", ns::HANR, name);
    if let Ok(Some(bytes)) = get_kv(&kv_key) {
        return Ok(Some(
            core::str::from_utf8(&bytes)
                .map(String::from)
                .unwrap_or_default(),
        ));
    }
    // fallback MemoryDoc L7
    match get_doc(MemoryLayer::L7Identity, name)? {
        Some(doc) => Ok(Some(
            core::str::from_utf8(&doc.payload)
                .map(String::from)
                .unwrap_or_default(),
        )),
        None => Ok(None),
    }
}

/// Meta de pacote (JSON-ish leve: linhas key=value).
pub fn put_pkg_meta(package_id: &str, meta: &str) -> Result<(), &'static str> {
    put_kv(&format!("{}{}", ns::PKG, package_id), meta.as_bytes())
}

pub fn get_pkg_meta(package_id: &str) -> Result<Option<Vec<u8>>, &'static str> {
    get_kv(&format!("{}{}", ns::PKG, package_id))
}

pub fn put_pkg_body(package_id: &str, body: &[u8]) -> Result<(), &'static str> {
    if body.len() > 4096 {
        return Err("body too large for tickv");
    }
    put_kv(&format!("{}{}/body", ns::PKG, package_id), body)
}

pub fn get_pkg_body(package_id: &str) -> Result<Option<Vec<u8>>, &'static str> {
    get_kv(&format!("{}{}/body", ns::PKG, package_id))
}

pub fn put_skill_blob(name: &str, description: &str) -> Result<(), &'static str> {
    super::layers::index_skill(name, description);
    if !ready() {
        return Ok(()); // ART indexado; TickvLite opcional
    }
    put_kv(
        &format!("{}{}", ns::SKILL, name),
        description.as_bytes(),
    )
}

pub fn with_store<R>(f: impl FnOnce(&mut AiosDatabaseEngine) -> R) -> Option<R> {
    ensure_ready();
    with_engine(f)
}

pub fn status() -> String {
    format!(
        "SgdbStore ready={} backend={}",
        ready(),
        backend()
    )
}
