//! Theme Engine — sistema de temas com 5 esquemas de cor.
//! Hot-swap via /theme <nome>. Persiste via BootTrustAgent.

use alloc::vec::Vec;
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: (u8, u8, u8),
    pub fg: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub secondary: (u8, u8, u8),
    pub error: (u8, u8, u8),
    pub success: (u8, u8, u8),
    pub terminal_bg: (u8, u8, u8),
}

impl Theme {
    pub const fn new(name: &'static str, bg: (u8,u8,u8), fg: (u8,u8,u8), accent: (u8,u8,u8),
        secondary: (u8,u8,u8), error: (u8,u8,u8), success: (u8,u8,u8), terminal_bg: (u8,u8,u8)) -> Self {
        Theme { name, bg, fg, accent, secondary, error, success, terminal_bg }
    }
}

pub static THEMES: [Theme; 5] = [
    Theme::new("hermes-dark", (10,10,20), (200,200,220), (0,200,200), (80,130,220), (220,50,50), (0,200,100), (15,15,30)),
    Theme::new("dracula", (30,30,45), (220,220,240), (255,120,200), (150,100,255), (255,80,80), (80,250,120), (35,35,55)),
    Theme::new("matrix", (0,10,0), (0,220,0), (0,255,50), (0,150,0), (255,50,50), (0,255,0), (0,15,0)),
    Theme::new("solarized", (0,45,55), (150,180,180), (50,160,160), (100,130,130), (220,50,50), (80,180,80), (5,55,65)),
    Theme::new("hermes-light", (230,230,240), (30,30,50), (0,120,180), (80,100,180), (200,40,40), (0,150,80), (240,240,250)),
];

pub static ACTIVE_THEME: Mutex<usize> = Mutex::new(0); // index in THEMES

pub fn current() -> &'static Theme {
    &THEMES[*ACTIVE_THEME.lock()]
}

pub fn apply(name: &str) -> Result<(), &'static str> {
    for (i, t) in THEMES.iter().enumerate() {
        if t.name == name {
            *ACTIVE_THEME.lock() = i;
            crate::serial_println!("[THEME] Aplicado: {}", name);
            return Ok(());
        }
    }
    Err("Theme not found")
}

pub fn list_names() -> Vec<&'static str> {
    THEMES.iter().map(|t| t.name).collect()
}
