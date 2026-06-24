use alloc::string::String;
use alloc::string::ToString;
use crate::tensor::Tensor;

pub const TOPIC_USER_INTENT: &str = "USER_INTENT";
pub const TOPIC_HERMES_RESPONSE: &str = "HERMES_RESPONSE";

const VOCAB: [&str; 16] = [
    "status", "memory", "ram", "cpu", "system",
    "info", "show", "echo", "reverse", "hello",
    "hi", "help", "test", "run", "what", "who",
];

pub enum Command {
    Status,
    Echo(String),
    Help,
    Chat(String),
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
        if name.eq_ignore_ascii_case("help") || name == "?" || name.eq_ignore_ascii_case("h") {
            return Command::Help;
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
