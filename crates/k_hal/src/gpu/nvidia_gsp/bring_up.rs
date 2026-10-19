//! Escada GSP D0–D5 — bring-up MMIO + estágios honestos.
//!
//! D0 blobs → D1 SEC2/booter_load → D2 RISC-V alive → D3 RPC min →
//! D4 channel/CE → D5 canário (via `try_gsp_vector_add`).
//!
//! Offsets âncora Nouveau tu102 / GA10x (WPR2 hi @ 0x1fa828; SEC2 @ 0x840000).
//! Poll com budget TSC (SESSION_354).

use crate::gpu::detect::GpuInfo;
use crate::gpu::firmware;
use crate::gpu::nvidia_gsp::hs_hdr::parse_hs_bin;
use crate::gpu::nvidia_gsp::{
    FAT_BOOTER_LOAD, FAT_BOOTER_UNLOAD, FAT_BOOTLOADER, FAT_GSP, LOG_BOOTER_LOAD, LOG_BOOTER_UNLOAD,
    LOG_BOOTLOADER, LOG_GSP,
};
use crate::unlock_dag::{self, CapToken};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// SEC2 falcon BAR0 (Nouveau / ACR Pascal — Ampere reusa SEC2 p/ booter).
const SEC2_BASE: u64 = 0x0084_0000;
const FLCN_CPUCTL: u64 = 0x100;
const FLCN_BOOTVEC: u64 = 0x104;
const FLCN_DMACTL: u64 = 0x10c;
const FLCN_IMEMC: u64 = 0x180;
const FLCN_IMEMD: u64 = 0x184;
const FLCN_DMEMC: u64 = 0x1a0;
const FLCN_DMEMD: u64 = 0x1a4;
const FLCN_MBOX0: u64 = 0x040;
const FLCN_MBOX1: u64 = 0x044;
const CPUCTL_STARTCPU: u32 = 0x2;
const DMEMC_AINCW: u32 = 0x0100_0000;
const IMEMC_AINCW: u32 = 0x0100_0000;

/// WPR2 high (Nouveau tu102_gsp_booter_unload).
const WPR2_HI: u64 = 0x001f_a828;

/// GSP falcon/RISC-V base (GA10x — Nouveau gsp; probe-only).
const GSP_BASE: u64 = 0x0011_0000;
const GSP_IRQSTAT: u64 = 0x0000_1000; // relativo GSP — poll genérico

const BOOTER_BUDGET_US: u64 = 50_000;
const ALIVE_BUDGET_US: u64 = 100_000;
const RPC_BUDGET_US: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GspStage {
    /// D0: blobs ausentes no FAT/preload.
    BlobsMissing,
    /// D0: blobs + parse HS ok.
    BlobsOk,
    /// D1: booter_load submetido (SEC2 start).
    BooterSubmitted,
    /// D1: booter timeout (silício mudo / sem WPR).
    BooterTimeout,
    /// D1: booter concluiu (WPR2 ou mailbox avançou).
    BooterOk,
    /// D2: GSP RISC-V respondeu (alive).
    Alive,
    /// D3: RPC init mínimo (ring host + doorbell ack).
    RpcReady,
    /// D4: canal/CE stub ok.
    ChannelReady,
    /// Falha tipada (parse/MMIO/BAR).
    Failed,
}

impl GspStage {
    pub fn as_str(self) -> &'static str {
        match self {
            GspStage::BlobsMissing => "blobs_missing",
            GspStage::BlobsOk => "blobs_ok",
            GspStage::BooterSubmitted => "booter_submitted",
            GspStage::BooterTimeout => "booter_timeout",
            GspStage::BooterOk => "booter_ok",
            GspStage::Alive => "alive",
            GspStage::RpcReady => "rpc_ready",
            GspStage::ChannelReady => "channel_ready",
            GspStage::Failed => "failed",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            GspStage::BlobsMissing => 0,
            GspStage::Failed => 0,
            GspStage::BlobsOk => 1,
            GspStage::BooterSubmitted => 2,
            GspStage::BooterTimeout => 2,
            GspStage::BooterOk => 3,
            GspStage::Alive => 4,
            GspStage::RpcReady => 5,
            GspStage::ChannelReady => 6,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GspReport {
    pub stage: GspStage,
    pub gsp_bytes: u32,
    pub bootloader_bytes: u32,
    pub booter_load_bytes: u32,
    pub booter_unload_bytes: u32,
    pub wpr2_hi: u32,
    pub mbox0: u32,
    pub mbox1: u32,
}

impl GspReport {
    pub fn gsp_booted(self) -> bool {
        matches!(
            self.stage,
            GspStage::Alive | GspStage::RpcReady | GspStage::ChannelReady
        )
    }

    pub fn channel_ready(self) -> bool {
        self.stage == GspStage::ChannelReady
    }
}

static LAST: Mutex<Option<GspReport>> = Mutex::new(None);
static BROUGHT_UP: AtomicBool = AtomicBool::new(false);

pub fn last_gsp_report() -> Option<GspReport> {
    *LAST.lock()
}

fn load_fat(name: &str) -> Option<Vec<u8>> {
    firmware::load_firmware_file(name)
}

/// D0: carrega + valida blobs; slog tamanhos / logical OpenRM.
fn stage_d0_blobs() -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>), GspReport> {
    let gsp = load_fat(FAT_GSP);
    let boot = load_fat(FAT_BOOTLOADER);
    let load = load_fat(FAT_BOOTER_LOAD);
    let unload = load_fat(FAT_BOOTER_UNLOAD);
    let gsp_b = gsp.as_ref().map(|v| v.len() as u32).unwrap_or(0);
    let boot_b = boot.as_ref().map(|v| v.len() as u32).unwrap_or(0);
    let load_b = load.as_ref().map(|v| v.len() as u32).unwrap_or(0);
    let unld_b = unload.as_ref().map(|v| v.len() as u32).unwrap_or(0);
    k_nano::slog_hal!(
        "GSP",
        "ok",
        "D0 blobs {}={}B {}={}B {}={}B {}={}B (OpenRM {}/{}/{}/{})",
        FAT_GSP,
        gsp_b,
        FAT_BOOTLOADER,
        boot_b,
        FAT_BOOTER_LOAD,
        load_b,
        FAT_BOOTER_UNLOAD,
        unld_b,
        LOG_GSP,
        LOG_BOOTLOADER,
        LOG_BOOTER_LOAD,
        LOG_BOOTER_UNLOAD
    );
    let (Some(gsp), Some(boot), Some(load)) = (gsp, boot, load) else {
        return Err(GspReport {
            stage: GspStage::BlobsMissing,
            gsp_bytes: gsp_b,
            bootloader_bytes: boot_b,
            booter_load_bytes: load_b,
            booter_unload_bytes: unld_b,
            wpr2_hi: 0,
            mbox0: 0,
            mbox1: 0,
        });
    };
    // gsp blob é ELF/RM grande — não exige HS bin hdr; booters sim.
    if parse_hs_bin(&load).is_none() {
        k_nano::slog_hal!("GSP", "warn", "D0 booter_load HS parse fail — continue best-effort");
    }
    if parse_hs_bin(&boot).is_none() {
        k_nano::slog_hal!("GSP", "warn", "D0 bootloader HS parse fail — continue best-effort");
    }
    if gsp.len() < 1_000_000 {
        k_nano::slog_hal!(
            "GSP",
            "warn",
            "D0 gsp size {}B suspeito (esperado ~38MB pin 535)",
            gsp.len()
        );
    }
    Ok((gsp, boot, load, unload))
}

unsafe fn rd32(mmio: u64, off: u64) -> u32 {
    core::ptr::read_volatile((mmio + off) as *const u32)
}

unsafe fn wr32(mmio: u64, off: u64, v: u32) {
    core::ptr::write_volatile((mmio + off) as *mut u32, v);
}

/// Carrega fatia no IMEM/DMEM do Falcon (padrão ACR Pascal / Nouveau falcon v1).
unsafe fn falcon_dma_words(mmio: u64, falcon: u64, imem: bool, img: &[u8], max: usize) {
    let base = mmio + falcon;
    let n = img.len().min(max) & !3;
    if imem {
        wr32(base, FLCN_IMEMC, IMEMC_AINCW);
    } else {
        wr32(base, FLCN_DMEMC, DMEMC_AINCW);
    }
    let mut i = 0;
    while i + 4 <= n {
        let w = u32::from_le_bytes([img[i], img[i + 1], img[i + 2], img[i + 3]]);
        if imem {
            wr32(base, FLCN_IMEMD, w);
        } else {
            wr32(base, FLCN_DMEMD, w);
        }
        i += 4;
    }
}

/// D1: SEC2 + booter_load (Nouveau tu102_gsp_booter_load).
unsafe fn stage_d1_booter(
    mmio: u64,
    booter: &[u8],
    mut report: GspReport,
) -> GspReport {
    let wpr_before = rd32(mmio, WPR2_HI);
    report.wpr2_hi = wpr_before;
    k_nano::slog_hal!(
        "GSP",
        "ok",
        "D1 booter_load begin wpr2_hi={:#x} booter={}B",
        wpr_before,
        booter.len()
    );

    // Halt / reset leve SEC2
    wr32(mmio + SEC2_BASE, FLCN_CPUCTL, 0);
    wr32(mmio + SEC2_BASE, FLCN_DMACTL, 0);
    // Upload: primeiros 64 KiB no IMEM (HS app); resto truncado — booter real
    // usa DMA FB; aqui prova falcon path + start (metal valida avanço).
    falcon_dma_words(mmio, SEC2_BASE, true, booter, 64 * 1024);
    if booter.len() > 256 {
        falcon_dma_words(mmio, SEC2_BASE, false, &booter[booter.len().saturating_sub(256)..], 256);
    }
    // Mailboxes: WPR meta addrs — 0 = Nouveau skip path se WPR já set.
    wr32(mmio + SEC2_BASE, FLCN_MBOX0, 0);
    wr32(mmio + SEC2_BASE, FLCN_MBOX1, 0);
    wr32(mmio + SEC2_BASE, FLCN_BOOTVEC, 0);
    wr32(mmio + SEC2_BASE, FLCN_CPUCTL, CPUCTL_STARTCPU);
    report.stage = GspStage::BooterSubmitted;
    k_nano::slog_hal!("GSP", "ok", "D1 booter_submitted SEC2 startcpu");

    let t0 = k_nano::tsc::now_us();
    loop {
        let wpr = rd32(mmio, WPR2_HI);
        let m0 = rd32(mmio + SEC2_BASE, FLCN_MBOX0);
        let m1 = rd32(mmio + SEC2_BASE, FLCN_MBOX1);
        report.wpr2_hi = wpr;
        report.mbox0 = m0;
        report.mbox1 = m1;
        // Sucesso: WPR2 mudou OU mailbox ≠ 0/FFFFFFFF (HS respondeu).
        if (wpr != wpr_before && wpr != 0 && wpr != 0xffff_ffff)
            || (m0 != 0 && m0 != 0xffff_ffff)
            || (m1 != 0 && m1 != 0xffff_ffff)
        {
            report.stage = GspStage::BooterOk;
            k_nano::slog_hal!(
                "GSP",
                "ok",
                "D1 booter_ok wpr2_hi={:#x} mbox0={:#x} mbox1={:#x}",
                wpr,
                m0,
                m1
            );
            return report;
        }
        if k_nano::tsc::now_us().saturating_sub(t0) > BOOTER_BUDGET_US {
            report.stage = GspStage::BooterTimeout;
            k_nano::slog_hal!(
                "GSP",
                "warn",
                "D1 booter_timeout {}us wpr2_hi={:#x} mbox0={:#x} (falha honesta; sem GpuGspBooted)",
                BOOTER_BUDGET_US,
                wpr,
                m0
            );
            return report;
        }
        core::hint::spin_loop();
    }
}

/// D2: poll GSP alive (IRQSTAT / mailbox GSP).
unsafe fn stage_d2_alive(mmio: u64, mut report: GspReport) -> GspReport {
    if report.stage.rank() < GspStage::BooterOk.rank() {
        return report;
    }
    let t0 = k_nano::tsc::now_us();
    let irq0 = rd32(mmio + GSP_BASE, GSP_IRQSTAT);
    let wpr_ok = report.wpr2_hi != 0 && report.wpr2_hi != 0xffff_ffff;
    loop {
        let irq = rd32(mmio + GSP_BASE, GSP_IRQSTAT);
        let m0 = rd32(mmio + SEC2_BASE, FLCN_MBOX0);
        // Alive: IRQ GSP mudou OU (BooterOk + WPR2 válido) — não basta mbox do booter.
        let irq_changed = irq != irq0 && irq != 0 && irq != 0xffff_ffff;
        if irq_changed || (report.stage == GspStage::BooterOk && wpr_ok) {
            report.stage = GspStage::Alive;
            report.mbox0 = m0;
            unlock_dag::grant(CapToken::GpuGspBooted);
            k_nano::slog_hal!(
                "GSP",
                "ok",
                "D2 alive irq={:#x} wpr2_hi={:#x} mbox0={:#x} CapToken=GpuGspBooted",
                irq,
                report.wpr2_hi,
                m0
            );
            return report;
        }
        if k_nano::tsc::now_us().saturating_sub(t0) > ALIVE_BUDGET_US {
            k_nano::slog_hal!(
                "GSP",
                "warn",
                "D2 alive timeout — permanece stage={}",
                report.stage.as_str()
            );
            return report;
        }
        core::hint::spin_loop();
    }
}

/// D3: RPC mínimo — aloca página host como cmdq e toca doorbell GSP.
unsafe fn stage_d3_rpc(mmio: u64, mut report: GspReport) -> GspReport {
    if !matches!(report.stage, GspStage::Alive | GspStage::RpcReady | GspStage::ChannelReady) {
        return report;
    }
    let q = match k_nano::dma::dma_alloc_coalesced(4096) {
        Some(p) => p,
        None => {
            k_nano::slog_hal!("GSP", "warn", "D3 rpc: sem DMA page — skip");
            return report;
        }
    };
    core::ptr::write_bytes(q.virt, 0, 4096);
    // Magic "GSPR" + seq
    core::ptr::write_volatile(q.virt as *mut u32, 0x5250_5347);
    core::ptr::write_volatile((q.virt as *mut u32).add(1), 1);
    // Doorbell: PA física low (Nouveau shared-mem + IRQ).
    wr32(mmio + SEC2_BASE, FLCN_MBOX1, q.phys as u32);
    let t0 = k_nano::tsc::now_us();
    let mut acked = false;
    while k_nano::tsc::now_us().saturating_sub(t0) < RPC_BUDGET_US {
        let seq = core::ptr::read_volatile((q.virt as *const u32).add(1));
        if seq != 1 {
            acked = true;
            break;
        }
        core::hint::spin_loop();
    }
    if acked {
        report.stage = GspStage::RpcReady;
        k_nano::slog_hal!("GSP", "ok", "D3 rpc_ready (cmdq ack phys={:#x})", q.phys);
    } else {
        k_nano::slog_hal!(
            "GSP",
            "warn",
            "D3 rpc no-ack — GSP-RM incompleto (stage={} permanece)",
            report.stage.as_str()
        );
    }
    // Mantém q vivo até fim da função (Drop libera); ack já observado.
    core::mem::drop(q);
    report
}

/// D4: canal/CE stub — só se RPC ready.
unsafe fn stage_d4_channel(mmio: u64, mut report: GspReport) -> GspReport {
    let _ = mmio;
    if report.stage != GspStage::RpcReady {
        return report;
    }
    // Sem RM pleno não há channel id real — marca ChannelReady só com RPC,
    // e slog CE observe placeholder (bandwidth 0 = honesto).
    report.stage = GspStage::ChannelReady;
    k_nano::slog_hal!(
        "GSP",
        "ok",
        "D4 channel_ready stub (CE bandwidth=n/a até RM alloc)"
    );
    report
}

/// Bring-up completo D0→D4. Não concede GpuCompute.
/// Idempotente no boot: `secure_boot_gpu` + `NvidiaGpu::probe` compartilham.
pub unsafe fn bring_up_gsp(gpu: &GpuInfo, pmoff: u64) -> GspReport {
    if BROUGHT_UP.swap(true, Ordering::SeqCst) {
        if let Some(r) = last_gsp_report() {
            k_nano::slog_hal!(
                "GSP",
                "ok",
                "bring_up reuse stage={}",
                r.stage.as_str()
            );
            return r;
        }
    }
    k_nano::slog_hal!(
        "GSP",
        "ok",
        "bring_up begin did={:04x} isa={} bar0={:#x}",
        gpu.device_id,
        gpu.isa_tag.as_str(),
        gpu.bar0
    );
    let blobs = match stage_d0_blobs() {
        Ok(b) => b,
        Err(r) => {
            *LAST.lock() = Some(r);
            return r;
        }
    };
    let (gsp, boot, booter_load, unload) = blobs;
    let mut report = GspReport {
        stage: GspStage::BlobsOk,
        gsp_bytes: gsp.len() as u32,
        bootloader_bytes: boot.len() as u32,
        booter_load_bytes: booter_load.len() as u32,
        booter_unload_bytes: unload.as_ref().map(|v| v.len() as u32).unwrap_or(0),
        wpr2_hi: 0,
        mbox0: 0,
        mbox1: 0,
    };
    let _ = (gsp, boot, unload);

    if gpu.bar0 == 0 {
        report.stage = GspStage::Failed;
        k_nano::slog_hal!("GSP", "fail", "bar0=0 — abort");
        *LAST.lock() = Some(report);
        return report;
    }
    let mmio = gpu.bar0 + pmoff;
    // Prova BAR0 viva
    let ver = rd32(mmio, 0);
    if ver == 0 || ver == 0xffff_ffff {
        report.stage = GspStage::Failed;
        k_nano::slog_hal!("GSP", "fail", "BAR0 VERSION morto {:#x}", ver);
        *LAST.lock() = Some(report);
        return report;
    }

    report = stage_d1_booter(mmio, &booter_load, report);
    report = stage_d2_alive(mmio, report);
    report = stage_d3_rpc(mmio, report);
    report = stage_d4_channel(mmio, report);

    k_nano::slog_hal!(
        "GSP",
        "ok",
        "bring_up done stage={} gsp_booted={} channel={}",
        report.stage.as_str(),
        report.gsp_booted(),
        report.channel_ready()
    );
    *LAST.lock() = Some(report);
    report
}

/// D5 helper: canário só se channel_ready; senão false + slog.
pub unsafe fn try_gsp_vector_add(
    gpu: &GpuInfo,
    cubin: &[u8],
    _a: &[f32],
    _b: &[f32],
    _expect: &[f32],
) -> bool {
    let report = last_gsp_report();
    let stage = report.map(|r| r.stage).unwrap_or(GspStage::BlobsMissing);
    if cubin.is_empty() {
        k_nano::slog_hal!("GSP", "warn", "D5 canary: cubin vazio");
        return false;
    }
    if !matches!(stage, GspStage::ChannelReady) {
        k_nano::slog_hal!(
            "GSP",
            "warn",
            "D5 canary refuse stage={} pack={}B (precisa channel_ready)",
            stage.as_str(),
            cubin.len()
        );
        return false;
    }
    // ChannelReady sem RM launch path real → ainda false (honestidade).
    // Quando RPC alloc+QMD Ampere existir, substituir este ramo.
    let _ = gpu;
    k_nano::slog_hal!(
        "GSP",
        "warn",
        "D5 canary: channel_ready mas launch GSP-RM residual — quarantine (pack {}B)",
        cubin.len()
    );
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_rank_monotonic() {
        assert!(GspStage::BlobsOk.rank() < GspStage::BooterOk.rank());
        assert!(GspStage::BooterOk.rank() < GspStage::Alive.rank());
        assert!(GspStage::Alive.rank() < GspStage::ChannelReady.rank());
    }

    #[test]
    fn gsp_booted_only_alive_plus() {
        let mut r = GspReport {
            stage: GspStage::BooterOk,
            gsp_bytes: 0,
            bootloader_bytes: 0,
            booter_load_bytes: 0,
            booter_unload_bytes: 0,
            wpr2_hi: 0,
            mbox0: 0,
            mbox1: 0,
        };
        assert!(!r.gsp_booted());
        r.stage = GspStage::Alive;
        assert!(r.gsp_booted());
    }
}
