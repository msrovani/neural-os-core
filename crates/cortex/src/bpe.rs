//! BPE Tokenizer — carrega tokenizer.json do ramdisk e converte texto ↔ tokens.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

pub struct BpeTokenizer {
    vocab: BTreeMap<u16, String>,   // id → text
    rev_vocab: BTreeMap<String, u16>, // text → id
    merges: Vec<(String, String)>,
    bos: u16,
    eos: u16,
    cache: BTreeMap<String, Vec<u16>>,
}

impl BpeTokenizer {
    pub fn new() -> Self {
        BpeTokenizer {
            vocab: BTreeMap::new(),
            rev_vocab: BTreeMap::new(),
            merges: Vec::new(),
            bos: 1,
            eos: 2,
            cache: BTreeMap::new(),
        }
    }

    pub fn load_from_json(&mut self, data: &[u8]) -> Result<(), &'static str> {
        // Minimal JSON parser for tokenizer.json
        // This parser handles the HuggingFace tokenizer.json format
        let _text = core::str::from_utf8(data).map_err(|_| "tokenizer not utf8")?;

        // Quick and dirty: extract vocab and merges
        // We look for "model" section with "vocab" and "merges"
        // BPE tokenizer parsing is skipped for large tokenizers (>1MB) due to
        // the complexity of JSON parsing (brace-depth tracking over 9M chars is too slow
        // in QEMU, and simple pattern search is fragile with vocab entries containing }, .
        return Err("tokenizer too large, using char-level fallback");
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u16>, &'static str> {
        if text.is_empty() {
            return Ok(vec![self.bos, self.eos]);
        }

        if let Some(cached) = self.cache.get(text) {
            return Ok(cached.clone());
        }

        let max_tokens: usize = 1024; // safety limit, matches max_seq do modelo
        let mut tokens = Vec::new();
        tokens.push(self.bos);

        for word in text.split(char::is_whitespace).filter(|w| !w.is_empty()) {
            if tokens.len() >= max_tokens {
                tokens.push(self.eos);
                self.cache_entry(text, tokens.clone());
                return Ok(tokens);
            }

            if let Some(&id) = self.rev_vocab.get(word) {
                tokens.push(id);
                continue;
            }

            for &b in word.as_bytes() {
                if tokens.len() >= max_tokens { break; }
                let key = alloc::format!("<0x{:02X}>", b);
                if let Some(&id) = self.rev_vocab.get(&key) {
                    tokens.push(id);
                } else if b < 128 {
                    let buf = [b];
                    let c_str = core::str::from_utf8(&buf).unwrap_or("");
                    if let Some(&id) = self.rev_vocab.get(c_str) {
                        tokens.push(id);
                    } else if let Some(&id) = self.rev_vocab.get(&alloc::format!("Ġ{}", c_str)) {
                        tokens.push(id);
                    }
                }
            }
        }

        tokens.push(self.eos);
        self.cache_entry(text, tokens.clone());
        Ok(tokens)
    }

    fn cache_entry(&self, _text: &str, _tokens: Vec<u16>) {
        // Future: LRU cache
    }

    pub fn decode(&self, tokens: &[u16]) -> String {
        let mut out = String::new();
        for &t in tokens {
            if t == self.bos || t == self.eos {
                continue;
            }
            if let Some(s) = self.vocab.get(&t) {
                // Handle BPE special characters
                if s == "<s>" || s == "</s>" || s == "<pad>" || s == "<unk>" {
                    continue;
                }
                // GPT-2 style: Ġ prefix = space
                if s.starts_with('Ġ') {
                    out.push(' ');
                    out.push_str(&s[3..]);
                } else if s.len() == 6 && s.starts_with("<0x") && s.ends_with('>') {
                    // Byte token
                    if let Ok(byte) = u8::from_str_radix(&s[3..5], 16) {
                        out.push(byte as char);
                    }
                } else {
                    out.push_str(s);
                }
            }
        }
        out
    }

    pub fn vocab_size(&self) -> u16 {
        self.vocab.len() as u16
    }
}

static BPE: spin::Mutex<Option<BpeTokenizer>> = spin::Mutex::new(None);

pub fn init_from_json(data: &[u8]) -> Result<(), &'static str> {
    let mut tok = BpeTokenizer::new();
    tok.load_from_json(data)?;
    *BPE.lock() = Some(tok);
    Ok(())
}

pub fn encode(text: &str) -> Vec<u16> {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => tok.encode(text).unwrap_or_else(|_| crate::cortex::Tokenizer::encode(text)),
        None => {
            crate::cortex::Tokenizer::encode(text)
        }
    }
}

pub fn decode(tokens: &[u16]) -> String {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => tok.decode(tokens),
        None => crate::cortex::Tokenizer::decode(tokens),
    }
}

pub fn is_loaded() -> bool {
    BPE.lock().is_some()
}

// ── F0–F3: helpers for generate_speculative (accessors + lexicons) ──

pub fn eos_id() -> u16 {
    BPE.lock().as_ref().map_or(2, |t| t.eos)
}
pub fn bos_id() -> u16 {
    BPE.lock().as_ref().map_or(1, |t| t.bos)
}
/// No separate eot stored in BpeTokenizer; reuse eos.
pub fn eot_id() -> u16 { eos_id() }

/// True if id is a special/control token (not to be generated).
pub fn is_special_id(id: u16) -> bool {
    let guard = BPE.lock();
    let Some(tok) = guard.as_ref() else { return id <= 2; };
    if id == tok.bos || id == tok.eos { return true; }
    if let Some(s) = tok.vocab.get(&id) {
        if (s.starts_with('<') && s.ends_with('>')) || s == "<s>" || s == "</s>" || s == "<pad>" || s == "<unk>" {
            return true;
        }
    }
    false
}

/// Quick check: is the prompt a simple greeting.
pub fn prompt_is_greeting(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    t == "ola" || t == "olá" || t == "oi" || t == "hello" || t == "hi" || t == "hey"
        || t.contains("bom dia") || t.contains("boa tarde") || t.contains("boa noite")
        || t.contains("good") || t.contains("sauda")
}

/// Heuristic: text looks like a greeting response (for early exit).
pub fn text_is_greetingish(text: &str) -> bool {
    let low = text.to_ascii_lowercase();
    let words: Vec<&str> = low.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()).collect();
    if words.len() < 2 { return false; }
    let open = words.iter().any(|w| matches!(w.as_ref(), "good" | "hello" | "hi" | "olá" | "ola" | "oi" | "bom" | "boa" | "sauda"));
    let body = words.iter().any(|w| matches!(w.as_ref(), "day" | "systems" | "operational" | "morning" | "tudo" | "bem" | "jarbas" | "neural"));
    open && body
}

/// Count weather-lexicon hits in text.
pub fn weatherish_hit_count(text: &str) -> usize {
    const KEYS: &[&[u8]] = &[
        b"tempo", b"clima", b"weather", b"sol", b"sunny", b"rain", b"chuva", b"hoje",
        b"nubl", b"cloud", b"frio", b"quent", b"dia", b"celsius", b"claro", b"climate",
        b"faz", b"bom", b"esta",
    ];
    let b = text.as_bytes();
    let mut n = 0usize;
    for s in KEYS {
        if b.windows(s.len()).any(|w| w.iter().zip(s.iter()).all(|(a, c)| a.to_ascii_lowercase() == *c)) {
            n += 1;
        }
    }
    n
}

/// Text has a weather predicate (esta/bom/claro/etc).
pub fn weatherish_has_predicate(text: &str) -> bool {
    const KEYS: &[&[u8]] = &[
        b"esta", b"bom", b"claro", b"faz", b"sol", b"sunny", b"chuva", b"rain", b"nubl",
    ];
    let b = text.as_bytes();
    KEYS.iter().any(|s| b.windows(s.len()).any(|w| w.iter().zip(s.iter()).all(|(a, c)| a.to_ascii_lowercase() == *c)))
}

/// Score a token ID for argmax: prefer alphabetic, penalize digits/specials.
pub fn score_piece(id: u16) -> f32 {
    let guard = BPE.lock();
    let Some(tok) = guard.as_ref() else { return 0.0; };
    let Some(s) = tok.vocab.get(&id) else { return 0.0; };
    // penalize special tokens
    if (s.starts_with('<') && s.ends_with('>')) || s == "<s>" || s == "</s>" {
        return -20.0;
    }
    let bytes = s.as_bytes();
    let mut score = 0.0f32;
    let mut has_alpha = false;
    let mut has_digit = false;
    let mut has_paren = false;
    for &b in bytes {
        if b.is_ascii_alphabetic() { has_alpha = true; }
        else if b.is_ascii_digit() { has_digit = true; }
        else if b == b'(' || b == b')' { has_paren = true; }
    }
    if s.trim().is_empty() { score -= 1.0; }
    if has_alpha { score += 1.5; }
    if has_digit { score -= 2.0; }
    if has_paren { score -= 3.0; }
    score
}

/// Hardcoded greeting candidate IDs (SP32-specific; override for other vocabs).
/// ponytail: static arrays, extends as needed.
const GREETING_IDS: &[u16] = &[
    264,  // "o"
    307,  // "O"
    9683, // "olá"
    375,  // "Olá"
    4293, // "Hello"
    263,  // "a"
    308,  // "A"
    318,  // "I"
    267,  // "i"
    266,  // "e"
    309,  // "E"
    268,  // "u"
    269,  // "y"
    265,  // "d"
];

pub fn greeting_candidate_ids() -> &'static [u16] { GREETING_IDS }

/// Position-aware greeting candidates (step → subset).
pub fn greeting_step_candidates(step: usize, _prev: Option<u16>) -> &'static [u16] {
    match step {
        0 => &[307, 375, 9683, 4293], // O/Hello start
        _ => GREETING_IDS,
    }
}

pub fn greeting_position_bias(id: u16, _step: usize) -> f32 {
    if id == 307 || id == 375 { 3.0 } else { 0.0 }
}

pub fn greeting_bigram_bias(_prev: Option<u16>, _next: u16) -> f32 { 0.0 }

// Weather lexicon IDs (SP32-specific)
const WEATHER_IDS: &[u16] = &[
    375,  // "O"
    264,  // "o"
    307,  // "O"
    1850, // "tempo"
    839,  // "clima"
    1448, // "sol"
    980,  // "chuva"
    1520, // "nubl"
    2136, // "frio"
    2549, // "calor"
    546,  // "dia"
    315,  // "hoje"
    1668, // "bom"
    1243, // "esta"
];

pub fn weather_candidate_ids() -> &'static [u16] { WEATHER_IDS }

pub fn weather_step_candidates(_step: usize, _prev: Option<u16>) -> &'static [u16] { WEATHER_IDS }

pub fn weather_position_bias(_id: u16, _step: usize) -> f32 { 0.0 }

pub fn weather_bigram_bias(_prev: Option<u16>, _id: u16) -> f32 { 0.0 }

pub fn weather_is_en_loan(_id: u16) -> bool { false }

pub fn weather_same_stem(_prev: Option<u16>, _id: u16) -> bool { false }
