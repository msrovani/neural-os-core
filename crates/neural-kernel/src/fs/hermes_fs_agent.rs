//! HermesFsAgent — conversacao como filesystem.
//! Mount: /chat/
//! write("/chat/send", "mensagem") → envia para LLM
//! read("/chat/last_response") → ultima resposta
//! read("/chat/history") → historico completo

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use crate::fs::FilesystemAgent;
use crate::serial_println;

const HISTORY_MAX: usize = 50;

static CHAT_HISTORY: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static LAST_RESPONSE: Mutex<String> = Mutex::new(String::new());
static CHAT_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct HermesFsAgent;

impl HermesFsAgent {
    pub fn new() -> Self {
        serial_println!("[CHAT-FS] /chat/ pronto.");
        HermesFsAgent
    }
}

impl FilesystemAgent for HermesFsAgent {
    fn name(&self) -> &str { "hermes" }
    fn mount_point(&self) -> &str { "/chat" }

    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        match path.trim_matches('/') {
            "last_response" | "response" => {
                let resp = LAST_RESPONSE.lock();
                if resp.is_empty() { Ok(Vec::from("[Hermes] No response yet\n")) }
                else { Ok(resp.as_bytes().to_vec()) }
            }
            "history" | "log" => {
                let hist = CHAT_HISTORY.lock();
                let mut out = String::from("=== Chat History ===\n");
                for (i, msg) in hist.iter().enumerate() {
                    out.push_str(&alloc::format!("{}: {}\n", i, msg));
                }
                Ok(out.into_bytes())
            }
            "count" | "messages" => {
                let c = CHAT_COUNTER.load(Ordering::Relaxed);
                Ok(alloc::format!("Messages: {}\n", c).into_bytes())
            }
            "info" | "help" => {
                Ok(Vec::from(b"Chat FS:\n/send - write message\n/last_response - read last reply\n/history - conversation log\n/clear - write anything to clear\n"))
            }
            _ => Err("file not found"),
        }
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), &str> {
        let text = core::str::from_utf8(data).unwrap_or("");
        match path.trim_matches('/') {
            "send" | "message" | "" => {
                let msg = alloc::format!("[User] {}", text.trim());
                let mut hist = CHAT_HISTORY.lock();
                hist.push_back(msg);
                if hist.len() > HISTORY_MAX { hist.pop_front(); }
                CHAT_COUNTER.fetch_add(1, Ordering::Relaxed);

                // Publish to EventBus for LLM processing
                let _ = crate::EVENT_BUS.publish(crate::Event {
                    id: CHAT_COUNTER.load(Ordering::Relaxed),
                    topic: String::from(crate::cortex::TOPIC_LLM_REQUEST),
                    payload: data.to_vec(),
                    token: crate::CapabilityToken::Legacy(1),
                });

                // Stub: echo response
                let reply = alloc::format!("[Hermes] Recebido: {} (processed via LLM)\n", text.trim());
                let mut resp = LAST_RESPONSE.lock();
                *resp = reply.clone();
                hist.push_back(reply);
                if hist.len() > HISTORY_MAX { hist.pop_front(); }

                serial_println!("[CHAT-FS] Sent: {}", text.trim());
                Ok(())
            }
            "clear" | "reset" => {
                let mut hist = CHAT_HISTORY.lock();
                hist.clear();
                LAST_RESPONSE.lock().clear();
                serial_println!("[CHAT-FS] History cleared.");
                Ok(())
            }
            _ => Err("file not found"),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<String>, &str> {
        match path.trim_matches('/') {
            "" => Ok(vec![
                String::from("send"), String::from("last_response"),
                String::from("history"), String::from("count"),
                String::from("clear"), String::from("info"),
            ]),
            _ => Ok(Vec::new()),
        }
    }
}
