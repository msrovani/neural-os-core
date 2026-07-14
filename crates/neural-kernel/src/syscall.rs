//! Syscall / trap mínimo — int 0x90 + Cap bitflags (MVP C / ADR-0041).
//! Vetores 0x80–0x82 ficam com IPI SMP; ABI staging via atomics até Ring3.

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::idt::InterruptStackFrame;

/// Vetor de software syscall (fora da faixa IPI).
pub const SYSCALL_VECTOR: u8 = 0x90;

pub const SYS_PING: u64 = 1;
pub const SYS_WRITE_RING: u64 = 2;
pub const SYS_READ_RING: u64 = 3;
pub const SYS_SEND_TCP: u64 = 4;

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
        _ => Err("ENOSYS"),
    }
}

/// Invoca o trap `int 0x90` (prova de gate no IDT).
pub fn soft_syscall(nr: u64, arg: u64, cap: Cap) -> Result<u64, &'static str> {
    SYS_NR.store(nr, Ordering::SeqCst);
    SYS_ARG.store(arg, Ordering::SeqCst);
    SYS_CAP.store(cap.bits(), Ordering::SeqCst);
    SYS_STATUS.store(0, Ordering::SeqCst);
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
