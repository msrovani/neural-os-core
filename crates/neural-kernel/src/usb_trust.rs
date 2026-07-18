//! USB device TrustTable — IDEA #6 / #12–15 (ADR-0014 + Onda 4).
//! Persistência: `/mnt/neural/system/trust/usb.tbl` (NeuralFS) quando montado.
//! Policy: deny-by-default quando `USB_TRUST_ENFORCE=1`; boot stick = allow-once.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

const MAGIC: &[u8; 4] = b"USBT";
const VERSION: u32 = 1;
const MAX_ENTRIES: usize = 32;
const ENTRY_BYTES: usize = 24;

pub const FLAG_TRUSTED: u16 = 1 << 0;
pub const FLAG_BOOT: u16 = 1 << 1;
pub const FLAG_ONCE: u16 = 1 << 2;

#[derive(Clone, Copy)]
pub struct UsbTrustEntry {
    pub vid: u16,
    pub pid: u16,
    pub flags: u16,
    pub tag: [u8; 16],
}

impl UsbTrustEntry {
    pub const fn empty() -> Self {
        Self {
            vid: 0,
            pid: 0,
            flags: 0,
            tag: [0; 16],
        }
    }

    fn is_active(&self) -> bool {
        self.flags & FLAG_TRUSTED != 0 || self.flags & FLAG_BOOT != 0
    }
}

pub struct UsbTrustTable {
    entries: [UsbTrustEntry; MAX_ENTRIES],
    count: usize,
    /// Quando true: desconhecido → Deny (e tenta disable_port).
    enforce: bool,
}

impl UsbTrustTable {
    pub const fn new() -> Self {
        Self {
            entries: [UsbTrustEntry::empty(); MAX_ENTRIES],
            count: 0,
            enforce: false,
        }
    }

    pub fn set_enforce(&mut self, on: bool) {
        self.enforce = on;
    }

    pub fn is_enforce(&self) -> bool {
        self.enforce
    }

    pub fn is_trusted(&self, vid: u16, pid: u16) -> bool {
        for e in self.entries.iter().take(self.count) {
            if e.is_active() && e.vid == vid && e.pid == pid {
                return true;
            }
        }
        false
    }

    pub fn allow(&mut self, vid: u16, pid: u16, flags: u16, tag: &str) -> bool {
        if self.is_trusted(vid, pid) {
            return true;
        }
        if self.count >= MAX_ENTRIES {
            return false;
        }
        let mut e = UsbTrustEntry {
            vid,
            pid,
            flags: flags | FLAG_TRUSTED,
            tag: [0; 16],
        };
        let bytes = tag.as_bytes();
        let n = bytes.len().min(16);
        e.tag[..n].copy_from_slice(&bytes[..n]);
        self.entries[self.count] = e;
        self.count += 1;
        true
    }

    pub fn revoke(&mut self, vid: u16, pid: u16) -> bool {
        let mut i = 0;
        while i < self.count {
            if self.entries[i].vid == vid && self.entries[i].pid == pid {
                self.entries[i] = UsbTrustEntry::empty();
                if i + 1 < self.count {
                    self.entries[i] = self.entries[self.count - 1];
                }
                self.count -= 1;
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.count * ENTRY_BYTES);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.count as u32).to_le_bytes());
        for e in self.entries.iter().take(self.count) {
            out.extend_from_slice(&e.vid.to_le_bytes());
            out.extend_from_slice(&e.pid.to_le_bytes());
            out.extend_from_slice(&e.flags.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&e.tag);
        }
        out
    }

    pub fn load_bytes(&mut self, data: &[u8]) -> Result<usize, &'static str> {
        if data.len() < 12 || &data[0..4] != MAGIC {
            return Err("bad magic");
        }
        let ver = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        if ver != VERSION {
            return Err("bad version");
        }
        let n = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if n > MAX_ENTRIES {
            return Err("too many entries");
        }
        let need = 12 + n * ENTRY_BYTES;
        if data.len() < need {
            return Err("truncated");
        }
        self.count = 0;
        for i in 0..n {
            let o = 12 + i * ENTRY_BYTES;
            let vid = u16::from_le_bytes([data[o], data[o + 1]]);
            let pid = u16::from_le_bytes([data[o + 2], data[o + 3]]);
            let flags = u16::from_le_bytes([data[o + 4], data[o + 5]]);
            let mut tag = [0u8; 16];
            tag.copy_from_slice(&data[o + 8..o + 24]);
            self.entries[self.count] = UsbTrustEntry {
                vid,
                pid,
                flags,
                tag,
            };
            self.count += 1;
        }
        Ok(self.count)
    }
}

static TABLE: Mutex<UsbTrustTable> = Mutex::new(UsbTrustTable::new());
static BOOT_MSC_ALLOWED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbPolicy {
    Allow,
    Deny,
    /// Soft observe: permite mas loga (enforce=false).
    Observe,
}

/// Decisão para classe MSC/UAC/HID antes de configurar endpoints.
pub fn decide(vid: u16, pid: u16, class_tag: &str) -> UsbPolicy {
    let tbl = TABLE.lock();
    if tbl.is_trusted(vid, pid) {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "ALLOW trusted class={} vid={:#06x} pid={:#06x}",
            class_tag,
            vid,
            pid
        );
        return UsbPolicy::Allow;
    }
    // Boot stick: primeira MSC da sessão sem tabela → Allow + seed BOOT
    if class_tag == "msc" && !BOOT_MSC_ALLOWED.load(Ordering::Relaxed) && tbl.count == 0 {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "ALLOW boot-once class=msc (empty table)"
        );
        return UsbPolicy::Allow;
    }
    if tbl.is_enforce() {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "DENY unknown class={} vid={:#06x} pid={:#06x}",
            class_tag,
            vid,
            pid
        );
        UsbPolicy::Deny
    } else {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "OBSERVE unknown class={} vid={:#06x} pid={:#06x} (set USB_TRUST_ENFORCE=1)",
            class_tag,
            vid,
            pid
        );
        UsbPolicy::Observe
    }
}

/// Chamado apos MSC probe OK — marca boot-once e seed BOOT na tabela.
pub fn note_boot_msc_ok() {
    BOOT_MSC_ALLOWED.store(true, Ordering::Relaxed);
    let mut tbl = TABLE.lock();
    if tbl.count == 0 {
        let _ = tbl.allow(0, 0, FLAG_BOOT | FLAG_TRUSTED, "boot-msc");
        k_nano::slog_bin!("USB-TRUST", "info", "seeded BOOT entry vid=0 pid=0");
    }
}

pub fn set_enforce(on: bool) {
    TABLE.lock().set_enforce(on);
    k_nano::slog_bin!("USB-TRUST", "info", "enforce={}", on);
}

pub fn allow(vid: u16, pid: u16, tag: &str) -> bool {
    let ok = TABLE.lock().allow(vid, pid, FLAG_TRUSTED | FLAG_ONCE, tag);
    if ok {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "trust-once ALLOWED vid={:#06x} pid={:#06x} tag={}",
            vid,
            pid,
            tag
        );
    }
    ok
}

pub fn revoke(vid: u16, pid: u16) -> bool {
    let ok = TABLE.lock().revoke(vid, pid);
    if ok {
        k_nano::slog_bin!(
            "USB-TRUST",
            "info",
            "REVOKED vid={:#06x} pid={:#06x}",
            vid,
            pid
        );
    }
    ok
}

pub fn serialize() -> Vec<u8> {
    TABLE.lock().serialize()
}

pub fn load_bytes(data: &[u8]) -> Result<usize, &'static str> {
    let n = TABLE.lock().load_bytes(data)?;
    k_nano::slog_bin!("USB-TRUST", "info", "loaded {} entries from usb.tbl", n);
    Ok(n)
}

/// Path canônico NeuralFS (relativo a /mnt/neural).
pub const TBL_PATH: &str = "system/trust/usb.tbl";

/// Aplica DENY: tenta desabilitar portas CCS sem PED de teclado (best-effort).
pub fn enforce_deny_ports() {
    unsafe {
        let n = crate::xhci::disable_untrusted_ports();
        k_nano::slog_bin!("USB-TRUST", "info", "disable_untrusted_ports n={}", n);
    }
}
