//! Plano de bind NIC/storage (R0) — evidência PCI, sem política Trust.
//! k_ai observa o DeviceTree (k_hal) e instala a ordem; o bin só executa.
//! AIOS (ADR-0088): não martelar E1000→I225→RTL se o silício já disse o contrário.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Família NIC conhecida no R0 (tabela, não NN — SESSION_248).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NicKind {
    None = 0,
    I225 = 1,
    Virtio = 2,
    E1000 = 3,
    Rtl8139 = 4,
}

impl NicKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NicKind::None => "none",
            NicKind::I225 => "i225",
            NicKind::Virtio => "virtio-net",
            NicKind::E1000 => "e1000",
            NicKind::Rtl8139 => "rtl8139",
        }
    }
}

/// Prioridade quando vários NICs existem: I225 (HW real) > VirtIO (QEMU) > e1000 > RTL.
const PRIORITY: [NicKind; 4] = [
    NicKind::I225,
    NicKind::Virtio,
    NicKind::E1000,
    NicKind::Rtl8139,
];

/// Fallback se H1 não publicou DeviceTree (scan vazio / ainda não rodou).
const LEGACY: [NicKind; 4] = [
    NicKind::E1000,
    NicKind::I225,
    NicKind::Rtl8139,
    NicKind::None,
];

static INSTALLED: AtomicBool = AtomicBool::new(false);
static SLOT0: AtomicUsize = AtomicUsize::new(0);
static SLOT1: AtomicUsize = AtomicUsize::new(0);
static SLOT2: AtomicUsize = AtomicUsize::new(0);
static SLOT3: AtomicUsize = AtomicUsize::new(0);
static N: AtomicUsize = AtomicUsize::new(0);
static TREE_N: AtomicUsize = AtomicUsize::new(0);

pub fn classify_nic(vid: u16, did: u16) -> NicKind {
    if crate::i225::is_i225_family(vid, did) {
        return NicKind::I225;
    }
    if vid == 0x1AF4
        && (did == crate::virtio_net::VIRTIO_NET_TRANSITIONAL
            || did == crate::virtio_net::VIRTIO_NET_MODERN)
    {
        return NicKind::Virtio;
    }
    if crate::e1000::is_e1000_family(vid, did) {
        return NicKind::E1000;
    }
    if vid == 0x10EC && did == 0x8139 {
        return NicKind::Rtl8139;
    }
    NicKind::None
}

/// Rank estável a partir dos kinds observados (dedupe + PRIORITY).
pub fn rank_present(present: &[NicKind]) -> ([NicKind; 4], usize) {
    let mut out = [NicKind::None; 4];
    let mut n = 0usize;
    for want in PRIORITY {
        if present.iter().any(|k| *k == want) {
            out[n] = want;
            n += 1;
        }
    }
    (out, n)
}

/// k_ai chama após observar o DeviceTree.
/// `tree_len == 0` → plano legado (H1 ainda não viu silício).
/// `tree_len > 0` e nenhum NIC → n=0 (não martelar driver ausente).
pub fn install_plan(present: &[NicKind], tree_len: usize) {
    TREE_N.store(tree_len, Ordering::Relaxed);
    let (order, n) = if tree_len == 0 {
        (LEGACY, 3usize)
    } else {
        rank_present(present)
    };
    SLOT0.store(order[0] as usize, Ordering::Relaxed);
    SLOT1.store(order[1] as usize, Ordering::Relaxed);
    SLOT2.store(order[2] as usize, Ordering::Relaxed);
    SLOT3.store(order[3] as usize, Ordering::Relaxed);
    N.store(n, Ordering::Relaxed);
    INSTALLED.store(true, Ordering::Relaxed);
}

fn kind_from_slot(v: usize) -> NicKind {
    match v {
        1 => NicKind::I225,
        2 => NicKind::Virtio,
        3 => NicKind::E1000,
        4 => NicKind::Rtl8139,
        _ => NicKind::None,
    }
}

/// Ordem a executar no DriverInit. Se k_ai ainda não instalou, legado.
pub fn nic_probe_order() -> ([NicKind; 4], usize) {
    if !INSTALLED.load(Ordering::Relaxed) {
        return (LEGACY, 3);
    }
    (
        [
            kind_from_slot(SLOT0.load(Ordering::Relaxed)),
            kind_from_slot(SLOT1.load(Ordering::Relaxed)),
            kind_from_slot(SLOT2.load(Ordering::Relaxed)),
            kind_from_slot(SLOT3.load(Ordering::Relaxed)),
        ],
        N.load(Ordering::Relaxed),
    )
}

pub fn observed_tree_len() -> usize {
    TREE_N.load(Ordering::Relaxed)
}

/// Storage: NVMe > AHCI > USB-MSC (live) > ATA PIO (último — hang TCG/SESSION_243).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StorageKind {
    None = 0,
    Nvme = 1,
    Ahci = 2,
    UsbHost = 3,
    Ata = 4,
}

impl StorageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StorageKind::None => "none",
            StorageKind::Nvme => "nvme",
            StorageKind::Ahci => "ahci",
            StorageKind::UsbHost => "usb-msc",
            StorageKind::Ata => "ata-pio",
        }
    }
}

const STOR_PRIORITY: [StorageKind; 4] = [
    StorageKind::Nvme,
    StorageKind::Ahci,
    StorageKind::UsbHost,
    StorageKind::Ata,
];

const STOR_LEGACY: [StorageKind; 4] = STOR_PRIORITY;

static STOR_ON: AtomicBool = AtomicBool::new(false);
static STOR0: AtomicUsize = AtomicUsize::new(0);
static STOR1: AtomicUsize = AtomicUsize::new(0);
static STOR2: AtomicUsize = AtomicUsize::new(0);
static STOR3: AtomicUsize = AtomicUsize::new(0);
static STOR_N: AtomicUsize = AtomicUsize::new(0);
static HAS_SND: AtomicBool = AtomicBool::new(true);

pub fn classify_storage(pci_class: u8, pci_subclass: u8) -> StorageKind {
    match (pci_class, pci_subclass) {
        (0x01, 0x08) => StorageKind::Nvme,
        (0x01, 0x06) => StorageKind::Ahci,
        (0x0C, 0x03) => StorageKind::UsbHost,
        (0x01, _) => StorageKind::Ata,
        _ => StorageKind::None,
    }
}

pub fn rank_storage(present: &[StorageKind]) -> ([StorageKind; 4], usize) {
    let mut out = [StorageKind::None; 4];
    let mut n = 0usize;
    for want in STOR_PRIORITY {
        if present.iter().any(|k| *k == want) {
            out[n] = want;
            n += 1;
        }
    }
    (out, n)
}

pub fn install_storage_plan(present: &[StorageKind], tree_len: usize) {
    let (order, n) = if tree_len == 0 {
        (STOR_LEGACY, 4usize)
    } else {
        rank_storage(present)
    };
    STOR0.store(order[0] as usize, Ordering::Relaxed);
    STOR1.store(order[1] as usize, Ordering::Relaxed);
    STOR2.store(order[2] as usize, Ordering::Relaxed);
    STOR3.store(order[3] as usize, Ordering::Relaxed);
    STOR_N.store(n, Ordering::Relaxed);
    STOR_ON.store(true, Ordering::Relaxed);
}

pub fn set_has_snd(v: bool) {
    HAS_SND.store(v, Ordering::Relaxed);
}

fn stor_from_slot(v: usize) -> StorageKind {
    match v {
        1 => StorageKind::Nvme,
        2 => StorageKind::Ahci,
        3 => StorageKind::UsbHost,
        4 => StorageKind::Ata,
        _ => StorageKind::None,
    }
}

pub fn storage_plan_active() -> bool {
    STOR_ON.load(Ordering::Relaxed)
}

pub fn storage_probe_order() -> ([StorageKind; 4], usize) {
    if !STOR_ON.load(Ordering::Relaxed) {
        return (STOR_LEGACY, 4);
    }
    (
        [
            stor_from_slot(STOR0.load(Ordering::Relaxed)),
            stor_from_slot(STOR1.load(Ordering::Relaxed)),
            stor_from_slot(STOR2.load(Ordering::Relaxed)),
            stor_from_slot(STOR3.load(Ordering::Relaxed)),
        ],
        STOR_N.load(Ordering::Relaxed),
    )
}

pub fn storage_includes(kind: StorageKind) -> bool {
    if !STOR_ON.load(Ordering::Relaxed) {
        return true;
    }
    let (o, n) = storage_probe_order();
    o[..n].iter().any(|k| *k == kind)
}

pub fn should_probe_snd() -> bool {
    if !STOR_ON.load(Ordering::Relaxed) {
        return true;
    }
    HAS_SND.load(Ordering::Relaxed)
}

/// xHCI/MSC: só se o plano viu UsbHost (ou H1 ainda não instalou).
pub fn should_probe_usb_host() -> bool {
    storage_includes(StorageKind::UsbHost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_nics() {
        assert_eq!(classify_nic(0x8086, 0x15F2), NicKind::I225);
        assert_eq!(classify_nic(0x8086, 0x100E), NicKind::E1000);
        assert_eq!(classify_nic(0x1AF4, 0x1041), NicKind::Virtio);
        assert_eq!(classify_nic(0x10EC, 0x8139), NicKind::Rtl8139);
        assert_eq!(classify_nic(0x8086, 0x9A14), NicKind::None);
    }

    #[test]
    fn rank_i225_beats_e1000() {
        let (o, n) = rank_present(&[NicKind::E1000, NicKind::I225]);
        assert_eq!(n, 2);
        assert_eq!(o[0], NicKind::I225);
        assert_eq!(o[1], NicKind::E1000);
    }

    #[test]
    fn rank_empty_present_is_skip() {
        let (_o, n) = rank_present(&[]);
        assert_eq!(n, 0);
    }

    #[test]
    fn classify_storage_families() {
        assert_eq!(classify_storage(0x01, 0x08), StorageKind::Nvme);
        assert_eq!(classify_storage(0x01, 0x06), StorageKind::Ahci);
        assert_eq!(classify_storage(0x01, 0x01), StorageKind::Ata);
        assert_eq!(classify_storage(0x0C, 0x03), StorageKind::UsbHost);
        assert_eq!(classify_storage(0x02, 0x00), StorageKind::None);
    }

    #[test]
    fn nvme_before_ata() {
        let (o, n) = rank_storage(&[StorageKind::Ata, StorageKind::Nvme]);
        assert_eq!(n, 2);
        assert_eq!(o[0], StorageKind::Nvme);
        assert_eq!(o[1], StorageKind::Ata);
    }
}
