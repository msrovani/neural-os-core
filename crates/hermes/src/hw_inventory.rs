//! HW inventory snapshot — "o que a AIOS achou" (produtor: HwDetectAgent).
//!
//! O agente monta o snapshot (política: qual subsistema, bound ou não) e publica
//! `TOPIC_HW_INVENTORY_STATE`; o compositor (jarbas) só desenha o card a partir de
//! `snapshot()` — nada de DeviceTree/cards por frame.
//!
//! Honestidade (regra do repo): device não suportado NÃO some — vira linha
//! `unknown` no subsistema certo E entra na lista `PCI vvvv:dddd <classe>`.
//! Ausência (`absent`) != desconhecido (`unknown`) != funcionando (`ok`); `n/a` != 0.

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use k_ai::hw_capability::{HwCapabilityCard, HwFamily, HwNextAction};
use k_nano::boot_bind::NicKind;
use spin::Mutex;

/// Topic EventBus: snapshot de inventário mudou (payload vazio — consumidor lê o static).
pub const TOPIC_HW_INVENTORY_STATE: &str = "HW_INVENTORY_STATE";

/// Subsistemas mostrados no card (ordem = ordem das linhas).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum HwRow {
    Nic = 0,
    Wifi = 1,
    Gpu = 2,
    Storage = 3,
    Usb = 4,
}
pub const HW_ROWS: usize = 5;

impl HwRow {
    pub fn label(self) -> &'static str {
        match self {
            HwRow::Nic => "nic",
            HwRow::Wifi => "wifi",
            HwRow::Gpu => "gpu",
            HwRow::Storage => "storage",
            HwRow::Usb => "usb",
        }
    }
    #[inline]
    fn idx(self) -> usize {
        self as usize
    }
}

/// Estado honesto da linha: ausente != desconhecido != funcionando.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum HwState {
    Absent = 0,
    Unknown = 1,
    Ok = 2,
}

impl HwState {
    pub fn as_str(self) -> &'static str {
        match self {
            HwState::Absent => "absent",
            HwState::Unknown => "unknown",
            HwState::Ok => "ok",
        }
    }
    #[inline]
    pub fn code(self) -> u8 {
        self as u8
    }
}

pub const HW_DETAIL_LEN: usize = 44;
pub const HW_UNKNOWN_MAX: usize = 8;
pub const HW_UNKNOWN_LEN: usize = 40;
const HW_CAND_MAX: usize = 4;

/// Uma linha do card (label + estado + detalhe ASCII, sem alocação).
#[derive(Clone, Copy)]
pub struct HwRowEntry {
    pub row: HwRow,
    pub state: HwState,
    detail: [u8; HW_DETAIL_LEN],
    detail_len: u8,
}

impl HwRowEntry {
    pub fn detail(&self) -> &str {
        core::str::from_utf8(&self.detail[..self.detail_len as usize]).unwrap_or("")
    }
    pub fn new(row: HwRow, state: HwState, detail: &str) -> Self {
        let mut e = empty_row(row);
        e.state = state;
        let b = detail.as_bytes();
        let n = b.len().min(HW_DETAIL_LEN);
        e.detail[..n].copy_from_slice(&b[..n]);
        e.detail_len = n as u8;
        e
    }
}

const fn empty_row(row: HwRow) -> HwRowEntry {
    HwRowEntry {
        row,
        state: HwState::Absent,
        detail: [0u8; HW_DETAIL_LEN],
        detail_len: 0,
    }
}

/// Device não suportado, listado honestamente (`PCI vvvv:dddd <classe>`).
#[derive(Clone, Copy)]
pub struct HwUnknown {
    label: [u8; HW_UNKNOWN_LEN],
    len: u8,
}

impl HwUnknown {
    pub fn label(&self) -> &str {
        core::str::from_utf8(&self.label[..self.len as usize]).unwrap_or("")
    }
    pub fn new(label: &str) -> Self {
        let mut u = HwUnknown {
            label: [0u8; HW_UNKNOWN_LEN],
            len: 0,
        };
        let b = label.as_bytes();
        let n = b.len().min(HW_UNKNOWN_LEN);
        u.label[..n].copy_from_slice(&b[..n]);
        u.len = n as u8;
        u
    }
}

const EMPTY_UNKNOWN: HwUnknown = HwUnknown {
    label: [0u8; HW_UNKNOWN_LEN],
    len: 0,
};

/// Snapshot Copy — o compositor lê e desenha, zero alocação.
#[derive(Clone, Copy)]
pub struct HwInventory {
    pub rows: [HwRowEntry; HW_ROWS],
    pub unknown: [HwUnknown; HW_UNKNOWN_MAX],
    pub unknown_len: u8,
    pub unknown_total: u16,
    pub found: u16,
    pub gen: u64,
}

impl HwInventory {
    pub const fn empty() -> Self {
        HwInventory {
            rows: [
                empty_row(HwRow::Nic),
                empty_row(HwRow::Wifi),
                empty_row(HwRow::Gpu),
                empty_row(HwRow::Storage),
                empty_row(HwRow::Usb),
            ],
            unknown: [EMPTY_UNKNOWN; HW_UNKNOWN_MAX],
            unknown_len: 0,
            unknown_total: 0,
            found: 0,
            gen: 0,
        }
    }
}

// ── Staging (produtor escreve uma vez; consumidor nunca aloca) ──────────────

#[derive(Clone, Copy)]
struct Cand {
    vid: u16,
    did: u16,
    class: u8,
    family: u8,
    next_action: u8,
    nic_kind: u8,
    known: bool,
}

const EMPTY_CAND: Cand = Cand {
    vid: 0,
    did: 0,
    class: 0,
    family: 0,
    next_action: 0,
    nic_kind: 0,
    known: false,
};

#[derive(Clone, Copy)]
struct StageRow {
    cands: [Cand; HW_CAND_MAX],
    n: u8,
}

const EMPTY_STAGE_ROW: StageRow = StageRow {
    cands: [EMPTY_CAND; HW_CAND_MAX],
    n: 0,
};

#[derive(Clone, Copy)]
struct Stage {
    rows: [StageRow; HW_ROWS],
    unknown: [HwUnknown; HW_UNKNOWN_MAX],
    unknown_len: u8,
    unknown_total: u16,
    found: u16,
}

impl Stage {
    const fn empty() -> Self {
        Stage {
            rows: [EMPTY_STAGE_ROW; HW_ROWS],
            unknown: [EMPTY_UNKNOWN; HW_UNKNOWN_MAX],
            unknown_len: 0,
            unknown_total: 0,
            found: 0,
        }
    }
}

static STAGE: Mutex<Option<Stage>> = Mutex::new(None);
static SNAPSHOT: Mutex<Option<HwInventory>> = Mutex::new(None);
static GEN: AtomicU64 = AtomicU64::new(0);

/// Começa um inventário novo (chamado 1× pelo HwDetectAgent antes do loop).
pub fn reset() {
    *STAGE.lock() = Some(Stage::empty());
}

/// Classifica o subsistema pela família (tabela/heurística) e, se a família é
/// desconhecida, pela classe PCI — assim um WiFi/NIC/GPU sem driver ainda cai
/// no subsistema certo em vez de sumir.
fn classify_row(card: &HwCapabilityCard) -> Option<HwRow> {
    match card.family {
        HwFamily::IntelIwlWifi
        | HwFamily::RealtekWifi
        | HwFamily::AtherosWifi
        | HwFamily::BroadcomWifi => Some(HwRow::Wifi),
        HwFamily::IntelE1000 | HwFamily::VirtioNet | HwFamily::RealtekEth => Some(HwRow::Nic),
        HwFamily::NvidiaGpu
        | HwFamily::IntelI915
        | HwFamily::AmdGpu
        | HwFamily::QemuVga
        | HwFamily::VirtioGpu => Some(HwRow::Gpu),
        HwFamily::StorageAta => Some(HwRow::Storage),
        HwFamily::UsbHostXhci => Some(HwRow::Usb),
        _ => match (card.class, card.subclass) {
            (0x0D, _) => Some(HwRow::Wifi),
            (0x02, 0x80) => Some(HwRow::Wifi),
            (0x02, _) => Some(HwRow::Nic),
            (0x03, _) => Some(HwRow::Gpu),
            (0x01, _) => Some(HwRow::Storage),
            (0x0C, 0x03) => Some(HwRow::Usb),
            _ => None,
        },
    }
}

/// HwDetectAgent chama por card detectado.
pub fn record_card(card: &HwCapabilityCard) {
    let mut g = STAGE.lock();
    let Some(st) = g.as_mut() else { return };
    st.found = st.found.saturating_add(1);

    if let Some(r) = classify_row(card) {
        let sr = &mut st.rows[r.idx()];
        if (sr.n as usize) < HW_CAND_MAX {
            sr.cands[sr.n as usize] = Cand {
                vid: card.vid,
                did: card.did,
                class: card.class,
                family: card.family as u8,
                next_action: card.next_action as u8,
                nic_kind: k_nano::boot_bind::classify_nic(card.vid, card.did) as u8,
                known: card.family != HwFamily::Unknown,
            };
            sr.n += 1;
        }
    }

    // Device não suportado (família unknown) — nunca desaparece.
    if card.family == HwFamily::Unknown {
        st.unknown_total = st.unknown_total.saturating_add(1);
        if (st.unknown_len as usize) < HW_UNKNOWN_MAX {
            let label = format!(
                "PCI {:04x}:{:04x} {}",
                card.vid,
                card.did,
                k_ai::hw_agents::class_name(card.class)
            );
            st.unknown[st.unknown_len as usize] = HwUnknown::new(&label);
            st.unknown_len += 1;
        }
    }
}

/// Prefere o candidato de família conhecida (mais informativo) — senão o 1º.
fn prefer_known(sr: &StageRow) -> Option<Cand> {
    let mut first: Option<Cand> = None;
    for c in sr.cands.iter().take(sr.n as usize) {
        if first.is_none() {
            first = Some(*c);
        }
        if c.known {
            return Some(*c);
        }
    }
    first
}

fn family_str(f: u8) -> &'static str {
    match f {
        1 => "intel_e1000",
        2 => "virtio_net",
        3 => "realtek_eth",
        4 => "intel_iwlwifi",
        5 => "realtek_wifi",
        6 => "atheros_wifi",
        7 => "broadcom_wifi",
        8 => "nvidia_gpu",
        9 => "intel_i915",
        10 => "amd_gpu",
        11 => "qemu_vga",
        12 => "virtio_gpu",
        13 => "usb_xhci",
        14 => "intel_hda",
        15 => "storage_ata",
        16 => "pci_bridge",
        _ => "unknown",
    }
}

/// NIC bound (globals R0). Ordem = prioridade boot_bind.
fn bound_nic() -> (&'static str, u8) {
    if k_nano::nic_globals::I225.lock().is_some() {
        return ("i225", NicKind::I225 as u8);
    }
    if k_nano::nic_globals::VIRTIO_DEV.lock().is_some() {
        return ("virtio", NicKind::Virtio as u8);
    }
    if k_nano::nic_globals::E1000.lock().is_some() {
        return ("e1000", NicKind::E1000 as u8);
    }
    if k_nano::nic_globals::RTL8168.lock().is_some() {
        return ("rtl8168", NicKind::Rtl8168 as u8);
    }
    if k_nano::nic_globals::RTL8139.lock().is_some() {
        return ("rtl8139", NicKind::Rtl8139 as u8);
    }
    ("none", NicKind::None as u8)
}

fn resolve_nic(sr: &StageRow) -> HwRowEntry {
    let (drv, kind) = bound_nic();
    let none = NicKind::None as u8;
    if sr.n == 0 {
        return if kind != none {
            HwRowEntry::new(HwRow::Nic, HwState::Ok, &format!("{} ativo", drv))
        } else {
            HwRowEntry::new(HwRow::Nic, HwState::Absent, "ausente")
        };
    }
    let mut chosen = sr.cands[0];
    if kind != none {
        for c in sr.cands.iter().take(sr.n as usize) {
            if c.nic_kind == kind {
                chosen = *c;
                break;
            }
        }
        HwRowEntry::new(
            HwRow::Nic,
            HwState::Ok,
            &format!("{} {:04x}:{:04x} ativo", drv, chosen.vid, chosen.did),
        )
    } else {
        HwRowEntry::new(
            HwRow::Nic,
            HwState::Unknown,
            &format!("{:04x}:{:04x} sem driver", chosen.vid, chosen.did),
        )
    }
}

fn resolve_wifi(sr: &StageRow) -> HwRowEntry {
    if sr.n == 0 {
        return HwRowEntry::new(HwRow::Wifi, HwState::Absent, "ausente");
    }
    let chosen = prefer_known(sr).unwrap_or(sr.cands[0]);
    // Status honesto: denied (cap gate FE) > bound (probe real) > unknown.
    // WIFI_PRESENT só é setado após probe/bind real (k_hal::net).
    let (state, status) = match k_hal::net_port::status() {
        k_hal::net_port::NetPortStatus::Denied => (HwState::Unknown, "negado"),
        _ if k_hal::net::generic_wifi::WIFI_PRESENT.load(Ordering::Relaxed) => {
            (HwState::Ok, "ativo")
        }
        _ => (HwState::Unknown, "sem driver"),
    };
    let detail = format!(
        "{} {:04x}:{:04x} {} {}",
        family_str(chosen.family),
        chosen.vid,
        chosen.did,
        k_ai::hw_agents::class_name(chosen.class),
        status
    );
    HwRowEntry::new(HwRow::Wifi, state, &detail)
}

fn resolve_gpu(sr: &StageRow) -> HwRowEntry {
    if sr.n == 0 {
        return HwRowEntry::new(HwRow::Gpu, HwState::Absent, "ausente");
    }
    let chosen = prefer_known(sr).unwrap_or(sr.cands[0]);
    let fam = family_str(chosen.family);
    // next_action=Ready = backend usável (QEMU/VirtIO); LoadFirmware = sem backend.
    if chosen.next_action == HwNextAction::Ready as u8 {
        HwRowEntry::new(
            HwRow::Gpu,
            HwState::Ok,
            &format!("{} {:04x}:{:04x} pronto", fam, chosen.vid, chosen.did),
        )
    } else {
        HwRowEntry::new(
            HwRow::Gpu,
            HwState::Unknown,
            &format!("{} {:04x}:{:04x} sem backend", fam, chosen.vid, chosen.did),
        )
    }
}

/// Storage bound (globals R0). Prioridade boot_bind: NVMe > AHCI > ATA > USB-MSC.
fn bound_storage() -> (&'static str, bool) {
    if k_nano::disk_agent::nvme::NVME_DRIVER.lock().is_some() {
        return ("nvme", true);
    }
    if k_nano::globals::AHCI_DRIVER.lock().is_some() {
        return ("ahci", true);
    }
    if k_nano::globals::ATA_DRIVER.lock().is_some() {
        return ("ata", true);
    }
    if k_nano::globals::USB_MSC.lock().is_some() {
        return ("usb-msc", true);
    }
    ("none", false)
}

fn resolve_storage(sr: &StageRow) -> HwRowEntry {
    let (bus, bound) = bound_storage();
    if sr.n == 0 {
        return if bound {
            HwRowEntry::new(HwRow::Storage, HwState::Ok, &format!("{} ativo", bus))
        } else {
            HwRowEntry::new(HwRow::Storage, HwState::Absent, "ausente")
        };
    }
    let chosen = prefer_known(sr).unwrap_or(sr.cands[0]);
    if bound {
        HwRowEntry::new(
            HwRow::Storage,
            HwState::Ok,
            &format!("{} {:04x}:{:04x}", bus, chosen.vid, chosen.did),
        )
    } else {
        HwRowEntry::new(
            HwRow::Storage,
            HwState::Unknown,
            &format!("{:04x}:{:04x} sem driver", chosen.vid, chosen.did),
        )
    }
}

fn resolve_usb(sr: &StageRow) -> HwRowEntry {
    // xHCI stage 10 = host up (k_nano::xhci); sem ports = não enumerado.
    let stage = k_nano::xhci::xhci_last_stage();
    match k_nano::xhci::host_max_ports() {
        Some(p) => HwRowEntry::new(
            HwRow::Usb,
            if stage == 10 { HwState::Ok } else { HwState::Unknown },
            &format!("xhci {}p st{}", p, stage),
        ),
        None if sr.n == 0 => HwRowEntry::new(HwRow::Usb, HwState::Absent, "ausente"),
        None => HwRowEntry::new(HwRow::Usb, HwState::Unknown, "xhci sem portas"),
    }
}

/// Fecha o inventário, lê os binds reais e publica o snapshot.
pub fn finalize_and_publish() {
    let stage = *STAGE.lock();
    let Some(st) = stage else { return };
    let mut inv = HwInventory::empty();
    inv.found = st.found;
    inv.unknown = st.unknown;
    inv.unknown_len = st.unknown_len;
    inv.unknown_total = st.unknown_total;
    inv.rows[HwRow::Nic.idx()] = resolve_nic(&st.rows[HwRow::Nic.idx()]);
    inv.rows[HwRow::Wifi.idx()] = resolve_wifi(&st.rows[HwRow::Wifi.idx()]);
    inv.rows[HwRow::Gpu.idx()] = resolve_gpu(&st.rows[HwRow::Gpu.idx()]);
    inv.rows[HwRow::Storage.idx()] = resolve_storage(&st.rows[HwRow::Storage.idx()]);
    inv.rows[HwRow::Usb.idx()] = resolve_usb(&st.rows[HwRow::Usb.idx()]);

    let gen = GEN.fetch_add(1, Ordering::AcqRel) + 1;
    inv.gen = gen;
    *SNAPSHOT.lock() = Some(inv);
    publish();

    k_nano::slog_hermes!(
        "HW-INV",
        "ok",
        "found={} unknown={} nic={} wifi={} gpu={} storage={} usb={}",
        inv.found,
        inv.unknown_total,
        inv.rows[HwRow::Nic.idx()].state.as_str(),
        inv.rows[HwRow::Wifi.idx()].state.as_str(),
        inv.rows[HwRow::Gpu.idx()].state.as_str(),
        inv.rows[HwRow::Storage.idx()].state.as_str(),
        inv.rows[HwRow::Usb.idx()].state.as_str(),
    );
}

/// Snapshot corrente (Copy — o compositor desenha sem lock no paint).
pub fn snapshot() -> HwInventory {
    let g = SNAPSHOT.lock();
    match *g {
        Some(i) => i,
        None => HwInventory::empty(),
    }
}

pub fn generation() -> u64 {
    GEN.load(Ordering::Acquire)
}

fn publish() {
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_HW_INVENTORY_STATE),
        payload: alloc::vec::Vec::new(),
        token: event_bus::CapabilityToken::Legacy(1),
    });
}

/// Re-emite o snapshot guardado — consumidor que assinou tarde não perde o card.
pub fn republish() {
    if generation() > 0 {
        publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_ai::hw_capability::build_card;

    /// Serializa testes que mutam os statics (padrão boot_report).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn unknown_wifi_does_not_vanish() {
        let _g = TEST_LOCK.lock();
        reset();
        // MediaTek 14c3:7925 — WiFi (class 0x02 sub 0x80) sem tabela/heurística.
        let card = build_card(0x14c3, 0x7925, 0x02, 0x80, "PCI 14c3:7925");
        assert_eq!(card.family, HwFamily::Unknown);
        record_card(&card);
        finalize_and_publish();

        let inv = snapshot();
        let wifi = inv.rows[HwRow::Wifi as usize];
        assert_eq!(wifi.state, HwState::Unknown);
        assert!(wifi.detail().contains("14c3:7925"));
        assert_eq!(inv.unknown_len, 1);
        assert_eq!(inv.unknown[0].label(), "PCI 14c3:7925 Network");
    }

    #[test]
    fn known_nic_lands_on_nic_row() {
        let _g = TEST_LOCK.lock();
        reset();
        let card = build_card(0x8086, 0x100e, 0x02, 0x00, "Intel PRO/1000");
        assert_ne!(card.family, HwFamily::Unknown);
        record_card(&card);
        finalize_and_publish();
        let inv = snapshot();
        let nic = inv.rows[HwRow::Nic as usize];
        assert_ne!(nic.state, HwState::Absent);
        assert!(nic.detail().contains("8086:100e"));
        // Família conhecida não entra na lista de não-suportados.
        assert_eq!(inv.unknown_len, 0);
    }

    #[test]
    fn absent_subsystem_is_not_ok() {
        let _g = TEST_LOCK.lock();
        reset();
        finalize_and_publish();
        let inv = snapshot();
        assert_eq!(inv.rows[HwRow::Wifi as usize].state, HwState::Absent);
        assert_eq!(inv.rows[HwRow::Wifi as usize].detail(), "ausente");
    }

    #[test]
    fn unknown_list_is_capped_but_total_is_honest() {
        let _g = TEST_LOCK.lock();
        reset();
        // 9 devices desconhecidos → lista capa em 8, total honesto = 9.
        for i in 0..9u16 {
            let card = build_card(0x9999, 0x1000 + i, 0x02, 0x00, "x");
            record_card(&card);
        }
        finalize_and_publish();
        let inv = snapshot();
        assert_eq!(inv.unknown_len as usize, HW_UNKNOWN_MAX);
        assert_eq!(inv.unknown_total, 9);
    }
}
