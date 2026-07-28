// Legacy re-export wrapper -- prefer direct use k_nano::module over crate::module
//! ADR-0042 — re-export k_nano::ext2_reader (ADR-0072 Labor 13).
#![allow(unused_imports)]
pub use k_nano::ext2_reader::*;
