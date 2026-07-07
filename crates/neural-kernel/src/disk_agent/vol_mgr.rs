use alloc::boxed::Box;
use alloc::vec::Vec;
use super::disk_info::*;

pub trait VolumeManagerProbe: Send {
    fn name(&self) -> &str;
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, max_lba: u64) -> Option<VolumeGroup>;
}

pub struct VolMgrRegistry {
    probes: Vec<Box<dyn VolumeManagerProbe>>,
}

impl VolMgrRegistry {
    pub fn new() -> Self {
        let mut reg = VolMgrRegistry { probes: Vec::new() };
        reg.register(Box::new(Lvm2Probe));
        reg.register(Box::new(LuksProbe));
        reg
    }

    pub fn register(&mut self, probe: Box<dyn VolumeManagerProbe>) {
        self.probes.push(probe);
    }

    pub fn detect_all(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, max_lba: u64) -> Vec<VolumeGroup> {
        let mut groups = Vec::new();
        for probe in &self.probes {
            if let Some(vg) = probe.probe(read_fn, max_lba) {
                groups.push(vg);
            }
        }
        groups
    }
}

// ── LVM2 ──────────────────────────────────────────────────
pub struct Lvm2Probe;
impl VolumeManagerProbe for Lvm2Probe {
    fn name(&self) -> &str { "lvm2" }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<VolumeGroup> {
        let mut buf = [0u8; 512];
        if !read_fn(1, &mut buf) { return None; }
        if &buf[0..8] != b"LABELONE" { return None; }
        if &buf[24..28] != b"LVM2" { return None; }
        let pv_uuid = core::str::from_utf8(&buf[40..72]).unwrap_or("").trim_end().into();
        let vg_name_len = buf[106] as usize;
        if vg_name_len == 0 || vg_name_len > 128 { return None; }
        let vg_name = core::str::from_utf8(&buf[108..108+vg_name_len]).unwrap_or("").trim_end().into();
        Some(VolumeGroup { name: vg_name, uuid: pv_uuid, technology: VolumeTech::Lvm2, sub_volumes: Vec::new() })
    }
}

// ── LUKS1/2 ───────────────────────────────────────────────
pub struct LuksProbe;
impl VolumeManagerProbe for LuksProbe {
    fn name(&self) -> &str { "luks" }
    fn probe(&self, read_fn: &dyn Fn(u64, &mut [u8]) -> bool, _ml: u64) -> Option<VolumeGroup> {
        let mut hdr = [0u8; 512];
        if !read_fn(0, &mut hdr) { return None; }
        if &hdr[0..6] != b"LUKS\xBA\xBE" { // LUKS1
            // LUKS2: header starts with "LUKS" at offset 0, magic at 0x00
            if &hdr[0..4] != b"LUKS" { return None; }
            let version = u16::from_le_bytes([hdr[4], hdr[5]]);
            if version != 1 && version != 2 { return None; }
        }
        let cipher = core::str::from_utf8(&hdr[0x70..0x90]).unwrap_or("").trim_end().into();
        let uuid = core::str::from_utf8(&hdr[0xA0..0xC0]).unwrap_or("").trim_end().into();
        Some(VolumeGroup { name: cipher, uuid, technology: VolumeTech::LUKS1, sub_volumes: Vec::new() })
    }
}
