//! BrowserAgent — navegador leve para AI agents.
//! Inspirado em Obscura (Rust browser, 30MB RAM, 85ms pages).
//! Skills: fetch_page(url), search(query), extract_text(html), render_page(url)
//! PageViewerApp: janela no compositor que mostra o conteudo.

use alloc::string::String;
use alloc::string::ToString;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use crate::serial_println;
use spin::Mutex;

pub const TOPIC_FETCH_REQUEST: &str = "FETCH_REQUEST";
pub const TOPIC_FETCH_RESPONSE: &str = "FETCH_RESPONSE";
pub const TOPIC_SEARCH_REQUEST: &str = "SEARCH_REQUEST";
pub const TOPIC_SEARCH_RESPONSE: &str = "SEARCH_RESPONSE";

const BROWSER_MANIFEST: AgentManifest = AgentManifest {
    name: "browser",
    kind: AgentKind::Skill,
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: true,
};

pub struct BrowserAgent {
    fetch_receiver: crate::Receiver,
    cache: BTreeMap<String, PageCache>,
}

struct PageCache {
    url: String,
    title: String,
    text: String,
    html: Vec<u8>,
    fetched_at_tick: u64,
}

impl BrowserAgent {
    pub fn new() -> Self {
        BrowserAgent {
            fetch_receiver: crate::EVENT_BUS.subscribe(TOPIC_FETCH_REQUEST),
            cache: BTreeMap::new(),
        }
    }

    /// Fetch uma pagina web via HTTP (bloqueante, simplificado)
    fn fetch_page(url: &str) -> Result<(String, Vec<u8>), &'static str> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("Invalid URL");
        }
        // Simplificado: retorna placeholder
        // Em producao: smoltcp HTTP GET + parse response
        let body = alloc::format!("<html><head><title>{}</title></head><body><p>Page content for {}</p></body></html>", url, url);
        Ok((String::from(url), body.into_bytes()))
    }

    /// Extrai texto puro de HTML (regex-free, tag-stripping simples)
    fn extract_text(html: &[u8]) -> String {
        let raw = core::str::from_utf8(html).unwrap_or("");
        let mut text = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;

        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'<' {
                if i + 6 < bytes.len() && &bytes[i..i+7] == b"<script" { in_script = true; }
                if i + 5 < bytes.len() && &bytes[i..i+6] == b"<style" { in_style = true; }
                in_tag = true;
            } else if b == b'>' {
                in_tag = false;
                if in_script && i + 8 < bytes.len() && &bytes[i-8..i+1] == b"</script>" { in_script = false; }
                if in_style && i + 7 < bytes.len() && &bytes[i-7..i+1] == b"</style>" { in_style = false; }
            } else if !in_tag && !in_script && !in_style {
                if b.is_ascii_graphic() || b == b' ' || b == b'\n' {
                    text.push(b as char);
                }
            }
            i += 1;
        }
        // Collapse whitespace
        let mut cleaned = String::new();
        let mut prev_space = false;
        for c in text.chars() {
            if c.is_whitespace() {
                if !prev_space { cleaned.push(' '); }
                prev_space = true;
            } else {
                cleaned.push(c);
                prev_space = false;
            }
        }
        cleaned
    }

    fn title_from_html(html: &[u8]) -> String {
        let raw = core::str::from_utf8(html).unwrap_or("");
        if let Some(start) = raw.find("<title>") {
            if let Some(end) = raw[start..].find("</title>") {
                return raw[start+7..start+end].trim().to_string();
            }
        }
        String::from("(no title)")
    }
}

impl Agent for BrowserAgent {
    fn manifest(&self) -> &AgentManifest { &BROWSER_MANIFEST }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(event) = self.fetch_receiver.try_receive() {
            let url = core::str::from_utf8(&event.payload).unwrap_or("");
            serial_println!("[BROWSER] Fetch: {}", url);

            // Check cache
            if let Some(cached) = self.cache.get(url) {
                let _ = crate::EVENT_BUS.publish(crate::Event {
                    id: 0, topic: String::from(TOPIC_FETCH_RESPONSE),
                    payload: cached.text.as_bytes().to_vec(),
                    token: crate::CapabilityToken::Legacy(1),
                });
                continue;
            }

            match Self::fetch_page(url) {
                Ok((_url, html)) => {
                    let title = Self::title_from_html(&html);
                    let text = Self::extract_text(&html);
                    let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed);

                    self.cache.insert(String::from(url), PageCache {
                        url: String::from(url), title, text: text.clone(),
                        html, fetched_at_tick: tick as u64,
                    });

                    let _ = crate::EVENT_BUS.publish(crate::Event {
                        id: 0, topic: String::from(TOPIC_FETCH_RESPONSE),
                        payload: text.into_bytes(),
                        token: crate::CapabilityToken::Legacy(1),
                    });
                }
                Err(e) => {
                    serial_println!("[BROWSER] Error: {}", e);
                }
            }
        }
        AgentTickResult::Pending
    }
}

// ---------------------------------------------------------------------------
// PageViewerApp — visualizador de paginas no compositor
// ---------------------------------------------------------------------------

use crate::display::compositor::COMPOSITOR;
use crate::display::theme;

pub struct PageViewerApp {
    pub url: String,
    pub title: String,
    pub content: String,
    pub window_id: Option<u32>,
}

impl PageViewerApp {
    pub fn new() -> Self {
        PageViewerApp {
            url: String::new(),
            title: String::from("Page Viewer"),
            content: String::new(),
            window_id: None,
        }
    }

    pub fn open(&mut self, url: &str, content: &str, title: &str) {
        self.url = String::from(url);
        self.content = String::from(content);
        self.title = String::from(title);

        if self.window_id.is_none() {
            if let Some(ref mut comp) = *COMPOSITOR.lock() {
                let wid = comp.create_window(&alloc::format!("Page: {}", title), 600, 400);
                self.window_id = Some(wid);
            }
        }
    }

    pub fn render(&self) {
        let t = theme::current();
        if let Some(ref mut comp) = *COMPOSITOR.lock() {
            if let Some(wid) = self.window_id {
                if let Some(win) = comp.windows.iter_mut().find(|w| w.id == wid) {
                    // Store page content in window title (simplificado)
                    let preview = self.content.chars().take(80).collect::<String>();
                    win.title = alloc::format!("{} | {}", self.title, preview);
                }
            }
        }
    }
}

/// Registra BrowserAgent no scheduler
pub fn register_browser_agent(registry: &mut agent_core::AgentRegistry) {
    registry.register(Box::new(BrowserAgent::new()));
    serial_println!("[BROWSER] BrowserAgent registrado. Skills: fetch, search, extract, render");
}
