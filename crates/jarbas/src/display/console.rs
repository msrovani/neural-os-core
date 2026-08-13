//! Hermes overlay text — buffer de linhas para o painel direito do compositor.
//!
//! Diversos agentes publicam HERMES_RESPONSE no EventBus, que o ConsoleAgent
//! (em neural-kernel) coleta e alimenta neste buffer. O compositor lê e
//! renderiza no painel direito.

use alloc::string::{String, ToString};
use spin::Mutex;

/// Buffer circular de texto para o painel Hermes (direito).
/// Alimentado pelo ConsoleAgent (neural-kernel) quando recebe HERMES_RESPONSE.
pub static OVERLAY_TEXT: Mutex<String> = Mutex::new(String::new());

/// Adiciona linha ao buffer overlay.
pub fn push_overlay_line(line: &str) {
    let mut txt = OVERLAY_TEXT.lock();
    txt.push_str(line);
    txt.push('\n');
    // Mantém tamanho gerenciável (~4KB)
    if txt.len() > 4096 {
        let remove = txt.len() - 2048;
        if let Some(nl) = txt[remove..].find('\n') {
            let cut = remove + nl + 1;
            let remaining = txt[cut..].to_string();
            *txt = remaining;
        }
    }
}
