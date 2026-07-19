//! Escolha de modo GOP com cascata EDID → teto 1080p → uncapped.
//! Em QEMU: alvo ~1280×720 (faixa 1024–1280) para poupar soft-float.
//! Roda só com BootServices vivos; Hz é informativo (SetMode não recebe refresh).

use core::arch::x86_64::__cpuid;
use uefi::{
    Handle,
    prelude::{Boot, SystemTable},
    proto::{
        Protocol, unsafe_protocol,
        console::gop::{GraphicsOutput, Mode, PixelFormat},
    },
    table::boot::{OpenProtocolAttributes, OpenProtocolParams},
    table::cfg::{SMBIOS3_GUID, SMBIOS_GUID},
};

/// Teto HW / soft-float amigável (plano EDID/GOP).
pub const CAP_W: usize = 1920;
pub const CAP_H: usize = 1080;
/// Piso desejável sob o teto HW (se existir modo linear).
pub const FLOOR_W: usize = 1280;
pub const FLOOR_H: usize = 720;

/// Teto QEMU — resolução intermediária (bom custo/benefício no compositor).
pub const QEMU_CAP_W: usize = 1280;
pub const QEMU_CAP_H: usize = 800;
pub const QEMU_TARGET_W: usize = 1280;
pub const QEMU_TARGET_H: usize = 720;
pub const QEMU_MIN_W: usize = 1024;
pub const QEMU_MIN_H: usize = 600;

#[derive(Clone, Copy)]
struct CapPolicy {
    max_w: usize,
    max_h: usize,
    floor_w: usize,
    floor_h: usize,
    qemu: bool,
}

/// EFI_EDID_ACTIVE_PROTOCOL — buffer bruto do monitor ativo.
#[repr(C)]
#[unsafe_protocol("bd8c1056-9f36-44ec-92a8-a6337f817986")]
struct EdidActive {
    size_of_edid: u32,
    edid: *const u8,
}

/// EFI_EDID_DISCOVERED_PROTOCOL — EDID lido do display (sem override).
#[repr(C)]
#[unsafe_protocol("1c0c34f6-d380-41fa-a049-8ad06c1a66aa")]
struct EdidDiscovered {
    size_of_edid: u32,
    edid: *const u8,
}

#[derive(Clone, Copy)]
pub struct EdidPreferred {
    pub width: usize,
    pub height: usize,
    /// Refresh aproximado (Hz) do DTD0; 0 se não calculável.
    pub hz: u32,
}

pub struct PickResult {
    pub mode: Mode,
    pub reason: &'static str,
    pub edid: Option<EdidPreferred>,
}

/// Tenta Active no handle GOP, depois locate Active, depois Discovered.
pub fn read_edid_preferred(
    image_handle: Handle,
    st: &SystemTable<Boot>,
    gop_handle: Handle,
) -> Option<EdidPreferred> {
    if let Some(p) = try_edid_on_handle::<EdidActive>(image_handle, st, gop_handle) {
        return Some(p);
    }
    if let Some(p) = try_edid_locate::<EdidActive>(image_handle, st) {
        return Some(p);
    }
    if let Some(p) = try_edid_on_handle::<EdidDiscovered>(image_handle, st, gop_handle) {
        return Some(p);
    }
    try_edid_locate::<EdidDiscovered>(image_handle, st)
}

fn try_edid_on_handle<P>(
    image_handle: Handle,
    st: &SystemTable<Boot>,
    handle: Handle,
) -> Option<EdidPreferred>
where
    P: Protocol + EdidBuffer,
{
    let proto = unsafe {
        st.boot_services()
            .open_protocol::<P>(
                OpenProtocolParams {
                    handle,
                    agent: image_handle,
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .ok()?
    };
    parse_edid_bytes(proto.edid_slice()?)
}

fn try_edid_locate<P>(image_handle: Handle, st: &SystemTable<Boot>) -> Option<EdidPreferred>
where
    P: Protocol + EdidBuffer,
{
    let handles = st
        .boot_services()
        .locate_handle_buffer(uefi::table::boot::SearchType::from_proto::<P>())
        .ok()?;
    for handle in handles.iter() {
        if let Some(p) = try_edid_on_handle::<P>(image_handle, st, *handle) {
            return Some(p);
        }
    }
    None
}

trait EdidBuffer {
    fn edid_slice(&self) -> Option<&[u8]>;
}

impl EdidBuffer for EdidActive {
    fn edid_slice(&self) -> Option<&[u8]> {
        edid_ptr_slice(self.size_of_edid, self.edid)
    }
}

impl EdidBuffer for EdidDiscovered {
    fn edid_slice(&self) -> Option<&[u8]> {
        edid_ptr_slice(self.size_of_edid, self.edid)
    }
}

fn edid_ptr_slice(size: u32, ptr: *const u8) -> Option<&'static [u8]> {
    if size < 128 || ptr.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(ptr, size as usize) })
}

/// Checksum + header + DTD0 (preferred timing).
pub fn parse_edid_bytes(edid: &[u8]) -> Option<EdidPreferred> {
    if edid.len() < 128 {
        return None;
    }
    let block = &edid[..128];
    if block[0] != 0x00 || block[7] != 0x00 {
        return None;
    }
    for b in &block[1..7] {
        if *b != 0xFF {
            return None;
        }
    }
    let sum: u8 = block.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    if sum != 0 {
        return None;
    }

    let dtd = &block[54..72];
    let pixel_clock_10khz = u16::from_le_bytes([dtd[0], dtd[1]]) as u32;
    if pixel_clock_10khz == 0 {
        return None;
    }

    let h_active = (dtd[2] as u32) | (((dtd[4] as u32) & 0xF0) << 4);
    let h_blank = (dtd[3] as u32) | (((dtd[4] as u32) & 0x0F) << 8);
    let v_active = (dtd[5] as u32) | (((dtd[7] as u32) & 0xF0) << 4);
    let v_blank = (dtd[6] as u32) | (((dtd[7] as u32) & 0x0F) << 8);

    if h_active < 640 || v_active < 480 {
        return None;
    }

    let h_total = h_active.saturating_add(h_blank);
    let v_total = v_active.saturating_add(v_blank);
    let hz = if h_total > 0 && v_total > 0 {
        ((pixel_clock_10khz as u64) * 10_000 / (h_total as u64 * v_total as u64)) as u32
    } else {
        0
    };

    Some(EdidPreferred {
        width: h_active as usize,
        height: v_active as usize,
        hz,
    })
}

/// QEMU/TCG/KVM via CPUID; WHPX via SMBIOS ("QEMU"/"BOCHS").
pub fn detect_qemu(st: &SystemTable<Boot>) -> bool {
    if cpuid_hypervisor_qemu_like() {
        return true;
    }
    smbios_mentions_qemu(st)
}

fn smbios_mentions_qemu(st: &SystemTable<Boot>) -> bool {
    for entry in st.config_table() {
        let table = if entry.guid == SMBIOS3_GUID {
            smbios3_table_slice(entry.address as *const u8)
        } else if entry.guid == SMBIOS_GUID {
            smbios2_table_slice(entry.address as *const u8)
        } else {
            None
        };
        let Some(bytes) = table else { continue };
        if find_ascii(bytes, b"QEMU") || find_ascii(bytes, b"BOCHS") {
            return true;
        }
    }
    false
}

/// Entry point SMBIOS 2.x (`_SM_`) → tabela de estruturas.
fn smbios2_table_slice(ep: *const u8) -> Option<&'static [u8]> {
    if ep.is_null() {
        return None;
    }
    let hdr = unsafe { core::slice::from_raw_parts(ep, 31) };
    if hdr.len() < 31 || &hdr[0..4] != b"_SM_" {
        // Fallback: varredura curta no entry point
        return Some(unsafe { core::slice::from_raw_parts(ep, 512) });
    }
    let table_addr = u32::from_le_bytes([hdr[0x18], hdr[0x19], hdr[0x1A], hdr[0x1B]]) as usize;
    let table_len = u16::from_le_bytes([hdr[0x16], hdr[0x17]]) as usize;
    if table_addr == 0 || table_len == 0 {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(table_addr as *const u8, table_len.min(8192)) })
}

/// Entry point SMBIOS 3.x (`_SM3_`) → tabela de estruturas.
fn smbios3_table_slice(ep: *const u8) -> Option<&'static [u8]> {
    if ep.is_null() {
        return None;
    }
    let hdr = unsafe { core::slice::from_raw_parts(ep, 24) };
    if hdr.len() < 24 || &hdr[0..5] != b"_SM3_" {
        return Some(unsafe { core::slice::from_raw_parts(ep, 512) });
    }
    let table_addr = u64::from_le_bytes([
        hdr[0x10], hdr[0x11], hdr[0x12], hdr[0x13], hdr[0x14], hdr[0x15], hdr[0x16], hdr[0x17],
    ]) as usize;
    let max_size = u32::from_le_bytes([hdr[0x0C], hdr[0x0D], hdr[0x0E], hdr[0x0F]]) as usize;
    if table_addr == 0 || max_size == 0 {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(table_addr as *const u8, max_size.min(8192)) })
}

fn find_ascii(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn cpuid_hypervisor_qemu_like() -> bool {
    let leaf1 = __cpuid(1);
    if leaf1.ecx & (1 << 31) == 0 {
        return false;
    }
    let hyp = __cpuid(0x4000_0000);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&hyp.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&hyp.ecx.to_le_bytes());
    vendor[8..12].copy_from_slice(&hyp.edx.to_le_bytes());
    // TCG / KVM — clássicos QEMU. WHPX = "Microsoft Hv" → SMBIOS abaixo.
    &vendor == b"TCGTCGTCGTCG" || &vendor[..9] == b"KVMKVMKVM"
}

fn is_linear(fmt: PixelFormat) -> bool {
    matches!(fmt, PixelFormat::Rgb | PixelFormat::Bgr)
}

fn under_cap(w: usize, h: usize, cap: CapPolicy) -> bool {
    w <= cap.max_w && h <= cap.max_h
}

fn meets_floor(w: usize, h: usize, cap: CapPolicy) -> bool {
    w >= cap.floor_w && h >= cap.floor_h
}

fn meets_config_min(w: usize, h: usize, min_w: Option<usize>, min_h: Option<usize>) -> bool {
    if let Some(mw) = min_w {
        if w < mw {
            return false;
        }
    }
    if let Some(mh) = min_h {
        if h < mh {
            return false;
        }
    }
    true
}

/// Aspect dentro de ±2%: |w*ph - pw*h| * 100 <= 2 * pw * h
fn same_aspect(w: usize, h: usize, pw: usize, ph: usize) -> bool {
    if h == 0 || pw == 0 || ph == 0 {
        return false;
    }
    let delta = ((w as i64) * (ph as i64) - (pw as i64) * (h as i64)).unsigned_abs();
    delta.saturating_mul(100) <= 2u64.saturating_mul(pw as u64).saturating_mul(h as u64)
}

fn better_area(cand_w: usize, cand_h: usize, best: &Option<(usize, usize)>) -> bool {
    let pixels = cand_w.saturating_mul(cand_h);
    match best {
        None => true,
        Some((bw, bh)) => pixels >= bw.saturating_mul(*bh),
    }
}

/// Cascata: [QEMU 1280] → edid_exact → edid_scaled → cap_max → uncapped_max.
pub fn pick_gop_mode(
    gop: &GraphicsOutput,
    edid: Option<EdidPreferred>,
    min_w: Option<usize>,
    min_h: Option<usize>,
    qemu: bool,
) -> Option<PickResult> {
    let cap = if qemu {
        CapPolicy {
            max_w: QEMU_CAP_W,
            max_h: QEMU_CAP_H,
            floor_w: QEMU_MIN_W,
            floor_h: QEMU_MIN_H,
            qemu: true,
        }
    } else {
        CapPolicy {
            max_w: CAP_W,
            max_h: CAP_H,
            floor_w: FLOOR_W,
            floor_h: FLOOR_H,
            qemu: false,
        }
    };

    let try_pick = |enforce_min: bool| -> Option<PickResult> {
        let min_ok = |w: usize, h: usize| {
            !enforce_min || meets_config_min(w, h, min_w, min_h)
        };

        // --- QEMU: alvo 1280×720 (faixa 1024–1280) — ignora EDID gigante da VM ---
        if cap.qemu {
            if let Some(mode) = find_exact(gop, QEMU_TARGET_W, QEMU_TARGET_H, min_ok) {
                return Some(PickResult {
                    mode,
                    reason: "qemu_1280x720",
                    edid,
                });
            }
            if let Some(mode) = find_exact(gop, QEMU_TARGET_W, QEMU_CAP_H, min_ok) {
                return Some(PickResult {
                    mode,
                    reason: "qemu_1280x800",
                    edid,
                });
            }
            if let Some(mode) = find_qemu_mid(gop, min_ok) {
                return Some(PickResult {
                    mode,
                    reason: "qemu_mid",
                    edid,
                });
            }
        }

        // --- edid_exact (só HW / não-QEMU; em QEMU EDID mente com frequência) ---
        if !cap.qemu {
            if let Some(pref) = edid {
                if under_cap(pref.width, pref.height, cap) {
                    for mode in gop.modes() {
                        let info = mode.info();
                        if !is_linear(info.pixel_format()) {
                            continue;
                        }
                        let (w, h) = info.resolution();
                        if !min_ok(w, h) {
                            continue;
                        }
                        if w == pref.width && h == pref.height {
                            return Some(PickResult {
                                mode,
                                reason: "edid_exact",
                                edid,
                            });
                        }
                    }
                }

                // --- edid_scaled ---
                let mut best: Option<(usize, usize, Mode)> = None;
                for mode in gop.modes() {
                    let info = mode.info();
                    if !is_linear(info.pixel_format()) {
                        continue;
                    }
                    let (w, h) = info.resolution();
                    if !min_ok(w, h) || !under_cap(w, h, cap) {
                        continue;
                    }
                    if !same_aspect(w, h, pref.width, pref.height) {
                        continue;
                    }
                    let replace = match &best {
                        None => true,
                        Some((bw, bh, _)) => better_area(w, h, &Some((*bw, *bh))),
                    };
                    if replace {
                        best = Some((w, h, mode));
                    }
                }
                if let Some((_, _, mode)) = best {
                    return Some(PickResult {
                        mode,
                        reason: "edid_scaled",
                        edid,
                    });
                }
            }
        }

        // --- cap_max: prefer floor sob teto ---
        let mut best_floor: Option<(usize, usize, Mode)> = None;
        let mut best_cap: Option<(usize, usize, Mode)> = None;
        for mode in gop.modes() {
            let info = mode.info();
            if !is_linear(info.pixel_format()) {
                continue;
            }
            let (w, h) = info.resolution();
            if !min_ok(w, h) || !under_cap(w, h, cap) {
                continue;
            }
            if meets_floor(w, h, cap) {
                let replace = match &best_floor {
                    None => true,
                    Some((bw, bh, _)) => better_area(w, h, &Some((*bw, *bh))),
                };
                if replace {
                    best_floor = Some((w, h, mode));
                }
            } else {
                let replace = match &best_cap {
                    None => true,
                    Some((bw, bh, _)) => better_area(w, h, &Some((*bw, *bh))),
                };
                if replace {
                    best_cap = Some((w, h, mode));
                }
            }
        }
        if let Some((_, _, mode)) = best_floor.or(best_cap) {
            return Some(PickResult {
                mode,
                reason: if cap.qemu { "qemu_cap_max" } else { "cap_max" },
                edid,
            });
        }

        // --- uncapped_max ---
        let mut best: Option<(usize, usize, Mode)> = None;
        for mode in gop.modes() {
            let info = mode.info();
            if !is_linear(info.pixel_format()) {
                continue;
            }
            let (w, h) = info.resolution();
            if !min_ok(w, h) {
                continue;
            }
            let replace = match &best {
                None => true,
                Some((bw, bh, _)) => better_area(w, h, &Some((*bw, *bh))),
            };
            if replace {
                best = Some((w, h, mode));
            }
        }
        best.map(|(_, _, mode)| PickResult {
            mode,
            reason: "uncapped_max",
            edid,
        })
    };

    try_pick(true).or_else(|| try_pick(false))
}

fn find_exact(
    gop: &GraphicsOutput,
    tw: usize,
    th: usize,
    min_ok: impl Fn(usize, usize) -> bool,
) -> Option<Mode> {
    for mode in gop.modes() {
        let info = mode.info();
        if !is_linear(info.pixel_format()) {
            continue;
        }
        let (w, h) = info.resolution();
        if min_ok(w, h) && w == tw && h == th {
            return Some(mode);
        }
    }
    None
}

/// Melhor modo em [1024–1280]×[600–800], mais próximo de 1280×720.
fn find_qemu_mid(
    gop: &GraphicsOutput,
    min_ok: impl Fn(usize, usize) -> bool,
) -> Option<Mode> {
    let mut best: Option<(u64, Mode)> = None;
    for mode in gop.modes() {
        let info = mode.info();
        if !is_linear(info.pixel_format()) {
            continue;
        }
        let (w, h) = info.resolution();
        if !min_ok(w, h) {
            continue;
        }
        if w < QEMU_MIN_W || w > QEMU_CAP_W || h < QEMU_MIN_H || h > QEMU_CAP_H {
            continue;
        }
        let dw = (w as i64 - QEMU_TARGET_W as i64).unsigned_abs();
        let dh = (h as i64 - QEMU_TARGET_H as i64).unsigned_abs();
        let dist = dw.saturating_mul(dw).saturating_add(dh.saturating_mul(dh));
        let replace = match &best {
            None => true,
            Some((bd, _)) => dist < *bd,
        };
        if replace {
            best = Some((dist, mode));
        }
    }
    best.map(|(_, m)| m)
}
