//! Observe DeviceTree — plano NIC+storage, cards PnP, Trust, memória.
//! Premissas: ADR-0088, emagrecer, Agent/Skill (cards+EventBus), Trust HITL,
//! DeviceRecipe (Escalate ≠ Auto), HANR quando SGDB acordar.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use k_hal::device_cap::DeviceClass;
use k_hal::device_recipe::RecipePromote;
use k_nano::boot_bind::{
    classify_nic, classify_storage, install_plan, install_storage_plan, set_has_snd, NicKind,
    StorageKind,
};
use spin::Mutex;

static PENDING_HANR: Mutex<Option<String>> = Mutex::new(None);

/// `trust_ok`: (token,agent,skill)=(1,boot_observe,plan). Deny não apaga evidência —
/// só recusa Auto em recipe Escalate (HITL).
pub fn observe_and_plan(trust_ok: bool) -> (usize, usize) {
    let tree = crate::inventory::khal_device_tree();
    let n = tree.len();
    let mut nics: Vec<NicKind> = Vec::new();
    let mut stores: Vec<StorageKind> = Vec::new();
    let mut blocks = 0u32;
    let mut nets = 0u32;
    let mut gpus = 0u32;
    let mut usb = 0u32;
    let mut snd = 0u32;
    let mut escalate_n = 0u32;
    let mut cards = 0u32;

    for cap in &tree {
        match cap.id.class {
            DeviceClass::Block => blocks = blocks.saturating_add(1),
            DeviceClass::Net | DeviceClass::Wifi => nets = nets.saturating_add(1),
            DeviceClass::Gpu => gpus = gpus.saturating_add(1),
            DeviceClass::UsbHost => usb = usb.saturating_add(1),
            DeviceClass::Snd => snd = snd.saturating_add(1),
            _ => {}
        }
        let promote = cap.recipe_promote;
        if promote == RecipePromote::Escalate as u8 {
            escalate_n = escalate_n.saturating_add(1);
            publish_health_escalate(cap.id.vendor_id, cap.id.device_id);
        }
        let nic = classify_nic(cap.id.vendor_id, cap.id.device_id);
        let allow_auto = promote != RecipePromote::Escalate as u8;
        if nic != NicKind::None && allow_auto && !nics.iter().any(|k| *k == nic) {
            nics.push(nic);
        }
        let st = if cap.id.class == DeviceClass::UsbHost {
            StorageKind::UsbHost
        } else {
            classify_storage(cap.id.pci_class, cap.id.pci_subclass)
        };
        if st != StorageKind::None && allow_auto && !stores.iter().any(|k| *k == st) {
            // SESSION_293: ATA SEMPRE no plano (allow_probe).
            // Antes, TCG puxava ATA do plano → boot sem disco → sem BOOT.LOG.
            if !k_nano::storage_bw::allow_probe() {
                k_nano::slog_kai!("Boot", "observe", "storage probe bloqueado (allow_probe=false)");
            } else {
                stores.push(st);
            }
        }
        if cards < 16 {
            emit_card(cap.id.vendor_id, cap.id.device_id, cap.id.pci_class, cap.id.pci_subclass, cap.name);
            cards = cards.saturating_add(1);
        }
    }

    if !trust_ok {
        k_nano::slog_kai!(
            "Boot",
            "observe",
            "Trust DENY (1,boot_observe,plan) — plano por evidencia; Escalate nao Auto"
        );
    }

    install_plan(&nics, n);
    install_storage_plan(&stores, n);
    set_has_snd(n == 0 || snd > 0);
    let (_no, nic_n) = k_nano::boot_bind::nic_probe_order();
    let (_so, sto_n) = k_nano::boot_bind::storage_probe_order();

    let summary = format!(
        "BOOT_OBSERVE:devices={}:nics={}:storage={}:block={}:gpu={}:usb={}:snd={}:escalate={}:trust={}",
        n, nic_n, sto_n, blocks, gpus, usb, snd, escalate_n, trust_ok
    );
    let ai = k_nano::boot_report::BootAiCounts {
        observe: n as u32,
        plan: (nic_n.saturating_add(sto_n)) as u32,
        act: (nics.len().saturating_add(stores.len())) as u32,
        escalate: escalate_n,
        verify: 0,
    };
    k_nano::boot_report::note_ai(ai);
    // T-003 visível na consola (sev=ok, não trace "aios")
    k_nano::slog_kai!("Boot", "ok", "{}", ai.line());
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("BOOT_AI"),
        payload: ai.line().into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    // T-001 R2 mirror keeps lock-free counters in sync (ponytail: no extra alloc)
    crate::boot_metrics::set_mirror(ai);
    k_nano::slog_kai!("Boot", "observe", "{} (tabela+recipe; Cortex sem pesos)", summary);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("BOOT_OBSERVE"),
        payload: summary.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
    *PENDING_HANR.lock() = Some(summary);
    (n, nic_n)
}

/// Triplas VID-gated a partir do DeviceTree — sem re-scan PCI (SESSION_262 hang).
pub fn heal_triples_from_tree() -> Vec<(u16, u16, u8, u8)> {
    crate::inventory::khal_device_tree()
        .iter()
        .map(|c| {
            (
                c.id.vendor_id,
                c.id.device_id,
                c.id.pci_class,
                c.id.pci_subclass,
            )
        })
        .collect()
}

fn emit_card(vid: u16, did: u16, class: u8, subclass: u8, name: &str) {
    let card = crate::hw_capability::build_card(vid, did, class, subclass, name);
    let wire = card.to_wire();
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(crate::hw_capability::TOPIC_HW_CAPABILITY),
        payload: wire.as_bytes().to_vec(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

fn publish_health_escalate(vid: u16, did: u16) {
    let msg = format!("HEALTH_ISSUE:HITL:recipe_escalate:{:04X}:{:04X}", vid, did);
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from("HEALTH_ISSUE"),
        payload: msg.into_bytes(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Depois de `sgdb::boot_init` — memoriza o plano (ADR-0088 mandamento 3).
pub fn hydrate_memory() {
    let Some(s) = PENDING_HANR.lock().take() else {
        return;
    };
    if crate::sgdb::ready() {
        let _ = crate::sgdb::put_hanr("boot_bind", &s);
        crate::self_state::record_life_event(&s);
        k_nano::boot_report::note_ai_verify();
        crate::boot_metrics::inc_verify(1);
        // T-001 final emit após verify (observe/plan/act/verify com escalate)
        k_nano::boot_report::publish_boot_ai();
    } else {
        k_nano::slog_kai!("Boot", "observe", "SGDB nao ready — plano so no EventBus (honesto)");
        *PENDING_HANR.lock() = Some(s);
    }
}

// ═══ IDEA #539 item (c): sgdb_ingest_bootlog — ADR-0088 Remember entre boots ═══
// O ramlog de boot vira memória L3 episódica no SGDB; o boot seguinte faz recall
// e aprende (fecha o loop Remember entre boots).

/// Cap do texto do ramlog no doc L3 (64 KiB).
const BOOTLOG_CAP: usize = 64 * 1024;

/// Assinatura mínima de um boot (aprendizado v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSignature {
    /// Último checkpoint `K<N>:` visto no ramlog (0 = nenhum).
    pub last_ckpt: u8,
    /// Alguma linha contém "PANIC" ou "fail".
    pub panic_or_fail: bool,
}

/// Extrai o primeiro `K<N>:` da linha.
fn ckpt_of_line(line: &str) -> Option<u8> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'K' {
            let mut j = i + 1;
            let mut n: u32 = 0;
            while j < b.len() && b[j].is_ascii_digit() && n < 256 {
                n = n * 10 + (b[j] - b'0') as u32;
                j += 1;
            }
            if j > i + 1 && j < b.len() && b[j] == b':' && n <= 255 {
                return Some(n as u8);
            }
        }
        i += 1;
    }
    None
}

/// Assinatura do boot a partir do texto do ramlog.
pub fn boot_signature(text: &str) -> BootSignature {
    let mut sig = BootSignature {
        last_ckpt: 0,
        panic_or_fail: false,
    };
    for line in text.lines() {
        if let Some(k) = ckpt_of_line(line) {
            sig.last_ckpt = k;
        }
        if !sig.panic_or_fail && (line.contains("PANIC") || line.contains("fail")) {
            sig.panic_or_fail = true;
        }
    }
    sig
}

/// Trunca em fronteira de char com sufixo honesto.
fn cap_utf8(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return String::from(text);
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncado: original {} bytes]", &text[..end], text.len())
}

/// Função pura (testável com RamFlash): grava o ramlog como doc L3 episódico
/// (`md/L3/boot/<tick:07>`, zero-padded p/ ordenação) e ANTES faz recall do
/// boot anterior via `scan_prefix_nsgdb("md/L3/boot/")` — sloga o resumo e, se
/// o boot anterior teve PANIC/fail, registra `record_life_event`. Erros são
/// não-fatais (o wrapper sloga warn; SGDB pode estar volátil).
pub fn ingest_bootlog_text(text: &str, tick: u64) -> Result<String, &'static str> {
    let key = format!("boot/{:07}", tick);
    let sk = format!("md/L3/{}", key);

    // ── Recall: boot anterior mais recente ≠ atual ──
    let history = crate::sgdb::nsgdb_bridge::scan_prefix_nsgdb("md/L3/boot/");
    let mut prevs: Vec<&str> = history.iter().map(|(k, _)| k.as_str()).collect();
    prevs.retain(|k| *k != sk.as_str());
    let prev_tick = prevs
        .into_iter()
        .max() // zero-padded → lexicográfico = tick mais recente
        .and_then(|k| k.strip_prefix("md/L3/boot/"))
        .and_then(|s| s.parse::<u64>().ok());

    if let Some(ptick) = prev_tick {
        let prev_key = format!("boot/{:07}", ptick);
        let sig = crate::sgdb::nsgdb_bridge::with_nsgdb(|db| {
            db.get(neural_sgdb::MemoryLayer::L3EpisodicLong, &prev_key)
                .ok()
                .flatten()
                .and_then(|d| core::str::from_utf8(&d.payload).ok().map(String::from))
                .map(|t| boot_signature(&t))
        })
        .flatten()
        .unwrap_or(BootSignature {
            last_ckpt: 0,
            panic_or_fail: false,
        });
        k_nano::slog_kai!(
            "Boot",
            "remember",
            "boot anterior tick={} último_ckpt=K{} panic={} — {} docs de boot no histórico",
            ptick,
            sig.last_ckpt,
            sig.panic_or_fail,
            history.len()
        );
        if sig.panic_or_fail {
            crate::self_state::record_life_event(&format!(
                "boot_failure tick={} last_ckpt=K{} panic=true",
                ptick, sig.last_ckpt
            ));
        }
    }

    // ── Write: NSGDB primeiro (indexa ART já na sessão); fallback engine interno ──
    let payload = cap_utf8(text, BOOTLOG_CAP);
    let wrote = crate::sgdb::nsgdb_bridge::with_nsgdb(|db| {
        db.put(neural_sgdb::MemoryDoc::new(
            neural_sgdb::MemoryLayer::L3EpisodicLong,
            key.as_str(),
            payload.as_bytes().to_vec(),
        ))
        .is_ok()
    })
    .unwrap_or(false);
    if wrote {
        // entity explícito p/ recall por entidade (IDEA #539c)
        let entity = format!("boot/{}", tick);
        let _ = crate::sgdb::nsgdb_bridge::with_nsgdb(|db| db.set_entities(&sk, &[&entity]));
    } else {
        crate::sgdb::put_doc(crate::sgdb::MemoryDoc::new(
            crate::sgdb::MemoryLayer::L3EpisodicLong,
            key.as_str(),
            payload.as_bytes().to_vec(),
        ))
        .map(|_| ())?;
    }
    Ok(key)
}

/// Wrapper fino: lê o ramlog físico (k_nano) e ingere com TIMER_TICKS atual.
/// Ramlog vazio/off → skip honesto (sem pânico).
pub fn ingest_bootlog() {
    let Some(text) = k_nano::boot_ramlog::snapshot() else {
        k_nano::slog_kai!("Boot", "ok", "ramlog vazio — skip honesto (ingest)");
        return;
    };
    let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
    match ingest_bootlog_text(&text, tick) {
        Ok(key) => k_nano::slog_kai!(
            "Boot",
            "ok",
            "ingest ramlog → SGDB L3 {} ({} bytes)",
            key,
            text.len()
        ),
        Err(e) => k_nano::slog_kai!("Boot", "warn", "ingest: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use k_nano::boot_bind::{rank_storage, StorageKind};

    #[test]
    fn qemu_e1000_only_plan() {
        let (o, n) = k_nano::boot_bind::rank_present(&[k_nano::boot_bind::NicKind::E1000]);
        assert_eq!(n, 1);
        assert_eq!(o[0], k_nano::boot_bind::NicKind::E1000);
    }

    #[test]
    fn triples_empty_without_h1() {
        let t = super::heal_triples_from_tree();
        assert!(t.is_empty());
    }

    #[test]
    fn usb_live_skips_ata_in_rank() {
        let (o, n) = rank_storage(&[StorageKind::UsbHost, StorageKind::Nvme]);
        assert_eq!(n, 2);
        assert_eq!(o[0], StorageKind::Nvme);
        assert_eq!(o[1], StorageKind::UsbHost);
    }

    #[test]
    fn boot_ai_line_roundtrip() {
        let c = k_nano::boot_report::BootAiCounts {
            observe: 3,
            plan: 2,
            act: 2,
            escalate: 1,
            verify: 0,
        };
        let p = k_nano::boot_report::parse_boot_ai_line(&c.line()).unwrap();
        assert_eq!(p.act, 2);
        assert_eq!(p.escalate, 1);
    }

    /// IDEA #539c: ramlog → L3 episódico + recall cross-boot
    /// (RamFlash = backend de teste interop, padrão SESSION_267).
    #[test]
    fn ingest_bootlog_cross_boot_order_and_cap() {
        // Storage limpo com RamFlash
        *k_nano::storage::TICKV.lock() = None;
        *k_nano::storage::FLASH.lock() = None;
        k_nano::storage::install_ram_flash(1024 * 1024);
        {
            let mut g = k_nano::storage::TICKV.lock();
            g.get_or_insert_with(k_nano::storage::TickvLite::new)
                .mount()
                .expect("mount");
        }
        crate::sgdb::init_global(1);
        crate::sgdb::nsgdb_bridge::nsgdb_init();

        // Boot N (anterior): PANIC no fim
        let prev_text = "[T+10] K40: drivers ok\n[T+90] K51: fleet\nPANIC: ata timeout\n";
        assert_eq!(
            super::ingest_bootlog_text(prev_text, 100).expect("ingest #1"),
            "boot/0000100"
        );

        // Boot N+1: deve enxergar o anterior via scan_prefix → PANIC → life event
        let cur_text = "[T+5] K22: memory ok\n[T+99] K52: init_phase done\n";
        assert_eq!(
            super::ingest_bootlog_text(cur_text, 200).expect("ingest #2"),
            "boot/0000200"
        );
        // (a) aprendizado cross-boot: record_life_event disparou exatamente 1×
        let seq = crate::sgdb::get_kv("sys/life_seq")
            .expect("kv read")
            .expect("life_seq existe");
        assert_eq!(seq, 1u64.to_le_bytes().to_vec());

        // (b) keys ordenam por tick (zero-padded → lexicográfico)
        let mut keys: Vec<String> = crate::sgdb::nsgdb_bridge::scan_prefix_nsgdb("md/L3/boot/")
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        keys.sort();
        let i1 = keys
            .iter()
            .position(|k| k == "md/L3/boot/0000100")
            .expect("key boot 100");
        let i2 = keys
            .iter()
            .position(|k| k == "md/L3/boot/0000200")
            .expect("key boot 200");
        assert!(i1 < i2);

        // Boot seguinte sem falha NÃO gera life event novo (aprendizado condicional)
        super::ingest_bootlog_text("[T+1] K10: ok\n", 250).expect("ingest #2b");
        let seq2 = crate::sgdb::get_kv("sys/life_seq")
            .expect("kv read")
            .expect("life_seq persiste");
        assert_eq!(seq2, 1u64.to_le_bytes().to_vec());

        // (c) cap 64KB com sufixo honesto
        let mut big = "x".repeat(80 * 1024);
        big.push_str("\nK52: done\n");
        assert_eq!(
            super::ingest_bootlog_text(&big, 300).expect("ingest #3"),
            "boot/0000300"
        );
        let payload = crate::sgdb::nsgdb_bridge::with_nsgdb(|db| {
            db.get(neural_sgdb::MemoryLayer::L3EpisodicLong, "boot/0000300")
                .ok()
                .flatten()
                .map(|d| d.payload)
        })
        .flatten()
        .expect("doc boot/0000300");
        assert!(
            payload.len() <= 64 * 1024 + 64,
            "payload {} acima do cap",
            payload.len()
        );
        let s = core::str::from_utf8(&payload).expect("utf8");
        assert!(s.contains("[truncado"), "sufixo honesto ausente");
        assert!(
            s.contains(&format!("original {} bytes", big.len())),
            "tamanho original no sufixo"
        );
        assert!(s.starts_with(&"x".repeat(16)), "prefixo preservado");
    }

    #[test]
    fn boot_signature_parses_ckpt_and_panic() {
        let sig = super::boot_signature("[T+1] K22: memory\n[T+9] K52: done\n");
        assert_eq!(sig.last_ckpt, 52);
        assert!(!sig.panic_or_fail);
        let bad = super::boot_signature("[T+1] K40: ok\nPANIC: ata timeout\n");
        assert_eq!(bad.last_ckpt, 40);
        assert!(bad.panic_or_fail);
        assert_eq!(
            super::boot_signature("sem checkpoint aqui").last_ckpt,
            0
        );
    }
}
