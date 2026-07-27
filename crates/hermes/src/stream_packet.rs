//! StreamPacket protocol — typed streaming packets para o ciclo LLM.
//! Substitui o texto plano do HERMES_RESPONSE por um protocolo tipado
//! que o DisplayAgent consome para renderizar timeline + mensagens.
//!
//! Inspirado no protocolo Onyx SSE (streaming_models.py).
//! Adaptado para EventBus bare-metal (sem HTTP/SSE).

use alloc::string::String;
use alloc::vec::Vec;

/// Tópico do EventBus para pacotes LLM stream.
pub const TOPIC_LLM_STREAM: &str = "LLM_STREAM";

/// Tipos de ferramenta que o LLM pode chamar.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolKind {
    Search,        // busca web/interna
    UrlFetch,      // HTTP GET
    CodeExec,      // python/bash
    Reasoning,     // chain-of-thought
    Custom(String),// MCP ou action customizada
    Memory,        // recall SGDB
    FileReader,    // leitura de arquivo FAT/SGDB
    DeepResearch,  // pesquisa multi-passo
    Bash,          // comando shell
}

/// Pacote individual do stream LLM.
/// Serializado como payload do EventBus (encode/decode via JSON simplificado ou string delimitada).
#[derive(Debug, Clone)]
pub enum StreamPacket {
    /// Sessão nova — reseta o chat window
    SessionStart { session_id: u32 },

    /// LLM começou a raciocinar (chain-of-thought)
    ReasoningStart,
    /// Token de reasoning streaming
    ReasoningDelta { content: String },
    /// Reasoning completo
    ReasoningDone,

    /// Ferramenta começou a executar
    ToolStart { id: u32, kind: ToolKind, label: String },
    /// Saída parcial da ferramenta
    ToolDelta { id: u32, content: String },
    /// Ferramenta terminou
    ToolDone { id: u32, result_summary: Option<String> },

    /// Resposta final começou a chegar (após tools)
    MessageStart {
        /// Segundos de pré-processamento (tool execution)
        pre_answer_seconds: Option<f32>,
    },
    /// Token da resposta final
    MessageDelta { content: String },

    /// Citação: documento referenciado
    Citation { doc_id: u32, text: String, url: Option<String> },

    /// Mensagem do usuário registrada na árvore
    UserMessage { content: String },

    /// Erro durante processamento
    Error { message: String },

    /// Sinal de fim do stream
    Stop,
}

impl StreamPacket {
    /// Serializa pacote para bytes (payload do EventBus).
    /// Formato: "TYPE|arg1|arg2\n"
    /// Compacto o suficiente para o bare-metal sem depender de serde.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            StreamPacket::SessionStart { session_id } => {
                alloc::format!("SESSION|{}\n", session_id).into_bytes()
            }
            StreamPacket::ReasoningStart => b"REASONING_START|\n".to_vec(),
            StreamPacket::ReasoningDelta { content } => {
                alloc::format!("REASONING_DELTA|{}\n", content).into_bytes()
            }
            StreamPacket::ReasoningDone => b"REASONING_DONE|\n".to_vec(),
            StreamPacket::ToolStart { id, kind, label } => {
                let kind_str = kind.encode();
                alloc::format!("TOOL_START|{}|{}|{}\n", id, kind_str, label).into_bytes()
            }
            StreamPacket::ToolDelta { id, content } => {
                alloc::format!("TOOL_DELTA|{}|{}\n", id, content).into_bytes()
            }
            StreamPacket::ToolDone { id, result_summary } => {
                let summary = result_summary.as_deref().unwrap_or("");
                alloc::format!("TOOL_DONE|{}|{}\n", id, summary).into_bytes()
            }
            StreamPacket::MessageStart { pre_answer_seconds } => {
                if let Some(secs) = pre_answer_seconds {
                    alloc::format!("MSG_START|{:.1}\n", secs).into_bytes()
                } else {
                    b"MSG_START|\n".to_vec()
                }
            }
            StreamPacket::MessageDelta { content } => {
                alloc::format!("MSG_DELTA|{}\n", content).into_bytes()
            }
            StreamPacket::Citation { doc_id, text, url } => {
                let u = url.as_deref().unwrap_or("");
                alloc::format!("CITATION|{}|{}|{}\n", doc_id, text, u).into_bytes()
            }
            StreamPacket::UserMessage { content } => {
                alloc::format!("USER_MSG|{}\n", content).into_bytes()
            }
            StreamPacket::Error { message } => {
                alloc::format!("ERROR|{}\n", message).into_bytes()
            }
            StreamPacket::Stop => b"STOP|\n".to_vec(),
        }
    }

    /// Decodifica bytes do EventBus de volta pra StreamPacket.
    /// Formato: "TYPE|arg1|arg2\n"
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let s = core::str::from_utf8(bytes).ok()?;
        let s = s.trim_end_matches('\n');
        let (type_str, rest) = s.split_once('|')?;
        match type_str {
            "SESSION" => {
                let id = rest.parse().ok()?;
                Some(StreamPacket::SessionStart { session_id: id })
            }
            "REASONING_START" => Some(StreamPacket::ReasoningStart),
            "REASONING_DELTA" => Some(StreamPacket::ReasoningDelta { content: rest.into() }),
            "REASONING_DONE" => Some(StreamPacket::ReasoningDone),
            "TOOL_START" => {
                let parts: Vec<&str> = rest.splitn(3, '|').collect();
                if parts.len() < 3 { return None; }
                let id = parts[0].parse().ok()?;
                let kind = ToolKind::decode(parts[1])?;
                Some(StreamPacket::ToolStart { id, kind, label: parts[2].into() })
            }
            "TOOL_DELTA" => {
                let (id, content) = rest.split_once('|')?;
                Some(StreamPacket::ToolDelta { id: id.parse().ok()?, content: content.into() })
            }
            "TOOL_DONE" => {
                let (id, summary) = rest.split_once('|').unwrap_or((rest, ""));
                let summary = if summary.is_empty() { None } else { Some(summary.into()) };
                Some(StreamPacket::ToolDone { id: id.parse().ok()?, result_summary: summary })
            }
            "MSG_START" => {
                let secs = if rest.is_empty() { None } else { rest.parse().ok() };
                Some(StreamPacket::MessageStart { pre_answer_seconds: secs })
            }
            "MSG_DELTA" => Some(StreamPacket::MessageDelta { content: rest.into() }),
            "CITATION" => {
                let parts: Vec<&str> = rest.splitn(3, '|').collect();
                if parts.len() < 3 { return None; }
                let url = if parts[2].is_empty() { None } else { Some(parts[2].into()) };
                Some(StreamPacket::Citation { doc_id: parts[0].parse().ok()?, text: parts[1].into(), url })
            }
            "USER_MSG" => Some(StreamPacket::UserMessage { content: rest.into() }),
            "ERROR" => Some(StreamPacket::Error { message: rest.into() }),
            "STOP" => Some(StreamPacket::Stop),
            _ => None,
        }
    }
}

impl ToolKind {
    pub fn encode(&self) -> &'static str {
        match self {
            ToolKind::Search => "search",
            ToolKind::UrlFetch => "fetch",
            ToolKind::CodeExec => "code",
            ToolKind::Reasoning => "reason",
            ToolKind::Custom(_) => "custom",
            ToolKind::Memory => "memory",
            ToolKind::FileReader => "file",
            ToolKind::DeepResearch => "deep_research",
            ToolKind::Bash => "bash",
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "search" => Some(ToolKind::Search),
            "fetch" => Some(ToolKind::UrlFetch),
            "code" => Some(ToolKind::CodeExec),
            "reason" => Some(ToolKind::Reasoning),
            "custom" => Some(ToolKind::Custom(String::new())),
            "memory" => Some(ToolKind::Memory),
            "file" => Some(ToolKind::FileReader),
            "deep_research" => Some(ToolKind::DeepResearch),
            "bash" => Some(ToolKind::Bash),
            _ => None,
        }
    }
}
