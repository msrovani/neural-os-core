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

/// Retorna o conteúdo atual do overlay (últimas linhas).
pub fn get_overlay_text() -> String {
    let txt = OVERLAY_TEXT.lock();
    // Limita a ~2000 chars p/ não estourar memória
    let slice = if txt.len() > 2000 {
        let start = txt.len() - 2000;
        // Tenta quebrar no início de uma linha
        let s = &txt[start..];
        if let Some(nl) = s.find('\n') {
            &txt[start + nl + 1..]
        } else {
            s
        }
    } else {
        txt.as_str()
    };
    String::from(slice)
}

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
