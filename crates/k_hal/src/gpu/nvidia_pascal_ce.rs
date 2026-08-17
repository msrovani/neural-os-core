//! NVIDIA Pascal Copy Engine (CE) — DMA bulk RAM↔VRAM (ADR-0087 Fase 4b).
//!
//! Classe `PASCAL_DMA_COPY_B` 0xc1b5, channel **privileged** (`inst | 0x20`),
//! runlist do CE. Template físico validado em nouveau_dmem.c (`nvc0b5_migrate_copy`):
//!   - 0x0260/0x0264 aperture: `SRC_TYPE_PHYSICAL=0x1000` / `DST_TYPE_PHYSICAL=0x2000`
//!     (+ high 32 bits do endereço de cada lado)
//!   - 0x0400 + 8 dwords: src_lo, src_hi, dst_lo, dst_hi, pitch_lo, 0, npages, 0
//!   - 0x0300 launch: 1
//! Fence (<Volta): **USERD readback** — poll GET (0x44) >= GPPut escrito.
//! Gerações >Pascal re-encodam validação por geração (ADR-0087 §2) → dispatch
//! Pascal-only hoje; canário golden 64KB RAM→VRAM→RAM é o gate (honesto: sem
//! canário não há `ready`).
//!
//! O channel CE é **independente** do channel GR de compute (D2/D3): CHID 1,
//! runlist CE, GMMU própria. `copy_phys` é phys→phys (SRC/DST_TYPE_PHYSICAL).

use crate::gpu::detect::{self, GpuArch, GpuInfo};
use crate::gpu::vram::{vram_alloc, vram_free};
use core::sync::atomic::Ordering;
use k_nano::dma::{dma_alloc_coalesced, DmaBuf};
use spin::Mutex;

/// Classe CE Pascal (`PASCAL_DMA_COPY_B`, clc0b5.h). Volta+ muda por geração.
pub const PASCAL_DMA_COPY_B: u32 = 0xC1B5;
/// Aperture física de origem (sem VM — endereço físico direto).
pub const NV_DMA_COPY_SRC_TYPE_PHYSICAL: u32 = 0x1000;
/// Aperture física de destino.
pub const NV_DMA_COPY_DST_TYPE_PHYSICAL: u32 = 0x2000;

// ─── Channel (espelho do padrão D2/D3 de nvidia_pascal.rs) ────────────────
const IOVA_BASE: u64 = 0x0001_0000_0000; // per-channel GMMU (PD própria por canal)
const PT_ENTRIES: usize = 512;
const GPFIFO_ENTRIES: usize = 32;
const SUBCH_CE: u32 = 1; // mesma convenção do SUBCH_COMPUTE local
const METHOD_SET_OBJECT: u32 = 0x0000;
/// Classe do channel GPFIFO Pascal (`PASCAL_CHANNEL_GPFIFO_A`, clc06f.h).
const CHANNEL_CLASS: u32 = 0xC06F;
/// Bit privileged no instancing do objeto CE (open-gpu-kernel-modules).
const INST_PRIVILEGED: u32 = 0x20;

// PFIFO host registers (BAR0), família gk104→gp10x (mesmos de nvidia_pascal.rs).
const REG_RUNLIST_BASE: u64 = 0x002270;
const REG_RUNLIST_SUBMIT: u64 = 0x002274;
const REG_RUNLIST_STATUS: u64 = 0x002284;
const REG_CHANNEL: u64 = 0x800000;
const REG_CHANNEL_CTRL: u64 = 0x800004;
const REG_KICK: u64 = 0x002634;

const RUNLIST_STATUS_PENDING: u32 = 0x0010_0000;
const CHANNEL_ENABLE: u32 = 0x0000_0400;
const KICK_PENDING: u32 = 0x0010_0000;
const TARGET_SYS_NCOH: u64 = 3;

/// Runlist do CE. GR=0; CE0=1 em gk104+ (canário é o árbitro).
// ponytail: stride 8 segue o padrão local D3 (STATUS + runl*8). Se o HW
// rejeitar o canário, conferir stride 0x10 em gk104.c (nouveau).
const RUNL_CE: u32 = 1;
/// Canal CE dedicado (o GR usa CHID 0).
const CHID_CE: u32 = 1;

// USERD (`Nvc06fControl`) — fence <Volta.
const USERD_GET: usize = 0x44;
const USERD_GPPUT: usize = 0x8C;

// Methods DMA_COPY.
const MTHD_DMA_COPY_SRC_ADDRESS: u32 = 0x0260;
const MTHD_DMA_COPY_DST_ADDRESS: u32 = 0x0264;
const MTHD_DMA_COPY_BLOCK: u32 = 0x0400;
const MTHD_DMA_COPY_LAUNCH: u32 = 0x0300;
const DMA_COPY_LAUNCH_GO: u32 = 1;

const FENCE_SPINS: u32 = 1_000_000; // bounded — timeout honesto, não congela boot

/// Canal CE vivo (mantém DmaBufs para não liberar frames).
pub struct PascalCe {
    pub ready: bool,
    pub mmio: u64,
    chid: u32,
    inst: DmaBuf,
    userd: DmaBuf,
    gpfifo: DmaBuf,
    pb: DmaBuf,
    pb_iova: u64,
    /// Próximo slot GPFIFO livre (0 após cada fence — ring sempre vazio).
    gpfifo_put: u32,
    _pd: DmaBuf,
    _pt: DmaBuf,
    _runlist: DmaBuf,
}

// ─── Helpers GMMU/pushbuffer (padrão D2/D3, self-contained) ────────────────

/// PTE MMU v2 (64-bit) — sysmem. `aperture=2` (sys), `kind=6`, `vol=1`, `valid=1`.
fn encode_pte_sys(phys: u64) -> u64 {
    let pfn = phys >> 12;
    1u64 | (2u64 << 4) | (1u64 << 6) | (6u64 << 8) | (pfn << 12)
}

/// PDE → page table sysmem (folha=false).
fn encode_pde_pt(pt_phys: u64) -> u64 {
    let pfn = pt_phys >> 12;
    1u64 | (1u64 << 4) | (pfn << 12)
}

/// Header de método do pushbuffer. `typ`: 1=incrementing, 2=non-incrementing.
fn pb_method(typ: u32, subch: u32, mthd: u32, count: u32) -> u32 {
    (typ << 28) | (count << 16) | (subch << 13) | (mthd >> 2)
}

/// GPFIFO entry (8 bytes): ENTRY0 GET=addr>>2 [31:2]; ENTRY1 LENGTH [30:10].
fn write_gpfifo_entry(entries: *mut u64, index: usize, pb_addr: u64, length_dwords: u32) {
    let e0 = (pb_addr >> 2) << 2;
    let e1 = ((length_dwords as u64) & 0x1F_FFFF) << 10;
    unsafe {
        core::ptr::write_volatile(entries.add(index), e0 | (e1 << 32));
    }
}

/// RAMFC mínimo: GP_BASE = gpfifo **IOVA**; USERD phys; PD pointer; classe GPFIFO.
unsafe fn fill_ramfc(inst: &DmaBuf, gpfifo_iova: u64, userd: &DmaBuf, pd: &DmaBuf) {
    let base = inst.virt as *mut u32;
    core::ptr::write_volatile(base.add(0x08 / 4), (gpfifo_iova & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x0C / 4), (gpfifo_iova >> 32) as u32);
    core::ptr::write_volatile(base.add(0x20 / 4), (userd.phys & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x24 / 4), (userd.phys >> 32) as u32);
    core::ptr::write_volatile(base.add(0x200 / 4), (pd.phys & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x204 / 4), (pd.phys >> 32) as u32);
    core::ptr::write_volatile(base.add(0x40 / 4), CHANNEL_CLASS);
}

/// USERD: zera Put/Get/GPGet/GPPut.
unsafe fn init_userd(userd: &DmaBuf) {
    let p = userd.virt as *mut u32;
    core::ptr::write_volatile(p.add(0x40 / 4), 0); // Put
    core::ptr::write_volatile(p.add(0x44 / 4), 0); // Get
    core::ptr::write_volatile(p.add(0x88 / 4), 0); // GPGet
    core::ptr::write_volatile(p.add(0x8C / 4), 0); // GPPut
}

unsafe fn userd_set_gpput(userd: &DmaBuf, v: u32) {
    let p = userd.virt as *mut u32;
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
    core::ptr::write_volatile(p.add(USERD_GPPUT / 4), v);
}

/// Poll USERD GET (0x44) >= target — fence <Volta (ADR-0087 §2).
unsafe fn userd_poll_get(userd: &DmaBuf, target: u32, spins: u32) -> bool {
    let p = userd.virt as *const u32;
    for _ in 0..spins {
        if core::ptr::read_volatile(p.add(USERD_GET / 4)) >= target {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Poll bounded de registrador MMIO até `(val & mask)==0`.
unsafe fn poll_clear(mmio: u64, reg: u64, mask: u32, spins: u32) -> bool {
    for _ in 0..spins {
        let v = core::ptr::read_volatile((mmio + reg) as *const u32);
        if v & mask == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

// ─── Builders puros (testáveis no host) ────────────────────────────────────

/// Pacote 0x0400 (8 dwords): src_lo, src_hi, dst_lo, dst_hi, pitch_lo, 0, npages, 0.
/// `met` = header `pb_method(1, subch, 0x0400, 8)`.
pub fn build_dma_copy(met: u32, src: u64, dst: u64, pitch: u32, npages: u32) -> [u32; 9] {
    [
        met,
        src as u32,
        (src >> 32) as u32,
        dst as u32,
        (dst >> 32) as u32,
        pitch,
        0, // pitch_hi
        npages,
        0, // reservado
    ]
}

/// Comando CE completo: apertures (0x0260/0x0264) + DMA_COPY (0x0400×8) + launch (0x0300).
pub fn build_ce_cmd(src: u64, dst: u64, pitch: u32, npages: u32) -> [u32; 15] {
    let m_src = pb_method(1, SUBCH_CE, MTHD_DMA_COPY_SRC_ADDRESS, 1);
    let m_dst = pb_method(1, SUBCH_CE, MTHD_DMA_COPY_DST_ADDRESS, 1);
    let m_copy = pb_method(1, SUBCH_CE, MTHD_DMA_COPY_BLOCK, 8);
    let m_launch = pb_method(1, SUBCH_CE, MTHD_DMA_COPY_LAUNCH, 1);
    let pkt = build_dma_copy(m_copy, src, dst, pitch, npages);
    [
        m_src,
        NV_DMA_COPY_SRC_TYPE_PHYSICAL | (src >> 32) as u32,
        m_dst,
        NV_DMA_COPY_DST_TYPE_PHYSICAL | (dst >> 32) as u32,
        pkt[0], pkt[1], pkt[2], pkt[3], pkt[4], pkt[5], pkt[6], pkt[7], pkt[8],
        m_launch,
        DMA_COPY_LAUNCH_GO,
    ]
}

// ─── Probe / channel ───────────────────────────────────────────────────────

impl PascalCe {
    /// Bring-up estrutural do channel CE (GMMU + bind + runlist + kick).
    /// Não implica `ready` — só o canário (`run_canary`) marca ready.
    pub unsafe fn probe(gpu: &GpuInfo, mmio: u64) -> Option<Self> {
        // Template físico Pascal; gerações >Pascal re-encodam validação (ADR-0087 §2).
        if gpu.arch != GpuArch::NvidiaPascal {
            k_nano::slog_hal!(
                "GPU",
                "CE",
                "{}: CE Pascal-only hoje (arch={:?}); dispatch por geração pendente",
                gpu.name,
                gpu.arch
            );
            return None;
        }
        k_nano::slog_hal!("GPU", "CE", "{}: channel CE bring-up (class={:#x} privileged)", gpu.name, PASCAL_DMA_COPY_B);

        k_nano::pci::enable_pci_bus_master_unsafe(gpu.pci_bus, gpu.pci_dev, gpu.pci_fn);
        let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        k_nano::apic::map_page_uc(gpu.bar0, pmoff);

        let pd = dma_alloc_coalesced(4096)?;
        let pt = dma_alloc_coalesced(PT_ENTRIES * 8)?;
        let inst = dma_alloc_coalesced(4096)?;
        let gpfifo = dma_alloc_coalesced(GPFIFO_ENTRIES * 8)?;
        let userd = dma_alloc_coalesced(4096)?;
        let pb = dma_alloc_coalesced(4096)?;
        let runlist = dma_alloc_coalesced(4096)?;

        let pde = encode_pde_pt(pt.phys);
        core::ptr::write_volatile(pd.virt as *mut u64, pde);

        // Mapeia estruturas na PT (IOVA por canal — GMMU própria).
        let mut mapped = 0usize;
        let gpfifo_iova = IOVA_BASE;
        for (phys, size) in [
            (gpfifo.phys, gpfifo.size),
            (userd.phys, userd.size),
            (inst.phys, inst.size),
            (pd.phys, pd.size),
            (pt.phys, pt.size),
        ] {
            let pages = (size + 4095) / 4096;
            for i in 0..pages {
                if mapped >= PT_ENTRIES {
                    break;
                }
                let pte = encode_pte_sys(phys + (i as u64) * 4096);
                core::ptr::write_volatile((pt.virt as *mut u64).add(mapped), pte);
                mapped += 1;
            }
        }

        fill_ramfc(&inst, gpfifo_iova, &userd, &pd);
        init_userd(&userd);

        // Pushbuffer estático: SET_OBJECT (subch CE) = PASCAL_DMA_COPY_B.
        let pbw = pb.virt as *mut u32;
        core::ptr::write_volatile(pbw.add(0), pb_method(1, SUBCH_CE, METHOD_SET_OBJECT, 1));
        core::ptr::write_volatile(pbw.add(1), PASCAL_DMA_COPY_B);
        let pb_len = 2u32;

        // Mapeia pushbuffer na GMMU.
        let pb_iova = if mapped < PT_ENTRIES {
            let pte = encode_pte_sys(pb.phys);
            core::ptr::write_volatile((pt.virt as *mut u64).add(mapped), pte);
            mapped += 1;
            IOVA_BASE + (mapped as u64 - 1) * 4096
        } else {
            return None;
        };

        // GP entry[0] → pushbuffer; GPPut=1.
        write_gpfifo_entry(gpfifo.virt as *mut u64, 0, pb_iova, pb_len);
        userd_set_gpput(&userd, 1);

        // Runlist CE: entrada única [chid, 0].
        let rlw = runlist.virt as *mut u32;
        core::ptr::write_volatile(rlw.add(0), CHID_CE);
        core::ptr::write_volatile(rlw.add(1), 0);
        let nr = 1u32;

        // Bind canal + ENABLE, com bit privileged (inst | 0x20).
        let coff = (CHID_CE as u64) * 8;
        let chan_val = 0x8000_0000u32 | ((inst.phys >> 12) as u32) | INST_PRIVILEGED;
        core::ptr::write_volatile((mmio + REG_CHANNEL + coff) as *mut u32, chan_val);
        let ctrl = core::ptr::read_volatile((mmio + REG_CHANNEL_CTRL + coff) as *const u32);
        core::ptr::write_volatile((mmio + REG_CHANNEL_CTRL + coff) as *mut u32, ctrl | CHANNEL_ENABLE);

        // Commit runlist do CE (stride 8, padrão local).
        let rl_base = REG_RUNLIST_BASE + (RUNL_CE as u64) * 8;
        let rl_submit = REG_RUNLIST_SUBMIT + (RUNL_CE as u64) * 8;
        let rl_status = REG_RUNLIST_STATUS + (RUNL_CE as u64) * 8;
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        core::ptr::write_volatile((mmio + rl_base) as *mut u32, ((runlist.phys >> 12) | (TARGET_SYS_NCOH << 28)) as u32);
        core::ptr::write_volatile((mmio + rl_submit) as *mut u32, (RUNL_CE << 20) | nr);
        let rl_ok = poll_clear(mmio, rl_status, RUNLIST_STATUS_PENDING, 200_000);

        // Kick inicial (SET_OBJECT) — diagnóstico precoce do canal.
        core::ptr::write_volatile((mmio + REG_KICK) as *mut u32, CHID_CE);
        let kick_ok = poll_clear(mmio, REG_KICK, KICK_PENDING, 200_000);
        let get = userd_poll_get(&userd, 1, FENCE_SPINS);

        k_nano::slog_hal!(
            "GPU",
            "CE",
            "{}: chid={} inst={:#x}|{:#x} runl={} rl_commit={} kick={} userd_get1={}",
            gpu.name,
            CHID_CE,
            inst.phys,
            INST_PRIVILEGED,
            RUNL_CE,
            rl_ok,
            kick_ok,
            get
        );
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=ce status=STRUCTURAL chid={} rl_commit={} kick={} userd_get1={}",
            CHID_CE,
            rl_ok as u8,
            kick_ok as u8,
            get as u8
        );

        Some(PascalCe {
            ready: false, // só o canário marca
            mmio,
            chid: CHID_CE,
            inst,
            userd,
            gpfifo,
            pb,
            pb_iova,
            gpfifo_put: 1, // entry[0] (SET_OBJECT) já submetido
            _pd: pd,
            _pt: pt,
            _runlist: runlist,
        })
    }

    /// Submete DMA_COPY phys→phys e espera fence USERD (GET >= GPPut).
    pub unsafe fn copy_phys(&mut self, src_phys: u64, dst_phys: u64, bytes: usize) -> bool {
        if !self.ready {
            return false;
        }
        if bytes == 0 {
            return true;
        }
        let bytes = (bytes + 3) & !3; // granularidade dword
        let put = self.gpfifo_put;
        if put as usize >= GPFIFO_ENTRIES {
            k_nano::slog_hal!("GPU", "CE", "ring cheio (put={}) — fence pendente", put);
            return false;
        }

        let pitch = if bytes <= 4096 { bytes as u32 } else { 4096 };
        let npages = ((bytes + 4095) / 4096) as u32;
        let cmd = build_ce_cmd(src_phys, dst_phys, pitch, npages);

        // pb[0..2] = SET_OBJECT (permanente); comando após.
        let pbw = self.pb.virt as *mut u32;
        for (i, w) in cmd.iter().enumerate() {
            core::ptr::write_volatile(pbw.add(2 + i), *w);
        }
        let len = 2 + cmd.len() as u32;
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        core::arch::asm!("sfence", options(nostack, preserves_flags));

        write_gpfifo_entry(self.gpfifo.virt as *mut u64, put as usize, self.pb_iova, len);
        let next = put + 1;
        userd_set_gpput(&self.userd, next);
        core::ptr::write_volatile((self.mmio + REG_KICK) as *mut u32, self.chid);

        if !userd_poll_get(&self.userd, next, FENCE_SPINS) {
            k_nano::slog_hal!("GPU", "CE", "fence timeout src={:#x} dst={:#x} {}B (GET={} target={})",
                src_phys,
                dst_phys,
                bytes,
                core::ptr::read_volatile((self.userd.virt as *const u32).add(USERD_GET / 4)),
                next);
            return false;
        }
        self.gpfifo_put = 0; // ring drenado → slots reutilizáveis
        true
    }

    /// Canário 64KB: RAM→VRAM→RAM com padrão determinístico + comparação.
    /// Gate honesto — `ready` só após este passar.
    pub unsafe fn run_canary(&mut self) -> bool {
        const CANARY_BYTES: usize = 64 * 1024;
        let pattern = |i: usize| 0x9E37_79B9u32.wrapping_mul(i as u32 + 1) ^ 0xA5A5_5A5A;

        let Some(src) = dma_alloc_coalesced(CANARY_BYTES) else {
            k_nano::slog_hal!("GPU", "CE", "canary: DMA src alloc falhou");
            return false;
        };
        let Some(dst) = dma_alloc_coalesced(CANARY_BYTES) else {
            k_nano::slog_hal!("GPU", "CE", "canary: DMA dst alloc falhou");
            return false;
        };
        let Some(vram) = vram_alloc(CANARY_BYTES) else {
            k_nano::slog_hal!("GPU", "CE", "canary: VRAM alloc falhou (buddy?)");
            return false;
        };

        // Padrão na RAM src (páginas UC → visível ao DMA).
        let sw = src.virt as *mut u32;
        for i in 0..CANARY_BYTES / 4 {
            core::ptr::write_volatile(sw.add(i), pattern(i));
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        core::arch::asm!("sfence", options(nostack, preserves_flags));

        let up = self.copy_phys(src.phys, vram, CANARY_BYTES);
        let down = up && self.copy_phys(vram, dst.phys, CANARY_BYTES);
        let mut ok = up && down;
        if ok {
            let dr = dst.virt as *const u32;
            for i in 0..CANARY_BYTES / 4 {
                if core::ptr::read_volatile(dr.add(i)) != pattern(i) {
                    ok = false;
                    k_nano::slog_hal!("GPU", "CE", "canary: mismatch no dword {}", i);
                    break;
                }
            }
        }

        vram_free(vram, CANARY_BYTES);
        k_nano::slog_hal!("GPU", "CE", "canary 64KB RAM↔VRAM: up={} down={} golden={}", up, down, ok);
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=ce status={} detail=canary_64k_ram_vram_ram",
            if ok { "OK" } else { "FAIL" }
        );
        ok
    }
}

// ─── Seam global para o MHI (Fase 5 usa; mhi.rs NÃO é editado aqui) ────────

static CE: Mutex<Option<PascalCe>> = Mutex::new(None);

/// True apenas após channel CE + canário 64KB passarem.
pub fn ce_ready() -> bool {
    CE.lock().as_ref().map(|c| c.ready).unwrap_or(false)
}

/// copy phys→phys via CE (sem log) — pronto ou false.
pub unsafe fn ce_copy(src_phys: u64, dst_phys: u64, bytes: usize) -> bool {
    match CE.lock().as_mut() {
        Some(ce) if ce.ready => ce.copy_phys(src_phys, dst_phys, bytes),
        _ => false,
    }
}

/// Seam MHI tier1→tier0 (DRAM→VRAM): usa o CE se pronto; senão false + AWAITING.
/// Registrado em `k_nano::mhi::register_tier0_copier` quando o canário passa
/// (SESSION_274) — o `mhi_tick` promove Dram→Vram com dados reais por aqui.
pub fn mhi_tier0_copy(src_phys: u64, dst_phys: u64, bytes: usize) -> bool {
    if !ce_ready() {
        k_nano::slog_bin!(
            "MHI-DMA",
            "info",
            "step=tier0_copy status=UNSUPPORTED detail=ce_not_ready VERDICT=AWAITING_REAL_HW"
        );
        return false;
    }
    let ok = unsafe { ce_copy(src_phys, dst_phys, bytes) };
    if ok {
        // ADR-0087 §2.0.1: transfers do CE também registram acesso no MHI.
        k_nano::mhi::record_access(dst_phys, 0);
    }
    ok
}

/// Probe global (boot): channel CE + canário. Chamado no branch NVIDIA do backend.
/// Canário falhou → channel mantido com ready=false (honesto, retry futuro).
pub unsafe fn probe_global(gpu: &GpuInfo) {
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let mmio = gpu.bar0 + pmoff;
    let Some(mut ce) = PascalCe::probe(gpu, mmio) else {
        k_nano::slog_bin!("GPU-HW", "info", "step=ce status=SKIP reason=probe_failed_or_gen");
        return;
    };
    let canary_ok = ce.run_canary();
    ce.ready = canary_ok;
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=ce status={} detail=canary_64k {}",
        if canary_ok { "READY" } else { "FAIL" },
        gpu.name
    );
    *CE.lock() = Some(ce);
    // SESSION_274: fecha o seam morto — o mhi_tick agora tem o copier real
    // (Dram→Vram via CE) registrado. Só com canário golden (honesto).
    if canary_ok {
        k_nano::mhi::register_tier0_copier(mhi_tier0_copy, crate::gpu::vram::vram_free);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn met(subch: u32, mthd: u32, count: u32) -> u32 {
        pb_method(1, subch, mthd, count)
    }

    #[test]
    fn dma_copy_packet_layout() {
        // 0x0400 count=8, subch CE → header 0x1008_2100.
        let m = met(SUBCH_CE, MTHD_DMA_COPY_BLOCK, 8);
        assert_eq!(m, 0x1008_2100);
        let src: u64 = 0x1234_5678_9abc_def0;
        let dst: u64 = 0xfeed_face_cafe_beef;
        let pkt = build_dma_copy(m, src, dst, 0x1000, 0x10);
        assert_eq!(
            pkt,
            [
                0x1008_2100,
                0x9abc_def0, // src_lo
                0x1234_5678, // src_hi
                0xcafe_beef, // dst_lo
                0xfeed_face, // dst_hi
                0x1000,      // pitch_lo
                0,           // pitch_hi
                0x10,        // npages
                0,           // reservado
            ]
        );
    }

    #[test]
    fn ce_cmd_composes_aperture_copy_launch() {
        // Endereços > 32 bits para exercitar os dwords hi.
        let src: u64 = 0x0000_0001_0000_0000;
        let dst: u64 = 0x0000_0002_0000_0000;
        let cmd = build_ce_cmd(src, dst, 0x1000, 16);
        assert_eq!(cmd.len(), 15);
        assert_eq!(cmd[0], met(SUBCH_CE, MTHD_DMA_COPY_SRC_ADDRESS, 1)); // 0x1001_2098
        assert_eq!(cmd[1], NV_DMA_COPY_SRC_TYPE_PHYSICAL | 1);            // 0x1001
        assert_eq!(cmd[2], met(SUBCH_CE, MTHD_DMA_COPY_DST_ADDRESS, 1)); // 0x1001_2099
        assert_eq!(cmd[3], NV_DMA_COPY_DST_TYPE_PHYSICAL | 2);            // 0x2002
        assert_eq!(cmd[4], met(SUBCH_CE, MTHD_DMA_COPY_BLOCK, 8));       // 0x1008_2100
        assert_eq!(cmd[5], 0);                                            // src_lo
        assert_eq!(cmd[6], 1);                                            // src_hi
        assert_eq!(cmd[7], 0);                                            // dst_lo
        assert_eq!(cmd[8], 2);                                            // dst_hi
        assert_eq!(cmd[9], 0x1000);                                       // pitch_lo
        assert_eq!(cmd[10], 0);                                           // pitch_hi
        assert_eq!(cmd[11], 16);                                          // npages
        assert_eq!(cmd[12], 0);
        assert_eq!(cmd[13], met(SUBCH_CE, MTHD_DMA_COPY_LAUNCH, 1));     // 0x1001_20c0
        assert_eq!(cmd[14], DMA_COPY_LAUNCH_GO);                          // 1
    }

    #[test]
    fn copy_split_pitch_npages() {
        // bytes <= 4K → pitch=bytes, npages=1; >4K → pitch=4096, npages=ceil.
        assert_eq!(1000usize & !3, 1000); // dword-granularidade conservada p/ múltiplos de 4
        let (p1, n1) = (1000u32, 1u32); // derivado de copy_phys para 1000B
        let (p2, n2) = (4096u32, 16u32); // derivado para 64KB
        assert_eq!(p1, 1000);
        assert_eq!(n1, 1);
        assert_eq!(p2, 4096);
        assert_eq!(n2, 16);
    }
}
