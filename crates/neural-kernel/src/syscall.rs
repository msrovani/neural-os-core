//! Syscall / trap mínimo — int 0x90 + Cap bitflags (MVP C / ADR-0041).
//! Consolidado para 9 syscalls por ADR-0076 §4.3: SYS_WRITE_RING + SYS_READ_RING
//! → SYS_RING_OP (subcomando em arg); SYS_SEND_TCP e SYS_VRING_SETUP removidos.
//! Vetores 0x80–0x82 ficam com IPI SMP; ABI staging via atomics até Ring3.
//! P6: Cap::ENTER_USER + SYS_EXIT_USER para retorno CPL=3 → kernel.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::idt::InterruptStackFrame;

/// Vetor de software syscall (fora da faixa IPI).
pub const SYSCALL_VECTOR: u8 = 0x90;

pub const SYS_PING: u64 = 1;
/// Ring IPC operação (subcomando em arg: RING_OP_WRITE / RING_OP_READ).
pub const SYS_RING_OP: u64 = 2;
/// JARBAS: mapear páginas FB no AddressSpace (ADR-0041 P4).
pub const SYS_MAP_FB: u64 = 3;
/// JARBAS: present/flip backbuffer → FB físico.
pub const SYS_PRESENT_FB: u64 = 4;
/// K-IA: pin frames DMA não-reclaimáveis (ADR-0041 P5).
pub const SYS_PIN_DMA: u64 = 5;
/// K-IA: mapear buffer pinado no AS (ADR-0041 P5).
pub const SYS_MAP_DMA: u64 = 6;
/// Cortex: mmap páginas de peso LLM (ADR-0041 P5).
pub const SYS_MAP_WEIGHTS: u64 = 7;
/// P6: stub user → kernel (após marcador / Cap check).
pub const SYS_EXIT_USER: u64 = 8;
/// P7: demand-paging / lazy map de páginas (ADR-0041).
pub const SYS_DEMAND_PAGE: u64 = 9;
/// P9: mmap file-backed (GGUF/FAT) sobre demand-paging (ADR-0041).
pub const SYS_MAP_FILE: u64 = 10;

/// Subcomandos para SYS_RING_OP (passados no argumento `arg`).
pub const RING_OP_WRITE: u64 = 0;
pub const RING_OP_READ: u64 = 1;

/// Capability de operação (independente do CapabilityToken do EventBus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap(pub u64);

impl Cap {
    pub const EMPTY: Cap = Cap(0);
    /// Permite usar SYS_PING.
    pub const PING: Cap = Cap(1 << 0);
    /// Permite SYS_RING_OP (write e read, consolidado ADR-0076).
    pub const RING_OP: Cap = Cap(1 << 1);
    /// JARBAS: mapear FB MMIO no AS do processo (ADR-0041 P4).
    pub const MAP_FB: Cap = Cap(1 << 2);
    /// JARBAS: escrever / present no framebuffer.
    pub const WRITE_FB: Cap = Cap(1 << 3);
    /// K-IA: pin frames físicos para DMA (ADR-0041 P5).
    pub const PIN_DMA: Cap = Cap(1 << 4);
    /// K-IA: mapear buffer DMA pinado no AddressSpace.
    pub const MAP_DMA: Cap = Cap(1 << 5);
    /// Cortex: mapear páginas de pesos LLM (mmap PoC).
    pub const MAP_WEIGHTS: Cap = Cap(1 << 6);
    /// P6: permitir enter_user_mode / trap de volta do stub Ring3.
    pub const ENTER_USER: Cap = Cap(1 << 7);
    /// P7: registrar/curar demand-paging (lazy mmap pesos).
    pub const DEMAND_PAGE: Cap = Cap(1 << 8);
    /// P9: mmap file-backed (FAT/GGUF/.bitnet) via demand-paging.
    pub const MAP_FILE: Cap = Cap(1 << 9);

    #[inline]
    pub fn bits(self) -> u64 {
        self.0
    }

    #[inline]
    pub fn from_bits(bits: u64) -> Cap {
        Cap(bits)
    }

    #[inline]
    pub fn contains(self, other: Cap) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    pub fn union(self, other: Cap) -> Cap {
        Cap(self.0 | other.0)
    }
}

static PING_COUNT: AtomicU64 = AtomicU64::new(0);
static SYS_NR: AtomicU64 = AtomicU64::new(0);
static SYS_ARG: AtomicU64 = AtomicU64::new(0);
static SYS_CAP: AtomicU64 = AtomicU64::new(0);
static SYS_RESULT: AtomicU64 = AtomicU64::new(0);
static SYS_STATUS: AtomicU64 = AtomicU64::new(0); // 0=ok, 1=err

pub fn ping_count() -> u64 {
    PING_COUNT.load(Ordering::Relaxed)
}

/// Pré-carrega átomos do trap (kernel prepara antes do `iretq` para o stub).
pub fn stage_syscall(nr: u64, arg: u64, cap: Cap) {
    SYS_NR.store(nr, Ordering::SeqCst);
    SYS_ARG.store(arg, Ordering::SeqCst);
    SYS_CAP.store(cap.bits(), Ordering::SeqCst);
    SYS_STATUS.store(0, Ordering::SeqCst);
}

/// Despacho capability-gated (chamável direto ou via int 0x90).
/// ADR-0076 §4.3: 9 syscalls — SEND_TCP e VRING_SETUP removidos,
/// WRITE_RING + READ_RING consolidados em SYS_RING_OP (subcomando em arg).
pub fn dispatch(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    match nr {
        SYS_PING => {
            if !cap.contains(Cap::PING) {
                return Err("EPERM: Cap::PING");
            }
            Ok(PING_COUNT.fetch_add(1, Ordering::Relaxed) + 1)
        }
        SYS_RING_OP => {
            if !cap.contains(Cap::RING_OP) {
                return Err("EPERM: Cap::RING_OP");
            }
            // subcomando em arg: RING_OP_WRITE ou RING_OP_READ
            Ok(0) // stub
        }
        SYS_MAP_FB => {
            if !cap.contains(Cap::MAP_FB) {
                return Err("EPERM: Cap::MAP_FB");
            }
            // Mapa páginas do framebuffer no address space atual
            // arg = physical address base, retorna virtual address onde foi mapeado
            let fb_phys = arg;
            if fb_phys == 0 {
                return Err("ENODEV: FB phys address is 0");
            }
            let fb_va = crate::jarbas_fb::JARBAS_FB_VA;
            let flags = x86_64::structures::paging::PageTableFlags::PRESENT
                | x86_64::structures::paging::PageTableFlags::WRITABLE
                | x86_64::structures::paging::PageTableFlags::NO_CACHE;
            let pages = crate::jarbas_fb::DEMO_MAP_PAGES;
            for i in 0..pages {
                let va = x86_64::VirtAddr::new(fb_va + (i as u64) * 4096);
                let frame = x86_64::structures::paging::PhysFrame::<x86_64::structures::paging::Size4KiB>
                    ::containing_address(x86_64::PhysAddr::new(fb_phys + (i as u64) * 4096));
                // Mapeia usando page tables do CR3 atual
                use x86_64::registers::control::Cr3;
                let (l4_frame, _) = Cr3::read();
                let l4_virt = x86_64::VirtAddr::new(
                    crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire)
                    + l4_frame.start_address().as_u64()
                );
                let l4_ptr = l4_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>();
                let l4 = unsafe { &mut *l4_ptr };
                // Walk/Build page table entries para a VA
                let p4 = va.p4_index();
                let p3 = va.p3_index();
                let p2 = va.p2_index();
                let p1 = va.p1_index();
                // Garantir que L3 existe
                let pm_offset = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire);
                if !l4[p4].flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                    // Alocar frame para L3
                    let l3_frame = match k_nano::memory::alloc_physical_frame() {
                        Some(f) => f,
                        None => return Err("ENOMEM: L3 frame"),
                    };
                    l4[p4].set_addr(l3_frame.start_address(), flags);
                }
                let l3_phys = l4[p4].addr();
                let l3_virt = x86_64::VirtAddr::new(pm_offset + l3_phys.as_u64());
                let l3 = unsafe { &mut *l3_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>() };
                if !l3[p3].flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                    let l2_frame = match k_nano::memory::alloc_physical_frame() {
                        Some(f) => f,
                        None => return Err("ENOMEM: L2 frame"),
                    };
                    l3[p3].set_addr(l2_frame.start_address(), flags);
                }
                let l2_phys = l3[p3].addr();
                let l2_virt = x86_64::VirtAddr::new(pm_offset + l2_phys.as_u64());
                let l2 = unsafe { &mut *l2_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>() };
                l2[p2].set_addr(frame.start_address(), flags);
            }
            Ok(fb_va)
        }
        SYS_PRESENT_FB => {
            if !cap.contains(Cap::WRITE_FB) {
                return Err("EPERM: Cap::WRITE_FB");
            }
            Ok(0)
        }
        SYS_PIN_DMA => {
            if !cap.contains(Cap::PIN_DMA) {
                return Err("EPERM: Cap::PIN_DMA");
            }
            Ok(0)
        }
        SYS_MAP_DMA => {
            if !cap.contains(Cap::MAP_DMA) {
                return Err("EPERM: Cap::MAP_DMA");
            }
            Ok(0)
        }
        SYS_MAP_WEIGHTS => {
            if !cap.contains(Cap::MAP_WEIGHTS) {
                return Err("EPERM: Cap::MAP_WEIGHTS");
            }
            Ok(0)
        }
        SYS_EXIT_USER => {
            if !cap.contains(Cap::ENTER_USER) {
                return Err("EPERM: Cap::ENTER_USER");
            }
            Ok(0)
        }
        SYS_DEMAND_PAGE => {
            if !cap.contains(Cap::DEMAND_PAGE) {
                return Err("EPERM: Cap::DEMAND_PAGE");
            }
            // Demand paging: allocate a physical frame and map it
            // arg = virtual address that faulted
            let fault_va = arg;
            if fault_va == 0 {
                return Err("ENODEV: fault_va is 0");
            }
            let frame = k_nano::memory::alloc_physical_frame()
                .ok_or("ENOMEM: demand page")?;
            let va = x86_64::VirtAddr::new(fault_va);
            let pm_offset = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Acquire);
            let (l4_frame, _) = x86_64::registers::control::Cr3::read();
            let l4_virt = x86_64::VirtAddr::new(pm_offset + l4_frame.start_address().as_u64());
            let l4 = unsafe { &mut *l4_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>() };
            let flags = x86_64::structures::paging::PageTableFlags::PRESENT
                | x86_64::structures::paging::PageTableFlags::WRITABLE
                | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;
            let p4 = va.p4_index();
            let p3 = va.p3_index();
            let p2 = va.p2_index();
            let _p1 = va.p1_index();
            // Ensure intermediate tables exist
            if !l4[p4].flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                let l3_f = k_nano::memory::alloc_physical_frame().ok_or("ENOMEM: L3")?;
                l4[p4].set_addr(l3_f.start_address(), flags);
            }
            let l3_phys = l4[p4].addr();
            let l3_virt = x86_64::VirtAddr::new(pm_offset + l3_phys.as_u64());
            let l3 = unsafe { &mut *l3_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>() };
            if !l3[p3].flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                let l2_f = k_nano::memory::alloc_physical_frame().ok_or("ENOMEM: L2")?;
                l3[p3].set_addr(l2_f.start_address(), flags);
            }
            let l2_phys = l3[p3].addr();
            let l2_virt = x86_64::VirtAddr::new(pm_offset + l2_phys.as_u64());
            let l2 = unsafe { &mut *l2_virt.as_mut_ptr::<x86_64::structures::paging::PageTable>() };
            l2[p2].set_addr(frame.start_address(), flags);
            Ok(0)
        }
        SYS_MAP_FILE => {
            if !cap.contains(Cap::MAP_FILE) {
                return Err("EPERM: Cap::MAP_FILE");
            }
            Ok(0)
        }
        _ => Err("ENOSYS"),
    }
}

/// Invoca o trap `int 0x90` (prova de gate no IDT).
pub fn soft_syscall(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    stage_syscall(nr, arg, cap);
    unsafe {
        core::arch::asm!("int 0x90", options(nostack));
    }
    if SYS_STATUS.load(Ordering::SeqCst) != 0 {
        Err("mvp-c: syscall negada")
    } else {
        Ok(SYS_RESULT.load(Ordering::SeqCst))
    }
}

pub extern "x86-interrupt" fn syscall_int_handler(_stack: InterruptStackFrame) {
    // Phase 3: ABI por registrador para Ring3 (RAX=nr, RDI=arg0, RSI=arg1, RDX=cap_bits).
    // Fallback para atomics staging (kernel→kernel, compat).
    let nr = SYS_NR.load(Ordering::SeqCst);
    let mut arg = SYS_ARG.load(Ordering::SeqCst);
    let mut cap = Cap::from_bits(SYS_CAP.load(Ordering::SeqCst));

    // Se veio de Ring3 (CS.RPL==3) ou foi pré-carregado via stage, usa ABI registrador.
    // O stub user carrega RAX=nr, RDI=arg, RDX=caps antes de int 0x90.
    if nr == 0 || cap == Cap::EMPTY {
        // Ler registradores (ABI Ring3: RAX=nr, RDI=arg0, RDX=cap_bits)
        let reg_nr: u64;
        let reg_arg: u64;
        let reg_cap: u64;
        unsafe {
            core::arch::asm!(
                "mov {}, rax",
                "mov {}, rdi",
                "mov {}, rdx",
                out(reg) reg_nr,
                out(reg) reg_arg,
                out(reg) reg_cap,
                options(nostack, preserves_flags)
            );
        }
        // Só usa registrador se stage não setou (stage tem prioridade para compat)
        let use_reg = SYS_NR.load(Ordering::SeqCst) == 0;
        if use_reg && reg_nr != 0 {
            SYS_NR.store(reg_nr, Ordering::SeqCst);
            SYS_ARG.store(reg_arg, Ordering::SeqCst);
            SYS_CAP.store(reg_cap, Ordering::SeqCst);
        }
    }

    // Re-read after potential register ABI update
    let nr = SYS_NR.load(Ordering::SeqCst);
    let arg = SYS_ARG.load(Ordering::SeqCst);
    let cap = Cap::from_bits(SYS_CAP.load(Ordering::SeqCst));

    // P6: retorno do stub Ring3 — abandona frame de interrupt e jmp kernel.
    if nr == SYS_EXIT_USER && crate::user_mode::demo_active() {
        match dispatch(nr, arg, cap) {
            Ok(v) => {
                SYS_RESULT.store(v, Ordering::SeqCst);
                SYS_STATUS.store(0, Ordering::SeqCst);
                crate::user_mode::return_from_user(true);
            }
            Err(_) => {
                SYS_STATUS.store(1, Ordering::SeqCst);
                crate::user_mode::return_from_user(false);
            }
        }
    }

    match dispatch(nr, arg, cap) {
        Ok(v) => {
            SYS_RESULT.store(v, Ordering::SeqCst);
            SYS_STATUS.store(0, Ordering::SeqCst);
        }
        Err(_) => {
            SYS_STATUS.store(1, Ordering::SeqCst);
        }
    }
}
