//! Theme Engine — sistema de temas com 5 esquemas de cor + COSMIC design tokens.
//! Hot-swap via /theme <nome>. Persiste via BootTrustAgent.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use spin::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
    HighContrast,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: (u8, u8, u8),
    pub bg_alt: (u8, u8, u8),
    pub fg: (u8, u8, u8),
    pub fg_muted: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub accent_hover: (u8, u8, u8),
    pub border: (u8, u8, u8),
    pub card_bg: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub error: (u8, u8, u8),
    pub success: (u8, u8, u8),
    pub warning: (u8, u8, u8),
    pub terminal_bg: (u8, u8, u8),
}

impl Theme {
    pub const fn new(
        name: &'static str,
        bg: (u8,u8,u8), bg_alt: (u8,u8,u8),
        fg: (u8,u8,u8), fg_muted: (u8,u8,u8),
        accent: (u8,u8,u8), accent_hover: (u8,u8,u8),
        border: (u8,u8,u8), card_bg: (u8,u8,u8),
        secondary: (u8,u8,u8), error: (u8,u8,u8),
        success: (u8,u8,u8), warning: (u8,u8,u8),
        terminal_bg: (u8,u8,u8)
    ) -> Self {
        Theme { name, bg, bg_alt, fg, fg_muted, accent, accent_hover, border, card_bg, secondary, error, success, warning, terminal_bg }
    }

    pub fn cosmic_dark() -> Self {
        Self {
            name: "cosmic-dark",
            bg: (8, 12, 24),
            bg_alt: (0x18, 0x18, 0x1C),
            fg: (0xE8, 0xE8, 0xE8),
            fg_muted: (0x88, 0x88, 0x88),
            accent: (0xFF, 0x8C, 0x00),
            accent_hover: (0xCC, 0x70, 0x00),
            border: (0x2A, 0x2A, 0x30),
            card_bg: (0x14, 0x14, 0x18),
            secondary: (0x2A, 0x2A, 0x30),
            error: (0xF4, 0x43, 0x36),
            success: (0x4C, 0xAF, 0x50),
            warning: (0xFF, 0xB3, 0x00),
            terminal_bg: (8, 12, 24),
        }
    }

    pub fn cosmic_light() -> Self {
        Self {
            name: "cosmic-light",
            bg: (0xF5, 0xF5, 0xF5),
            bg_alt: (0xFF, 0xFF, 0xFF),
            fg: (0x1A, 0x1A, 0x1A),
            fg_muted: (0x66, 0x66, 0x66),
            accent: (0xE6, 0x73, 0x00),
            accent_hover: (0xCC, 0x60, 0x00),
            border: (0xDD, 0xDD, 0xDD),
            card_bg: (0xFF, 0xFF, 0xFF),
            secondary: (0xEE, 0xEE, 0xEE),
            error: (0xD3, 0x2F, 0x2F),
            success: (0x38, 0x8E, 0x3C),
            warning: (0xF5, 0x7F, 0x17),
            terminal_bg: (0xF5, 0xF5, 0xF5),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            name: "high-contrast",
            bg: (0x00, 0x00, 0x00),
            bg_alt: (0x11, 0x11, 0x11),
            fg: (0xFF, 0xFF, 0xFF),
            fg_muted: (0xAA, 0xAA, 0xAA),
            accent: (0xFF, 0xFF, 0x00),
            accent_hover: (0xCC, 0xCC, 0x00),
            border: (0xFF, 0xFF, 0xFF),
            card_bg: (0x11, 0x11, 0x11),
            secondary: (0x22, 0x22, 0x22),
            error: (0xFF, 0x00, 0x00),
            success: (0x00, 0xFF, 0x00),
            warning: (0xFF, 0xFF, 0x00),
            terminal_bg: (0x00, 0x00, 0x00),
        }
    }
}

pub static THEMES: [Theme; 5] = [
    Theme::new("hermes-dark", (10,10,20), (15,15,30), (200,200,220), (150,150,170), (0,200,200), (0,180,180), (80,130,220), (20,20,30), (80,130,220), (220,50,50), (0,200,100), (255,180,0), (15,15,30)),
    Theme::new("dracula", (30,30,45), (35,35,55), (220,220,240), (180,180,200), (255,120,200), (200,100,180), (150,100,255), (40,40,60), (150,100,255), (255,80,80), (80,250,120), (255,180,0), (35,35,55)),
    Theme::new("matrix", (0,10,0), (0,15,0), (0,220,0), (0,180,0), (0,255,50), (0,200,40), (0,150,0), (0,20,0), (0,150,0), (255,50,50), (0,255,0), (255,180,0), (0,15,0)),
    Theme::new("solarized", (0,45,55), (5,55,65), (150,180,180), (100,130,130), (50,160,160), (40,140,140), (100,130,130), (10,50,60), (100,130,130), (220,50,50), (80,180,80), (255,180,0), (5,55,65)),
    Theme::new("hermes-light", (230,230,240), (240,240,250), (30,30,50), (60,60,80), (0,120,180), (0,100,160), (80,100,180), (240,240,250), (80,100,180), (200,40,40), (0,150,80), (255,180,0), (240,240,250)),
];

pub static ACTIVE_THEME: AtomicUsize = AtomicUsize::new(0);
static THEME_MODE_ATOMIC: AtomicU8 = AtomicU8::new(0); // 0=Dark, 1=Light, 2=HighContrast

// Static theme constants for cosmic modes (used by current_theme).
const COSMIC_DARK: Theme = Theme::new(
    "cosmic-dark",
    (0x0F,0x0F,0x12), (0x18,0x18,0x1C),
    (0xE8,0xE8,0xE8), (0x88,0x88,0x88),
    (0xFF,0x8C,0x00), (0xCC,0x70,0x00),
    (0x2A,0x2A,0x30), (0x14,0x14,0x18),
    (0x2A,0x2A,0x30), (0xF4,0x43,0x36),
    (0x4C,0xAF,0x50), (0xFF,0xB3,0x00),
    (0x0F,0x0F,0x12),
);
const COSMIC_LIGHT: Theme = Theme::new(
    "cosmic-light",
    (0xF5,0xF5,0xF5), (0xFF,0xFF,0xFF),
    (0x1A,0x1A,0x1A), (0x66,0x66,0x66),
    (0xE6,0x73,0x00), (0xCC,0x60,0x00),
    (0xDD,0xDD,0xDD), (0xFF,0xFF,0xFF),
    (0xEE,0xEE,0xEE), (0xD3,0x2F,0x2F),
    (0x38,0x8E,0x3C), (0xF5,0x7F,0x17),
    (0xF5,0xF5,0xF5),
);
const HIGH_CONTRAST: Theme = Theme::new(
    "high-contrast",
    (0x00,0x00,0x00), (0x11,0x11,0x11),
    (0xFF,0xFF,0xFF), (0xAA,0xAA,0xAA),
    (0xFF,0xFF,0x00), (0xCC,0xCC,0x00),
    (0xFF,0xFF,0xFF), (0x11,0x11,0x11),
    (0x22,0x22,0x22), (0xFF,0x00,0x00),
    (0x00,0xFF,0x00), (0xFF,0xFF,0x00),
    (0x00,0x00,0x00),
);

// ── Onda 9 T-070 S5: 1 widget tema — JARVIS high-contrast (ponytail: standalone const
// reuse embedded-graphics DrawTarget já em jarbas; TTF não duplicado — ttf_engine separado).
// 4 cores núcleo: bg preto, fg branco, accent cyan #00D4FF, border branco. Não entra em
// THEMES[5] nem THEME_MODE_ATOMIC para não churnar switcher; promover a THEMES[6]+mode=3 quando UI precisar.
// Se TTF já em eg.rs, apenas validar — eg.rs já tem FbTarget, TTF fica em ttf_engine.rs.
pub const JARVIS_DARK_HIGH_CONTRAST: Theme = Theme::new(
    "jarvis-hc",
    (0x00, 0x00, 0x00),
    (0x0A, 0x0F, 0x14),
    (0xFF, 0xFF, 0xFF),
    (0xC8, 0xD0, 0xD8),
    (0x00, 0xD4, 0xFF),
    (0x00, 0xA8, 0xCC),
    (0xFF, 0xFF, 0xFF),
    (0x0A, 0x0A, 0x0A),
    (0x1A, 0x1A, 0x20),
    (0xFF, 0x3B, 0x30),
    (0x30, 0xFF, 0x90),
    (0xFF, 0xCC, 0x00),
    (0x00, 0x00, 0x00),
);

pub fn current() -> &'static Theme {
    &THEMES[ACTIVE_THEME.load(Ordering::Relaxed)]
}

pub fn current_mode() -> ThemeMode {
    match THEME_MODE_ATOMIC.load(Ordering::Relaxed) {
        1 => ThemeMode::Light,
        2 => ThemeMode::HighContrast,
        _ => ThemeMode::Dark,
    }
}

/// Lock-free theme lookup — called ~30x/frame no render loop.
#[inline(always)]
pub fn current_theme() -> &'static Theme {
    match THEME_MODE_ATOMIC.load(Ordering::Relaxed) {
        1 => &COSMIC_LIGHT,
        2 => &HIGH_CONTRAST,
        _ => &COSMIC_DARK,
    }
}

pub fn apply(name: &str) -> Result<(), &'static str> {
    for (i, t) in THEMES.iter().enumerate() {
        if t.name == name {
            ACTIVE_THEME.store(i, Ordering::Relaxed);
            k_nano::slog_jarbas!("THEME", "info", "Aplicado: {}", name);
            return Ok(());
        }
    }
    Err("Theme not found")
}

pub fn set_mode(mode: ThemeMode) {
    THEME_MODE_ATOMIC.store(mode as u8, Ordering::Relaxed);
}

pub fn toggle_mode() {
    let prev = THEME_MODE_ATOMIC.load(Ordering::Relaxed);
    let next = match prev {
        0 => 1u8, // Dark → Light
        1 => 2,   // Light → HighContrast
        _ => 0,   // HighContrast → Dark
    };
    THEME_MODE_ATOMIC.store(next, Ordering::Relaxed);
}

pub fn list_names() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.name).collect()
}

// ── host tests Onda 9 T-070..072 (ponytail: 1 arquivo, mínimo) ───────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_does_not_panic() {
        // T-070: Theme::current() / current_theme() não quebram após adicionar JARVIS_HC
        let a = current();
        assert!(!a.name.is_empty());
        assert!(a.bg != a.fg, "bg != fg");
        let b = current_theme();
        assert!(!b.name.is_empty());
        // novo tema standalone acessível sem quebrar índices
        assert_eq!(JARVIS_DARK_HIGH_CONTRAST.name, "jarvis-hc");
        assert_eq!(JARVIS_DARK_HIGH_CONTRAST.bg, (0x00, 0x00, 0x00));
        assert_eq!(JARVIS_DARK_HIGH_CONTRAST.accent, (0x00, 0xD4, 0xFF));
        assert_eq!(JARVIS_DARK_HIGH_CONTRAST.border, (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn jarvis_hc_four_core_colors() {
        // 4 cores núcleo distintas (bg/fg/accent/border) — critério T-070
        let t = &JARVIS_DARK_HIGH_CONTRAST;
        assert_ne!(t.bg, t.fg);
        assert_ne!(t.bg, t.accent);
        assert_ne!(t.fg, t.accent);
        assert_ne!(t.border, t.bg);
        // fg branco puro, accent cyan JARVIS
        assert_eq!(t.fg, (0xFF, 0xFF, 0xFF));
        assert_eq!(t.accent, (0x00, 0xD4, 0xFF));
    }

    #[test]
    fn toggle_mode_still_cycles_three() {
        // T-070: THEME_MODE_ATOMIC 0..2 continua — JARVIS_HC standalone não churnou ciclo
        let prev = THEME_MODE_ATOMIC.load(core::sync::atomic::Ordering::Relaxed);
        toggle_mode();
        let after = THEME_MODE_ATOMIC.load(core::sync::atomic::Ordering::Relaxed);
        assert_ne!(prev, after);
        // restaura
        THEME_MODE_ATOMIC.store(prev, core::sync::atomic::Ordering::Relaxed);
        let t = current_theme();
        assert!(t.name == "cosmic-dark" || t.name == "cosmic-light" || t.name == "high-contrast");
    }

    #[test]
    fn tick_does_not_call_render_card_session_261() {
        // T-072: SESSION_261 — tick() nunca desenha cards diretamente; só render() pinta
        let agent_src = include_str!("agent.rs");
        // DisplayAgent::tick não deve conter render_card/draw_card — só spawn_card + render()
        // agent.rs atualmente tem 0 ocorrências; se alguém reintroduzir, este teste quebra
        assert!(
            !agent_src.contains("render_card"),
            "agent.rs tick() must not call render_card directly — SESSION_261"
        );
        assert!(
            !agent_src.contains("draw_card"),
            "agent.rs tick() must not call draw_card — SESSION_261"
        );
        // compositor render deve conter render_card (único lugar permitido)
        let comp_src = include_str!("compositor.rs");
        assert!(comp_src.contains("fn render("), "compositor must have render()");
        assert!(
            comp_src.contains("render_card"),
            "compositor::render must call render_card (allowed painting site)"
        );
        assert!(
            comp_src.contains("self.dock.height") && comp_src.contains("swap_rect"),
            "present_frame must swap dock band or clock stays 00:00"
        );
        // card.rs deve usar FbTarget (DrawTarget reuse, T-070)
        let eg_src = include_str!("eg.rs");
        assert!(eg_src.contains("DrawTarget"), "eg.rs must expose DrawTarget");
        assert!(eg_src.contains("FbTarget"), "eg.rs must have FbTarget");
    }

    #[test]
    fn hda_playback_path_exists_not_uvc() {
        // T-071: HDA playback já existe (k_hal::audio::hda::write_hda_playback); UVC = AWAITING_HW
        // Valida via include_str que o path não foi quebrado: mixer.rs chama write_hda_playback,
        // uvc_driver.rs existe mas é AWAITING_HW (não implementado aqui)
        let mixer_src = include_str!("../audio/mixer.rs");
        assert!(
            mixer_src.contains("write_hda_playback"),
            "mixer must call write_hda_playback (HDA playback path)"
        );
        let uvc_src = include_str!("../uvc_driver.rs");
        assert!(uvc_src.contains("UvcDriverAgent"), "uvc_driver.rs must still exist (AWAITING_HW)");
    }
}
