//! App trait + AppRegistry — ciclo de vida de aplicativos desktop.

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
    fn icon_hint(&self) -> &str;
    fn on_click(&mut self, x: i32, y: i32) -> Option<String>;
    fn render(&self) -> &[u8];
    fn window_size(&self) -> (u32, u32);
}

pub struct AppEntry {
    pub app: Box<dyn App>,
    pub window_id: Option<u32>,
}

pub static APP_REGISTRY: Mutex<BTreeMap<&'static str, AppEntry>> = Mutex::new(BTreeMap::new());

pub fn register_app(name: &'static str, app: Box<dyn App>) {
    let (ww, wh) = app.window_size();
    // Try to create a window in the compositor
    let wid = {
        let mut comp = crate::display::compositor::COMPOSITOR.lock();
        comp.as_mut().map(|c| c.create_window(name, ww, wh))
    };
    APP_REGISTRY.lock().insert(name, AppEntry { app, window_id: wid });
}

pub fn app_names() -> Vec<&'static str> {
    APP_REGISTRY.lock().keys().cloned().collect()
}

pub fn init_apps() {
    register_app("hermes", Box::new(hermes_app::HermesApp::new()));
    register_app("settings", Box::new(settings_app::SettingsApp::new()));
    register_app("power", Box::new(power_app::PowerApp::new()));
    let names = app_names();
    let wcount = crate::display::compositor::COMPOSITOR.lock().as_ref().map_or(0, |c| c.windows.len());
    crate::serial_println!("[APPS] {} apps, {} windows no compositor.", names.len(), wcount);
}
