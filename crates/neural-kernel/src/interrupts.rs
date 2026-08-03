//! Facade — implementação canônica de interrupções/exceções/GDT/TSS/PIC vive em
//! `k_nano` R0 (fonte única, SESSION_237/244). Residuals bin-only (Ring3/TSS
//! per-process, syscall 0x90, hooks demand-page) ficam em `crate::interrupts_ext`.
pub use k_nano::interrupts::*;
