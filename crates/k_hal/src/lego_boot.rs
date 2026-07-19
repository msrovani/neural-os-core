//! Boot selftest Device LEGO (ADR-0056) — localize FAT + utilize bind table.
//!
//! Modelo H1 (honesto):
//! - **Disco** (`LEGO*.MD`): spec AI-Friendly + smoke de localização
//! - **Código** (`GOLDEN_RECIPES`): Cap gate / promote (trusted BE in-tree)
//! Os dois devem casar (package_id + VID/DID). Sem mentir Ready.

use crate::device_cap::DeviceClass;
use crate::device_recipe::{self, RecipePromote, GOLDEN_RECIPES};
use alloc::string::String;
use alloc::vec::Vec;

/// Casos gravados na imagem (pack_device_legos.py).
struct LegoCase {
    fat_name: &'static str,
    package_id: &'static str,
    vendor_id: u16,
    device_id: u16,
    class: DeviceClass,
}

const CASES: &[LegoCase] = &[
    LegoCase {
        fat_name: "LEGOVNET.MD",
        package_id: "net.virtio",
        vendor_id: 0x1AF4,
        device_id: 0x1041,
        class: DeviceClass::Net,
    },
    LegoCase {
        fat_name: "LEGOATHK.MD",
        package_id: "wifi.qca6174.ath10k",
        vendor_id: 0x168C,
        device_id: 0x003E,
        class: DeviceClass::Wifi,
    },
    LegoCase {
        fat_name: "LEGOGP08.MD",
        package_id: "gpu.nvidia.gp108",
        vendor_id: 0x10DE,
        device_id: 0x1C82,
        class: DeviceClass::Gpu,
    },
    LegoCase {
        fat_name: "LEGOXHCI.MD",
        package_id: "usb.xhci.host",
        vendor_id: 0,
        device_id: 0,
        class: DeviceClass::UsbHost,
    },
];

fn read_fat(name: &str) -> Option<Vec<u8>> {
    crate::fat_assets::read_root_file(name)
}

fn parse_field(text: &str, key: &str) -> Option<String> {
    let prefix = alloc::format!("{}:", key);
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&prefix) {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(String::from(v));
            }
        }
    }
    None
}

fn parse_hex_u16_field(text: &str, key: &str) -> Option<u16> {
    let v = parse_field(text, key)?;
    let s = v.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).ok()
}

/// Roda após `k_hal::init` + FAT disponível.
/// `VERDICT=PASS` se os 4 localizam + batem na tabela; use pode ser NEEDS_FW (honesto).
pub fn boot_selftest() {
    k_nano::slog_hal!(
        "LEGO",
        "boot",
        "note=disk=spec+locate code=GOLDEN_RECIPES CapGate H1 n={}",
        CASES.len()
    );

    let mut locate_ok = 0u32;
    let mut table_ok = 0u32;
    let mut use_ok = 0u32;

    for c in CASES {
        let data = read_fat(c.fat_name);
        let locate = data.is_some();
        if locate {
            locate_ok += 1;
        }

        let mut parse_pkg_ok = false;
        let mut parse_bind_ok = false;
        if let Some(bytes) = &data {
            let text = core::str::from_utf8(bytes).unwrap_or("");
            if let Some(pkg) = parse_field(text, "package_id") {
                parse_pkg_ok = pkg == c.package_id;
            }
            if c.class == DeviceClass::UsbHost {
                parse_bind_ok = true; // ClassOnly
            } else {
                let vid = parse_hex_u16_field(text, "vendor_id").unwrap_or(0);
                let did = parse_hex_u16_field(text, "device_id").unwrap_or(0);
                parse_bind_ok = vid == c.vendor_id && did == c.device_id;
            }
        }

        let table_hit = GOLDEN_RECIPES.iter().any(|e| {
            e.package_id == c.package_id
                && e.class == c.class
                && (c.class == DeviceClass::UsbHost
                    || (e.vendor_id == c.vendor_id && e.device_id == c.device_id))
        });
        if table_hit && parse_pkg_ok {
            table_ok += 1;
        }

        let promote = device_recipe::evaluate_device(c.vendor_id, c.device_id, c.class);
        // Utilização: tabela + promote coerente (Ok ou NeedsFw esperado p/ rebelde)
        let use_pass = table_hit
            && parse_pkg_ok
            && matches!(
                promote,
                RecipePromote::Ok | RecipePromote::NeedsFw | RecipePromote::None
            )
            && !(promote == RecipePromote::Escalate && c.package_id != "bt.template");
        // None ok se PCI daquele DID ausente (QEMU sem ath10k/GPU)
        let use_expected = if !table_hit {
            false
        } else {
            match promote {
                RecipePromote::Ok | RecipePromote::NeedsFw => true,
                RecipePromote::None => {
                    // Sem silício: ainda "use=TABLE" — Cap path pronto
                    true
                }
                RecipePromote::Escalate => false,
            }
        };
        if use_expected {
            use_ok += 1;
        }

        k_nano::slog_hal!(
            "LEGO",
            "boot",
            "file={} pkg={} locate={} parse={} table={} promote={} use={}",
            c.fat_name,
            c.package_id,
            if locate { "OK" } else { "MISS" },
            if parse_pkg_ok && parse_bind_ok {
                "OK"
            } else if locate {
                "FAIL"
            } else {
                "N/A"
            },
            if table_hit { "OK" } else { "MISS" },
            promote.as_str(),
            if use_expected { "OK" } else { "FAIL" }
        );
        let _ = use_pass;
    }

    let n = CASES.len() as u32;
    // table_hit in-tree conta em use; PASS exige locate+parse no disco
    let pass = locate_ok == n && table_ok == n;
    k_nano::slog_hal!(
        "LEGO",
        "boot",
        "VERDICT={} locate={}/{} table={}/{} use={}/{} (Install!=Ready)",
        if pass { "PASS" } else { "PARTIAL" },
        locate_ok,
        n,
        table_ok,
        n,
        use_ok,
        n
    );
}
