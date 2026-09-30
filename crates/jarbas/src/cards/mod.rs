//! ADR-0040 #419 — Card modules for storage, disk management, and HW info.
//! Each card returns a `UiDeclaration` for the compositor to render.

pub mod disk_selection_card;
pub mod file_manager_card;
pub mod hints_card; // s426: telemetria H3 ao vivo (HINT_FORWARD_US/HINTS_GENERATED)
pub mod hw_inventory_card;
pub mod terminal_card;
