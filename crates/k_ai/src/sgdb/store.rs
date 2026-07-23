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

/// Boot / init: sobe engine (TickvLite já deve estar montado pelo smoke).
pub fn boot_init() {
    ensure_ready();
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

/// Persist RAG blob (bridge 0064).
pub fn put_vdb_blob(data: &[u8]) -> Result<(), &'static str> {
    put_kv("vdb/blob", data)
}

pub fn get_vdb_blob() -> Result<Option<Vec<u8>>, &'static str> {
    get_kv("vdb/blob")
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
