//! Facade: a fila MPMC canônica vive em [`crate::sync::mpmc`].
//! (H10: este módulo era duplicata divergente da versão seq-based com
//! zero callers externos — BEI/cellular usam `sync::mpmc`.)
//! ponytail: re-export, sem segunda implementação.
//! API vigente: `MpmcQueue::new(usize) -> Option<Self>`,
//! `try_send(_) -> Result<(), T>`, `try_recv(_) -> Option<T>`.
pub use crate::sync::mpmc::*;
