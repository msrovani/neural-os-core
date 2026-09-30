//! Intel VMD (Volume Management Device) — s422.
//!
//! O VMD é uma ponte host PCIe SECUNDÁRIA (domínio PCI separado): em notebooks
//! Alder Lake+ com "Intel RST/VMD" ativo no BIOS, o NVMe só existe atrás dele e
//! é INVISÍVEL ao scan CF8/CFC (`pci::scan_bus`) — o card mostrava
//! `8086:a77f sem driver` e o storage caía no USB-MSC. Referência canônica de
//! layout: `linux/drivers/pci/controller/vmd.c` (registradores/semântica; sem
//! código copiado).
//!
//! Contrato implementado (caminho NATIVO, honesto):
//! - CFGBAR (BAR0, 64-bit) = config space ECAM dos filhos (≥1MB → N buses).
//! - `busn_start` via VMCAP(0x40).BUS_RESTRICT + VMCONFIG(0x44)[9:8]: 0/128/224.
//! - ECAM offset = `(bus-busn_start)<<20 | dev<<15 | fn<<12 | reg` (dword).
//! - MMIO/DMA dos filhos: windows MEMBAR1 (BAR2)/MEMBAR2 (BAR4) pré-configurados
//!   pelo BIOS. Vendor-cap "SHDW" (vmd.c vmd_get_phys_offsets) dá
//!   offset = MEMBAR_cpu − host_phys; nativo ⇒ 0 ⇒ bus addr == host addr ⇒ o
//!   BAR do filho é phys CPU direto e o DMA é identidade.
//!   Offset != 0 = visão guest (s429): o BUS window do domínio começa em
//!   host_phys (bus = cpu − offset ⇒ cpu = bus + offset) e o BAR do filho é
//!   BUS addr DENTRO dessa janela — o acesso CPU ao MMIO do filho vai pela
//!   MEMBAR mapeada UC. DMA de RAM do filho segue UNTRANSLATED (identidade):
//!   o vmd.c aplica offset apenas a RECURSOS (pci_add_resource_offset) —
//!   nenhum offset em DMA (só dma_set_mask) — e o bus space de RAM sobe
//!   idêntico ao phys host. vmd_in_guest no vmd.c só exige MSI remap; nosso
//!   driver é polling. Sem SHDW≠0 + MEMBAR size válidos, abort honesto.
//! - MSI-X do VMD NÃO é usado (driver NVMe é polling).
//!
//! Fail-closed: qualquer passo divergente = slog + false/None, nunca panic.
//! VMD ausente é caso NORMAL (só notebooks Intel RST têm o device) — log info.

use crate::pci::PciDevice;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

/// DIDs do `vmd.c` do Linux (driver_data VMD_FEATS_CLIENT exceto os server
/// 201d/28c0/28c1 — o caminho nativo aqui é o mesmo para todos; 28c1 tem
/// USE_BIOS_INFO e é abortado com honestidade abaixo).
const VMD_DIDS: &[u16] = &[
    0x201d, 0x28c0, 0x28c1, 0x467f, 0x4c3d, 0xa77f, 0x7d0b, 0xad0b, 0x9a0b,
    0xb60b, 0xb06f, 0xb07f, 0xd70b, 0xd73b,
];

/// DID 28C1 (Arrow Lake) usa VMD_FEAT_USE_BIOS_INFO (busn_start/shadows no
/// BAR4) — contrato diferente, não suportado neste binder (fail-closed).
const DID_USE_BIOS_INFO: u16 = 0x28c1;

pub fn is_vmd(vid: u16, did: u16) -> bool {
    vid == 0x8086 && VMD_DIDS.contains(&did)
}

// Registradores do próprio VMD (config space domínio 0 — CF8/CFC; vmd.c §regs).
const VMCAP: u8 = 0x40;
const VMCONFIG: u8 = 0x44;
const BUS_RESTRICT_CAP_BIT: u16 = 0x1;
/// Magic "SHDW" da vendor-cap de shadow MEMBAR (vmd.c vmd_get_phys_offsets).
const SHDW_MAGIC: u32 = 0x5348_4457;

pub static VMD_ACTIVE: AtomicBool = AtomicBool::new(false);
static VMD_CFG_VA: AtomicU64 = AtomicU64::new(0);
static VMD_CFGBAR_BYTES: AtomicU64 = AtomicU64::new(0);
static VMD_BUSN_START: AtomicU32 = AtomicU32::new(0);
static VMD_CHILDREN: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());

pub fn vmd_active() -> bool {
    VMD_ACTIVE.load(Ordering::Acquire)
}

/// VMCONFIG[9:8] → primeiro bus do domínio VMD (vmd.c vmd_get_bus_number_start).
/// Sem BUS_RESTRICT_CAP → 0. Retorno None = setting desconhecido (abort honesto).
fn busn_start_from_regs(vmcap: u16, vmcfg: u16) -> Option<u8> {
    if vmcap & BUS_RESTRICT_CAP_BIT == 0 {
        return Some(0);
    }
    match (vmcfg >> 8) & 0x3 {
        0 => Some(0),
        1 => Some(128),
        2 | 3 => Some(224),
        _ => None,
    }
}

/// ECAM offset dentro do CFGBAR (vmd.c vmd_cfg_addr; PCIE_ECAM_OFFSET).
/// `reg` é truncado ao dword (leitor cfg_read32 faz o mesmo — nunca dobrar).
fn ecam_offset(bus_rel: u32, dev: u32, func: u32, reg: u32) -> u32 {
    (bus_rel << 20) | (dev << 15) | (func << 12) | (reg & 0xFFC)
}

/// Offset de tradução SHDW para UMA window: MEMBAR_cpu − host_phys.
/// Nativo ⇒ 0 (bus addr == host addr). Guest ⇒ != 0 (traduzido no DMA — s429).
fn shdw_delta(membar_cpu: u64, host_phys: u64) -> i64 {
    membar_cpu as i64 - (host_phys & !0xF) as i64
}

// ── Visão guest (s429) — semântica vmd.c/pci_add_resource_offset ──────────
// offset = MEMBAR_cpu − host_phys (shdw_delta). pci_add_resource_offset:
// bus = cpu − offset ⇒ cpu = bus + offset. Logo a JANELA BUS do domínio
// começa em host_phys (bus window = [host_phys, +size)) e a CPU window começa
// em MEMBAR_cpu. BAR do filho (bus addr B na janela) é acessado pela CPU em
// B + offset (dentro da MEMBAR, mapeada UC no init). DMA de RAM do filho:
// UNTRANSLATED/identidade (vmd.c não aplica offset a DMA nenhum).
struct MemWindow {
    /// host_phys da janela == base da janela BUS (MEM_MASK aplicado pelo SHDW)
    host_base: u64,
    /// tamanho da MEMBAR (BAR size readback)
    len: u64,
    /// offset SHDW: cpu = bus + offset
    offset: i64,
}

static VMD_DMA_WINS: Mutex<[Option<MemWindow>; 2]> = Mutex::new([None, None]);

/// vmd.c (vmd_in_guest): offset != 0 ⇒ visão guest/direct assign.
fn windows_are_guest(off1: i64, off2: i64) -> bool {
    off1 != 0 || off2 != 0
}

/// Traduz BAR BUS do filho → phys CPU (visão guest). Puro (testável).
/// `Some(cpu)` = BAR dentro da janela BUS (cpu = bus + offset); `None` = BAR
/// fora de qualquer janela — caller trata como nativo/phys direto.
fn translate_bar(bus: u64, host_base: u64, len: u64, offset: i64) -> Option<u64> {
    if bus >= host_base && bus < host_base.saturating_add(len) {
        Some((bus as i64).wrapping_add(offset) as u64)
    } else {
        None
    }
}

/// Converte o BAR0 do filho (BUS addr na visão guest) para phys CPU + flag de
/// mapeamento pendente.
/// - Guest: BAR dentro da janela BUS [host_phys, +size) ⇒ phys = bus + offset
///   (dentro da MEMBAR, já mapeada UC no init) → needs_map=false.
/// - Nativo (offset 0): BAR já é phys host → needs_map=true (caller mapeia UC).
pub unsafe fn child_bar_cpu(bar0_bus: u64) -> (u64, bool) {
    let wins = VMD_DMA_WINS.lock();
    for w in wins.iter().flatten() {
        if let Some(cpu) = translate_bar(bar0_bus, w.host_base, w.len, w.offset) {
            return (cpu, false);
        }
    }
    (bar0_bus, true)
}

/// true se algum offset SHDW não-nativo foi registrado (visão guest).
pub fn guest_mode() -> bool {
    let wins = VMD_DMA_WINS.lock();
    wins[0].is_some() || wins[1].is_some()
}

#[inline]
unsafe fn cfg_read32(cfg_va: u64, reg: u32) -> u32 {
    ((cfg_va + (reg & 0xFFC) as u64) as *const u32).read_volatile()
}

#[inline]
unsafe fn cfg_read16(cfg_va: u64, reg: u32) -> u16 {
    (cfg_read32(cfg_va, reg) >> ((reg & 2) * 8)) as u16
}

#[inline]
unsafe fn cfg_read8(cfg_va: u64, reg: u32) -> u8 {
    (cfg_read32(cfg_va, reg) >> ((reg & 3) * 8)) as u8
}

/// Lê a vendor-cap "SHDW" (vmd.c vmd_get_phys_offsets, native_hint=false) e
/// deriva (offset1, offset2) = MEMBAR_cpu − host_phys. Sem cap ⇒ (0,0) nativo.
/// unsafe sobre CF8 do próprio VMD (domínio 0).
unsafe fn shdw_offsets(bus: u8, dev: u8, func: u8, membar1: u64, membar2: u64) -> (i64, i64) {
    let caps = crate::pci::read_pci_capabilities(bus, dev, func);
    for (cap_id, ptr) in &caps {
        if *cap_id != 0x09 {
            continue;
        }
        let p = *ptr;
        if crate::pci::read_config_dword(bus, dev, func, p.wrapping_add(4)) != SHDW_MAGIC {
            continue;
        }
        let lo1 = crate::pci::read_config_dword(bus, dev, func, p.wrapping_add(8)) as u64;
        let hi1 = crate::pci::read_config_dword(bus, dev, func, p.wrapping_add(12)) as u64;
        let lo2 = crate::pci::read_config_dword(bus, dev, func, p.wrapping_add(16)) as u64;
        let hi2 = crate::pci::read_config_dword(bus, dev, func, p.wrapping_add(20)) as u64;
        let phys1 = (hi1 << 32) | lo1;
        let phys2 = (hi2 << 32) | lo2;
        return (shdw_delta(membar1, phys1), shdw_delta(membar2, phys2));
    }
    (0, 0)
}

/// Detecta e inicializa o domínio VMD. Idempotente. Retorna false honesto
/// (VMD ausente é caso normal — só notebooks Intel RST têm o device).
pub unsafe fn init() -> bool {
    if VMD_ACTIVE.load(Ordering::Acquire) {
        return true;
    }
    let devs = crate::pci::scan_pci();
    let Some(vmd) = devs.iter().find(|d| is_vmd(d.vendor_id, d.device_id)) else {
        crate::slog_nano!("VMD", "info", "ausente no dominio 0 (normal fora de Intel RST)");
        return false;
    };
    if vmd.device_id == DID_USE_BIOS_INFO {
        crate::slog_nano!("VMD", "warn", "DID 28C1 (USE_BIOS_INFO) nao suportado — abort honesto");
        return false;
    }
    crate::slog_nano!(
        "VMD", "info",
        "encontrado {:02x}:{:02x}.{:02x} did={:#06x} class={:02x}:{:02x}",
        vmd.bus, vmd.device, vmd.function, vmd.device_id, vmd.class, vmd.subclass
    );

    // D0 antes de tocar BARs (SESSION_260: MMIO em D3 = hang de barramento).
    let (dstate, _) = crate::pci::pci_power_state(vmd.bus, vmd.device, vmd.function);
    if dstate != 0 && dstate != 0xFF {
        let after = crate::pci::pci_power_on_d0(vmd.bus, vmd.device, vmd.function);
        crate::slog_nano!("VMD", "info", "D-state {} -> {}", dstate, after);
        if after != 0 {
            crate::slog_nano!("VMD", "fail", "D-state {} incoerente (power-on falhou)", after);
            return false;
        }
    }
    crate::pci::enable_pci_bus_master(vmd);

    // CFGBAR = BAR0 64-bit (vmd.c VMD_CFGBAR = resource 0). <1MB = -ENOMEM no vmd.c.
    let cfgbar = (vmd.bar0 & !0xF) | ((vmd.bar1 & !0xF) << 32);
    if cfgbar == 0 {
        crate::slog_nano!("VMD", "fail", "CFGBAR (BAR0) zerado — abort honesto");
        return false;
    }
    let cfgbar_size = crate::pci::read_bar_size(vmd.bus, vmd.device, vmd.function, 0);
    if cfgbar_size < (1 << 20) {
        crate::slog_nano!("VMD", "fail", "CFGBAR {} bytes < 1MB (abort)", cfgbar_size);
        return false;
    }
    let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let pages = crate::apic::map_region_uc_2mb(cfgbar, cfgbar_size, pmoff);
    if pages == 0 {
        crate::slog_nano!("VMD", "fail", "mapeamento UC do CFGBAR @ {:#x} falhou", cfgbar);
        return false;
    }
    let cfg_va = cfgbar + pmoff;

    let vmcap = crate::pci::read_config_word(vmd.bus, vmd.device, vmd.function, VMCAP);
    let vmcfg = crate::pci::read_config_word(vmd.bus, vmd.device, vmd.function, VMCONFIG);
    let Some(busn_start) = busn_start_from_regs(vmcap, vmcfg) else {
        crate::slog_nano!("VMD", "fail", "bus offset setting desconhecido (vmcfg={:#06x})", vmcfg);
        return false;
    };

    // Offsets SHDW: nativo ⇒ 0 (bus == host, BAR/CPU/DMA identidade — s422).
    // Não-nativo = visão guest (s429): janela BUS começa em host_phys e o BAR
    // do filho é BUS addr — acesso CPU ao MMIO vai pela MEMBAR (cpu = bus +
    // offset). DMA de RAM do filho permanece identidade (vmd.c: offset é
    // aplicado a recursos, nunca a DMA — see módulo header).
    let membar1 = vmd.bar2 & !0xF;
    let membar2 = vmd.bar4 & !0xF;
    let (off1, off2) = shdw_offsets(vmd.bus, vmd.device, vmd.function, membar1, membar2);
    let guest = windows_are_guest(off1, off2);
    if guest {
        let sz1 = if off1 != 0 {
            crate::pci::read_bar_size(vmd.bus, vmd.device, vmd.function, 2)
        } else {
            0
        };
        let sz2 = if off2 != 0 {
            crate::pci::read_bar_size(vmd.bus, vmd.device, vmd.function, 4)
        } else {
            0
        };
        // Fail-closed: janela guest com size 0/inválido não dá para traduzir.
        if (off1 != 0 && sz1 == 0) || (off2 != 0 && sz2 == 0) {
            crate::slog_nano!(
                "VMD", "fail",
                "guest com MEMBAR size 0 (sz1={:#x} sz2={:#x}) — traducao impossivel: abort honesto",
                sz1, sz2
            );
            return false;
        }
        let pm = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        // Mapeia UC as MEMBARs: o BAR do filho é BUS addr dentro da janela e o
        // acesso CPU (cpu = bus + offset) cai dentro da MEMBAR — é por ela que
        // o kernel chega ao MMIO do NVMe no guest.
        if pm != 0 {
            for (base, sz, tag) in [(membar1, sz1, "MB1"), (membar2, sz2, "MB2")] {
                if base != 0 && sz != 0 {
                    if crate::apic::map_region_uc_2mb(base, sz, pm) == 0 {
                        crate::slog_nano!("VMD", "fail", "mapeamento UC da {} @ {:#x} falhou", tag, base);
                        return false;
                    }
                }
            }
        }
        // host_phys da window = MEMBAR_cpu − offset (inverso do shdw_delta);
        // é também a base da janela BUS do domínio (bus = cpu − offset).
        let host1 = (membar1 as i64 - off1) as u64;
        let host2 = (membar2 as i64 - off2) as u64;
        *VMD_DMA_WINS.lock() = [
            (off1 != 0).then(|| MemWindow { host_base: host1, len: sz1, offset: off1 }),
            (off2 != 0).then(|| MemWindow { host_base: host2, len: sz2, offset: off2 }),
        ];
        crate::slog_nano!(
            "VMD", "ok",
            "GUEST: traducao MMIO ativa (off1={:#x} off2={:#x} bus1={:#x}/{:#x}MB bus2={:#x}/{:#x}MB; DMA identidade)",
            off1, off2, host1, sz1 >> 20, host2, sz2 >> 20
        );
    }

    // Teto de buses: CFGBAR bytes >> 20, clamp 256-busn_start (vmd.c busn_end≤0xff).
    let max_buses = (cfgbar_size >> 20) as usize;
    let n_buses = core::cmp::min(max_buses, 256 - busn_start as usize).max(1).min(255) as u8;

    let mut children = scan_children(cfg_va, busn_start, n_buses);
    let nvme_n = children.iter().filter(|d| d.class == 0x01 && d.subclass == 0x08).count();
    crate::slog_nano!(
        "VMD", "ok",
        "dominio ativo: cfgbar={:#x} ({}MB) busn_start={} buses={} filhos={} nvme={}",
        cfgbar, cfgbar_size >> 20, busn_start, n_buses, children.len(), nvme_n
    );

    *VMD_CHILDREN.lock() = core::mem::take(&mut children);
    VMD_BUSN_START.store(busn_start as u32, Ordering::Release);
    VMD_CFGBAR_BYTES.store(cfgbar_size, Ordering::Release);
    VMD_CFG_VA.store(cfg_va, Ordering::Release);
    VMD_ACTIVE.store(true, Ordering::Release);
    true
}

/// Varre TODOS os buses do range ECAM do VMD (flat — bridges 06:04 são
/// cobertos pelo range, sem recursão). Bus armazenado = numeração do domínio.
unsafe fn scan_children(cfg_va: u64, busn_start: u8, n_buses: u8) -> Vec<PciDevice> {
    let mut out = Vec::new();
    for rel in 0..n_buses {
        let bus = busn_start.wrapping_add(rel);
        for dev in 0..32u8 {
            let f0 = cfg_va + ecam_offset(rel as u32, dev as u32, 0, 0) as u64;
            let vid = cfg_read16(f0, 0x00);
            if vid == 0xFFFF || vid == 0x0000 {
                continue;
            }
            let n_fns: u8 = if cfg_read8(f0, 0x0E) & 0x80 != 0 { 8 } else { 1 };
            for func in 0..n_fns {
                let base = cfg_va + ecam_offset(rel as u32, dev as u32, func as u32, 0) as u64;
                let vid = cfg_read16(base, 0x00);
                if vid == 0xFFFF || vid == 0x0000 {
                    continue;
                }
                out.push(read_child_device(base, bus, dev, func, vid));
            }
        }
    }
    out
}

/// Lê o config space de um filho (via ECAM MMIO) montando o `PciDevice` com a
/// MESMA convenção de BAR do scan domínio-0 (64-bit combinado no low, raw no high).
unsafe fn read_child_device(base: u64, bus: u8, dev: u8, func: u8, vid: u16) -> PciDevice {
    let did = cfg_read16(base, 0x02);
    let cw = cfg_read16(base, 0x0A);
    let prog_if = (cfg_read16(base, 0x08) >> 8) as u8;
    let mut bars = [0u64; 6];
    let mut i = 0usize;
    while i < 6 {
        let low = cfg_read32(base, 0x10 + (4 * i as u32));
        if low & 1 == 1 {
            bars[i] = (low & !0xFFu32) as u64; // I/O
            i += 1;
        } else if (low >> 1) & 3 == 2 && i + 1 < 6 {
            let high = cfg_read32(base, 0x10 + (4 * (i + 1) as u32));
            bars[i] = ((low & !0xFu32) as u64) | ((high & !0xFu32) as u64) << 32;
            bars[i + 1] = (high & !0xFu32) as u64;
            i += 2;
        } else {
            bars[i] = (low & !0xFu32) as u64;
            i += 1;
        }
    }
    PciDevice {
        bus,
        device: dev,
        function: func,
        vendor_id: vid,
        device_id: did,
        class: (cw >> 8) as u8,
        subclass: (cw & 0xFF) as u8,
        prog_if,
        bar0: bars[0],
        bar1: bars[1],
        bar2: bars[2],
        bar3: bars[3],
        bar4: bars[4],
        bar5: bars[5],
    }
}

fn child_cfg_base(bus: u8, dev: u8, func: u8) -> Option<u64> {
    if !VMD_ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let busn_start = VMD_BUSN_START.load(Ordering::Acquire) as u8;
    let rel = bus.checked_sub(busn_start)? as u32;
    let buses = VMD_CFGBAR_BYTES.load(Ordering::Acquire) >> 20;
    if buses == 0 || rel as u64 >= buses {
        return None;
    }
    Some(VMD_CFG_VA.load(Ordering::Acquire) + ecam_offset(rel, dev as u32, func as u32, 0) as u64)
}

/// Habilita MEM/IO/BUSMASTER no filho via config MMIO do domínio (o CF8/CFC não
/// alcança o domínio VMD). Read-back força o completion (vmd.c vmd_pci_write).
pub unsafe fn child_enable_cmd(bus: u8, dev: u8, func: u8, bits: u16) -> bool {
    let Some(base) = child_cfg_base(bus, dev, func) else {
        return false;
    };
    let cmd = cfg_read16(base, 0x04);
    if cmd == 0xFFFF {
        return false;
    }
    let new = cmd | bits;
    if new != cmd {
        ((base + 0x04) as *mut u16).write_volatile(new);
    }
    cfg_read16(base, 0x04) & bits == bits
}

/// Procura NVMe (class 01:08) entre os filhos do domínio e roda o bring-up do
/// driver sobre o BAR0 do filho. Requer `init()` bem-sucedido antes.
pub unsafe fn probe_vmd_nvme() -> Option<crate::disk_agent::nvme::NvmeDriver> {
    if !VMD_ACTIVE.load(Ordering::Acquire) {
        return None;
    }
    let nvme_dev = {
        let children = VMD_CHILDREN.lock();
        children.iter().find(|d| d.class == 0x01 && d.subclass == 0x08).copied()?
    };
    crate::slog_nano!(
        "VMD", "info",
        "NVMe filho {:02x}:{:02x}.{:02x} did={:#06x} bar0={:#x}",
        nvme_dev.bus, nvme_dev.device, nvme_dev.function, nvme_dev.device_id, nvme_dev.bar0
    );
    // Root ports (06:04) + filho: MEM/BUSMASTER (BIOS costuma deixar; garantir
    // é barato e honesto — readback confirma).
    {
        let children = VMD_CHILDREN.lock();
        for d in children.iter() {
            if d.class == 0x06 && d.subclass == 0x04 {
                let _ = child_enable_cmd(d.bus, d.device, d.function, 0x7);
            }
        }
    }
    if !child_enable_cmd(nvme_dev.bus, nvme_dev.device, nvme_dev.function, 0x7) {
        crate::slog_nano!("VMD", "fail", "cmd do filho NVMe nao aceitou MEM/MASTER (readback divergiu)");
        return None;
    }
    let bar0 = (nvme_dev.bar0 & !0xF) | ((nvme_dev.bar1 & !0xF) << 32);
    // Guest: BAR0 do filho é BUS addr — phys CPU = bus + offset SHDW (dentro
    // da MEMBAR, já mapeada UC no init). Nativo: BAR0 é phys host — mapear UC.
    let (bar0_phys, needs_map) = child_bar_cpu(bar0);
    if needs_map {
        let pm = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
        crate::apic::map_page_uc(bar0_phys, pm);
        crate::apic::map_page_uc(bar0_phys + 0x1000, pm);
        crate::slog_nano!("VMD", "info", "BAR0 nativo {:#x} mapeado UC", bar0_phys);
    } else {
        crate::slog_nano!("VMD", "info", "BAR0 bus {:#x} -> cpu {:#x} via MEMBAR (guest)", bar0, bar0_phys);
    }
    // DMA de RAM do filho é UNTRANSLATED (identidade) em ambos os modos — o
    // driver NVMe não precisa de tradução (vmd.c aplica offset só a recursos).
    let pm = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    crate::disk_agent::nvme::NvmeDriver::probe_at_mmio_va((bar0_phys + pm) as *mut u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dids_conhecidos_incluem_a77f_e_client() {
        assert!(is_vmd(0x8086, 0xa77f)); // Alder Lake-P (notebook do lab)
        assert!(is_vmd(0x8086, 0x9a0b)); // Alder Lake (VMD_9A0B)
        assert!(is_vmd(0x8086, 0x28c0)); // HEDT/server
        assert!(!is_vmd(0x10de, 0x25ac)); // GPU NVIDIA não é VMD
        assert!(!is_vmd(0x8086, 0x1234));
    }

    #[test]
    fn busn_start_da_tabela_do_vmd_c() {
        assert_eq!(busn_start_from_regs(0x0001, 0x0000), Some(0));
        assert_eq!(busn_start_from_regs(0x0001, 0x0100), Some(128));
        assert_eq!(busn_start_from_regs(0x0001, 0x0200), Some(224));
        assert_eq!(busn_start_from_regs(0x0001, 0x0300), Some(224));
        // sem BUS_RESTRICT_CAP → 0 independente do config
        assert_eq!(busn_start_from_regs(0x0000, 0x0100), Some(0));
    }

    #[test]
    fn ecam_offset_casa_com_a_formula_pcie() {
        // 1 bus = 1MB; dev = 32KB; fn = 4KB; reg dword-aligned
        assert_eq!(ecam_offset(0, 0, 0, 0x00), 0);
        assert_eq!(ecam_offset(1, 0, 1, 0), 0x0010_1000);
        assert_eq!(ecam_offset(0, 1, 0, 0), 0x0000_8000);
        assert_eq!(ecam_offset(0, 2, 0, 0), 0x0001_0000);
        assert_eq!(ecam_offset(0, 0, 3, 0), 0x0000_3000);
        assert_eq!(ecam_offset(0, 0, 0, 0x40), 0x40);
        assert_eq!(ecam_offset(0, 0, 0, 0x42), 0x40); // alinhado a dword
    }

    #[test]
    fn shdw_nativo_significa_offset_zero() {
        // membar CPU == host phys ⇒ offset 0 (caminho nativo suportado)
        assert_eq!(shdw_delta(0x6030_0000_0000, 0x6030_0000_0000), 0i64);
        // guest: host != cpu ⇒ offset != 0 (tradução MMIO no guest — s429)
        assert_ne!(shdw_delta(0x6030_0000_0000, 0x6020_0000_0000), 0i64);
        // host_phys desalinhado: mascara !0xF antes do delta (MEM_MASK)
        assert_eq!(shdw_delta(0x1000, 0x1008), 0i64);
    }

    #[test]
    fn s429_traducao_bar_bus_para_cpu_via_offset_shdw() {
        // Guest típico: MEMBAR_cpu = 0x6030_0000_0000, host_phys = 0x6020_0000_0000
        // ⇒ offset = +0x10_0000_0000; janela BUS = [host, host+size).
        let host_base = 0x6020_0000_0000u64;
        let len = 0x1000_0000u64; // 256MB (típico de MEMBAR de domínio VMD)
        let offset = shdw_delta(0x6030_0000_0000, host_base);
        // shdw_delta = cpu − host = 0x6030_0000_0000 − 0x6020_0000_0000
        assert_eq!(offset, 0x10_0000_0000i64);
        // BAR do filho em BUS addr dentro da janela ⇒ cpu = bus + offset
        let bar_bus = host_base + 0x400_0000;
        assert_eq!(
            translate_bar(bar_bus, host_base, len, offset),
            Some(bar_bus + 0x10_0000_0000)
        );
        // bordas: primeiro e último byte da janela traduzem
        assert_eq!(translate_bar(host_base, host_base, len, offset), Some(0x6030_0000_0000));
        assert_eq!(
            translate_bar(host_base + len - 1, host_base, len, offset),
            Some(0x6030_0000_0000 + len - 1)
        );
        // fora da janela (antes/depois) = None (caller trata como nativo)
        assert_eq!(translate_bar(host_base - 1, host_base, len, offset), None);
        assert_eq!(translate_bar(host_base + len, host_base, len, offset), None);
    }

    #[test]
    fn s429_nativo_offset_zero_traduz_para_si_mesmo() {
        // Nativo: offset 0 ⇒ cpu == bus (identidade) dentro da janela.
        let base = 0x6000_0000u64;
        assert_eq!(translate_bar(base + 0x1000, base, 0x100_0000, 0), Some(base + 0x1000));
        assert_eq!(translate_bar(base - 1, base, 0x100_0000, 0), None);
    }

    #[test]
    fn s429_windows_guest_deteccao_vmd_c() {
        // vmd.c: vmd_in_guest = offset[0] || offset[1]
        assert!(windows_are_guest(0, 0x1000));
        assert!(windows_are_guest(-0x1000, 0));
        assert!(!windows_are_guest(0, 0));
        // offset negativo (MEMBAR_cpu abaixo do host) também é guest válido
        let off = shdw_delta(0x1000_0000, 0x2000_0000);
        assert_eq!(off, -0x1000_0000i64);
        assert!(windows_are_guest(off, 0));
        // e a tradução ainda fecha: bus = host ⇒ cpu = bus + off
        assert_eq!(translate_bar(0x2000_0000, 0x2000_0000, 0x1000, off), Some(0x1000_0000));
    }

    #[test]
    fn teto_de_buses_nunca_passa_de_255() {
        // 256MB CFGBAR = 256 buses; com busn_start 224 sobram só 32
        let max_buses = (256usize << 20) >> 20;
        let busn_start = 224usize;
        let n = core::cmp::min(max_buses, 256 - busn_start).max(1) as u8;
        assert_eq!(n, 32);
        // busn_start 0 → 256 clampado para 255 buses (u8 range, sem overflow)
        let n0 = core::cmp::min(256usize, 256 - 0).max(1).min(255) as u8;
        assert_eq!(n0, 255);
    }
}
