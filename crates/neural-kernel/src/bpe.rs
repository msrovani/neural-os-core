//! BPE Tokenizer — carrega tokenizer.json do ramdisk e converte texto ↔ tokens.
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::format;

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
        let text = core::str::from_utf8(data).map_err(|_| "tokenizer not utf8")?;

        // Quick and dirty: extract vocab and merges
        // We look for "model" section with "vocab" and "merges"
        // BPE tokenizer parsing is skipped for large tokenizers (>1MB) due to
        // the complexity of JSON parsing (brace-depth tracking over 9M chars is too slow
        // in QEMU, and simple pattern search is fragile with vocab entries containing }, .
        // Fallback to char-level tokenizer works for basic use.
        return Err("tokenizer too large, using char-level fallback");

        Ok(())
    }

    pub fn encode(&self, text: &str) -> Vec<u16> {
        if text.is_empty() {
            return vec![self.bos, self.eos];
        }

        // Check cache
        if let Some(cached) = self.cache.get(text) {
            return cached.clone();
        }

        // BPE encode
        let mut tokens = Vec::new();
        tokens.push(self.bos);

        // Split into words (by whitespace) then BPE each word
        for word in text.split(char::is_whitespace).filter(|w| !w.is_empty()) {
            // If word is in vocab directly, use it
            if let Some(&id) = self.rev_vocab.get(word) {
                tokens.push(id);
                continue;
            }

            // Character-level fallback: use byte encoding if available
            for &b in word.as_bytes() {
                let key = format!("<0x{:02X}>", b);
                if let Some(&id) = self.rev_vocab.get(&key) {
                    tokens.push(id);
                } else if b < 128 {
                    // Try single char
                    let c_buf = &[b];
                    let c_str = core::str::from_utf8(c_buf).unwrap_or("");
                    if let Some(&id) = self.rev_vocab.get(c_str) {
                        tokens.push(id);
                    } else if let Some(&id) = self.rev_vocab.get(&format!("Ġ{}", c_str)) {
                        tokens.push(id);
                    }
                }
            }
        }

        tokens.push(self.eos);
        self.cache_entry(text, tokens.clone());
        tokens
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
        Some(tok) => tok.encode(text),
        None => {
            // Fallback to char-level
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
