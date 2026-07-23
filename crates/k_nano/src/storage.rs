//! Storage — ADR-0063 Flash + TickvLite; stub nvme legacy retained.
//!
//! NVMe de produção: `disk_agent::nvme` (SESSION_171).

pub mod flash;
pub mod nvme;
pub mod tickv;

pub use flash::{init_flash, ActiveFlash, FlashController, FLASH};
pub use tickv::{
    backend_name, get_blob, is_ready, power_loss_smoke, put_blob, smoke as tickv_smoke,
    TickvLite, TICKV,
};
