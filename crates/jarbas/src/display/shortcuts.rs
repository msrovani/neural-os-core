//! Keyboard shortcuts — KeyCombo → WmAction mapping.
//! Tabela estática (não no SkillRegistry — WM é core).

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
    Delete,
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
    OpenChat,                    // Space — abre/foca o chat do Jarbas
    PowerMenu,                   // Ctrl+Alt+Del — mostra menu de desligar
    ShowHelp,                    // H — mostra card de atalhos do teclado
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

            // Bare Space (no modifiers) → OpenChat
            (Modifiers::NONE, Space) => Some(OpenChat),
            // Ctrl+Alt+Delete → PowerMenu
            (Modifiers { ctrl: true, alt: true, .. }, Delete) => Some(PowerMenu),
            // Bare H (no modifiers) → ShowHelp
            (Modifiers::NONE, H) => Some(ShowHelp),

            // Standard close shortcuts
            (Modifiers::ALT, F4) => Some(CloseWindow),
            (Modifiers::CTRL, Q) => Some(CloseWindow),

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
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::Space }, WmAction::OpenChat),
    (KeyCombo { modifiers: Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }, key: KeyCode::Delete }, WmAction::PowerMenu),
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::H }, WmAction::ShowHelp),
    (KeyCombo { modifiers: Modifiers::ALT, key: KeyCode::F4 }, WmAction::CloseWindow),
    (KeyCombo { modifiers: Modifiers::CTRL, key: KeyCode::Q }, WmAction::CloseWindow),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Enter }, WmAction::LaunchApp(AppId::HermesChat)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::D }, WmAction::ToggleDock),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::T }, WmAction::ToggleTiling),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Space }, WmAction::ShowLauncher),
];

/// Mapeia scancode PS/2 set 1 → KeyCode. Retorna None se não é tecla WM-mappable.
/// Cobre: letras, números, F1-F12, setas, Enter, Esc, Space, Tab, modificadores.
pub fn scancode_to_keycode(scancode: u8) -> Option<KeyCode> {
    match scancode {
        // Letras
        0x10 => Some(KeyCode::Q),
        0x11 => Some(KeyCode::W),
        0x12 => Some(KeyCode::E),
        0x13 => Some(KeyCode::R),
        0x14 => Some(KeyCode::T),
        0x15 => Some(KeyCode::Y),
        0x16 => Some(KeyCode::U),
        0x17 => Some(KeyCode::I),
        0x18 => Some(KeyCode::O),
        0x19 => Some(KeyCode::P),
        0x1E => Some(KeyCode::A),
        0x1F => Some(KeyCode::S),
        0x20 => Some(KeyCode::D),
        0x21 => Some(KeyCode::F),
        0x22 => Some(KeyCode::G),
        0x23 => Some(KeyCode::H),
        0x24 => Some(KeyCode::J),
        0x25 => Some(KeyCode::K),
        0x26 => Some(KeyCode::L),
        0x2C => Some(KeyCode::Z),
        0x2D => Some(KeyCode::X),
        0x2E => Some(KeyCode::C),
        0x2F => Some(KeyCode::V),
        0x30 => Some(KeyCode::B),
        0x31 => Some(KeyCode::N),
        0x32 => Some(KeyCode::M),
        // Números (top row)
        0x02 => Some(KeyCode::Key1),
        0x03 => Some(KeyCode::Key2),
        0x04 => Some(KeyCode::Key3),
        0x05 => Some(KeyCode::Key4),
        0x06 => Some(KeyCode::Key5),
        0x07 => Some(KeyCode::Key6),
        0x08 => Some(KeyCode::Key7),
        0x09 => Some(KeyCode::Key8),
        0x0A => Some(KeyCode::Key9),
        // F1-F12
        0x3B => Some(KeyCode::F1),
        0x3C => Some(KeyCode::F2),
        0x3D => Some(KeyCode::F3),
        0x3E => Some(KeyCode::F4),
        0x3F => Some(KeyCode::F5),
        0x40 => Some(KeyCode::F6),
        0x41 => Some(KeyCode::F7),
        0x42 => Some(KeyCode::F8),
        0x43 => Some(KeyCode::F9),
        0x44 => Some(KeyCode::F10),
        0x57 => Some(KeyCode::F11),
        0x58 => Some(KeyCode::F12),
        // Setas (extended — prefix 0xE0 + byte; tratado fora desta fn)
        // Especiais
        0x1C => Some(KeyCode::Enter),
        0x01 => Some(KeyCode::Escape),
        0x39 => Some(KeyCode::Space),
        0x0F => Some(KeyCode::Tab),
        0x53 => Some(KeyCode::Delete),   // Delete key / Keypad period
        _ => None,
    }
}

pub fn help_text() -> &'static str {
    "ATALHOS DO TECLADO\n\
     \n\
     [Workspace]\n\
     Super+1-9     — Trocar workspace\n\
     Super+Left    — Workspace anterior\n\
     Super+Right   — Workspace seguinte\n\
     \n\
     [Janelas]\n\
     Alt+Tab       — Ciclar janelas\n\
     Super+Q       — Fechar janela\n\
     Alt+F4        — Fechar janela\n\
     Ctrl+Q        — Fechar janela\n\
     Super+M       — Maximizar\n\
     Super+N       — Minimizar\n\
     \n\
     [Sistema]\n\
     Espaco        — Abrir Chat Jarbas\n\
     H             — Ajuda (esta tela)\n\
     Ctrl+Alt+Del  — Menu de energia\n\
     \n\
     [Layout]\n\
     Super+H       — Tile horizontal\n\
     Super+V       — Tile vertical\n\
     Super+Shift+Seta  — Redimensionar tile\n\
     Super+Shift+Space — Alternar flutuante\n\
     Super+D       — Alternar dock\n\
     Super+T       — Alternar tiling"
}