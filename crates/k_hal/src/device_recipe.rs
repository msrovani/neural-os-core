//! DeviceRecipe bind table (ADR-0056 H1) — promove bind só com recipe trusted.
//! Unsigned / hash miss → Escalate; FAT miss → NeedsFw; sem fake Ready.

use crate::device_cap::DeviceClass;
use crate::unlock_dag::{self, CapToken, UnlockStage};

/// Como casar a entrada na tabela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeMatch {
    /// VID+DID exactos
    VidDid,
    /// Qualquer dispositivo da classe (ex.: xHCI)
    ClassOnly,
}

/// Entrada estática (goldens in-tree). Runtime PackageHub pode espelhar depois.
#[derive(Clone, Copy)]
pub struct RecipeBindEntry {
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: DeviceClass,
    pub match_mode: RecipeMatch,
    pub package_id: &'static str,
    /// true = recipe nativa / BE in-tree trusted seed
    pub trusted: bool,
    /// HW rebelde: exige recipe+FW path (Wifi/Gpu)
    pub rebel: bool,
    /// Se true, exige blobs FAT presentes antes de promote Ok
    pub requires_fw: bool,
    pub fat_names: &'static [&'static str],
}

/// Goldens alinhados a docs/specs/device-lego/examples/
pub static GOLDEN_RECIPES: &[RecipeBindEntry] = &[
    RecipeBindEntry {
        vendor_id: 0x1AF4,
        device_id: 0x1041,
        class: DeviceClass::Net,
        match_mode: RecipeMatch::VidDid,
        package_id: "net.virtio",
        trusted: true,
        rebel: false,
        requires_fw: false,
        fat_names: &[],
    },
    RecipeBindEntry {
        vendor_id: 0x168C,
        device_id: 0x003E,
        class: DeviceClass::Wifi,
        match_mode: RecipeMatch::VidDid,
        package_id: "wifi.qca6174.ath10k",
        trusted: true,
        rebel: true,
        requires_fw: true,
        fat_names: &["AT10K_F6.BIN", "AT10K_B2.BIN", "AT10K_BD.BIN"],
    },
    RecipeBindEntry {
        vendor_id: 0x10DE,
        device_id: 0x1C82,
        class: DeviceClass::Gpu,
        match_mode: RecipeMatch::VidDid,
        package_id: "gpu.nvidia.gp108",
        trusted: true,
        rebel: true,
        requires_fw: true,
        // Subset short-name (main.rs GP108 preload)
        fat_names: &["ACR_BL.BIN", "ACRLOAD.BIN", "GPCCS_IN.BIN"],
    },
    RecipeBindEntry {
        vendor_id: 0,
        device_id: 0,
        class: DeviceClass::UsbHost,
        match_mode: RecipeMatch::ClassOnly,
        package_id: "usb.xhci.host",
        trusted: true,
        rebel: false,
        requires_fw: false,
        fat_names: &[],
    },
    // Template BT — observe/escalate até path A/B medido
    RecipeBindEntry {
        vendor_id: 0,
        device_id: 0,
        class: DeviceClass::Bluetooth,
        match_mode: RecipeMatch::ClassOnly,
        package_id: "bt.template",
        trusted: false,
        rebel: true,
        requires_fw: false,
        fat_names: &[],
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RecipePromote {
    Ok = 0,
    Escalate = 1,
    NeedsFw = 2,
    None = 3,
}

impl RecipePromote {
    pub fn as_str(self) -> &'static str {
        match self {
            RecipePromote::Ok => "OK",
            RecipePromote::Escalate => "ESCALATE",
            RecipePromote::NeedsFw => "NEEDS_FW",
            RecipePromote::None => "NONE",
        }
    }

    pub fn to_stage(self) -> UnlockStage {
        match self {
            RecipePromote::Ok => UnlockStage::Partial, // Ok ≠ Ready
            RecipePromote::Escalate => UnlockStage::Quarantined,
            RecipePromote::NeedsFw => UnlockStage::NeedsFw,
            RecipePromote::None => UnlockStage::Locked,
        }
    }
}

fn fat_root_has(name: &str) -> bool {
    crate::fat_assets::root_has(name)
}

/// Presença dos blobs (H1). Hash criptográfico pleno = residual PackageHub.
pub fn fat_blobs_ok(names: &[&str]) -> bool {
    if names.is_empty() {
        return true;
    }
    names.iter().all(|n| fat_root_has(n))
}

/// Hint: algum FAT montável (UnlockDAG FatReadable).
pub fn fat_readable_hint() -> bool {
    fat_root_has("BOOT.LOG")
        || fat_root_has("AT10K_F6.BIN")
        || fat_root_has("HWEXPRT.BIN")
        || fat_root_has("BITNET2B.BIN")
}

fn entry_matches(e: &RecipeBindEntry, vid: u16, did: u16, class: DeviceClass) -> bool {
    match e.match_mode {
        RecipeMatch::VidDid => e.vendor_id == vid && e.device_id == did,
        RecipeMatch::ClassOnly => e.class == class,
    }
}

pub fn find_entry(vid: u16, did: u16, class: DeviceClass) -> Option<&'static RecipeBindEntry> {
    GOLDEN_RECIPES
        .iter()
        .find(|e| entry_matches(e, vid, did, class))
}

/// Match VID/DID/class → promoção. `fw_ok` = caller ou fat_blobs_ok.
pub fn try_promote(
    vid: u16,
    did: u16,
    class: DeviceClass,
    fw_ok: bool,
) -> (RecipePromote, Option<&'static RecipeBindEntry>) {
    let Some(entry) = find_entry(vid, did, class) else {
        return (RecipePromote::None, None);
    };
    if !entry.trusted {
        k_nano::slog_hal!(
            "RECIPE",
            "info",
            "promote=ESCALATE pkg={} (unsigned)",
            entry.package_id
        );
        return (RecipePromote::Escalate, Some(entry));
    }
    if entry.requires_fw && !fw_ok {
        k_nano::slog_hal!(
            "RECIPE",
            "info",
            "promote=NEEDS_FW pkg={}",
            entry.package_id
        );
        return (RecipePromote::NeedsFw, Some(entry));
    }
    k_nano::slog_hal!(
        "RECIPE",
        "info",
        "promote=OK pkg={} class={} (≠Ready)",
        entry.package_id,
        entry.class.as_str()
    );
    (RecipePromote::Ok, Some(entry))
}

/// Avalia dispositivo PCI: FAT + promote + stage.
pub fn evaluate_device(vid: u16, did: u16, class: DeviceClass) -> RecipePromote {
    let entry = match find_entry(vid, did, class) {
        Some(e) => e,
        None => return RecipePromote::None,
    };
    let fw_ok = if entry.requires_fw {
        fat_blobs_ok(entry.fat_names)
    } else {
        true
    };
    let (p, _) = try_promote(vid, did, class, fw_ok);
    p
}

/// Gate HalOffer::bind: só restringe se houver **recipe** casada.
/// Sem recipe → L1 legado (BE in-tree / main.rs). Com recipe rebelde → trusted+FW.
pub fn gate_bind_class(class: DeviceClass) -> Result<&'static str, RecipePromote> {
    use crate::discovery;

    let tree = discovery::device_tree();
    let of_class: alloc::vec::Vec<_> = tree.into_iter().filter(|c| c.id.class == class).collect();

    if of_class.is_empty() {
        return Ok("no_device");
    }

    let mut matched = false;
    let mut best_pkg: &'static str = "legacy_l1";
    let mut worst = RecipePromote::Ok;

    for cap in &of_class {
        let Some(e) = find_entry(cap.id.vendor_id, cap.id.device_id, class) else {
            continue;
        };
        matched = true;
        best_pkg = e.package_id;
        let fw_ok = if e.requires_fw {
            fat_blobs_ok(e.fat_names)
        } else {
            true
        };
        let (p, _) = try_promote(cap.id.vendor_id, cap.id.device_id, class, fw_ok);
        worst = worse(worst, p);
        let _ = e.rebel; // rebel só informativo; promote já encapsula trust/FW
    }

    if !matched {
        return Ok("legacy_l1");
    }
    match worst {
        RecipePromote::Ok => Ok(best_pkg),
        other => Err(other),
    }
}

fn worse(a: RecipePromote, b: RecipePromote) -> RecipePromote {
    use RecipePromote::*;
    let rank = |p: RecipePromote| match p {
        Ok => 0u8,
        None => 1,
        NeedsFw => 2,
        Escalate => 3,
    };
    if rank(b) > rank(a) {
        b
    } else {
        a
    }
}

/// Log de descoberta + tokens auxiliares.
pub fn log_match(vid: u16, did: u16, class: DeviceClass) -> RecipePromote {
    let p = evaluate_device(vid, did, class);
    if let Some(entry) = find_entry(vid, did, class) {
        k_nano::slog_hal!(
            "RECIPE",
            "info",
            "match={:04x}:{:04x} pkg={} status={} stage={}",
            vid,
            did,
            entry.package_id,
            p.as_str(),
            p.to_stage().as_str()
        );
        if p == RecipePromote::Ok && entry.class == DeviceClass::UsbHost {
            unlock_dag::grant(CapToken::UsbHostReset);
        }
    }
    p
}
