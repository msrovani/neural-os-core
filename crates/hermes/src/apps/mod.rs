//! App trait + AppRegistry — sistema de comandos do Hermes Chat.
//! Sem multi-window — apps expõem comandos via Hermes.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub mod hermes_app;
pub mod settings_app;
pub mod power_app;

pub trait App: Send {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn icon_hint(&self) -> &str { "" }
    fn window_size(&self) -> (u32, u32) { (400, 300) }
    fn on_click(&mut self, _x: i32, _y: i32) -> Option<String> { None }
    fn render(&self) -> &[u8] { &[] }
}

pub struct AppEntry {
    pub app: Box<dyn App>,
}

pub static APP_REGISTRY: Mutex<BTreeMap<&'static str, AppEntry>> = Mutex::new(BTreeMap::new());

pub fn register_app(name: &'static str, app: Box<dyn App>) {
    APP_REGISTRY.lock().insert(name, AppEntry { app });
}

pub fn app_names() -> Vec<&'static str> {
    APP_REGISTRY.lock().keys().cloned().collect()
}

pub fn init_apps() {
    register_app("hermes", Box::new(hermes_app::HermesApp::new()));
    register_app("settings", Box::new(settings_app::SettingsApp::new()));
    register_app("power", Box::new(power_app::PowerApp::new()));
    let names = app_names();
    k_nano::serial_println!("[APPS] {} apps registrados no Hermes Chat.", names.len());
}
