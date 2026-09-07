//! #388 J.A.R.V.I.S. Context Window Manager.
//! Gerencia a janela de contexto entre Cortex (LLM) + Hermes (orquestrador).
//! Compacta, prioriza, rotaciona mensagens para caber no limite do modelo.

use alloc::collections::VecDeque;
use alloc::string::String;
use k_nano::kjson;

const MAX_TOKENS: usize = 4096;
const EST_TOKENS_PER_CHAR: usize = 4; // ~4 chars per token

#[derive(Clone)]
pub struct ContextMessage {
    pub role: String,   // "user", "assistant", "system", "tool"
    pub content: String,
    pub priority: u8,   // 0=low, 5=normal, 10=critical
    pub tick: u64,
}

pub struct ContextWindow {
    pub messages: VecDeque<ContextMessage>,
    pub max_tokens: usize,
    pub system_prompt: String,
}

impl ContextWindow {
    pub fn new() -> Self {
        ContextWindow {
            messages: VecDeque::new(),
            max_tokens: MAX_TOKENS,
            system_prompt: String::new(),
        }
    }

    pub fn set_system(&mut self, prompt: &str) {
        self.system_prompt = String::from(prompt);
    }

    pub fn add(&mut self, role: &str, content: &str, priority: u8) {
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        self.messages.push_back(ContextMessage {
            role: String::from(role), content: String::from(content),
            priority, tick,
        });
        self.maybe_compact();
        kjson!("CTX", role, "add", "len", content.len(), "prio", priority);
    }

    /// Remove mensagens de baixa prioridade quando o orcamento estourar
    fn maybe_compact(&mut self) {
        loop {
            let used = self.estimated_tokens();
            if used <= self.max_tokens { break; }
            let idx = self.messages.iter().enumerate()
                .filter(|(_, m)| m.priority < 10)
                .min_by_key(|(_, m)| (m.priority, m.tick))
                .map(|(i, _)| i);
            if let Some(i) = idx {
                let removed = self.messages.remove(i);
                if let Some(m) = removed {
                    kjson!("CTX", "COMPACT", "drop", "role", &m.role, "prio", m.priority);
                }
            } else { break; }
        }
    }

    /// Monta o prompt final com system + historico
    pub fn build_prompt(&self) -> String {
        let mut prompt = String::new();
        if !self.system_prompt.is_empty() {
            prompt.push_str(&self.system_prompt);
            prompt.push('\n');
        }
        for msg in &self.messages {
            let prefix = match msg.role.as_str() {
                "user" => "User: ",
                "assistant" => "Assistant: ",
                "tool" => "Tool: ",
                _ => "",
            };
            prompt.push_str(prefix);
            prompt.push_str(&msg.content);
            prompt.push('\n');
        }
        prompt
    }

    /// Curated context: retorna as ultimas N exchanges formatadas para LLM.
    pub fn curated_context(&self, max_chars: usize) -> String {
        let mut ctx = String::new();
        for msg in self.messages.iter().rev().take(10) {
            let prefix = match msg.role.as_str() {
                "user" => "User: ",
                "assistant" => "Assistant: ",
                _ => continue,
            };
            let line = alloc::format!("{}{}
", prefix, msg.content);
            if ctx.len() + line.len() > max_chars { break; }
            ctx.push_str(&line);
        }
        ctx
    }

    fn estimated_tokens(&self) -> usize {
        let total_chars: usize = self.messages.iter().map(|m| m.content.len()).sum::<usize>()
            + self.system_prompt.len();
        total_chars / EST_TOKENS_PER_CHAR
    }

    pub fn status(&self) -> String {
        alloc::format!("[CTX] {} msgs, ~{} tokens / {} max", self.messages.len(), self.estimated_tokens(), self.max_tokens)
    }
}

/// Global singleton - acessivel por CortexAgent, HermesAgent, e qualquer modulo.
static CONTEXT_WINDOW: spin::Lazy<spin::Mutex<ContextWindow>> = spin::Lazy::new(|| {
    spin::Mutex::new(ContextWindow::new())
});

/// Acesso global ao ContextWindow.
pub fn context_window() -> &'static spin::Mutex<ContextWindow> {
    &CONTEXT_WINDOW
}

/// Convenience: build_prompt lock-free snapshot.
pub fn build_prompt_global() -> String {
    CONTEXT_WINDOW.lock().build_prompt()
}

/// Convenience: curated context for LLM prompt enrichment.
pub fn curated_context_global(max_chars: usize) -> String {
    CONTEXT_WINDOW.lock().curated_context(max_chars)
}

/// Convenience: add message to global context window.
pub fn add_global(role: &str, content: &str, priority: u8) {
    CONTEXT_WINDOW.lock().add(role, content, priority);
}

/// Convenience: set system prompt on global context window.
pub fn set_system_global(prompt: &str) {
    CONTEXT_WINDOW.lock().set_system(prompt);
}
