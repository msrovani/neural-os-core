use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use crate::tensor::Tensor;

pub const TOPIC_USER_INTENT: &str = "USER_INTENT";
pub const TOPIC_HERMES_RESPONSE: &str = "HERMES_RESPONSE";

const AUTO_COMPACT_THRESHOLD: usize = 3;

pub struct ConversationTracker {
    cycle_count: usize,
    buffer: Vec<(String, String)>,
}

impl ConversationTracker {
    pub const fn new() -> Self {
        ConversationTracker {
            cycle_count: 0,
            buffer: Vec::new(),
        }
    }

    pub fn record_exchange(&mut self, user_input: &str, hermes_response: &str) {
        self.buffer.push((String::from(user_input), String::from(hermes_response)));
        self.cycle_count = self.buffer.len();
    }

    pub fn needs_compact(&self) -> bool {
        self.cycle_count >= AUTO_COMPACT_THRESHOLD
    }

    pub fn compact(&mut self) -> String {
        let summary = alloc::format!(
            "[auto-compact] {} ciclos de conversa: ultima entrada: '{}', ultima resposta: '{}'",
            self.cycle_count,
            self.buffer.last().map_or("", |(u, _)| u.as_str()),
            self.buffer.last().map_or("", |(_, r)| r.as_str()),
        );
        self.buffer.clear();
        self.cycle_count = 0;
        summary
    }

    pub fn cycle_count(&self) -> usize {
        self.cycle_count
    }
}

const VOCAB: [&str; 16] = [
    "status", "memory", "ram", "cpu", "system",
    "info", "show", "echo", "reverse", "hello",
    "hi", "help", "test", "run", "what", "who",
];

pub enum Command {
    Status,
    Echo(String),
    Help,
    HardwareInfo,
    NetDiag,
    Fetch(String),
    Ping(String),
    TrustAllow(u64, String),
    TrustDeny(u64, String),
    Usage,
    Conversation,
    Chat(String),
    ShowSkills,
    AddSkill,
    RmSkill(String),
    ReloadSkills,
}

pub fn parse_command(line: &str) -> Command {
    let trimmed = line.trim();
    if let Some(cmd) = trimmed.strip_prefix('/') {
        let mut parts = cmd.splitn(2, |c: char| c.is_whitespace());
        let name = parts.next().unwrap_or("");
        if name.eq_ignore_ascii_case("status")
            || name.eq_ignore_ascii_case("stats")
            || name.eq_ignore_ascii_case("mem")
        {
            return Command::Status;
        }
        if name.eq_ignore_ascii_case("echo") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            return Command::Echo(arg);
        }
        if name.eq_ignore_ascii_case("hw") || name.eq_ignore_ascii_case("hardware") || name.eq_ignore_ascii_case("info") {
            return Command::HardwareInfo;
        }
        if name.eq_ignore_ascii_case("netdiag") || name.eq_ignore_ascii_case("netinfo") || name.eq_ignore_ascii_case("network") {
            return Command::NetDiag;
        }
        if name.eq_ignore_ascii_case("trust") {
            let remainder = parts.next().unwrap_or("");
            let mut sub_parts = remainder.splitn(3, |c: char| c.is_whitespace());
            let sub = sub_parts.next().unwrap_or("");
            if sub.eq_ignore_ascii_case("allow") {
                let token_str = sub_parts.next().unwrap_or("0");
                let skill = sub_parts.next().unwrap_or("").to_string();
                if let Ok(token) = token_str.parse::<u64>() {
                    return Command::TrustAllow(token, skill);
                }
            } else if sub.eq_ignore_ascii_case("deny") {
                let token_str = sub_parts.next().unwrap_or("0");
                let skill = sub_parts.next().unwrap_or("").to_string();
                if let Ok(token) = token_str.parse::<u64>() {
                    return Command::TrustDeny(token, skill);
                }
            }
        }
        if name.eq_ignore_ascii_case("fetch") || name.eq_ignore_ascii_case("get") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            return Command::Fetch(arg);
        }
        if name.eq_ignore_ascii_case("ping") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            return Command::Ping(arg);
        }
        if name.eq_ignore_ascii_case("usage") || name.eq_ignore_ascii_case("metrics") {
            return Command::Usage;
        }
        if name.eq_ignore_ascii_case("conv") || name.eq_ignore_ascii_case("conversation") || name.eq_ignore_ascii_case("log") {
            return Command::Conversation;
        }
        if name.eq_ignore_ascii_case("help") || name == "?" || name.eq_ignore_ascii_case("h") {
            return Command::Help;
        }
        if name.eq_ignore_ascii_case("show_skills") || name.eq_ignore_ascii_case("skills") || name.eq_ignore_ascii_case("list_skills") {
            return Command::ShowSkills;
        }
        if name.eq_ignore_ascii_case("add_skill") || name.eq_ignore_ascii_case("learn") {
            return Command::AddSkill;
        }
        if name.eq_ignore_ascii_case("rm_skill") || name.eq_ignore_ascii_case("remove_skill") || name.eq_ignore_ascii_case("forget") {
            let arg = parts.next().unwrap_or("").trim().to_string();
            return Command::RmSkill(arg);
        }
        if name.eq_ignore_ascii_case("reload_skills") || name.eq_ignore_ascii_case("reset_skills") {
            return Command::ReloadSkills;
        }
    }
    Command::Chat(trimmed.to_string())
}

pub struct IntentMlp {
    linear1: crate::nn::Linear,
    linear2: crate::nn::Linear,
}

impl IntentMlp {
    pub fn new() -> Self {
        let w1 = Tensor::from_row_major((8, 16), alloc::vec![
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0,
        ]).unwrap();
        let b1 = Tensor::from_row_major((1, 8), alloc::vec![0.0; 8]).unwrap();
        let l1 = crate::nn::Linear::new(w1, Some(b1));

        let w2 = Tensor::from_row_major((3, 8), alloc::vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0,
            2.0, 2.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0,
        ]).unwrap();
        let b2 = Tensor::from_row_major((1, 3), alloc::vec![0.0; 3]).unwrap();
        let l2 = crate::nn::Linear::new(w2, Some(b2));

        IntentMlp { linear1: l1, linear2: l2 }
    }

    pub fn classify(&self, text: &str) -> u8 {
        let bow = self.bow_encode(text);
        let mut hidden = self.linear1.forward(&bow);
        hidden.apply(crate::nn::silu);
        let logits = self.linear2.forward(&hidden);
        crate::nn::argmax(&logits) as u8
    }

    fn bow_encode(&self, text: &str) -> Tensor {
        let mut bow = alloc::vec![0.0f32; 16];
        for word in text.split_whitespace() {
            for (i, &v) in VOCAB.iter().enumerate() {
                if word.eq_ignore_ascii_case(v) {
                    bow[i] += 1.0;
                    break;
                }
            }
        }
        Tensor::from_row_major((1, 16), bow).unwrap()
    }
}
