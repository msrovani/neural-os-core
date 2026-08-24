//! Facade — W^X arena lives in k_nano::paging (R0).
//! Bin keeps only wire (ADR-0041 §11). No duplicate logic.

pub use k_nano::paging::{arena_self_test as self_test, jit_write_exec, jit_write_exec_user, user_arena_self_test};
