//! Theme Engine — sistema de temas com 5 esquemas de cor + COSMIC design tokens.
//! Hot-swap via /theme <nome>. Persiste via BootTrustAgent.

use alloc::vec::Vec;
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
            bg: (0x0F, 0x0F, 0x12),
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
            terminal_bg: (0x0F, 0x0F, 0x12),
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

pub static ACTIVE_THEME: Mutex<usize> = Mutex::new(0);
pub static THEME_MODE: Mutex<ThemeMode> = Mutex::new(ThemeMode::Dark);

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

pub fn current() -> &'static Theme {
    &THEMES[*ACTIVE_THEME.lock()]
}

pub fn current_mode() -> ThemeMode {
    *THEME_MODE.lock()
}

pub fn current_theme() -> &'static Theme {
    let mode = *THEME_MODE.lock();
    match mode {
        ThemeMode::Dark => &COSMIC_DARK,
        ThemeMode::Light => &COSMIC_LIGHT,
        ThemeMode::HighContrast => &HIGH_CONTRAST,
    }
}

pub fn apply(name: &str) -> Result<(), &'static str> {
    for (i, t) in THEMES.iter().enumerate() {
        if t.name == name {
            *ACTIVE_THEME.lock() = i;
            k_nano::slog_jarbas!("THEME", "info", "Aplicado: {}", name);
            return Ok(());
        }
    }
    Err("Theme not found")
}

pub fn set_mode(mode: ThemeMode) {
    *THEME_MODE.lock() = mode;
}

pub fn toggle_mode() {
    let mut mode = THEME_MODE.lock();
    *mode = match *mode {
        ThemeMode::Dark => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::HighContrast,
        ThemeMode::HighContrast => ThemeMode::Dark,
    };
}

pub fn list_names() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.name).collect()
}
