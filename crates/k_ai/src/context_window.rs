//! #388 J.A.R.V.I.S. Context Window Manager.
//! Gerencia a janela de contexto entre Cortex (LLM) + Hermes (orquestrador).
//! Compacta, prioriza, rotaciona mensagens para caber no limite do modelo.

use alloc::collections::VecDeque;
use alloc::string::String;
use k_nano::allocator::TalcBuf;
use k_nano::kjson;

const MAX_TOKENS: usize = 4096;
const EST_TOKENS_PER_CHAR: usize = 4; // ~4 chars per token

/// s443: `role`/`content` saíram de `String` para `TalcBuf`.
///
/// Motivo (não é prefs): o bump allocator nunca devolve memória — o `dealloc`
/// do híbrido é no-op. Então cada mensagem que `maybe_compact` DESCARTA deixa
/// seus bytes para sempre no bump. Como a janela é um consumer de LONGA VIDA
/// com churn (toda conversa adiciona e remove mensagens), o vazamento é
/// monotônico e invisível: a janela do bump (~2030MB) satura por conta de
/// histórico de chat. No TALC o `Drop` do `TalcBuf` devolve o chunk de verdade.
pub struct ContextMessage {
    pub role: TalcBuf,   // "user", "assistant", "system", "tool"
    pub content: TalcBuf,
    pub priority: u8,   // 0=low, 5=normal, 10=critical
    pub tick: u64,
}

pub struct ContextWindow {
    pub messages: VecDeque<ContextMessage>,
    pub max_tokens: usize,
    /// s443: também roteado — `set_system` SUBSTITUI o prompt anterior, e no
    /// bump a string velha vazaria a cada chamada.
    pub system_prompt: TalcBuf,
}

impl ContextWindow {
    pub fn new() -> Self {
        ContextWindow {
            messages: VecDeque::new(),
            max_tokens: MAX_TOKENS,
            system_prompt: TalcBuf::new(),
        }
    }

    pub fn set_system(&mut self, prompt: &str) {
        if let Some(b) = TalcBuf::from_str(prompt) {
            // Substituir = dropar o buffer velho = devolver os bytes (TALC).
            self.system_prompt = b;
        } else {
            // Sem espaço no TALC: mantém o prompt anterior (não degrada a
            // janela) e deixa o número falar no slog.
            kjson!("CTX", "set_system", "refused", "len", prompt.len());
        }
    }

    /// s443: `false` = a mensagem NÃO foi guardada (TALC sem espaço para o
    /// texto). Fail-closed e explícito: o caller pode avisar o usuário em vez
    /// de a janela fingir que lembrou.
    pub fn add(&mut self, role: &str, content: &str, priority: u8) -> bool {
        let (Some(r), Some(c)) = (TalcBuf::from_str(role), TalcBuf::from_str(content)) else {
            kjson!("CTX", role, "add", "refused", 1u32);
            return false;
        };
        let tick = k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        self.messages.push_back(ContextMessage {
            role: r, content: c, priority, tick,
        });
        self.maybe_compact();
        kjson!("CTX", role, "add", "len", content.len(), "prio", priority);
        true
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
                    // O drop de `m` devolve role+content ao TALC (free real).
                    kjson!("CTX", "COMPACT", "drop", "role", m.role.as_str(), "prio", m.priority);
                }
            } else { break; }
        }
    }

    /// Monta o prompt final com system + historico
    pub fn build_prompt(&self) -> String {
        let mut prompt = String::new();
        if !self.system_prompt.is_empty() {
            prompt.push_str(self.system_prompt.as_str());
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
            prompt.push_str(msg.content.as_str());
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
", prefix, msg.content.as_str());
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
static CONTEXT_WINDOW: spin::LazyLock<spin::Mutex<ContextWindow>> = spin::LazyLock::new(|| {
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

/// Convenience: add message to global context window (`false` = recusada).
pub fn add_global(role: &str, content: &str, priority: u8) -> bool {
    CONTEXT_WINDOW.lock().add(role, content, priority)
}

/// s443: bytes da janela de contexto que vivem NO TALC (paper trail da rota).
pub fn talc_bytes() -> usize {
    let cw = CONTEXT_WINDOW.lock();
    let msgs: usize = cw
        .messages
        .iter()
        .map(|m| m.role.bytes() + m.content.bytes())
        .sum();
    msgs + cw.system_prompt.bytes()
}

/// Convenience: set system prompt on global context window.
pub fn set_system_global(prompt: &str) {
    CONTEXT_WINDOW.lock().set_system(prompt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn janela_guarda_e_monta_prompt() {
        let mut cw = ContextWindow::new();
        assert!(cw.add("user", "oi jarbas", 5));
        assert!(cw.add("assistant", "ola", 5));
        let p = cw.build_prompt();
        assert!(p.contains("User: oi jarbas"), "prompt sem a mensagem: {}", p);
        assert!(p.contains("Assistant: ola"), "prompt sem a resposta: {}", p);
        assert_eq!(cw.messages.len(), 2);
    }

    #[test]
    fn compactar_descarta_e_libera() {
        let mut cw = ContextWindow::new();
        cw.max_tokens = 8; // ~32 chars: 3-4 mensagens estouram
        for i in 0..6 {
            assert!(cw.add("user", &alloc::format!("mensagem numero {}", i), 1));
        }
        assert!(
            cw.estimated_tokens() <= cw.max_tokens,
            "compactacao nao respeitou o orcamento: {} > {}",
            cw.estimated_tokens(),
            cw.max_tokens
        );
        // Prioridade 10 (critical) sobrevive ao compact.
        assert!(cw.add("user", "sistema critico", 10));
        let p = cw.build_prompt();
        assert!(p.contains("sistema critico"), "critico foi descartado: {}", p);
    }

    #[test]
    fn set_system_substitui_sem_acumular() {
        let mut cw = ContextWindow::new();
        cw.set_system("primeiro prompt");
        assert_eq!(cw.system_prompt.as_str(), "primeiro prompt");
        let before = cw.system_prompt.bytes();
        cw.set_system("segundo");
        assert_eq!(cw.system_prompt.as_str(), "segundo");
        // O buffer NOVO é menor: a capacidade velha foi devolvida (no bump ela
        // ficaria retida para sempre — é o vazamento que a rota conserta).
        assert!(
            cw.system_prompt.bytes() < before,
            "substituicao deveria liberar capacidade: {} -> {}",
            before,
            cw.system_prompt.bytes()
        );
    }

    #[test]
    fn curated_context_respeita_o_teto() {
        let mut cw = ContextWindow::new();
        for i in 0..8 {
            cw.add("user", &alloc::format!("pergunta longa {}", i), 5);
            cw.add("assistant", &alloc::format!("resposta longa {}", i), 5);
        }
        let ctx = cw.curated_context(64);
        assert!(ctx.len() <= 64, "curated estourou o teto: {}", ctx.len());
        assert!(!ctx.is_empty());
    }

    #[test]
    fn talc_bytes_conta_o_que_esta_vivo() {
        let mut cw = ContextWindow::new();
        assert_eq!(cw.system_prompt.bytes(), 0);
        cw.add("user", "0123456789", 5);
        let m = &cw.messages[0];
        assert_eq!(m.content.as_str(), "0123456789");
        assert!(m.role.bytes() > 0);
        assert!(m.content.bytes() >= 10);
    }
}
