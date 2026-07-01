use alloc::string::String;
use alloc::vec::Vec;
use crate::apps::App;

/// Hermes App — chatbot + shell em janela
pub struct HermesApp {
    history: Vec<String>,
    input_buf: String,
    shell_mode: bool,
}

impl HermesApp {
    pub fn new() -> Self {
        HermesApp { history: Vec::new(), input_buf: String::new(), shell_mode: false }
    }
}

impl App for HermesApp {
    fn name(&self) -> &str { "hermes" }
    fn icon_hint(&self) -> &str { "chat bubble neural" }
    fn window_size(&self) -> (u32, u32) { (500, 400) }

    fn on_click(&mut self, _x: i32, _y: i32) -> Option<String> {
        None // Chat input via keyboard, not mouse
    }

    fn render(&self) -> &[u8] {
        &[]
    }
}
