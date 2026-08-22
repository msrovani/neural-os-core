//! ACPI — RSDP preferencial via set_boot_rsdp (espelho neural-kernel).

use crate::{println};
use alloc::vec::Vec;
use core::ptr::read_volatile;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use x86_64::VirtAddr;

/// HW-5: Real APIC IDs coletados do MADT (não guess sequencial).
/// Populado durante `init_acpi()`, consumido por `smp::wake_aps_sequential()`.
pub static BOOT_APIC_IDS: spin::Mutex<alloc::vec::Vec<u32>> = spin::Mutex::new(alloc::vec::Vec::new());

static BOOT_RSDP_PHYS: AtomicU64 = AtomicU64::new(0);

pub fn set_boot_rsdp(phys: Option<u64>) {
    BOOT_RSDP_PHYS.store(phys.unwrap_or(0), Ordering::Release);
}

#[repr(C, packed)]
struct RsdpDescriptor {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[derive(Debug, Clone)]
pub struct AcpiInfo {
    pub lapic_base: u64,
    pub ioapic_base: u64,
    pub lapic_count: u16,
    pub ioapic_count: u8,
    pub has_x2apic: bool,
    pub phys_mem_offset: u64,
    pub iso_overrides: Vec<(u8, u32)>,
    /// I/O port PM1a_CNT (0 = ausente).
    pub pm1a_cnt_port: u16,
    /// SLP_TYPa de \_S5_ (None = tentar fallbacks 5 depois 0).
    pub slp_typa: Option<u8>,
    /// HW-7: SMI_CMD port address from FADT (0 = ausente).
    pub smi_cmd: u32,
    /// HW-7: Value to write to SMI_CMD to enable ACPI.
    pub acpi_enable: u8,
    /// HW-7: I/O port PM1b_CNT (0 = ausente).
    pub pm1b_cnt_blk: u16,
}

/// Registradores S5 descobertos no boot (power_off_s5).
static PM1A_CNT_PORT: AtomicU64 = AtomicU64::new(0);
static SLP_TYPA_STORED: AtomicU8 = AtomicU8::new(0xFF);
/// HW-7: SMI command port and ACPI enable value.
static SMI_CMD_PORT: AtomicU64 = AtomicU64::new(0);
static ACPI_ENABLE_VAL: AtomicU8 = AtomicU8::new(0);
/// HW-7: PM1b_CNT alternate port (0 = absent).
static PM1B_CNT_PORT: AtomicU64 = AtomicU64::new(0);

pub fn set_s5_regs(pm1a_cnt_port: u16, slp_typa: Option<u8>) {
    PM1A_CNT_PORT.store(pm1a_cnt_port as u64, Ordering::Release);
    SLP_TYPA_STORED.store(slp_typa.unwrap_or(0xFF), Ordering::Release);
}

/// HW-7: Store SMI_CMD / ACPI_ENABLE / PM1b_CNT_BLK from FADT.
pub fn set_power_mgmt_regs(smi_cmd: u32, acpi_enable: u8, pm1b_cnt_blk: u16) {
    SMI_CMD_PORT.store(smi_cmd as u64, Ordering::Release);
    ACPI_ENABLE_VAL.store(acpi_enable, Ordering::Release);
    PM1B_CNT_PORT.store(pm1b_cnt_blk as u64, Ordering::Release);
}

pub fn pm1a_cnt_port() -> u16 {
    PM1A_CNT_PORT.load(Ordering::Acquire) as u16
}

/// Escreve S5 em PM1a_CNT. Retorna true se tentou (porta conhecida).
pub fn power_off_s5() -> bool {
    let port = pm1a_cnt_port();
    if port == 0 {
        crate::slog_nano!("ACPI", "info", "S5 skip — PM1a_CNT ausente");
        return false;
    }
    let stored = SLP_TYPA_STORED.load(Ordering::Acquire);
    let mut types = [0u8; 3];
    let n = if stored == 0xFF {
        types[0] = 5;
        types[1] = 0;
        2
    } else {
        types[0] = stored;
        types[1] = 5;
        types[2] = 0;
        3
    };
    power_off_s5_try(port, &types[..n])
}

fn power_off_s5_try(port: u16, types: &[u8]) -> bool {
    // HW-7 Step 1: Disable SMI via SMI_CMD if available
    let smi_cmd = SMI_CMD_PORT.load(Ordering::Acquire) as u16;
    let acpi_enable = ACPI_ENABLE_VAL.load(Ordering::Acquire);
    if smi_cmd != 0 && acpi_enable != 0 {
        unsafe {
            core::arch::asm!("out dx, al", in("dx") smi_cmd, in("al") acpi_enable, options(nostack, preserves_flags));
        }
        crate::slog_nano!("ACPI", "info", "SMI disabled via SMI_CMD={:#x} val={}", smi_cmd, acpi_enable);
    }

    // HW-7 Step 2: WBINVD — flush all caches
    unsafe {
        core::arch::asm!("wbinvd", options(nostack, preserves_flags));
    }

    // HW-7 Step 3: Write PM1a_CNT with each SLP_TYP
    for &typ in types {
        // PM1_CNT: SLP_TYP[12:10] | SLP_EN[13]
        let val: u16 = ((typ as u16) << 10) | (1u16 << 13);
        crate::slog_nano!(
            "ACPI",
            "info",
            "S5 write PM1a={:#x} typ={} val={:#x}",
            port,
            typ,
            val
        );
        unsafe {
            core::arch::asm!(
                "out dx, ax",
                in("dx") port,
                in("ax") val,
                options(nostack, preserves_flags)
            );
        }
        // delay curto entre tentativas
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }

    // HW-7 Step 4: If PM1b_CNT_BLK is valid and different from PM1a, write there too
    let pm1b = PM1B_CNT_PORT.load(Ordering::Acquire) as u16;
    if pm1b != 0 && pm1b != port {
        for &typ in types {
            let val: u16 = ((typ as u16) << 10) | (1u16 << 13);
            crate::slog_nano!("ACPI", "info", "S5 write PM1b={:#x} typ={} val={:#x}", pm1b, typ, val);
            unsafe {
                core::arch::asm!(
                    "out dx, ax",
                    in("dx") pm1b,
                    in("ax") val,
                    options(nostack, preserves_flags)
                );
            }
            for _ in 0..100_000 {
                core::hint::spin_loop();
            }
        }
    }

    true
}

/// Extrai PM1a_CNT + SLP_TYPa de uma tabela FACP (+ DSDT \_S5_ e \_S3_ se possível).
pub unsafe fn parse_fadt_power(table_ptr: *const u8, phys_off: u64) -> (u16, Option<u8>) {
    let len = read_volatile(table_ptr.add(4) as *const u32) as usize;
    if len < 90 {
        return (0, None);
    }

    let mut pm1a = read_volatile(table_ptr.add(64) as *const u32) as u16;

    // X_PM1a_CNT_BLK (GAS) em FADT rev≥3 / length ≥ 244
    if len >= 244 {
        let space = read_volatile(table_ptr.add(172));
        let addr = read_volatile(table_ptr.add(176) as *const u64);
        if space == 1 && addr != 0 {
            pm1a = addr as u16;
        }
    }

    let mut slp: Option<u8> = None;

    // ─── FACS (Firmware ACPI Control Structure) ──────────────
    // FIRMWARE_CTRL @36 (32-bit) or X_FIRMWARE_CTRL @132 (64-bit, rev≥3 / len≥148).
    let facs_phys = if len >= 148 {
        let x_facs = read_volatile(table_ptr.add(132) as *const u64);
        if x_facs != 0 { x_facs } else { read_volatile(table_ptr.add(36) as *const u32) as u64 }
    } else {
        read_volatile(table_ptr.add(36) as *const u32) as u64
    };
    if facs_phys != 0 {
        parse_facs(facs_phys, phys_off);
    }

    // DSDT phys: legacy @40; X_DSDT @140 se len>=148
    let mut dsdt_phys = read_volatile(table_ptr.add(40) as *const u32) as u64;
    if len >= 148 {
        let x_dsdt = read_volatile(table_ptr.add(140) as *const u64);
        if x_dsdt != 0 {
            dsdt_phys = x_dsdt;
        }
    }
    if dsdt_phys != 0 {
        slp = parse_s5_from_dsdt(dsdt_phys, phys_off);
        // Also parse _S3 for suspend
        let s3_typ = parse_s3_from_dsdt(dsdt_phys, phys_off);
        if let Some(typ3) = s3_typ {
            set_s3_slp_typa(typ3);
        }
    }

    crate::slog_nano!(
        "ACPI",
        "info",
        "FADT PM1a_CNT={:#x} SLP_TYPa={:?} SLP_TYP3={:?}",
        pm1a,
        slp,
        SLP_TYP3.load(Ordering::Relaxed)
    );
    (pm1a, slp)
}

/// Parse `\_S3` (suspend-to-RAM) from DSDT AML bytecode.
/// Same pattern as `\_S5`, looking for the BytePrefix 0x0A + typ.
unsafe fn parse_s3_from_dsdt(dsdt_phys: u64, phys_off: u64) -> Option<u8> {
    let virt = VirtAddr::new(phys_off.wrapping_add(dsdt_phys)).as_u64() as *const u8;
    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(virt.add(i));
    }
    if &sig != b"DSDT" && &sig != b"SSDT" {
        return None;
    }
    let len = read_volatile(virt.add(4) as *const u32) as usize;
    if len < 36 || len > 2 * 1024 * 1024 {
        return None;
    }
    let needle = b"_S3_";
    let end = len.saturating_sub(needle.len() + 4);
    for i in 36..end {
        let mut ok = true;
        for (j, &b) in needle.iter().enumerate() {
            if read_volatile(virt.add(i + j)) != b {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        for k in (i + 4)..(i + 64).min(len.saturating_sub(1)) {
            if read_volatile(virt.add(k)) == 0x0A {
                let typ = read_volatile(virt.add(k + 1));
                crate::slog_nano!("ACPI", "info", "DSDT _S3_ SLP_TYP3={}", typ);
                return Some(typ);
            }
        }
    }
    None
}

// ─── S3 (Suspend-to-RAM) ────────────────────────────────────────────────────

static SLP_TYP3: AtomicU8 = AtomicU8::new(0xFF);
/// Physical address of the FACS table (0 = unknown).
static FACS_PHYS: AtomicU64 = AtomicU64::new(0);
/// Waking vector from FACS (for resume detection).
static FACS_WAKE_VECTOR: AtomicU64 = AtomicU64::new(0);

/// Internal: return the physical address of FACS (for writing wake vector).
pub fn internal_facs_phys() -> u64 {
    FACS_PHYS.load(Ordering::Acquire)
}

pub fn set_s3_slp_typa(typ3: u8) {
    SLP_TYP3.store(typ3, Ordering::Release);
}

pub fn s3_slp_typa() -> Option<u8> {
    let v = SLP_TYP3.load(Ordering::Acquire);
    if v == 0xFF { None } else { Some(v) }
}

pub fn facs_wake_vector() -> u64 {
    FACS_WAKE_VECTOR.load(Ordering::Acquire)
}

/// Parse FACS (Firmware ACPI Control Structure) for the S3 waking vector.
///
/// FACS layout (offset 0):
///   +0  signature[4]  "FACS"
///   +4  length (u32)
///   +8  hardware_signature (u32)
///   +12 firmware_waking_vector (u32) — 32-bit real-mode address
///   +16 global_lock (u32)
///   +20 flags (u32)
///   +24 x_firmware_waking_vector (u64) — 64-bit address (FACS v2, length ≥ 32)
///
/// # Safety
/// `facs_phys` must be a valid physical address to a FACS.
unsafe fn parse_facs(facs_phys: u64, phys_off: u64) {
    FACS_PHYS.store(facs_phys, Ordering::Release);
    let virt = VirtAddr::new(phys_off.wrapping_add(facs_phys));
    let ptr = virt.as_u64() as *const u8;

    let len = read_volatile(ptr.add(4) as *const u32) as usize;
    crate::slog_nano!("ACPI", "info", "FACS em 0x{:x} ({} bytes)", facs_phys, len);

    // 32-bit waking vector
    let wake32 = read_volatile(ptr.add(12) as *const u32) as u64;
    if wake32 != 0 {
        crate::slog_nano!("ACPI", "info", "FACS waking_vector=0x{:x}", wake32);
    }

    // 64-bit waking vector (FACS v2, length ≥ 32)
    let wake64 = if len >= 32 {
        read_volatile(ptr.add(24) as *const u64)
    } else {
        0
    };
    let wake = if wake64 != 0 { wake64 } else { wake32 };
    if wake != 0 {
        FACS_WAKE_VECTOR.store(wake, Ordering::Release);
        crate::slog_nano!("ACPI", "info", "FACS usando wake_vector=0x{:x}", wake);
    } else {
        crate::slog_nano!("ACPI", "warn", "FACS sem waking vector — S3 resume pode nao funcionar");
    }
}

unsafe fn parse_s5_values(dsdt: *const u8, pos: usize, max_len: usize) -> Option<(u8, u8)> {
    // Procura SLP_TYPa e SLP_TYPb após "_S5_" no bytecode AML.
    // Padrões comuns de BIOS:
    //   _S5_ → 0x12 0x04 ... 0x0A XX 0x0A XX    (Package com BytePrefix)
    //   _S5_ → 0x08 ... 0x0A XX 0x0A XX          (Name com BytePrefix)
    //   _S5_ → 0x0A XX 0x0A XX                    (BytePrefix direto)
    let search_start = pos + 5; // após "_S5_"
    let search_end = (search_start + 128).min(max_len); // janela maior

    let mut values = [0u8; 2];
    let mut found = 0;
    let mut i = search_start;

    while i < search_end && found < 2 {
        let b = read_volatile(dsdt.add(i));
        if b == 0x0A && i + 2 <= search_end {
            // BytePrefix (0x0A) followed by byte value
            values[found] = read_volatile(dsdt.add(i + 1));
            found += 1;
            i += 2;
        } else if b == 0x0B && i + 3 <= search_end {
            // WordPrefix (0x0B) followed by word value
            values[found] = read_volatile(dsdt.add(i + 1)); // low byte
            found += 1;
            i += 3;
        } else {
            i += 1;
        }
    }

    if found == 2 { Some((values[0], values[1])) } else { None }
}

unsafe fn parse_s5_from_dsdt(dsdt_phys: u64, phys_off: u64) -> Option<u8> {
    let virt = VirtAddr::new(phys_off.wrapping_add(dsdt_phys)).as_u64() as *const u8;
    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(virt.add(i));
    }
    if &sig != b"DSDT" && &sig != b"SSDT" {
        return None;
    }
    let len = read_volatile(virt.add(4) as *const u32) as usize;
    if len < 36 || len > 2 * 1024 * 1024 {
        return None;
    }
    // Procurar "_S5_" e extrair SLP_TYPa / SLP_TYPb
    let needle = b"_S5_";
    let end = len.saturating_sub(needle.len() + 4);
    for i in 36..end {
        let mut ok = true;
        for (j, &b) in needle.iter().enumerate() {
            if read_volatile(virt.add(i + j)) != b {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        // Usa parser robusto de bytecodes AML
        if let Some((typa, _typb)) = parse_s5_values(virt, i, len) {
            crate::slog_nano!("ACPI", "info", "DSDT _S5_ SLP_TYPa={}", typa);
            return Some(typa);
        }
    }
    None
}

fn checksum_valid(data: &[u8]) -> bool {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
}

/// ponytail: Check ACPI for a "TPM2" table before touching 0xFED4_0000.
/// On AMD AM5 the TIS fixed address may not respond → bus stall.
/// Returns true only if firmware actually reports a TPM2 table.
pub unsafe fn has_tpm2_table(physical_memory_offset: u64) -> bool {
    let rsdp_phys = match find_rsdp(physical_memory_offset) {
        Some(p) => p,
        None => return false,
    };
    let rsdp_virt = VirtAddr::new(physical_memory_offset + rsdp_phys);
    let rsdp = &*(rsdp_virt.as_u64() as *const RsdpDescriptor);

    let rsdt_phys = if rsdp.revision >= 2 && rsdp.xsdt_address != 0 {
        rsdp.xsdt_address
    } else {
        rsdp.rsdt_address as u64
    };
    let rsdt_virt = VirtAddr::new(physical_memory_offset + rsdt_phys);

    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile((rsdt_virt.as_u64() as *const u8).add(i));
    }
    let is_xsdt = &sig == b"XSDT";
    if &sig != b"RSDT" && !is_xsdt {
        return false;
    }

    let rsdt_len = read_volatile((rsdt_virt.as_u64() as *const u32).add(1)) as usize;
    let entry_size: usize = if is_xsdt { 8 } else { 4 };
    let entry_count = (rsdt_len - 36) / entry_size;

    for i in 0..entry_count {
        let entry_ptr = rsdt_virt.as_u64() as *const u8;
        let off = 36 + i * entry_size;
        let table_phys = if is_xsdt {
            read_volatile(entry_ptr.add(off) as *const u64)
        } else {
            read_volatile(entry_ptr.add(off) as *const u32) as u64
        };
        if table_phys == 0 {
            continue;
        }
        let table_virt = physical_memory_offset + table_phys;
        let mut tbl_sig = [0u8; 4];
        for j in 0..4 {
            tbl_sig[j] = read_volatile((table_virt as *const u8).add(j));
        }
        if &tbl_sig == b"TPM2" {
            return true;
        }
    }
    false
}

unsafe fn find_rsdp(physical_memory_offset: u64) -> Option<u64> {
    let boot_rsdp = BOOT_RSDP_PHYS.load(Ordering::Acquire);
    if boot_rsdp != 0 {
        let addr = VirtAddr::new(physical_memory_offset + boot_rsdp).as_u64();
        let ptr = addr as *const u8;
        if read_volatile(ptr.add(0)) == b'R'
            && read_volatile(ptr.add(1)) == b'S'
            && read_volatile(ptr.add(2)) == b'D'
            && read_volatile(ptr.add(3)) == b' '
            && read_volatile(ptr.add(4)) == b'P'
            && read_volatile(ptr.add(5)) == b'T'
            && read_volatile(ptr.add(6)) == b'R'
            && read_volatile(ptr.add(7)) == b' '
        {
            let rsdp = &*(addr as *const RsdpDescriptor);
            let len = if rsdp.revision >= 2 { 36usize } else { 20usize };
            let raw = core::slice::from_raw_parts(addr as *const u8, len);
            if checksum_valid(raw) {
                return Some(boot_rsdp);
            }
        }
    }

    let ebda_start = VirtAddr::new(physical_memory_offset + 0x0008_0000);
    let ebda_end = VirtAddr::new(physical_memory_offset + 0x000A_0000);
    let bios_start = VirtAddr::new(physical_memory_offset + 0x000E_0000);
    let bios_end = VirtAddr::new(physical_memory_offset + 0x0010_0000);

    let mut addr = ebda_start.as_u64();
    while addr < ebda_end.as_u64() {
        let ptr = addr as *const u8;
        if read_volatile(ptr.add(0)) == b'R'
            && read_volatile(ptr.add(1)) == b'S'
            && read_volatile(ptr.add(2)) == b'D'
            && read_volatile(ptr.add(3)) == b' '
            && read_volatile(ptr.add(4)) == b'P'
            && read_volatile(ptr.add(5)) == b'T'
            && read_volatile(ptr.add(6)) == b'R'
            && read_volatile(ptr.add(7)) == b' '
        {
            let rsdp = &*(addr as *const RsdpDescriptor);
            let len = if rsdp.revision >= 2 { 36usize } else { 20usize };
            let raw = core::slice::from_raw_parts(addr as *const u8, len);
            if checksum_valid(raw) {
                return Some(addr - physical_memory_offset);
            }
        }
        addr += 16;
    }

    addr = bios_start.as_u64();
    while addr < bios_end.as_u64() {
        let ptr = addr as *const u8;
        if read_volatile(ptr.add(0)) == b'R'
            && read_volatile(ptr.add(1)) == b'S'
            && read_volatile(ptr.add(2)) == b'D'
            && read_volatile(ptr.add(3)) == b' '
            && read_volatile(ptr.add(4)) == b'P'
            && read_volatile(ptr.add(5)) == b'T'
            && read_volatile(ptr.add(6)) == b'R'
            && read_volatile(ptr.add(7)) == b' '
        {
            let rsdp = &*(addr as *const RsdpDescriptor);
            let len = if rsdp.revision >= 2 { 36usize } else { 20usize };
            let raw = core::slice::from_raw_parts(addr as *const u8, len);
            if checksum_valid(raw) {
                return Some(addr - physical_memory_offset);
            }
        }
        addr += 16;
    }
    None
}

pub unsafe fn init_acpi(physical_memory_offset: u64) -> Option<AcpiInfo> {
    let rsdp_phys = find_rsdp(physical_memory_offset)?;
    let rsdp_virt = VirtAddr::new(physical_memory_offset + rsdp_phys);
    let rsdp = &*(rsdp_virt.as_u64() as *const RsdpDescriptor);

    let revision = rsdp.revision;
    let rsdt_phys = if revision >= 2 && rsdp.xsdt_address != 0 {
        rsdp.xsdt_address
    } else {
        rsdp.rsdt_address as u64
    };

    crate::slog_nano!("ACPI", "info", "RSDP encontrado em 0x{:x}. Revisao: {}. RSDT/XSDT em 0x{:x}", rsdp_phys, revision, rsdt_phys);
    println!(
        "[ACPI] RSDP encontrado. Revisao: {}. RSDT em 0x{:x}",
        revision, rsdt_phys
    );

    let rsdt_virt = VirtAddr::new(physical_memory_offset + rsdt_phys);
    let rsdt_signature_ptr = rsdt_virt.as_u64() as *const u8;

    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(rsdt_signature_ptr.add(i));
    }

    let is_xsdt = &sig == b"XSDT";
    if &sig != b"RSDT" && !is_xsdt {
        crate::slog_nano!("ACPI", "info", "Assinatura invalida: {:?}", core::str::from_utf8(&sig));
        println!("[ACPI] Assinatura invalida: {:?}", core::str::from_utf8(&sig));
        return None;
    }

    let rsdt_len_raw = rsdt_virt.as_u64() as *const u32;
    let rsdt_len = read_volatile(rsdt_len_raw.add(1)) as usize;
    let entry_size: usize = if is_xsdt { 8 } else { 4 };
    let entry_count = (rsdt_len - 36) / entry_size;

    crate::slog_nano!("ACPI", "info", "Tabela RSDT/XSDT: {} bytes, {} entradas ({} bytes cada).", rsdt_len, entry_count, entry_size);
    println!("[ACPI] Tabela: {} bytes, {} entradas.", rsdt_len, entry_count);

    let mut lapic_base = 0xFEE0_0000u64;
    let mut ioapic_base = 0xFEC0_0000u64;
    let mut lapic_count = 0u16;
    let mut ioapic_count = 0u8;
    let mut has_x2apic = false;
    let mut iso_overrides = Vec::new();
    let mut pm1a_cnt_port = 0u16;
    let mut slp_typa: Option<u8> = None;
    let mut smi_cmd_val = 0u32;
    let mut acpi_enable_val = 0u8;
    let mut pm1b_cnt_val = 0u16;

    for i in 0..entry_count {
        let entry_ptr = rsdt_virt.as_u64() as *const u8;
        let entry_offset = 36 + i * entry_size;
        let table_phys = if is_xsdt {
            read_volatile(entry_ptr.add(entry_offset) as *const u64)
        } else {
            read_volatile(entry_ptr.add(entry_offset) as *const u32) as u64
        };
        let table_virt = VirtAddr::new(physical_memory_offset + table_phys);
        let table_ptr = table_virt.as_u64() as *const u8;

        let mut table_sig = [0u8; 4];
        for j in 0..4 {
            table_sig[j] = read_volatile(table_ptr.add(j));
        }

        match &table_sig {
            b"FACP" => {
                crate::slog_nano!("ACPI", "info", "FADT encontrado em 0x{:x}", table_phys);
                let (pm1a, slp) = parse_fadt_power(table_ptr, physical_memory_offset);
                pm1a_cnt_port = pm1a;
                slp_typa = slp;
                set_s5_regs(pm1a, slp);
                // HW-7: Extrair SMI_CMD, ACPI_ENABLE, PM1b_CNT_BLK do FADT
                let fadt_len = read_volatile(table_ptr.add(4) as *const u32) as usize;
                smi_cmd_val = read_volatile(table_ptr.add(52) as *const u32);
                acpi_enable_val = read_volatile(table_ptr.add(56));
                let mut pm1b = read_volatile(table_ptr.add(72) as *const u32) as u16;
                // X_PM1b_CNT_BLK GAS (FADT rev >= 5, length >= 196)
                if fadt_len >= 196 {
                    let space = read_volatile(table_ptr.add(184));
                    let addr = read_volatile(table_ptr.add(188) as *const u64);
                    if space == 1 && addr != 0 {
                        pm1b = addr as u16;
                    }
                }
                pm1b_cnt_val = pm1b;
                set_power_mgmt_regs(smi_cmd_val, acpi_enable_val, pm1b_cnt_val);
            }
            b"APIC" => {
                crate::slog_nano!("ACPI", "info", "MADT encontrado em 0x{:x}", table_phys);
                let madt_len_raw = table_ptr.add(4) as *const u32;
                let madt_len = read_volatile(madt_len_raw) as usize;
                let madt_lapic_addr_raw = table_ptr.add(0x24) as *const u32;
                let madt_lapic_addr = read_volatile(madt_lapic_addr_raw) as u64;
                if madt_lapic_addr != 0 {
                    lapic_base = madt_lapic_addr;
                }

                let mut offset = 0x2Cu32 as usize;
                while offset < madt_len {
                    let entry_type_ptr = table_ptr.add(offset) as *const u8;
                    let entry_len_ptr = table_ptr.add(offset + 1) as *const u8;
                    let entry_type = read_volatile(entry_type_ptr);
                    let entry_len = read_volatile(entry_len_ptr) as usize;

                    match entry_type {
                        0 => {
                            // Type 0: Processor Local APIC. Flags@+4 bit0=Enabled (ACPI).
                            let flags = read_volatile(table_ptr.add(offset + 4) as *const u32);
                            if (flags & 1) == 0 {
                                crate::slog_nano!(
                                    "ACPI",
                                    "info",
                                    "MADT LAPIC id={} skipped (Enabled=0)",
                                    read_volatile(table_ptr.add(offset + 3))
                                );
                            } else {
                                let apic_id = read_volatile(table_ptr.add(offset + 3)) as u32;
                                BOOT_APIC_IDS.lock().push(apic_id);
                                lapic_count += 1;
                            }
                        }
                        1 => {
                            let ioapic_id = read_volatile(table_ptr.add(offset + 2) as *const u8);
                            let ioapic_addr_raw2 = table_ptr.add(offset + 4) as *const u32;
                            let ioapic_addr = read_volatile(ioapic_addr_raw2) as u64;
                            ioapic_base = ioapic_addr;
                            ioapic_count += 1;
                            crate::slog_nano!("ACPI", "info", "IOAPIC ID {} em 0x{:x}", ioapic_id, ioapic_addr);
                        }
                        2 => {
                            let source = read_volatile(table_ptr.add(offset + 3) as *const u8);
                            let gsi = read_volatile(table_ptr.add(offset + 4) as *const u32);
                            let flags = read_volatile(table_ptr.add(offset + 8) as *const u16);
                            crate::slog_nano!("ACPI", "info", "ISO: source={} gsi={} flags=0x{:04x}", source, gsi, flags);
                            iso_overrides.push((source, gsi));
                        }
                        5 => {
                            let lapic_addr_raw = table_ptr.add(offset + 4) as *const u32;
                            let new_lapic = read_volatile(lapic_addr_raw) as u64;
                            if new_lapic != 0 {
                                lapic_base = new_lapic;
                                crate::slog_nano!("ACPI", "info", "LAPIC Address Override: 0x{:x}", lapic_base);
                            }
                        }
                        9 => {
                            // Type 9: Processor Local x2APIC. Flags@+8 bit0=Enabled.
                            let flags = read_volatile(table_ptr.add(offset + 8) as *const u32);
                            if (flags & 1) == 0 {
                                crate::slog_nano!(
                                    "ACPI",
                                    "info",
                                    "MADT x2APIC id={} skipped (Enabled=0)",
                                    read_volatile(table_ptr.add(offset + 4) as *const u32)
                                );
                            } else {
                                let x2apic_id =
                                    read_volatile(table_ptr.add(offset + 4) as *const u32);
                                BOOT_APIC_IDS.lock().push(x2apic_id);
                                lapic_count += 1;
                                has_x2apic = true;
                            }
                        }
                        _ => {}
                    }
                    offset += entry_len;
                }

                crate::slog_nano!("ACPI", "info", "MADT: LAPIC base 0x{:x}, IOAPIC base 0x{:x}, LAPICs: {}, IOAPICs: {}", lapic_base, ioapic_base, lapic_count, ioapic_count);
                println!(
                    "[ACPI] MADT: LAPICs: {}, IOAPICs: {}",
                    lapic_count, ioapic_count
                );
            }
            _ => {}
        }
    }

    Some(AcpiInfo {
        lapic_base,
        ioapic_base,
        lapic_count,
        ioapic_count,
        has_x2apic,
        phys_mem_offset: physical_memory_offset,
        iso_overrides,
        pm1a_cnt_port,
        slp_typa,
        smi_cmd: smi_cmd_val,
        acpi_enable: acpi_enable_val,
        pm1b_cnt_blk: pm1b_cnt_val,
    })
}

// ─── ADR-0061: ACPI SRAT (NUMA topology) ────────────────────────────────

/// Faixa de memória física pertencente a um Proximity Domain NUMA.
#[derive(Debug, Clone, Copy)]
pub struct NumaMemoryRange {
    pub base: u64,
    pub length: u64,
    pub proximity_domain: u32,
}

/// Mapeamento APIC_ID → Proximity Domain NUMA.
#[derive(Debug, Clone, Copy)]
pub struct NumaApicAffinity {
    pub apic_id: u32,
    pub proximity_domain: u32,
}

/// Mapa de topologia NUMA extraído da tabela ACPI SRAT.
#[derive(Debug, Clone, Default)]
pub struct NumaTopologyMap {
    pub memory_ranges: Vec<NumaMemoryRange>,
    pub apic_affinities: Vec<NumaApicAffinity>,
    pub proximity_domain_count: u32,
}

impl NumaTopologyMap {
    pub fn is_multi_domain(&self) -> bool {
        self.proximity_domain_count > 1
    }

    /// Encontra o Proximity Domain para um endereço físico.
    pub fn domain_for_phys(&self, phys: u64) -> Option<u32> {
        for r in &self.memory_ranges {
            if phys >= r.base && phys < r.base + r.length {
                return Some(r.proximity_domain);
            }
        }
        None
    }

    /// Encontra o Proximity Domain para um APIC ID.
    pub fn domain_for_apic(&self, apic_id: u32) -> Option<u32> {
        for a in &self.apic_affinities {
            if a.apic_id == apic_id {
                return Some(a.proximity_domain);
            }
        }
        None
    }
}

/// Parseia a tabela ACPI SRAT (System Resource Affinity Table).
///
/// SRAT contém:
/// - Memory Affinity Structure (type 1): PhysRange → ProximityDomain
/// - x2APIC Local Domain Structure (type 3): APIC_ID → ProximityDomain
///
/// # Safety
/// `table_ptr` deve apontar para uma tabela SRAT válida.
pub unsafe fn parse_srat(table_ptr: *const u8) -> NumaTopologyMap {
    let mut map = NumaTopologyMap::default();

    // SRAT header: signature(4) + length(4) + revision(1) + checksum(1) + oem_id(6) + oem_table_id(8) + ...
    let len = read_volatile(table_ptr.add(4) as *const u32) as usize;
    if len < 48 {
        crate::slog_nano!("SRAT", "info", "SRAT muito pequeno: {} bytes", len);
        return map;
    }

    let mut max_domain = 0u32;
    let mut offset = 48usize; // após header ACPI padrão (36 bytes + 12 reserved)

    while offset + 2 <= len {
        let entry_type = read_volatile(table_ptr.add(offset));
        let entry_len = read_volatile(table_ptr.add(offset + 1)) as usize;

        if entry_len < 2 || offset + entry_len > len {
            break;
        }

        match entry_type {
            // Memory Affinity (type 1)
            1 => {
                if entry_len >= 24 {
                    let domain = read_volatile(table_ptr.add(offset + 2) as *const u32);
                    let base_lo = read_volatile(table_ptr.add(offset + 8) as *const u32);
                    let base_hi = read_volatile(table_ptr.add(offset + 12) as *const u32);
                    let len_lo = read_volatile(table_ptr.add(offset + 16) as *const u32);
                    let len_hi = read_volatile(table_ptr.add(offset + 20) as *const u32);
                    let base = (base_hi as u64) << 32 | (base_lo as u64);
                    let length = (len_hi as u64) << 32 | (len_lo as u64);
                    if length > 0 {
                        map.memory_ranges.push(NumaMemoryRange {
                            base,
                            length,
                            proximity_domain: domain,
                        });
                        if domain > max_domain {
                            max_domain = domain;
                        }
                    }
                }
            }
            // x2APIC Affinity (type 3)
            3 => {
                if entry_len >= 24 {
                    let domain = read_volatile(table_ptr.add(offset + 4) as *const u32);
                    let apic_id = read_volatile(table_ptr.add(offset + 8) as *const u32);
                    map.apic_affinities.push(NumaApicAffinity {
                        apic_id,
                        proximity_domain: domain,
                    });
                    if domain > max_domain {
                        max_domain = domain;
                    }
                }
            }
            // Processor Local APIC Affinity (type 0) — legacy
            0 => {
                if entry_len >= 16 {
                    let apic_id = read_volatile(table_ptr.add(offset + 3));
                    let domain = read_volatile(table_ptr.add(offset + 11) as *const u32);
                    map.apic_affinities.push(NumaApicAffinity {
                        apic_id: apic_id as u32,
                        proximity_domain: domain,
                    });
                    if domain > max_domain {
                        max_domain = domain;
                    }
                }
            }
            _ => {}
        }

        offset += entry_len;
    }

    map.proximity_domain_count = max_domain + 1;

    crate::slog_nano!(
        "SRAT",
        "info",
        "NUMA: {} domínios, {} faixas de memória, {} APICs",
        map.proximity_domain_count,
        map.memory_ranges.len(),
        map.apic_affinities.len()
    );

    map
}

/// Base física do HPET (ACPI "HPET" → GAS address, System Memory).
/// `None` se RSDP/tabela ausente, GAS não-MMIO, ou address=0.
/// Usado por `tsc::calibrate_tsc` (SESSION_277 — seam faltava no wire).
pub fn hpet_base_phys() -> Option<u64> {
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
    if pmoff == 0 {
        return None;
    }
    unsafe { find_hpet_base_phys(pmoff) }
}

unsafe fn find_hpet_base_phys(physical_memory_offset: u64) -> Option<u64> {
    let rsdp_phys = find_rsdp(physical_memory_offset)?;
    let rsdp_virt = VirtAddr::new(physical_memory_offset + rsdp_phys);
    let rsdp = &*(rsdp_virt.as_u64() as *const RsdpDescriptor);

    let rsdt_phys = if rsdp.revision >= 2 && rsdp.xsdt_address != 0 {
        rsdp.xsdt_address
    } else {
        rsdp.rsdt_address as u64
    };
    let rsdt_virt = VirtAddr::new(physical_memory_offset + rsdt_phys);
    let rsdt_ptr = rsdt_virt.as_u64() as *const u8;

    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(rsdt_ptr.add(i));
    }
    let is_xsdt = &sig == b"XSDT";
    if &sig != b"RSDT" && !is_xsdt {
        return None;
    }

    let rsdt_len = read_volatile(rsdt_ptr.add(4) as *const u32) as usize;
    if rsdt_len < 36 {
        return None;
    }
    let entry_size: usize = if is_xsdt { 8 } else { 4 };
    let entry_count = (rsdt_len - 36) / entry_size;

    for i in 0..entry_count {
        let entry_offset = 36 + i * entry_size;
        let table_phys = if is_xsdt {
            read_volatile(rsdt_ptr.add(entry_offset) as *const u64)
        } else {
            read_volatile(rsdt_ptr.add(entry_offset) as *const u32) as u64
        };
        if table_phys == 0 {
            continue;
        }
        let table_virt = physical_memory_offset + table_phys;
        let mut tbl_sig = [0u8; 4];
        for j in 0..4 {
            tbl_sig[j] = read_volatile((table_virt as *const u8).add(j));
        }
        if &tbl_sig != b"HPET" {
            continue;
        }
        // HPET: header 36 + block ID 4 + GAS@40 (space_id@40, address@44).
        let len = read_volatile((table_virt as *const u8).add(4) as *const u32) as usize;
        if len < 52 {
            return None;
        }
        let space_id = read_volatile((table_virt as *const u8).add(40));
        if space_id != 0 {
            // só System Memory (MMIO); I/O space → None (PIT fallback)
            return None;
        }
        let addr = read_volatile((table_virt as *const u8).add(44) as *const u64);
        if addr == 0 {
            return None;
        }
        return Some(addr);
    }
    None
}

/// Parseia SRAT a partir do RSDP já conhecido.
/// Retorna None se SRAT não for encontrada.
pub unsafe fn parse_srat_from_rsdp(physical_memory_offset: u64) -> Option<NumaTopologyMap> {
    let rsdp_phys = find_rsdp(physical_memory_offset)?;
    let rsdp_virt = VirtAddr::new(physical_memory_offset + rsdp_phys);
    let rsdp = &*(rsdp_virt.as_u64() as *const RsdpDescriptor);

    let rsdt_phys = if rsdp.revision >= 2 && rsdp.xsdt_address != 0 {
        rsdp.xsdt_address
    } else {
        rsdp.rsdt_address as u64
    };

    let rsdt_virt = VirtAddr::new(physical_memory_offset + rsdt_phys);
    let rsdt_ptr = rsdt_virt.as_u64() as *const u8;

    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(rsdt_ptr.add(i));
    }

    let is_xsdt = &sig == b"XSDT";
    if &sig != b"RSDT" && !is_xsdt {
        return None;
    }

    let rsdt_len = read_volatile(rsdt_ptr.add(4) as *const u32) as usize;
    let entry_size: usize = if is_xsdt { 8 } else { 4 };
    let entry_count = (rsdt_len - 36) / entry_size;

    for i in 0..entry_count {
        let entry_offset = 36 + i * entry_size;
        let table_phys = if is_xsdt {
            read_volatile(rsdt_ptr.add(entry_offset) as *const u64)
        } else {
            read_volatile(rsdt_ptr.add(entry_offset) as *const u32) as u64
        };
        let table_virt = VirtAddr::new(physical_memory_offset + table_phys);
        let table_ptr = table_virt.as_u64() as *const u8;

        let mut table_sig = [0u8; 4];
        for j in 0..4 {
            table_sig[j] = read_volatile(table_ptr.add(j));
        }

        if &table_sig == b"SRAT" {
            return Some(parse_srat(table_ptr));
        }
    }
    None
}
