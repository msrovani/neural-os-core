//! QEMU fw_cfg session persistence MVP (Labor 46).
//! Selector 0x510/0x511 — honesty SKIP se não QEMU.

use core::sync::atomic::{AtomicBool, Ordering};

static SEEN: AtomicBool = AtomicBool::new(false);

#[inline(always)]
unsafe fn outw(port: u16, val: u16) {
    core::arch::asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let v: u8;
    core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nostack, preserves_flags));
    v
}

/// Probe signature "QEMU" via fw_cfg file directory — best-effort.
pub fn probe() -> bool {
    unsafe {
        // FW_CFG_SIGNATURE = 0x0000
        outw(0x510, 0x0000);
        let mut sig = [0u8; 4];
        for b in &mut sig {
            *b = inb(0x511);
        }
        let ok = &sig == b"QEMU";
        SEEN.store(ok, Ordering::Relaxed);
        ok
    }
}

/// Read a file from fw_cfg by selector (legacy I/O mode).
/// Returns file contents as Vec<u8>, or None if not found/error.
pub fn read_file(selector: u16) -> Option<alloc::vec::Vec<u8>> {
    if !SEEN.load(Ordering::Relaxed) {
        return None;
    }
    unsafe {
        // Select file
        outw(0x510, selector);
        // Read size (4 bytes, big-endian)
        let mut size_bytes = [0u8; 4];
        for b in &mut size_bytes {
            *b = inb(0x511);
        }
        let size = u32::from_be_bytes(size_bytes) as usize;
        if size == 0 || size > 65536 {
            return None;
        }
        // Read data
        let mut data = alloc::vec::Vec::with_capacity(size);
        for _ in 0..size {
            data.push(inb(0x511));
        }
        Some(data)
    }
}

/// Read a file from fw_cfg by name (scans directory).
/// Directory selector is 0x0019 (FW_CFG_FILE_DIR).
pub fn read_file_by_name(name: &str) -> Option<alloc::vec::Vec<u8>> {
    if !SEEN.load(Ordering::Relaxed) {
        return None;
    }
    // Read directory
    let dir_data = read_file(0x0019)?;
    // Parse directory entries (each entry: 56 bytes name + 4 bytes selector + 4 bytes size)
    const ENTRY_SIZE: usize = 64;
    if dir_data.len() % ENTRY_SIZE != 0 {
        return None;
    }
    for chunk in dir_data.chunks(ENTRY_SIZE) {
        if chunk.len() < ENTRY_SIZE {
            break;
        }
        // Extract name (null-terminated)
        let mut entry_name = alloc::string::String::new();
        for &b in &chunk[0..56] {
            if b == 0 {
                break;
            }
            entry_name.push(b as char);
        }
        if entry_name == name {
            // Extract selector (bytes 56-59, big-endian)
            let selector = u16::from_be_bytes([chunk[56], chunk[57]]);
            return read_file(selector);
        }
    }
    None
}

/// Write a file to fw_cfg (DMA mode not implemented; I/O mode only for small files).
/// Returns true on success.
pub fn write_file(selector: u16, data: &[u8]) -> bool {
    if !SEEN.load(Ordering::Relaxed) {
        return false;
    }
    if data.len() > 65536 {
        return false;
    }
    unsafe {
        outw(0x510, selector);
        // Write size (4 bytes, big-endian)
        let size_bytes = (data.len() as u32).to_be_bytes();
        for &b in &size_bytes {
            core::arch::asm!("out dx, al", in("dx") 0x511u16, in("al") b, options(nostack, preserves_flags));
        }
        // Write data
        for &b in data {
            core::arch::asm!("out dx, al", in("dx") 0x511u16, in("al") b, options(nostack, preserves_flags));
        }
    }
    true
}

pub fn boot_smoke() -> bool {
    let ok = probe();
    if ok {
        crate::slog_nano!(
            "FWCFG",
            "info",
            "step=probe status=OK VERDICT=PARTIAL reason=qemu_signature"
        );
        // Test read directory
        if let Some(dir) = read_file(0x0019) {
            crate::slog_nano!(
                "FWCFG",
                "info",
                "step=dir_read status=OK VERDICT=PARTIAL entries={}",
                dir.len() / 64
            );
        }
    } else {
        crate::slog_nano!(
            "FWCFG",
            "info",
            "step=probe status=SKIP VERDICT=SKIP reason=no_qemu_fw_cfg"
        );
    }
    true
}