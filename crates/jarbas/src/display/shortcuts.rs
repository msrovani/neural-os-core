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
    OpenChat,                    // Ctrl+Space — abre/foca o chat do Jarbas
    PowerMenu,                   // Ctrl+Alt+Del — mostra menu de desligar
    ShowHelp,                    // H — mostra card de atalhos do teclado
    ToggleHubHealth,             // F12 — painel Hub Health (glass, fora do WM)
    CloseHubHealth,              // Esc — fecha o Hub Health
    ToggleHintsCard,             // F11 (s426) — card H3 ao vivo (fwd/n/stage)
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
            (Modifiers { alt: true, shift: true, .. }, Tab) => Some(CycleWindowReverse),
            (Modifiers { alt: true, .. }, Tab) => Some(CycleWindow),
            (Modifiers::SUPER, Q) => Some(CloseWindow),
            (Modifiers::SUPER, M) => Some(MaximizeWindow),
            (Modifiers::SUPER, N) => Some(MinimizeWindow),
            (Modifiers::SUPER_SHIFT, Space) => Some(ToggleFloating),

            // Ctrl+Space → OpenChat (bare Space é tecla de digitação — NÃO abrir)
            (Modifiers::CTRL, Space) => Some(OpenChat),
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
            // Hub Health (fora do WM): F12 alterna, Esc fecha.
            (Modifiers::NONE, F12) => Some(ToggleHubHealth),
            (Modifiers::NONE, Escape) => Some(CloseHubHealth),
            // Card H3 ao vivo (s426): F11 alterna spawn/update do card de hints.
            (Modifiers::NONE, F11) => Some(ToggleHintsCard),

            _ => None,
        }
    }

    /// Um atalho de 1 tecla (sem modificador) que NÃO deve disparar enquanto um
    /// campo de texto está focado: qualquer tecla que o usuário digita (letras,
    /// dígitos, espaço, pontuação) seria engolida do texto. Teclas de controle
    /// globais (F1-F12, Esc) ficam de fora — ver `is_global_control_key`.
    ///
    /// Regra GERAL, não special-case de 'H': um binding de letra/pontuação futuro
    /// já nasce protegido. Combos com qualquer modificador (Super/Alt/Ctrl/Shift)
    /// continuam disparando normalmente.
    pub fn suppressed_when_typing(combo: KeyCombo) -> bool {
        combo.modifiers == Modifiers::NONE && !is_global_control_key(combo.key)
    }
}

/// Teclas de controle que seguem disparando com 1 tecla mesmo com texto focado
/// (ex.: F12 Hub Health, Esc fechar). Não produzem texto, então não roubam digitação.
fn is_global_control_key(k: KeyCode) -> bool {
    matches!(
        k,
        KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::Escape
    )
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
    (KeyCombo { modifiers: Modifiers::CTRL, key: KeyCode::Space }, WmAction::OpenChat),
    (KeyCombo { modifiers: Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }, key: KeyCode::Delete }, WmAction::PowerMenu),
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::H }, WmAction::ShowHelp),
    (KeyCombo { modifiers: Modifiers::ALT, key: KeyCode::F4 }, WmAction::CloseWindow),
    (KeyCombo { modifiers: Modifiers::CTRL, key: KeyCode::Q }, WmAction::CloseWindow),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Enter }, WmAction::LaunchApp(AppId::HermesChat)),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::D }, WmAction::ToggleDock),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::T }, WmAction::ToggleTiling),
    (KeyCombo { modifiers: Modifiers::SUPER, key: KeyCode::Space }, WmAction::ShowLauncher),
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::F12 }, WmAction::ToggleHubHealth),
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::F11 }, WmAction::ToggleHintsCard),
    (KeyCombo { modifiers: Modifiers::NONE, key: KeyCode::Escape }, WmAction::CloseHubHealth),
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
     Super+1-9     - Trocar workspace\n\
     Super+Left    - Workspace anterior\n\
     Super+Right   - Workspace seguinte\n\
     \n\
     [Janelas]\n\
     Alt+Tab       - Ciclar janelas\n\
     Super+Q       - Fechar janela\n\
     Alt+F4        - Fechar janela\n\
     Ctrl+Q        - Fechar janela\n\
     Super+M       - Maximizar\n\
     Super+N       - Minimizar\n\
     \n\
     [Sistema]\n\
     Ctrl+Space    - Abrir Chat Jarbas\n\
     H             - Ajuda (esta tela)\n\
     F11           - Card Hints H3 (ao vivo)\n\
     Ctrl+Alt+Del  - Menu de energia\n\
     \n\
     [Layout]\n\
     Super+H       - Tile horizontal\n\
     Super+V       - Tile vertical\n\
     Super+Shift+Seta  - Redimensionar tile\n\
     Super+Shift+Space - Alternar flutuante\n\
     Super+D       - Alternar dock\n\
     Super+T       - Alternar tiling"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(mods: Modifiers, key: KeyCode) -> KeyCombo {
        KeyCombo { modifiers: mods, key }
    }

    /// Bug real: digitar 'h' no chat abria o card de ajuda. Com texto focado,
    /// tecla de 1 letra sem modificador é digitação, não comando.
    #[test]
    fn bare_letter_shortcut_is_suppressed_while_typing() {
        assert!(WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::H)));
        assert!(WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::A)));
        assert!(WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::Key1)));
        assert!(WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::Space)));
    }

    /// F12/Esc e qualquer combo com modificador continuam valendo com texto focado.
    #[test]
    fn global_control_and_modified_shortcuts_still_fire_while_typing() {
        assert!(!WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::F12)));
        assert!(!WmAction::suppressed_when_typing(combo(Modifiers::NONE, KeyCode::Escape)));
        assert!(!WmAction::suppressed_when_typing(combo(Modifiers::CTRL, KeyCode::Space)));
        assert!(!WmAction::suppressed_when_typing(combo(Modifiers::SUPER, KeyCode::H)));
        assert!(!WmAction::suppressed_when_typing(combo(Modifiers::ALT, KeyCode::Tab)));
    }

    /// Sem texto focado os atalhos de 1 tecla seguem normais (H abre ajuda).
    #[test]
    fn bare_h_and_ctrl_space_still_map() {
        assert_eq!(
            WmAction::from_keycombo(combo(Modifiers::NONE, KeyCode::H)),
            Some(WmAction::ShowHelp)
        );
        assert_eq!(
            WmAction::from_keycombo(combo(Modifiers::CTRL, KeyCode::Space)),
            Some(WmAction::OpenChat)
        );
    }

    /// Cenário do bug: digitar uma frase com 'h' no chat. Com texto focado,
    /// NENHUMA dessas teclas dispara atalho (o gate corta antes do mapeamento);
    /// sem foco, 'h' ainda mapeia para ShowHelp — o gate é a diferença.
    #[test]
    fn typing_sentence_with_h_triggers_nothing() {
        // "hello how are you"
        let sentence = [
            KeyCode::H, KeyCode::E, KeyCode::L, KeyCode::L, KeyCode::O,
            KeyCode::Space,
            KeyCode::H, KeyCode::O, KeyCode::W,
            KeyCode::Space,
            KeyCode::A, KeyCode::R, KeyCode::E,
            KeyCode::Space,
            KeyCode::Y, KeyCode::O, KeyCode::U,
        ];
        for key in sentence {
            let c = combo(Modifiers::NONE, key);
            assert!(
                WmAction::suppressed_when_typing(c),
                "tecla {:?} não deveria disparar atalho com texto focado",
                key
            );
            // Modela o dispatch: com foco o gate retorna None antes do mapeamento.
            let dispatched = if WmAction::suppressed_when_typing(c) {
                None
            } else {
                WmAction::from_keycombo(c)
            };
            assert_eq!(dispatched, None, "tecla {:?} disparou atalho no chat", key);
        }
        // Prova que o gate (e não a ausência de binding) é o que muda: sem foco 'h' abre ajuda.
        assert_eq!(
            WmAction::from_keycombo(combo(Modifiers::NONE, KeyCode::H)),
            Some(WmAction::ShowHelp)
        );
    }
}