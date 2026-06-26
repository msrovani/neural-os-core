use alloc::vec::Vec;

pub struct Cortex;

impl Cortex {
    pub const fn new() -> Self { Cortex }

    pub fn think(&self, text: &str) -> Intent {
        let lower = text.to_ascii_lowercase();

        if lower.contains("status") || lower.contains("system") || lower.contains("info") {
            Intent::SystemStatus
        } else if lower.contains("echo") || lower.contains("reverse") || lower.contains("repeat") {
            Intent::Echo
        } else if lower.contains("hw") || lower.contains("hardware") {
            Intent::HardwareInfo
        } else if lower.contains("trust allow") {
            Intent::TrustAllow
        } else if lower.contains("trust deny") {
            Intent::TrustDeny
        } else if lower.contains("ping") || lower.contains("net") || lower.contains("diag") {
            Intent::Network
        } else if lower.contains("fetch") || lower.contains("http") || lower.contains("get") || lower.contains("download") {
            Intent::HttpFetch
        } else if lower.contains("help") || lower.contains("?") {
            Intent::Help
        } else if lower.contains("conv") || lower.contains("history") || lower.contains("last") {
            Intent::Conversation
        } else if lower.contains("usage") || lower.contains("metrics") || lower.contains("count") {
            Intent::Usage
        } else if lower.contains("hello") || lower.contains("hi") || lower.contains("hey") || lower.contains("ola") || lower.contains("oi") {
            Intent::Greeting
        } else {
            Intent::Chat
        }
    }
}

#[derive(Debug)]
pub enum Intent {
    SystemStatus,
    Echo,
    HardwareInfo,
    TrustAllow,
    TrustDeny,
    Network,
    HttpFetch,
    Help,
    Conversation,
    Usage,
    Greeting,
    Chat,
}

impl Intent {
    pub fn skill_name(&self) -> &'static str {
        match self {
            Intent::SystemStatus => "system_status",
            Intent::Echo => "echo",
            Intent::HardwareInfo => "hardware_info",
            Intent::TrustAllow => "trust_allow",
            Intent::TrustDeny => "trust_deny",
            Intent::Network => "net_diag",
            Intent::HttpFetch => "http_fetch",
            Intent::Help => "help",
            Intent::Conversation => "conversation",
            Intent::Usage => "usage",
            Intent::Greeting => "greeting",
            Intent::Chat => "chat",
        }
    }
}
