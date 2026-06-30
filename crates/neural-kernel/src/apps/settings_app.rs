use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::apps::App;

pub struct SettingsApp {
    theme_names: Vec<&'static str>,
    profile_names: Vec<&'static str>,
}

impl SettingsApp {
    pub fn new() -> Self {
        SettingsApp {
            theme_names: crate::display::theme::list_names(),
            profile_names: vec!["engineer", "gamer", "student", "office", "browsing", "multimedia"],
        }
    }
}

impl App for SettingsApp {
    fn name(&self) -> &str { "settings" }
    fn icon_hint(&self) -> &str { "gear sliders config" }
    fn window_size(&self) -> (u32, u32) { (380, 320) }

    fn on_click(&mut self, x: i32, y: i32) -> Option<String> {
        if y >= 30 && y < 60 {
            let idx = (x / 120) as usize;
            if idx < self.theme_names.len() {
                let _ = crate::display::theme::apply(self.theme_names[idx]);
                return Some(alloc::format!("theme {}", self.theme_names[idx]));
            }
        }
        if y >= 80 && y < 110 {
            let idx = (x / 100) as usize;
            if idx < self.profile_names.len() {
                crate::profile::ProfileManager::set_from_name(self.profile_names[idx]);
                return Some(alloc::format!("profile {}", self.profile_names[idx]));
            }
        }
        None
    }

    fn render(&self) -> &[u8] { &[] }
}
