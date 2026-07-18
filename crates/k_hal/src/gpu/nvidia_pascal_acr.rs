//! Degrau ACR Pascal (GP107/GP108) — WPR/LSB + HS SEC2 (ADR-0048 P2).
//!
//! Layout alinhado a Nouveau `nvkm/subdev/acr/gp102.c` + `include/nvfw/acr.h`.
//! Honestidade: `HsBooted` só se headers WPR na VRAM mudarem para VALIDATION_DONE
//! ou BOOTSTRAP_READY — nunca por bit DMEMC genérico. Sem silício → HsTimeout.

use crate::gpu::detect::GpuInfo;
use crate::gpu::firmware;
use alloc::vec::Vec;
use k_nano::dma::dma_alloc_coalesced;

/// Metade WPR (área autenticada). Dual-shadow = 2× (Nouveau gp102).
pub const WPR_HALF: u64 = 0x20_0000; // 2 MiB
/// Total no topo da VRAM quando dual-shadow cabe.
pub const WPR_DUAL_TOTAL: u64 = WPR_HALF * 2;

const LSF_FECS: u32 = 2;
const LSF_GPCCS: u32 = 3;
const LSF_SEC2: u32 = 7;
const WPR_FALCON_INVALID: u32 = 0xffff_ffff;
const WPR_STATUS_COPY: u32 = 1;
const WPR_STATUS_VALIDATION_DONE: u32 = 4;
const WPR_STATUS_BOOTSTRAP_READY: u32 = 6;

const MAX_LSF_HEADERS: usize = 11;
const SUB_WPR_HDR: u32 = 0x100;

/// SEC2 falcon BAR0 base (Nouveau / nova-core).
const SEC2_BASE: u64 = 0x0084_0000;

// Falcon regs relativos à base do engine (Nouveau falcon v1).
const FLCN_CPUCTL: u64 = 0x100;
const FLCN_BOOTVEC: u64 = 0x104;
const FLCN_DMACTL: u64 = 0x10c;
const FLCN_IMEMC: u64 = 0x180;
const FLCN_IMEMD: u64 = 0x184;
const FLCN_DMEMC: u64 = 0x1a0;
const FLCN_DMEMD: u64 = 0x1a4;
const FLCN_MBOX0: u64 = 0x040;
const CPUCTL_STARTCPU: u32 = 0x2;
const DMEMC_AINCW: u32 = 0x0100_0000;
const IMEMC_AINCW: u32 = 0x0100_0000;
const IMEMC_SECURE: u32 = 0x1000_0000;

const HS_POLL_SPINS: u32 = 500_000;

/// Estágio ACR — nunca implica `has_compute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcrStage {
    BlobsMissing,
    WprBuilt,
    HsSubmitted,
    HsTimeout,
    /// HS concluiu (WPR status avançou) — ainda sem GR/compute Ready.
    HsBooted,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct AcrReport {
    pub stage: AcrStage,
    pub wpr_start: u64,
    pub wpr_end: u64,
    /// True se só single-buffer (VRAM pequena).
    pub shadow_skipped: bool,
    pub shadow_start: u64,
    pub fecs_img: u32,
    pub gpccs_img: u32,
}

impl AcrReport {
    pub fn hs_booted(self) -> bool {
        self.stage == AcrStage::HsBooted
    }

    pub fn wpr_ok(self) -> bool {
        matches!(
            self.stage,
            AcrStage::WprBuilt
                | AcrStage::HsSubmitted
                | AcrStage::HsTimeout
                | AcrStage::HsBooted
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WprHeaderV1 {
    falcon_id: u32,
    lsb_offset: u32,
    bootstrap_owner: u32,
    lazy_bootstrap: u32,
    bin_version: u32,
    status: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LsbTail {
    ucode_off: u32,
    ucode_size: u32,
    data_size: u32,
    bl_code_size: u32,
    bl_imem_off: u32,
    bl_data_off: u32,
    bl_data_size: u32,
    app_code_off: u32,
    app_code_size: u32,
    app_data_off: u32,
    app_data_size: u32,
    flags: u32,
}

/// `flcn_bl_dmem_desc_v2` (Nouveau) — 48 B mínimo usado no bld slot.
#[repr(C)]
#[derive(Clone, Copy)]
struct FlcnBlDmemDescV2 {
    reserved: [u32; 4],
    ctx_dma: u32,
    code_dma_base: u32, // low; hi in next on some gens — gp108 uses u64 split
    code_dma_base_hi: u32,
    non_sec_code_off: u32,
    non_sec_code_size: u32,
    code_entry_point: u32,
    data_dma_base: u32,
    data_dma_base_hi: u32,
    data_size: u32,
    argc: u32,
    argv: u32,
}

fn align_up(v: u32, a: u32) -> u32 {
    (v + a - 1) & !(a - 1)
}

fn write_vram(bar2: u64, off: u64, data: &[u8]) {
    unsafe {
        let ptr = (bar2 + off) as *mut u8;
        for (i, &b) in data.iter().enumerate() {
            core::ptr::write_volatile(ptr.add(i), b);
        }
    }
}

fn write_vram_u32(bar2: u64, off: u64, v: u32) {
    unsafe {
        core::ptr::write_volatile((bar2 + off) as *mut u32, v);
    }
}

fn read_vram_u32(bar2: u64, off: u64) -> u32 {
    unsafe { core::ptr::read_volatile((bar2 + off) as *const u32) }
}

fn write_bytes_at(bar2: u64, base: u64, off: u32, bytes: &[u8]) {
    write_vram(bar2, base + off as u64, bytes);
}

fn pod_bytes<T>(t: &T) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(t as *const T as *const u8, core::mem::size_of::<T>())
    }
}

/// Monta imagem LS: bl + inst + data (formato linux-firmware / Nouveau bl_inst_data).
fn build_ls_img(bl: &[u8], inst: &[u8], data: &[u8]) -> Vec<u8> {
    let mut img = Vec::with_capacity(bl.len() + inst.len() + data.len());
    img.extend_from_slice(bl);
    img.extend_from_slice(inst);
    img.extend_from_slice(data);
    img
}

fn fill_lsb_tail(bl_len: u32, inst_len: u32, data_len: u32) -> LsbTail {
    // Offsets relativos à imagem LS (bl | inst | data).
    let bl_code = align_up(bl_len, 256);
    LsbTail {
        ucode_off: 0,
        ucode_size: align_up(bl_code + inst_len, 256),
        data_size: data_len,
        bl_code_size: bl_code,
        bl_imem_off: 0,
        bl_data_off: 0,
        bl_data_size: 0,
        app_code_off: bl_len,
        app_code_size: inst_len,
        app_data_off: bl_len + inst_len,
        app_data_size: data_len,
        flags: 0,
    }
}

struct LsfSlots {
    falcon_id: u32,
    lsb_off: u32,
    img_off: u32,
    bld_off: u32,
    img: Vec<u8>,
    sig: Vec<u8>,
    bl_len: u32,
    inst_len: u32,
    data_len: u32,
}

fn compute_layout(lsfs: &mut [LsfSlots]) -> u32 {
    let mut wpr = (MAX_LSF_HEADERS as u32) * 24; // sizeof WprHeaderV1
    wpr = align_up(wpr, 256);
    wpr += SUB_WPR_HDR;
    for l in lsfs.iter_mut() {
        wpr = align_up(wpr, 256);
        l.lsb_off = wpr;
        wpr += 192 + 48; // sig v1 (192) + tail (48) ≈ lsb_header_v1
        wpr = align_up(wpr, 4096);
        l.img_off = wpr;
        wpr += l.img.len() as u32;
        wpr = align_up(wpr, 256);
        l.bld_off = wpr;
        wpr += align_up(core::mem::size_of::<FlcnBlDmemDescV2>() as u32, 256);
    }
    wpr
}

fn flcn_wr32(mmio: u64, off: u64, v: u32) {
    unsafe {
        core::ptr::write_volatile((mmio + SEC2_BASE + off) as *mut u32, v);
    }
}

fn flcn_rd32(mmio: u64, off: u64) -> u32 {
    unsafe { core::ptr::read_volatile((mmio + SEC2_BASE + off) as *const u32) }
}

/// PIO word write para IMEM (auto-increment).
unsafe fn falcon_load_imem(mmio: u64, data: &[u8], secure: bool) {
    let mut ctrl = IMEMC_AINCW;
    if secure {
        ctrl |= IMEMC_SECURE;
    }
    flcn_wr32(mmio, FLCN_IMEMC, ctrl);
    for chunk in data.chunks(4) {
        let mut word = 0u32;
        for (j, &b) in chunk.iter().enumerate() {
            word |= (b as u32) << (j * 8);
        }
        flcn_wr32(mmio, FLCN_IMEMD, word);
    }
}

unsafe fn falcon_load_dmem(mmio: u64, data: &[u8], dmem_off: u32) {
    flcn_wr32(mmio, FLCN_DMEMC, (dmem_off & 0x00ff_ffff) | DMEMC_AINCW);
    for chunk in data.chunks(4) {
        let mut word = 0u32;
        for (j, &b) in chunk.iter().enumerate() {
            word |= (b as u32) << (j * 8);
        }
        flcn_wr32(mmio, FLCN_DMEMD, word);
    }
}

/// Patch `flcn_acr_desc_v1` regiões no início do DMEM da imagem HS (após reserved 0x200).
fn patch_acr_desc(ucode: &mut [u8], wpr_start: u64, wpr_end: u64, shadow_start: u64) {
    const DESC_OFF: usize = 0x200;
    if ucode.len() < DESC_OFF + 0x80 {
        return;
    }
    let base = DESC_OFF + 16;
    write_u32_le(ucode, base, 1);
    let reg = base + 12;
    write_u32_le(ucode, reg, 2);
    write_u32_le(ucode, reg + 4, (wpr_start >> 8) as u32);
    write_u32_le(ucode, reg + 8, (wpr_end >> 8) as u32);
    write_u32_le(ucode, reg + 12, 1);
    write_u32_le(ucode, reg + 16, 0xf);
    write_u32_le(ucode, reg + 20, 0xc);
    write_u32_le(ucode, reg + 24, 0x2);
    write_u32_le(ucode, reg + 28, (shadow_start >> 8) as u32);
}

fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
    if off + 4 > buf.len() {
        return;
    }
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn require_blob(name: &str) -> Option<Vec<u8>> {
    firmware::load_firmware_file(name)
}

/// Bring-up ACR Pascal — não-fatal; retorna estágio honesto.
pub unsafe fn bring_up_acr(gpu: &GpuInfo, mmio: u64, bar2_virt: u64) -> AcrReport {
    k_nano::slog_hal!("NVIDIA", "ACR", "{}: Degrau WPR/LSB/HS (SEC2={:#x})",
        gpu.name,
        SEC2_BASE);

    let bl_hs = require_blob("bl.bin");
    let ucode_hs = require_blob("ucode_load.bin");
    let fecs_bl = require_blob("fecs_bl.bin");
    let fecs_data = require_blob("fecs_data.bin");
    let fecs_inst = require_blob("fecs_inst.bin");
    let fecs_sig = require_blob("fecs_sig.bin");
    let gpccs_bl = require_blob("gpccs_bl.bin");
    let gpccs_data = require_blob("gpccs_data.bin");
    let gpccs_inst = require_blob("gpccs_inst.bin");
    let gpccs_sig = require_blob("gpccs_sig.bin");

    let (
        Some(bl_hs),
        Some(mut ucode_hs),
        Some(fecs_bl),
        Some(fecs_data),
        Some(fecs_inst),
        Some(fecs_sig),
        Some(gpccs_bl),
        Some(gpccs_data),
        Some(gpccs_inst),
        Some(gpccs_sig),
    ) = (
        bl_hs, ucode_hs, fecs_bl, fecs_data, fecs_inst, fecs_sig, gpccs_bl, gpccs_data,
        gpccs_inst, gpccs_sig,
    )
    else {
        k_nano::slog_hal!("NVIDIA", "ACR", "BlobsMissing (HS bl/ucode_load + FECS/GPCCS+sig)");
        return AcrReport {
            stage: AcrStage::BlobsMissing,
            wpr_start: 0,
            wpr_end: 0,
            shadow_skipped: true,
            shadow_start: 0,
            fecs_img: 0,
            gpccs_img: 0,
        };
    };

    // Dual-shadow (gp102): 2× WPR_HALF no topo; senão single-buffer + ShadowSkipped.
    let dual = gpu.vram_size >= WPR_DUAL_TOTAL + 0x10_0000;
    let (shadow_start, wpr_start, wpr_end, shadow_skipped) = if dual {
        let shadow = gpu.vram_size - WPR_DUAL_TOTAL;
        let wpr = shadow + WPR_HALF;
        (shadow, wpr, gpu.vram_size, false)
    } else if gpu.vram_size >= WPR_HALF + 0x10_0000 {
        let wpr = gpu.vram_size - WPR_HALF;
        (wpr, wpr, gpu.vram_size, true)
    } else {
        k_nano::slog_hal!("NVIDIA", "ACR", "VRAM insuficiente para WPR");
        return AcrReport {
            stage: AcrStage::Failed,
            wpr_start: 0,
            wpr_end: 0,
            shadow_skipped: true,
            shadow_start: 0,
            fecs_img: 0,
            gpccs_img: 0,
        };
    };
    if shadow_skipped {
        k_nano::slog_hal!("NVIDIA", "ACR", "WPR single-buffer {:#x}..{:#x} (ShadowSkipped)", wpr_start, wpr_end);
    } else {
        k_nano::slog_hal!("NVIDIA", "ACR", "WPR dual-shadow shadow={:#x} wpr={:#x}..{:#x}", shadow_start, wpr_start, wpr_end);
    }

    let fecs_bl_len = fecs_bl.len() as u32;
    let fecs_inst_len = fecs_inst.len() as u32;
    let fecs_data_len = fecs_data.len() as u32;
    let gpccs_bl_len = gpccs_bl.len() as u32;
    let gpccs_inst_len = gpccs_inst.len() as u32;
    let gpccs_data_len = gpccs_data.len() as u32;

    let fecs_img = build_ls_img(&fecs_bl, &fecs_inst, &fecs_data);
    let gpccs_img = build_ls_img(&gpccs_bl, &gpccs_inst, &gpccs_data);
    let fecs_img_sz = fecs_img.len() as u32;
    let gpccs_img_sz = gpccs_img.len() as u32;

    let mut lsfs = [
        LsfSlots {
            falcon_id: LSF_FECS,
            lsb_off: 0,
            img_off: 0,
            bld_off: 0,
            img: fecs_img,
            sig: fecs_sig,
            bl_len: fecs_bl_len,
            inst_len: fecs_inst_len,
            data_len: fecs_data_len,
        },
        LsfSlots {
            falcon_id: LSF_GPCCS,
            lsb_off: 0,
            img_off: 0,
            bld_off: 0,
            img: gpccs_img,
            sig: gpccs_sig,
            bl_len: gpccs_bl_len,
            inst_len: gpccs_inst_len,
            data_len: gpccs_data_len,
        },
    ];

    let _layout_end = compute_layout(&mut lsfs);
    if _layout_end as u64 > WPR_HALF {
        k_nano::slog_hal!("NVIDIA", "ACR", "layout {}B > WPR half {}B — Failed",
            _layout_end,
            WPR_HALF);
        return AcrReport {
            stage: AcrStage::Failed,
            wpr_start,
            wpr_end,
            shadow_skipped,
            shadow_start,
            fecs_img: fecs_img_sz,
            gpccs_img: gpccs_img_sz,
        };
    }

    // Limpa início do WPR (headers).
    for i in 0..(MAX_LSF_HEADERS * 6) {
        write_vram_u32(bar2_virt, wpr_start + (i as u64) * 4, 0);
    }

    // Headers WPR + LSB + img + bld.
    for (i, l) in lsfs.iter().enumerate() {
        let hdr = WprHeaderV1 {
            falcon_id: l.falcon_id,
            lsb_offset: l.lsb_off,
            bootstrap_owner: LSF_SEC2,
            lazy_bootstrap: 0,
            bin_version: 0,
            status: WPR_STATUS_COPY,
        };
        write_bytes_at(bar2_virt, wpr_start, (i as u32) * 24, pod_bytes(&hdr));

        let sig_len = l.sig.len().min(192);
        write_bytes_at(bar2_virt, wpr_start, l.lsb_off, &l.sig[..sig_len]);
        let tail = fill_lsb_tail(l.bl_len, l.inst_len, l.data_len);
        write_bytes_at(bar2_virt, wpr_start, l.lsb_off + 192, pod_bytes(&tail));

        write_bytes_at(bar2_virt, wpr_start, l.img_off, &l.img);

        let code = wpr_start + l.img_off as u64 + l.bl_len as u64;
        let data = wpr_start + l.img_off as u64 + l.bl_len as u64 + l.inst_len as u64;
        let bld = FlcnBlDmemDescV2 {
            reserved: [0; 4],
            ctx_dma: 0,
            code_dma_base: (code & 0xffff_ffff) as u32,
            code_dma_base_hi: (code >> 32) as u32,
            non_sec_code_off: l.bl_len,
            non_sec_code_size: l.inst_len,
            code_entry_point: 0,
            data_dma_base: (data & 0xffff_ffff) as u32,
            data_dma_base_hi: (data >> 32) as u32,
            data_size: l.data_len,
            argc: 0,
            argv: 0,
        };
        write_bytes_at(bar2_virt, wpr_start, l.bld_off, pod_bytes(&bld));
    }
    write_vram_u32(
        bar2_virt,
        wpr_start + (lsfs.len() as u64) * 24,
        WPR_FALCON_INVALID,
    );

    // Espelha WPR → shadow (Nouveau: HS autentica a partir do shadow).
    if !shadow_skipped {
        let n = WPR_HALF as usize;
        unsafe {
            let src = (bar2_virt + wpr_start) as *const u8;
            let dst = (bar2_virt + shadow_start) as *mut u8;
            for i in 0..n {
                core::ptr::write_volatile(dst.add(i), core::ptr::read_volatile(src.add(i)));
            }
        }
        k_nano::slog_hal!("NVIDIA", "ACR", "mirrored {}B WPR→shadow @ {:#x}",
            WPR_HALF,
            shadow_start);
    }

    k_nano::slog_hal!("NVIDIA", "ACR", "WprBuilt fecs_img={}B gpccs_img={}B lsb=[{:#x},{:#x}] shadow_ok={}",
        fecs_img_sz,
        gpccs_img_sz,
        lsfs[0].lsb_off,
        lsfs[1].lsb_off,
        !shadow_skipped);

    patch_acr_desc(&mut ucode_hs, wpr_start, wpr_end, shadow_start);

    let hs_pages = ((bl_hs.len() + ucode_hs.len() + 4095) / 4096).max(1);
    let Some(hs_dma) = dma_alloc_coalesced(hs_pages * 4096) else {
        k_nano::slog_hal!("NVIDIA", "ACR", "DMA HS alloc fail");
        return AcrReport {
            stage: AcrStage::WprBuilt,
            wpr_start,
            wpr_end,
            shadow_skipped,
            shadow_start,
            fecs_img: fecs_img_sz,
            gpccs_img: gpccs_img_sz,
        };
    };
    core::ptr::copy_nonoverlapping(bl_hs.as_ptr(), hs_dma.virt as *mut u8, bl_hs.len());
    core::ptr::copy_nonoverlapping(
        ucode_hs.as_ptr(),
        (hs_dma.virt as *mut u8).add(bl_hs.len()),
        ucode_hs.len(),
    );

    flcn_wr32(mmio, FLCN_CPUCTL, 0);
    flcn_wr32(mmio, FLCN_DMACTL, 0);
    falcon_load_imem(mmio, &bl_hs, true);
    let dmem_bytes = ucode_hs.len().min(65536);
    falcon_load_dmem(mmio, &ucode_hs[..dmem_bytes], 0);
    flcn_wr32(mmio, FLCN_BOOTVEC, 0);
    flcn_wr32(mmio, FLCN_MBOX0, 0);
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
    flcn_wr32(mmio, FLCN_CPUCTL, CPUCTL_STARTCPU);

    k_nano::slog_hal!("NVIDIA", "ACR", "HsSubmitted bl={}B ucode={}B dma_phys={:#x}",
        bl_hs.len(),
        ucode_hs.len(),
        hs_dma.phys);

    let status_off = wpr_start + 20;
    let mut booted = false;
    for _ in 0..HS_POLL_SPINS {
        let st = read_vram_u32(bar2_virt, status_off);
        if st == WPR_STATUS_VALIDATION_DONE || st == WPR_STATUS_BOOTSTRAP_READY {
            booted = true;
            break;
        }
        let _mbox = flcn_rd32(mmio, FLCN_MBOX0);
        core::hint::spin_loop();
    }

    let _keep_hs = hs_dma;
    let stage = if booted {
        k_nano::slog_hal!("NVIDIA", "ACR", "HsBooted — WPR status avançou (não = has_compute)");
        AcrStage::HsBooted
    } else {
        let st = read_vram_u32(bar2_virt, status_off);
        k_nano::slog_hal!("NVIDIA", "ACR", "HsTimeout (wpr_status={:#x} cpuctl={:#x}) — esperado sem silício/QEMU",
            st,
            flcn_rd32(mmio, FLCN_CPUCTL));
        AcrStage::HsTimeout
    };

    AcrReport {
        stage,
        wpr_start,
        wpr_end,
        shadow_skipped,
        shadow_start,
        fecs_img: fecs_img_sz,
        gpccs_img: gpccs_img_sz,
    }
}
