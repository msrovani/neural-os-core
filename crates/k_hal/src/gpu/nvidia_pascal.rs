//! Degrau 2–4 Pascal (GP107/GP108) — GMMU + Channel + Runlist + QMD/fence.
//!
//! Contrato com o AIOS:
//! - K-Nano aloca páginas físicas UC (DMA).
//! - Este módulo monta PTE IOVA→phys, canal (instance/RAMFC/GPFIFO/USERD),
//!   runlist/kick (D3) e QMD v01_07 + semaphore (D4).
//!
//! Honestidade:
//! - `StructuresReady` ≠ engine GR pronto ≠ `has_compute`.
//! - `RunlistSubmitted` ≠ execução.
//! - D4 só retorna true se fence + golden; sem ACR/GR/CUBIN real → timeout → false.
//!
//! Offsets (públicos, Nouveau `nvkm/engine/fifo/gk104.c`, reused gp100):
//! - RUNLIST_BASE   = 0x002270  ((addr>>12) | (target<<28)); target=3 = SYS_NCOH
//! - RUNLIST_SUBMIT = 0x002274  ((runl<<20) | nr)
//! - RUNLIST_STATUS = 0x002284 + runl*8  (bit 0x00100000 = pending)
//! - CHANNEL        = 0x800000 + chid*8  (0x80000000 | inst>>12)
//! - CHANNEL_CTRL   = 0x800004 + chid*8  (bit 0x400 = ENABLE)
//! - KICK           = 0x002634  (write chid; poll bit 0x00100000)

use crate::gpu::compute_abi::{vector_add_check, VectorAddParams};
use crate::gpu::detect::GpuInfo;
use crate::gpu::firmware;
use crate::gpu::nvidia_pascal_qmd::{self, QmdLaunch, QMD_SIZE};
use crate::gpu::nvidia_pascal_sw::SwStatus;
use k_nano::dma::{dma_alloc_coalesced, DmaBuf};

/// Classe canal GPFIFO Pascal (`clc06f.h`).
pub const PASCAL_CHANNEL_GPFIFO_A: u32 = 0xC06F;
/// Classe compute GP102+ (`PASCAL_COMPUTE_B`).
pub const PASCAL_COMPUTE_B: u32 = 0xC1C0;

/// Janela IOVA reservada (4 KiB × 512 = 2 MiB).
const IOVA_BASE: u64 = 0x0001_0000_0000;
const PT_ENTRIES: usize = 512;
const GPFIFO_ENTRIES: usize = 32;

// PFIFO host registers (BAR0), família gk104→gp10x.
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

/// GR runlist id (runl 0 = GR/compute em gk104→gp10x).
const RUNL_GR: u32 = 0;
/// Canal único do AIOS.
const CHID: u32 = 0;

// Pushbuffer method types.
const SUBCH_COMPUTE: u32 = 1;
const METHOD_SET_OBJECT: u32 = 0x0000;

/// Estado Degrau 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PascalD2Status {
    StructuresReady,
    WaitingSwCtx,
    Failed,
}

/// Estado Degrau 3 (sistema nervoso).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PascalD3Status {
    NotStarted,
    /// Pushbuffer construído + mapeado + GP entry + GPPut.
    PushbufferReady,
    /// Runlist commit MMIO sem fault observável (não prova silício).
    RunlistSubmitted,
    /// Poll de runlist/kick estourou timeout (não-fatal).
    RunlistTimeout,
    Failed,
}

/// Estado Degrau 4 (QMD + fence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PascalD4Status {
    NotStarted,
    /// QMD+buffers montados; dispatch submetido.
    Dispatched,
    /// Fence timeout (esperado sem ACR/GR/CUBIN real).
    FenceTimeout,
    /// Fence ok mas golden falhou.
    GoldenMismatch,
    /// Fence + golden OK — único caminho para has_compute.
    GoldenPass,
    Failed,
}

/// Contexto Pascal vivo (mantém DmaBufs para não liberar frames).
pub struct PascalD2 {
    pub status: PascalD2Status,
    pub d3: PascalD3Status,
    pub d4: PascalD4Status,
    pub pd: DmaBuf,
    pub pt: DmaBuf,
    pub inst: DmaBuf,
    pub gpfifo: DmaBuf,
    pub userd: DmaBuf,
    pub pushbuffer: Option<DmaBuf>,
    pub runlist: Option<DmaBuf>,
    pub iova_base: u64,
    pub gpfifo_iova: u64,
    pub pushbuffer_iova: u64,
    pub mapped_pages: u32,
    pub sw_ctx_present: bool,
    pub bus_master: bool,
    /// Índice próximo livre no GPFIFO (após D3 = 1).
    pub gpfifo_put: u32,
}

impl PascalD2 {
    pub fn channel_structures_ok(&self) -> bool {
        matches!(
            self.status,
            PascalD2Status::StructuresReady | PascalD2Status::WaitingSwCtx
        )
    }

    pub fn nervous_system_ok(&self) -> bool {
        matches!(
            self.d3,
            PascalD3Status::PushbufferReady
                | PascalD3Status::RunlistSubmitted
                | PascalD3Status::RunlistTimeout
        )
    }

    pub fn d4_golden_pass(&self) -> bool {
        self.d4 == PascalD4Status::GoldenPass
    }

    pub fn gpfifo_phys(&self) -> u64 {
        self.gpfifo.phys
    }
    pub fn userd_phys(&self) -> u64 {
        self.userd.phys
    }
    pub fn pd_phys(&self) -> u64 {
        self.pd.phys
    }
}

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

/// GPFIFO entry (8 bytes): ENTRY0 GET=addr>>2 [31:2]; ENTRY1 LENGTH [30:10].
/// `pb_addr` é endereço **virtual do canal** (IOVA), não phys.
fn write_gpfifo_entry(entries: *mut u64, index: usize, pb_addr: u64, length_dwords: u32) {
    let e0 = (pb_addr >> 2) << 2;
    let e1 = ((length_dwords as u64) & 0x1F_FFFF) << 10;
    unsafe {
        core::ptr::write_volatile(entries.add(index), e0 | (e1 << 32));
    }
}

/// Header de método do pushbuffer.
/// `typ`: 1=incrementing, 2=non-incrementing.
fn pb_method(typ: u32, subch: u32, mthd: u32, count: u32) -> u32 {
    (typ << 28) | (count << 16) | (subch << 13) | (mthd >> 2)
}

fn sw_firmware_present() -> bool {
    firmware::has_named_blob("sw_ctx.bin")
        && firmware::has_named_blob("sw_bundle_init.bin")
        && firmware::has_named_blob("sw_method_init.bin")
        && firmware::has_named_blob("sw_nonctx.bin")
}

/// True se nonctx foi aplicado ou explicitamente ausente/skip (não WaitingSwCtx).
fn sw_nonctx_satisfied() -> bool {
    match firmware::last_sw_report() {
        Some(r) => matches!(
            r.status,
            SwStatus::NonctxApplied | SwStatus::PresentNotApplied | SwStatus::BlobsMissing
        ),
        None => !firmware::has_named_blob("sw_nonctx.bin"),
    }
}

/// RAMFC mínimo: GP_BASE = gpfifo **IOVA**; USERD phys; PD pointer.
unsafe fn fill_ramfc(inst: &DmaBuf, gpfifo_iova: u64, userd: &DmaBuf, pd: &DmaBuf) {
    let base = inst.virt as *mut u32;
    core::ptr::write_volatile(base.add(0x08 / 4), (gpfifo_iova & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x0C / 4), (gpfifo_iova >> 32) as u32);
    core::ptr::write_volatile(base.add(0x20 / 4), (userd.phys & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x24 / 4), (userd.phys >> 32) as u32);
    core::ptr::write_volatile(base.add(0x200 / 4), (pd.phys & 0xFFFF_FFFF) as u32);
    core::ptr::write_volatile(base.add(0x204 / 4), (pd.phys >> 32) as u32);
    core::ptr::write_volatile(base.add(0x40 / 4), PASCAL_CHANNEL_GPFIFO_A);
}

/// USERD (`Nvc06fControl`): zera Put/Get/GPGet/GPPut.
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
    core::ptr::write_volatile(p.add(0x8C / 4), v);
}

/// Degrau 2 — GMMU + estrutura de canal.
pub unsafe fn bring_up_d2(gpu: &GpuInfo, _mmio: u64) -> Option<PascalD2> {
    k_nano::slog_hal!("NVIDIA", "D2", "{}: GMMU+Channel bring-up (PASCAL_CHANNEL_GPFIFO_A={:#x})",
        gpu.name,
        PASCAL_CHANNEL_GPFIFO_A);

    k_nano::pci::enable_pci_bus_master_unsafe(gpu.pci_bus, gpu.pci_dev, gpu.pci_fn);
    let bus_master = true;

    let pd = dma_alloc_coalesced(4096)?;
    let pt = dma_alloc_coalesced(PT_ENTRIES * 8)?;
    let inst = dma_alloc_coalesced(4096)?;
    let gpfifo = dma_alloc_coalesced(GPFIFO_ENTRIES * 8)?;
    let userd = dma_alloc_coalesced(4096)?;

    let pde = encode_pde_pt(pt.phys);
    core::ptr::write_volatile(pd.virt as *mut u64, pde);

    // Mapeia estruturas na PT; guarda IOVA do gpfifo (primeira entrada).
    let mut mapped = 0u32;
    let gpfifo_iova = IOVA_BASE; // gpfifo mapeado primeiro
    let to_map = [
        (gpfifo.phys, gpfifo.size),
        (userd.phys, userd.size),
        (inst.phys, inst.size),
        (pd.phys, pd.size),
        (pt.phys, pt.size),
    ];
    for (phys, size) in to_map {
        let pages = (size + 4095) / 4096;
        for i in 0..pages {
            if mapped as usize >= PT_ENTRIES {
                break;
            }
            let pa = phys + (i as u64) * 4096;
            let pte = encode_pte_sys(pa);
            core::ptr::write_volatile((pt.virt as *mut u64).add(mapped as usize), pte);
            mapped += 1;
        }
    }

    fill_ramfc(&inst, gpfifo_iova, &userd, &pd);
    init_userd(&userd);

    let sw_blobs = sw_firmware_present();
    let sw_ok = sw_nonctx_satisfied();
    let status = if sw_ok {
        PascalD2Status::StructuresReady
    } else {
        PascalD2Status::WaitingSwCtx
    };
    if let Some(acr) = firmware::last_acr_report() {
        k_nano::slog_hal!("NVIDIA", "D2", "acr_stage={:?} wpr={:#x}..{:#x}", acr.stage, acr.wpr_start, acr.wpr_end);
    }
    if let Some(sw) = firmware::last_sw_report() {
        k_nano::slog_hal!("NVIDIA", "D2", "sw_status={:?} nonctx_pairs={}", sw.status, sw.nonctx_pairs);
    }

    k_nano::slog_hal!("NVIDIA", "D2", "pd={:#x} pt={:#x} inst={:#x} gpfifo={:#x}(iova {:#x}) userd={:#x}",
        pd.phys, pt.phys, inst.phys, gpfifo.phys, gpfifo_iova, userd.phys);
    k_nano::slog_hal!("NVIDIA", "D2", "mapped_pages={} sw_blobs={} sw_ok={} status={:?} bus_master={}", mapped, sw_blobs, sw_ok, status, bus_master);

    Some(PascalD2 {
        status,
        d3: PascalD3Status::NotStarted,
        d4: PascalD4Status::NotStarted,
        pd,
        pt,
        inst,
        gpfifo,
        userd,
        pushbuffer: None,
        runlist: None,
        iova_base: IOVA_BASE,
        gpfifo_iova,
        pushbuffer_iova: 0,
        mapped_pages: mapped,
        sw_ctx_present: sw_blobs,
        bus_master,
        gpfifo_put: 0,
    })
}

/// Mapeia páginas físicas extras na PT. Retorna IOVA inicial.
pub unsafe fn map_sys_pages(d2: &mut PascalD2, phys: u64, n_pages: u32) -> Option<u64> {
    if !d2.channel_structures_ok() || n_pages == 0 {
        return None;
    }
    let start_idx = d2.mapped_pages as usize;
    if start_idx + n_pages as usize > PT_ENTRIES {
        return None;
    }
    let iova0 = d2.iova_base + (start_idx as u64) * 4096;
    for i in 0..n_pages as usize {
        let pa = phys + (i as u64) * 4096;
        let pte = encode_pte_sys(pa);
        core::ptr::write_volatile((d2.pt.virt as *mut u64).add(start_idx + i), pte);
    }
    d2.mapped_pages += n_pages;
    Some(iova0)
}

/// Poll bounded de um registrador MMIO até `(val & mask)==0`. Retorna true se limpou.
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

/// Degrau 3 — pushbuffer + runlist + bind/kick.
///
/// Constrói um pushbuffer trivial (SET_OBJECT PASCAL_COMPUTE_B), o mapeia na GMMU,
/// escreve a GP entry, monta a runlist do canal e faz o bind/kick via MMIO PFIFO.
/// Sem HW/QEMU não há prova de execução — status honesto.
pub unsafe fn bring_up_d3(d2: &mut PascalD2, mmio: u64) -> PascalD3Status {
    if !d2.channel_structures_ok() {
        d2.d3 = PascalD3Status::Failed;
        return d2.d3;
    }

    // 1. Pushbuffer: SET_OBJECT (subch compute) = PASCAL_COMPUTE_B.
    let pb = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d3 = PascalD3Status::Failed;
            return d2.d3;
        }
    };
    let pbw = pb.virt as *mut u32;
    core::ptr::write_volatile(pbw.add(0), pb_method(1, SUBCH_COMPUTE, METHOD_SET_OBJECT, 1));
    core::ptr::write_volatile(pbw.add(1), PASCAL_COMPUTE_B);
    let pb_len_dwords = 2u32;

    // 2. Mapeia pushbuffer na GMMU → IOVA do canal.
    let pb_iova = match map_sys_pages(d2, pb.phys, 1) {
        Some(v) => v,
        None => {
            d2.d3 = PascalD3Status::Failed;
            return d2.d3;
        }
    };
    d2.pushbuffer_iova = pb_iova;

    // 3. GP entry[0] → pushbuffer IOVA; GPPut=1.
    write_gpfifo_entry(d2.gpfifo.virt as *mut u64, 0, pb_iova, pb_len_dwords);
    userd_set_gpput(&d2.userd, 1);

    // 4. Runlist: entrada única [chid, 0].
    let rl = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.pushbuffer = Some(pb);
            d2.d3 = PascalD3Status::Failed;
            return d2.d3;
        }
    };
    let rlw = rl.virt as *mut u32;
    core::ptr::write_volatile(rlw.add(0), CHID);
    core::ptr::write_volatile(rlw.add(1), 0);
    let nr = 1u32;

    k_nano::slog_hal!("NVIDIA", "D3", "pushbuffer iova={:#x} len={} gpfifo GPPut=1; runlist={:#x} nr={}", pb_iova, pb_len_dwords, rl.phys, nr);

    // 5. Bind canal (0x800000+coff) + ENABLE (0x800004+coff).
    let coff = (CHID as u64) * 8;
    let chan_val = 0x8000_0000u32 | ((d2.inst.phys >> 12) as u32);
    core::ptr::write_volatile((mmio + REG_CHANNEL + coff) as *mut u32, chan_val);
    let ctrl = core::ptr::read_volatile((mmio + REG_CHANNEL_CTRL + coff) as *const u32);
    core::ptr::write_volatile(
        (mmio + REG_CHANNEL_CTRL + coff) as *mut u32,
        ctrl | CHANNEL_ENABLE,
    );

    // 6. Commit runlist: BASE (addr>>12 | target<<28), SUBMIT (runl<<20 | nr).
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
    let base_val = (rl.phys >> 12) | (TARGET_SYS_NCOH << 28);
    core::ptr::write_volatile((mmio + REG_RUNLIST_BASE) as *mut u32, base_val as u32);
    core::ptr::write_volatile(
        (mmio + REG_RUNLIST_SUBMIT) as *mut u32,
        (RUNL_GR << 20) | nr,
    );

    let rl_ok = poll_clear(
        mmio,
        REG_RUNLIST_STATUS + (RUNL_GR as u64) * 8,
        RUNLIST_STATUS_PENDING,
        200_000,
    );

    // 7. Kick canal.
    core::ptr::write_volatile((mmio + REG_KICK) as *mut u32, CHID);
    let kick_ok = poll_clear(mmio, REG_KICK, KICK_PENDING, 200_000);

    d2.pushbuffer = Some(pb);
    d2.runlist = Some(rl);
    d2.gpfifo_put = 1;

    let status = if rl_ok && kick_ok {
        PascalD3Status::RunlistSubmitted
    } else {
        PascalD3Status::RunlistTimeout
    };
    d2.d3 = status;

    k_nano::slog_hal!("NVIDIA", "D3", "bind chid={} inst={:#x} runlist_commit={} kick={} status={:?}", CHID, d2.inst.phys, rl_ok, kick_ok, status);
    k_nano::slog_hal!("NVIDIA", "D3", "sistema nervoso montado (estrutural); QMD/fence={:#x} = Degrau 4", PASCAL_COMPUTE_B);
    status
}

// ─── Degrau 4: QMD + fence + golden ───────────────────────────────────────────

const METHOD_SET_PROGRAM_REGION_A: u32 = 0x1608;
const METHOD_SEND_PCAS_A: u32 = 0x02B4;
const METHOD_SEND_SIGNALING_PCAS_B: u32 = 0x02BC;
/// Invalidate + schedule (tinygrad / RESEARCH Pascal).
const PCAS_SIGNAL_SCHEDULE: u32 = 9;
const FENCE_PAYLOAD: u32 = 1;
const FENCE_SPINS: u32 = 100_000; // bounded — timeout honesto, não congela boot

fn is_cpu_stub_payload(cubin: &[u8]) -> bool {
    cubin.starts_with(b"CPU_VECTOR_ADD_STUB")
}

/// Degrau 4 — monta QMD v01_07, despacha via GPFIFO, poll fence, confere golden.
///
/// Retorna `true` **somente** se fence + `vector_add_check`. Sem ACR/GR/CUBIN
/// real o fence estoura → `false` (FailDispatch no canário). Não-fatal.
pub unsafe fn dispatch_vector_add(
    d2: &mut PascalD2,
    mmio: u64,
    cubin: &[u8],
    a: &[f32],
    b: &[f32],
    expect: &[f32],
) -> bool {
    if !d2.channel_structures_ok() || !d2.nervous_system_ok() {
        d2.d4 = PascalD4Status::Failed;
        k_nano::slog_hal!("NVIDIA", "D4", "abort: D2/D3 incompleto");
        return false;
    }
    if cubin.is_empty() || a.len() != b.len() || a.len() != expect.len() || a.is_empty() {
        d2.d4 = PascalD4Status::Failed;
        return false;
    }
    if is_cpu_stub_payload(cubin) {
        k_nano::slog_hal!("NVIDIA", "D4", "payload=CPU stub (sem SASS sm_61) — dispatch estrutural; golden exige CUBIN real");
    }

    let n = a.len();
    let code_pages = ((cubin.len() + 4095) / 4096).max(1);
    let code = match dma_alloc_coalesced(code_pages * 4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };
    core::ptr::copy_nonoverlapping(cubin.as_ptr(), code.virt as *mut u8, cubin.len());

    let cb = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };
    let qmd_buf = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };
    let fence = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };
    let vecs = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };
    let pb = match dma_alloc_coalesced(4096) {
        Some(b) => b,
        None => {
            d2.d4 = PascalD4Status::Failed;
            return false;
        }
    };

    // Layout em `vecs`: a[0..n], b[0..n], c[0..n] como f32.
    let a_off = 0usize;
    let b_off = n * 4;
    let c_off = n * 8;
    {
        let base = vecs.virt as *mut u8;
        for i in 0..n {
            core::ptr::write_volatile((base.add(a_off) as *mut f32).add(i), a[i]);
            core::ptr::write_volatile((base.add(b_off) as *mut f32).add(i), b[i]);
            core::ptr::write_volatile((base.add(c_off) as *mut f32).add(i), 0.0f32);
        }
    }
    core::ptr::write_volatile(fence.virt as *mut u32, 0);

    // Mapear tudo na GMMU.
    let Some(code_iova) = map_sys_pages(d2, code.phys, code_pages as u32) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };
    let Some(cb_iova) = map_sys_pages(d2, cb.phys, 1) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };
    let Some(qmd_iova) = map_sys_pages(d2, qmd_buf.phys, 1) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };
    let Some(fence_iova) = map_sys_pages(d2, fence.phys, 1) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };
    let Some(vecs_iova) = map_sys_pages(d2, vecs.phys, 1) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };
    let Some(pb_iova) = map_sys_pages(d2, pb.phys, 1) else {
        d2.d4 = PascalD4Status::Failed;
        return false;
    };

    // CB0 = VectorAddParams nvcc ABI (a*, b*, c*, n) — Labor 7.
    let params = VectorAddParams {
        a_pa: vecs_iova + a_off as u64,
        b_pa: vecs_iova + b_off as u64,
        c_pa: vecs_iova + c_off as u64,
        n: n as u32,
        _pad: 0,
    };
    core::ptr::write_volatile(cb.virt as *mut VectorAddParams, params);

    let launch = QmdLaunch::vector_add_canary(cb_iova, fence_iova);
    let qmd_bytes = nvidia_pascal_qmd::build_qmd_v01_07(&launch);
    core::ptr::copy_nonoverlapping(qmd_bytes.as_ptr(), qmd_buf.virt as *mut u8, QMD_SIZE);

    // Pushbuffer D4: SET_OBJECT + PROGRAM_REGION + SEND_PCAS.
    let pbw = pb.virt as *mut u32;
    let mut i = 0usize;
    core::ptr::write_volatile(pbw.add(i), pb_method(1, SUBCH_COMPUTE, METHOD_SET_OBJECT, 1));
    i += 1;
    core::ptr::write_volatile(pbw.add(i), PASCAL_COMPUTE_B);
    i += 1;
    core::ptr::write_volatile(
        pbw.add(i),
        pb_method(1, SUBCH_COMPUTE, METHOD_SET_PROGRAM_REGION_A, 2),
    );
    i += 1;
    core::ptr::write_volatile(pbw.add(i), (code_iova >> 32) as u32);
    i += 1;
    core::ptr::write_volatile(pbw.add(i), code_iova as u32);
    i += 1;
    // SET_PROGRAM_REGION_B is consecutive after A when count=2 — already covered.
    // SEND_PCAS_A
    core::ptr::write_volatile(pbw.add(i), pb_method(1, SUBCH_COMPUTE, METHOD_SEND_PCAS_A, 1));
    i += 1;
    core::ptr::write_volatile(pbw.add(i), (qmd_iova >> 8) as u32);
    i += 1;
    core::ptr::write_volatile(
        pbw.add(i),
        pb_method(1, SUBCH_COMPUTE, METHOD_SEND_SIGNALING_PCAS_B, 1),
    );
    i += 1;
    core::ptr::write_volatile(pbw.add(i), PCAS_SIGNAL_SCHEDULE);
    i += 1;
    let pb_len = i as u32;

    // GPFIFO entry + GPPut + kick.
    let put = d2.gpfifo_put as usize;
    if put >= GPFIFO_ENTRIES {
        d2.d4 = PascalD4Status::Failed;
        k_nano::slog_hal!("NVIDIA", "D4", "GPFIFO cheio");
        return false;
    }
    write_gpfifo_entry(d2.gpfifo.virt as *mut u64, put, pb_iova, pb_len);
    d2.gpfifo_put = put as u32 + 1;
    userd_set_gpput(&d2.userd, d2.gpfifo_put);
    core::ptr::write_volatile((mmio + REG_KICK) as *mut u32, CHID);

    d2.d4 = PascalD4Status::Dispatched;
    k_nano::slog_hal!("NVIDIA", "D4", "dispatched QMD iova={:#x} code={:#x} fence={:#x} pb_len={} stub={}",
        qmd_iova,
        code_iova,
        fence_iova,
        pb_len,
        is_cpu_stub_payload(cubin));
    k_nano::slog_bin!(
        "GPU-HW",
        "info",
        "step=golden status=dispatched fence={:#x} stub={}",
        fence_iova,
        is_cpu_stub_payload(cubin) as u8
    );

    // Poll fence (buffers ainda vivos).
    let mut hit = false;
    for _ in 0..FENCE_SPINS {
        let v = core::ptr::read_volatile(fence.virt as *const u32);
        if v == FENCE_PAYLOAD {
            hit = true;
            break;
        }
        core::hint::spin_loop();
    }

    if !hit {
        d2.d4 = PascalD4Status::FenceTimeout;
        k_nano::slog_hal!("NVIDIA", "D4", "fence timeout (esperado sem ACR/GR/CUBIN em QEMU) — sem has_compute");
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=fence_timeout"
        );
        let _keep = (code, cb, qmd_buf, fence, vecs, pb);
        return false;
    }

    // Ler C e comparar golden.
    let mut got = [0.0f32; 64];
    if n > got.len() {
        d2.d4 = PascalD4Status::Failed;
        let _keep = (code, cb, qmd_buf, fence, vecs, pb);
        return false;
    }
    let c_ptr = (vecs.virt as *const u8).add(c_off) as *const f32;
    for i in 0..n {
        got[i] = core::ptr::read_volatile(c_ptr.add(i));
    }
    let pass = vector_add_check(&got[..n], expect, 1e-5);
    let _keep = (code, cb, qmd_buf, fence, vecs, pb);

    if pass {
        d2.d4 = PascalD4Status::GoldenPass;
        k_nano::slog_hal!("NVIDIA", "D4", "GOLDEN PASS n={} — has_compute elegível", n);
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=OK n={} detail=fence_plus_check",
            n
        );
        true
    } else {
        d2.d4 = PascalD4Status::GoldenMismatch;
        k_nano::slog_hal!("NVIDIA", "D4", "fence ok mas golden mismatch");
        k_nano::slog_bin!(
            "GPU-HW",
            "info",
            "step=golden status=FAIL reason=golden_mismatch n={}",
            n
        );
        false
    }
}
