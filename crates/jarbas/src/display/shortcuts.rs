//! Keyboard shortcuts — KeyCombo → WmAction mapping.
//! Tabela estática (não no SkillRegistry — WM é core).

use alloc::vec::Vec;
use super::tiling::WindowId;
use super::window::AppId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub modifiers: Modifiers,
    pub key: KeyCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub super_key: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub const NONE: Self = Self { super_key: false, ctrl: false, alt: false, shift: false };
    pub const SUPER: Self = Self { super_key: true, ctrl: false, alt: false, shift: false };
    pub const SUPER_SHIFT: Self = Self { super_key: true, ctrl: false, alt: false, shift: true };
    pub const ALT: Self = Self { super_key: false, ctrl: false, alt: true, shift: false };
    pub const CTRL: Self = Self { super_key: false, ctrl: true, alt: false, shift: false };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    Tab, Enter, Escape, Space,
    Left, Right, Up, Down,
    Q, W, E, R, T, Y, U, I, O, P,
    A, S, D, F, G, H, J, K, L,
    Z, X, C, V, B, N, M,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmAction {
    WorkspaceSwitch(usize),      // Super+1-9
    WorkspacePrev,               // Super+Left
    WorkspaceNext,               // Super+Right
    WorkspacePrevious,           // Super+Tab (workspace anterior)
    TileSplitHorizontal,         // Super+H
    TileSplitVertical,           // Super+V
    TileResizeLeft,              // Super+Shift+Left
    TileResizeRight,             // Super+Shift+Right
    TileResizeUp,                // Super+Shift+Up
    TileResizeDown,              // Super+Shift+Down
    CycleWindow,                 // Alt+Tab
    CycleWindowReverse,          // Alt+Shift+Tab
    CloseWindow,                 // Super+Q
    MaximizeWindow,              // Super+M
    MinimizeWindow,              // Super+N
    ToggleFloating,              // Super+Shift+Space
    LaunchApp(AppId),            // Super+Enter (launcher)
    ToggleDock,                  // Super+D
    ToggleTiling,                // Super+T
    ShowLauncher,                // Super+Space
}

impl WmAction {
    pub fn from_keycombo(combo: KeyCombo) -> Option<Self> {
        use KeyCode::*;
        use WmAction::*;

        match (combo.modifiers, combo.key) {
            // Workspaces
            (Modifiers::SUPER, Key1) => Some(WorkspaceSwitch(0)),
            (Modifiers::SUPER, Key2) => Some(WorkspaceSwitch(1)),
            (Modifiers::SUPER, Key3) => Some(WorkspaceSwitch(2)),
            (Modifiers::SUPER, Key4) => Some(WorkspaceSwitch(3)),
            (Modifiers::SUPER, Key5) => Some(WorkspaceSwitch(4)),
            (Modifiers::SUPER, Key6) => Some(WorkspaceSwitch(5)),
            (Modifiers::SUPER, Key7) => Some(WorkspaceSwitch(6)),
            (Modifiers::SUPER, Key8) => Some(WorkspaceSwitch(7)),
            (Modifiers::SUPER, Key9) => Some(WorkspaceSwitch(8)),
            (Modifiers::SUPER, Left) => Some(WorkspacePrev),
            (Modifiers::SUPER, Right) => Some(WorkspaceNext),
            (Modifiers::SUPER, Tab) => Some(WorkspacePrevious),

            // Tiling
            (Modifiers::SUPER, H) => Some(TileSplitHorizontal),
            (Modifiers::SUPER, V) => Some(TileSplitVertical),
            (Modifiers::SUPER_SHIFT, Left) => Some(TileResizeLeft),
            (Modifiers::SUPER_SHIFT, Right) => Some(TileResizeRight),
            (Modifiers::SUPER_SHIFT, Up) => Some(TileResizeUp),
            (Modifiers::SUPER_SHIFT, Down) => Some(TileResizeDown),

            // Window management
            (Modifiers { alt: true, .. }, Tab) => Some(CycleWindow),
            (Modifiers { alt: true, shift: true, .. }, Tab) => Some(CycleWindowReverse),
            (Modifiers::SUPER, Q) => Some(CloseWindow),
            (Modifiers::SUPER, M) => Some(MaximizeWindow),
            (Modifiers::SUPER, N) => Some(MinimizeWindow),
            (Modifiers::SUPER_SHIFT, Space) => Some(ToggleFloating),

            // System
            (Modifiers::SUPER, Enter) => Some(LaunchApp(AppId::HermesChat)),
            (Modifiers::SUPER, D) => Some(ToggleDock),
            (Modifiers::SUPER, T) => Some(ToggleTiling),
            (Modifiers::SUPER, Space) => Some(ShowLauncher),

            _ => None,
        }
    }
}

// Tabela estática (não no SkillRegistry — WM é core)
pub static SHORTCUTS: &[(KeyCombo, WmAction)] = &[
    // Workspaces
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key1 }, WmAction::WorkspaceSwitch(0)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key2 }, WmAction::WorkspaceSwitch(1)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key3 }, WmAction::WorkspaceSwitch(2)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key4 }, WmAction::WorkspaceSwitch(3)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key5 }, WmAction::WorkspaceSwitch(4)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key6 }, WmAction::WorkspaceSwitch(5)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key7 }, WmAction::WorkspaceSwitch(6)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key8 }, WmAction::WorkspaceSwitch(7)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Key9 }, WmAction::WorkspaceSwitch(8)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Left }, WmAction::WorkspacePrev),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Right }, WmAction::WorkspaceNext),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Tab }, WmAction::WorkspacePrevious),

    // Tiling
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::H }, WmAction::TileSplitHorizontal),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::V }, WmAction::TileSplitVertical),
    (KeyCombo { modifiers: Modifiers::SUPER_SHIFT, key: KeyCode::Left }, WmAction::TileResizeLeft),
    (KeyCombo { modifiers: Modifiers::SUPER_SHIFT, key: KeyCode::Right }, WmAction::TileResizeRight),
    (KeyCombo { modifiers: Modifiers::SUPER_SHIFT, key: KeyCode::Up }, WmAction::TileResizeUp),
    (KeyCombo { modifiers: Modifiers::SUPER_SHIFT, key: KeyCode::Down }, WmAction::TileResizeDown),

    // Window
    (KeyCombo { modifiers: Modifiers::ALT, key: KeyCode::Tab }, WmAction::CycleWindow),
    (KeyCombo { modifiers: Modifiers { alt: true, shift: true, ..Modifiers::NONE }, key: KeyCode::Tab }, WmAction::CycleWindowReverse),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Q }, WmAction::CloseWindow),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::M }, WmAction::MaximizeWindow),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::N }, WmAction::MinimizeWindow),
    (KeyCombo { modifiers: Modifiers::SUPER_SHIFT, key: KeyCode::Space }, WmAction::ToggleFloating),

    // System
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Enter }, WmAction::LaunchApp(AppId::HermesChat)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::D }, WmAction::ToggleDock),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::T }, WmAction::ToggleTiling),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Space }, WmAction::ShowLauncher),
];