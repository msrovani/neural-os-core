//! Syscall / trap mínimo — int 0x90 + Cap bitflags (MVP C / ADR-0041).
//! Vetores 0x80–0x82 ficam com IPI SMP; ABI staging via atomics até Ring3.
//! P6: Cap::ENTER_USER + SYS_EXIT_USER para retorno CPL=3 → kernel.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::idt::InterruptStackFrame;

/// Vetor de software syscall (fora da faixa IPI).
pub const SYSCALL_VECTOR: u8 = 0x90;

pub const SYS_PING: u64 = 1;
pub const SYS_WRITE_RING: u64 = 2;
pub const SYS_READ_RING: u64 = 3;
pub const SYS_SEND_TCP: u64 = 4;
/// JARBAS: mapear páginas FB no AddressSpace (ADR-0041 P4).
pub const SYS_MAP_FB: u64 = 5;
/// JARBAS: present/flip backbuffer → FB físico.
pub const SYS_PRESENT_FB: u64 = 6;
/// K-IA: pin frames DMA não-reclaimáveis (ADR-0041 P5).
pub const SYS_PIN_DMA: u64 = 7;
/// K-IA: mapear buffer pinado no AS (ADR-0041 P5).
pub const SYS_MAP_DMA: u64 = 8;
/// Cortex: mmap páginas de peso LLM (ADR-0041 P5).
pub const SYS_MAP_WEIGHTS: u64 = 9;
/// P6: stub user → kernel (após marcador / Cap check).
pub const SYS_EXIT_USER: u64 = 10;
/// P7: demand-paging / lazy map de páginas (ADR-0041).
pub const SYS_DEMAND_PAGE: u64 = 11;

/// Capability de operação (independente do CapabilityToken do EventBus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cap(pub u64);

impl Cap {
    pub const EMPTY: Cap = Cap(0);
    pub const PING: Cap = Cap(1 << 0);
    pub const WRITE_RING: Cap = Cap(1 << 1);
    pub const READ_RING: Cap = Cap(1 << 2);
    /// Hermes/WASM: host `aios_send_tcp` / skill net (ADR-0041 P3).
    pub const SEND_TCP: Cap = Cap(1 << 3);
    /// JARBAS: mapear FB MMIO no AS do processo (ADR-0041 P4).
    pub const MAP_FB: Cap = Cap(1 << 4);
    /// JARBAS: escrever / present no framebuffer.
    pub const WRITE_FB: Cap = Cap(1 << 5);
    /// K-IA: pin frames físicos para DMA (ADR-0041 P5).
    pub const PIN_DMA: Cap = Cap(1 << 6);
    /// K-IA: mapear buffer DMA pinado no AddressSpace.
    pub const MAP_DMA: Cap = Cap(1 << 7);
    /// Cortex: mapear páginas de pesos LLM (mmap PoC).
    pub const MAP_WEIGHTS: Cap = Cap(1 << 8);
    /// P6: permitir enter_user_mode / trap de volta do stub Ring3.
    pub const ENTER_USER: Cap = Cap(1 << 9);
    /// P7: registrar/curar demand-paging (lazy mmap pesos).
    pub const DEMAND_PAGE: Cap = Cap(1 << 10);

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
pub fn dispatch(nr: u64, _arg: u64, cap: Cap) -> Result<u64, &'static str> {
    match nr {
        SYS_PING => {
            if !cap.contains(Cap::PING) {
                return Err("EPERM: Cap::PING");
            }
            Ok(PING_COUNT.fetch_add(1, Ordering::Relaxed) + 1)
        }
        SYS_WRITE_RING => {
            if !cap.contains(Cap::WRITE_RING) {
                return Err("EPERM: Cap::WRITE_RING");
            }
            Ok(0)
        }
        SYS_READ_RING => {
            if !cap.contains(Cap::READ_RING) {
                return Err("EPERM: Cap::READ_RING");
            }
            Ok(0)
        }
        SYS_SEND_TCP => {
            if !cap.contains(Cap::SEND_TCP) {
                return Err("EPERM: Cap::SEND_TCP");
            }
            Ok(0)
        }
        SYS_MAP_FB => {
            if !cap.contains(Cap::MAP_FB) {
                return Err("EPERM: Cap::MAP_FB");
            }
            Ok(0)
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
