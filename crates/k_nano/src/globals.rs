// Global driver instances shared across K²CHJ crates.
// Initialized at boot by neural-kernel's main.rs.
use spin::Mutex;

pub static ATA_DRIVER: Mutex<Option<crate::ata::AtaDriver>> = Mutex::new(None); // ponytail: Option::None is const
