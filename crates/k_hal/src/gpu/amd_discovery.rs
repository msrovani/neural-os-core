//! IP Discovery AMD — parse binary_header + GC (ADR-0049 P0).
//! Fonte: VRAM TMR (topo) se mapeado; senão ArchHint honesto (≠ discovery).

use crate::gpu::compute_abi::ComputeBackendKind;
use crate::gpu::detect::{GpuArch, GpuInfo};

const BINARY_SIGNATURE: u32 = 0x2821_1407;
const DISCOVERY_SIG: u32 = 0x5344_5049; // "IPDS"
const GC_TABLE_ID: u32 = 0x4347; // "GC"
const GC_HWID: u16 = 11;
/// Tamanho típico da região discovery no topo da VRAM (amdgpu TMR).
const DISCOVERY_SCAN: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    VramBinary,
    ArchHint,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct AmdIpId {
    pub gfx_major: u8,
    pub gfx_minor: u8,
    pub gfx_rev: u8,
    pub has_mes: bool,
    pub has_kiq: bool,
    pub wave_size: u16,
    pub source: DiscoverySource,
}

impl AmdIpId {
    pub fn backend_kind(self) -> ComputeBackendKind {
        if self.has_mes {
            ComputeBackendKind::Mes
        } else if self.has_kiq {
            ComputeBackendKind::KiQ
        } else {
            ComputeBackendKind::CpuFallback
        }
    }

    pub fn from_arch_hint(gpu: &GpuInfo) -> Option<Self> {
        let (maj, min, mes, kiq, wave) = match gpu.arch {
            GpuArch::AmdRdna4 => (12u8, 0u8, true, false, 32u16),
            GpuArch::AmdRdna3 => (11, 0, true, false, 32),
            GpuArch::AmdRdna2 => (10, 3, false, true, 32),
            GpuArch::AmdRdna1 => (10, 1, false, true, 32),
            GpuArch::AmdGcn => (9, 0, false, true, 64),
            _ => return None,
        };
        Some(AmdIpId {
            gfx_major: maj,
            gfx_minor: min,
            gfx_rev: 0,
            has_mes: mes,
            has_kiq: kiq,
            wave_size: wave,
            source: DiscoverySource::ArchHint,
        })
    }
}

fn read_u16(p: *const u8, off: usize) -> u16 {
    unsafe {
        let b0 = *p.add(off) as u16;
        let b1 = (*p.add(off + 1) as u16) << 8;
        b0 | b1
    }
}

fn read_u32(p: *const u8, off: usize) -> u32 {
    unsafe {
        let mut v = 0u32;
        for i in 0..4 {
            v |= (*p.add(off + i) as u32) << (8 * i);
        }
        v
    }
}

/// Parse binary_header em buffer; extrai GC major/minor via IP list ou GC table.
pub unsafe fn parse_discovery_bin(bin: *const u8, len: usize) -> Option<AmdIpId> {
    if len < 64 || bin.is_null() {
        return None;
    }
    let sig = read_u32(bin, 0);
    if sig != BINARY_SIGNATURE {
        return None;
    }
    let bin_size = read_u16(bin, 10) as usize;
    if bin_size == 0 || bin_size > len {
        return None;
    }

    // table_list[IP_DISCOVERY=0]: offset @ 12
    let ip_off = read_u16(bin, 12) as usize;
    let mut maj = 0u8;
    let mut min = 0u8;
    let mut rev = 0u8;
    let mut wave = 32u16;

    if ip_off + 16 <= bin_size {
        let ip_sig = read_u32(bin, ip_off);
        if ip_sig == DISCOVERY_SIG {
            // die_info[0].die_offset @ ip_off+16 (após signature..num_dies)
            let die_off = read_u16(bin, ip_off + 16) as usize;
            if die_off + 4 <= bin_size {
                let num_ips = read_u16(bin, die_off + 2) as usize;
                let mut cursor = die_off + 4;
                for _ in 0..num_ips.min(64) {
                    if cursor + 8 > bin_size {
                        break;
                    }
                    let hw_id = read_u16(bin, cursor);
                    let nbase = *bin.add(cursor + 3) as usize;
                    let major = *bin.add(cursor + 4);
                    let minor = *bin.add(cursor + 5);
                    let revision = *bin.add(cursor + 6);
                    let stride = 8 + nbase * 4;
                    if hw_id == GC_HWID {
                        maj = major;
                        min = minor;
                        rev = revision;
                        break;
                    }
                    cursor += stride;
                }
            }
        }
    }

    // GC table (index 1): offset @ 12+8
    let gc_off = read_u16(bin, 20) as usize;
    if gc_off + 16 <= bin_size {
        let tid = read_u32(bin, gc_off);
        if tid == GC_TABLE_ID {
            // gc_info_v1: wave @ offset ~48 from header start (after 12B hdr + fields)
            // Conservador: ler dword em +0x2C se size permitir (gc_wave_size em v1_0).
            let gsize = read_u32(bin, gc_off + 8) as usize;
            if gsize >= 0x30 && gc_off + 0x30 <= bin_size {
                let w = read_u32(bin, gc_off + 0x2C);
                if w == 32 || w == 64 {
                    wave = w as u16;
                }
            }
        }
    }

    if maj == 0 {
        return None;
    }
    let has_mes = maj >= 11;
    let has_kiq = maj < 11;
    k_nano::slog_hal!("AMD", "DISC", "VramBinary GC={}.{} rev={} wave={} mes={} kiq={}", maj, min, rev, wave, has_mes, has_kiq);
    Some(AmdIpId {
        gfx_major: maj,
        gfx_minor: min,
        gfx_rev: rev,
        has_mes,
        has_kiq,
        wave_size: wave,
        source: DiscoverySource::VramBinary,
    })
}

/// Tenta discovery no topo da VRAM mapeada; fallback ArchHint.
pub unsafe fn probe_ip(gpu: &GpuInfo, vram_va: u64, vram_size: u64) -> AmdIpId {
    if vram_va != 0 && vram_size >= DISCOVERY_SCAN as u64 {
        let scan = DISCOVERY_SCAN.min(vram_size as usize);
        let base = (vram_va + vram_size - scan as u64) as *const u8;
        // Procurar signature nos últimos 256KiB (alinhado 4K).
        let mut off = 0usize;
        while off + 64 <= scan {
            if read_u32(base, off) == BINARY_SIGNATURE {
                if let Some(ip) = parse_discovery_bin(base.add(off), scan - off) {
                    return ip;
                }
            }
            off += 4096;
        }
        k_nano::slog_hal!("AMD", "DISC", "VRAM scan sem BINARY_SIGNATURE — ArchHint");
    } else {
        k_nano::slog_hal!("AMD", "DISC", "VRAM indisponível — ArchHint");
    }
    AmdIpId::from_arch_hint(gpu).unwrap_or(AmdIpId {
        gfx_major: 0,
        gfx_minor: 0,
        gfx_rev: 0,
        has_mes: false,
        has_kiq: false,
        wave_size: 32,
        source: DiscoverySource::Failed,
    })
}
