//! ADR-0063 — SgdbStore facade: contrato único KV/doc para consumidores AIOS.
//! Namespaces: hanr/ md/ pkg/ skill/ audit/ vdb/ sys/
//! Preferir esta API a `put_blob` cru (exceto bridge legado RAG).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

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
    pub const HW: &str = "hw/";
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

use core::sync::atomic::AtomicBool;

/// Rebuild+nsgdb_open adiado (K33[28] soft-hang em backend=file ATA — SESSION_346/354).
static HEAVY_DEFERRED: AtomicBool = AtomicBool::new(false);
static HEAVY_DONE: AtomicBool = AtomicBool::new(false);

fn boot_ckpt(tag: &str) {
    k_nano::slog_kai!("SGDB", "ok", "boot_init:{}", tag);
}

/// Boot / init: Hamming + engine leve. Em `backend=file|nvme`:
/// - rebuild ART/BQ + `nsgdb_init` → [`boot_init_deferred`] (Runtime)
/// - `populate_hw` também adiado (cada put_kv podia disparar compact ATA)
/// Causa raiz deep (SESSION_354): `HIGH_WATER=256KB` + `maybe_gc→compact`
/// wipe do NSGDB.BIN via PIO = soft-hang em `K33[28] sgdb...`.
/// RAM continua síncrono (dev/test rápido).
pub fn boot_init() {
    boot_ckpt("hamming");
    super::hamming_dispatch::select_best_hamming_kernel();
    boot_ckpt("ensure");
    ensure_ready();
    if k_nano::storage::is_ready() {
        let backend = k_nano::storage::backend_name();
        let (md_keys, append_off, live) = k_nano::storage::with_tickv(|kv| {
            (
                kv.keys_with_prefix("md/").len(),
                kv.append_off(),
                kv.live_keys(),
            )
        })
        .unwrap_or((0, 0, 0));
        boot_ckpt(&format!(
            "probe backend={} md_keys={} live={} append_off={}",
            backend, md_keys, live, append_off
        ));
        if backend == "ram" {
            run_heavy_index_boot(backend, md_keys);
            boot_ckpt("hw_ns");
            populate_hw_namespace();
        } else {
            HEAVY_DEFERRED.store(true, Ordering::Release);
            k_nano::slog_kai!(
                "SGDB",
                "warn",
                "boot_init LIGHT — defer rebuild+nsgdb+hw_ns (backend={} md_keys={} live={} append={}) → Runtime",
                backend,
                md_keys,
                live,
                append_off
            );
        }
    } else {
        boot_ckpt("tickv_not_ready");
    }
    boot_ckpt("hydrate");
    // hydrate pode put_hanr — GC auto off em file (tickv); seguro.
    crate::boot_observe::hydrate_memory();
    boot_ckpt("done");
}

fn run_heavy_index_boot(backend: &str, md_keys: usize) {
    let t0 = k_nano::tsc::now_us();
    boot_ckpt("rebuild_k_ai");
    let n = with_engine(|e| e.rebuild_indices_from_tickv()).unwrap_or(0);
    let t1 = k_nano::tsc::now_us();
    k_nano::slog_kai!(
        "SGDB",
        "ok",
        "rebuild_indices n={} md_keys={} backend={} us={}",
        n,
        md_keys,
        backend,
        t1.saturating_sub(t0)
    );
    boot_ckpt("nsgdb_open");
    let nsgdb_n = super::nsgdb_bridge::nsgdb_init();
    let t2 = k_nano::tsc::now_us();
    k_nano::slog_kai!(
        "SGDB",
        "ok",
        "nsgdb_init records≈{} us={}",
        nsgdb_n,
        t2.saturating_sub(t1)
    );
    HEAVY_DONE.store(true, Ordering::Release);
    HEAVY_DEFERRED.store(false, Ordering::Release);
}

/// Runtime: completa rebuild+nsgdb se [`boot_init`] adiou (backend file/nvme).
/// Idempotente. Depois `ingest_bootlog` + `publish_boot_ai`.
pub fn boot_init_deferred() {
    if HEAVY_DONE.load(Ordering::Acquire) {
        return;
    }
    if !HEAVY_DEFERRED.load(Ordering::Acquire) && super::nsgdb_bridge::nsgdb_is_ready() {
        HEAVY_DONE.store(true, Ordering::Release);
        return;
    }
    if !k_nano::storage::is_ready() {
        k_nano::slog_kai!("SGDB", "warn", "boot_init_deferred SKIP (tickv not ready)");
        return;
    }
    let backend = k_nano::storage::backend_name();
    let md_keys = k_nano::storage::with_tickv(|kv| kv.keys_with_prefix("md/").len())
        .unwrap_or(0);
    k_nano::slog_kai!(
        "SGDB",
        "ok",
        "boot_init_deferred START backend={} md_keys={}",
        backend,
        md_keys
    );
    // Corpus enorme no stick: não bloquear o scheduler (UI/fleet). Índices
    // ficam frios até SleepCycle/recall forçar rebuild pontual.
    if md_keys > 512 {
        k_nano::slog_kai!(
            "SGDB",
            "warn",
            "boot_init_deferred SKIP heavy (md_keys={} >512) — indices COLD; ingest only",
            md_keys
        );
        boot_ckpt("hw_ns_deferred");
        populate_hw_namespace();
        // Honesty: NÃO marcar HEAVY_DONE — ART/BQ/NSGDB ainda frios.
        // Mantém HEAVY_DEFERRED para SleepCycle/recall forçar rebuild.
        HEAVY_DEFERRED.store(true, Ordering::Release);
        HEAVY_DONE.store(false, Ordering::Release);
        // Remember cross-boot é barato vs rebuild — não engolir no skip.
        crate::boot_observe::ingest_bootlog();
        k_nano::boot_report::publish_boot_ai();
        k_nano::storage::set_gc_suspended(false);
        return;
    }
    run_heavy_index_boot(backend, md_keys);
    boot_ckpt("hw_ns_deferred");
    populate_hw_namespace();
    crate::boot_observe::ingest_bootlog();
    k_nano::boot_report::publish_boot_ai();
    // Retoma flag de suspend; file/nvme ainda não auto-compactam em put
    // (maybe_gc skip) — SleepCycle/compact() explícito.
    k_nano::storage::set_gc_suspended(false);
}

/// True se o caminho pesado (rebuild+nsgdb) ainda não correu.
pub fn boot_sgdb_heavy_pending() -> bool {
    HEAVY_DEFERRED.load(Ordering::Acquire) && !HEAVY_DONE.load(Ordering::Acquire)
}

/// SleepCycle CONSOLIDATE / HITL: força rebuild+nsgdb mesmo se md_keys>512.
/// AIOS Remember: índices frios = cognição morta — não deixar HEAVY_DEFERRED eterno.
pub fn force_heavy_index_boot() {
    if !boot_sgdb_heavy_pending() {
        return;
    }
    if !k_nano::storage::is_ready() {
        k_nano::slog_kai!("SGDB", "warn", "force_heavy SKIP (tickv not ready)");
        return;
    }
    let backend = k_nano::storage::backend_name();
    let md_keys = k_nano::storage::with_tickv(|kv| kv.keys_with_prefix("md/").len()).unwrap_or(0);
    k_nano::slog_kai!(
        "SGDB",
        "warn",
        "force_heavy_index_boot START backend={} md_keys={} (SleepCycle Remember)",
        backend,
        md_keys
    );
    run_heavy_index_boot(backend, md_keys);
    boot_ckpt("hw_ns_forced");
    populate_hw_namespace();
    crate::boot_observe::ingest_bootlog();
    k_nano::boot_report::publish_boot_ai();
    k_nano::storage::set_gc_suspended(false);
}

/// ADR-0082 Onda CPU: `hw/<categoria>/<propriedade>` — valores string lowercase,
/// via `k_nano::platform_probe::hw_info()`. Falhas de put são não-fatais (log warn).
fn populate_hw_namespace() {
    let hw = k_nano::platform_probe::hw_info();
    let write = |key: &str, value: &str| {
        if let Err(e) = put_kv(key, value.as_bytes()) {
            k_nano::slog_kai!("SGDB", "warn", "put_kv {}: {}", key, e);
        }
    };
    let flag = |b: bool| if b { "true" } else { "false" };
    write(&format!("{}cpu/isa", ns::HW), hw.isa_name());
    write(&format!("{}cpu/avx2", ns::HW), flag(hw.avx2_ready()));
    write(&format!("{}cpu/avx512", ns::HW), flag(hw.avx512_ready()));
    write(&format!("{}cpu/fma", ns::HW), flag(hw.cpu.fma));
    write(&format!("{}cpu/hv", ns::HW), hw.hv.name());
    write(&format!("{}cache/l1d", ns::HW), &format!("{}", hw.cache.l1d));
    write(&format!("{}cache/l1i", ns::HW), &format!("{}", hw.cache.l1i));
    write(&format!("{}cache/l2", ns::HW), &format!("{}", hw.cache.l2));
    write(&format!("{}cache/l3", ns::HW), &format!("{}", hw.cache.l3));
    write(
        &format!("{}mem/total_mb", ns::HW),
        &format!("{}", k_nano::memory::TOTAL_RAM_MB.load(Ordering::Relaxed)),
    );
    k_nano::slog_kai!(
        "SGDB",
        "ok",
        "Onda CPU: /hw/* populado (isa={}, hv={}, ram_mb={})",
        hw.isa_name(),
        hw.hv.name(),
        k_nano::memory::TOTAL_RAM_MB.load(Ordering::Relaxed)
    );
    populate_hw_rest();
}

fn populate_hw_rest() {
    let write = |key: &str, value: &str| {
        if let Err(e) = put_kv(key, value.as_bytes()) {
            k_nano::slog_kai!("SGDB", "warn", "put_kv {}: {}", key, e);
        }
    };
    {
        let bus = k_nano::storage_bus::STORAGE_BUS.lock();
        write(
            &format!("{}storage/count", ns::HW),
            &format!("{}", bus.device_count()),
        );
        for (i, e) in bus.entries().iter().enumerate().take(8) {
            let kind = match e.kind {
                k_nano::storage_bus::BusKind::Nvme => "nvme",
                k_nano::storage_bus::BusKind::Ahci => "ahci",
                k_nano::storage_bus::BusKind::Ata => "ata",
                k_nano::storage_bus::BusKind::Usb => "usb",
                k_nano::storage_bus::BusKind::VirtioBlk => "virtio-blk",
            };
            write(&format!("{}storage/{}/kind", ns::HW, i), kind);
            write(
                &format!("{}storage/{}/sectors", ns::HW, i),
                &format!("{}", e.total_sectors_512),
            );
        }
    }
    let mut gpu_i = 0u32;
    let mut wifi_n = 0u32;
    for cap in crate::inventory::khal_device_tree() {
        if cap.id.class == k_hal::device_cap::DeviceClass::Gpu && gpu_i < 4 {
            write(&format!("{}gpu/{}/name", ns::HW, gpu_i), cap.name);
            gpu_i = gpu_i.saturating_add(1);
        }
        if cap.id.class == k_hal::device_cap::DeviceClass::Wifi {
            wifi_n = wifi_n.saturating_add(1);
        }
    }
    if wifi_n > 0 {
        write(&format!("{}wifi/present", ns::HW), "true");
    }
    let (order, n) = k_nano::boot_bind::nic_probe_order();
    write(&format!("{}net/plan_n", ns::HW), &format!("{}", n));
    for i in 0..n.min(4) {
        write(&format!("{}net/{}/kind", ns::HW, i), order[i].as_str());
    }
}

/// Varre PCI devices e escreve predições do HW Expert v4 no SGDB /hw/pci/.
/// GATED OFF (veredito 2026-08-04, docs/evidence/hwexpert-architecture-verdict-20260804.md):
/// a NN não atinge o gate de 65% em família específica (teto de sinal 59-63%) — predições
/// erradas não devem entrar no SGDB. Re-habilitar junto com o flip em `build_card`, após
/// provar o gate no protocolo honesto (split 90/10 por device + sweep QEMU).
pub fn predict_all_pci() {
    k_nano::slog_kai!("SGDB", "ok", "HW Expert v4 NN gated off (veredito 2026-08-04) — skip");
}

/// ADR-0082 Onda CPU — LEITURA (fecha o loop: consumidores leem /hw/* de
/// volta em vez de só escrever). `key` sem prefixo (ex. "cpu/isa") → lê
/// "hw/cpu/isa". None se indisponível (SGDB off / key ausente / não-utf8).
pub fn hw_get(key: &str) -> Option<String> {
    let full = format!("{}{}", ns::HW, key);
    match get_kv(&full) {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        _ => None,
    }
}

/// KV cru sob key absoluta (ex. `hanr/user`, `pkg/foo`, `audit/head`).
pub fn put_kv(key: &str, data: &[u8]) -> Result<(), &'static str> {
    ensure_ready();
    if !k_nano::storage::is_ready() {
        // honesty: sem TickvLite só indexa se for MemoryDoc path; KV puro exige flash
        return Err("tickv not ready");
    }
    let backend = k_nano::storage::backend_name();
    if backend == "ram" {
        // Rate-limit: uma vez por processo de boot (AtomicBool).
        static WARNED: core::sync::atomic::AtomicBool =
            core::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, core::sync::atomic::Ordering::Relaxed) {
            k_nano::slog_kai!(
                "SGDB",
                "warn",
                "put_kv on VOLATILE backend=ram — persists until reboot only (key example={})",
                key
            );
        }
    }
    let result = k_nano::storage::put_blob(key, data);
    if result.is_ok() {
        let layer = layer_from_key(key);
        super::nsgdb_bridge::sync_write_to_nsgdb(key, data, layer);
        super::crdt_sync::crdt_record_change_global();
    }
    result
}

/// Infer NSGDB MemoryLayer u8 from key prefix (`md/L4/...`, `hanr/`, …).
fn layer_from_key(key: &str) -> u8 {
    if let Some(rest) = key.strip_prefix("md/L") {
        if let Some(c) = rest.chars().next() {
            if let Some(d) = c.to_digit(10) {
                return d.min(7) as u8;
            }
        }
    }
    if key.starts_with("hanr/") {
        return 7;
    }
    3 // default L3 episodic long
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
    let sk = doc.storage_key();
    let layer = doc.layer as u8;
    let payload = doc.payload.clone();
    let result = with_engine(|e| e.put(doc)).unwrap_or(Err("engine down"));
    // #537 + s385: sync índices NSGDB + versão CRDT após write de doc
    if result.is_ok() {
        super::nsgdb_bridge::sync_write_to_nsgdb(&sk, &payload, layer);
        super::crdt_sync::crdt_record_change_global();
    }
    result
}

pub fn get_doc(layer: MemoryLayer, key: &str) -> Result<Option<MemoryDoc>, &'static str> {
    ensure_ready();
    with_engine(|e| e.get(layer, key)).unwrap_or(Err("engine down"))
}

/// SleepCycle CONSOLIDATE: flush L0/L1 RAM → Tickv (+ compact best-effort só em RAM).
pub fn checkpoint_working() -> Result<usize, &'static str> {
    ensure_ready();
    let n = with_engine(|e| e.checkpoint_l0l1()).unwrap_or(Err("engine down"))?;
    if ready() {
        let backend = k_nano::storage::backend_name();
        // Honesty: compact em file/nvme = wipe+rewrite PIO — nunca no SleepCycle hot path.
        if backend == "ram" {
            let _ = k_nano::storage::with_tickv(|kv| kv.compact());
        } else {
            k_nano::slog_kai!(
                "SGDB",
                "ok",
                "checkpoint_working skip compact (backend={}) — use compact() explícito HITL",
                backend
            );
        }
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
    put_doc(doc)?;
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
        k_nano::slog_kai!(
            "SGDB",
            "warn",
            "put_skill_blob {}: ART only — Tickv not ready (volatile index)",
            name
        );
        return Err("tickv not ready");
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
    let backend = backend();
    let volatile = backend == "ram";
    format!(
        "SgdbStore ready={} backend={} volatile={}",
        ready(),
        backend,
        volatile
    )
}
