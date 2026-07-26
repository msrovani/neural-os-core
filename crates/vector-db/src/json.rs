use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::store::{VectorEntry, VectorStore, VectorStoreInner};

// ─── Serialization ───────────────────────────────────────────────────────────

/// Serialize the VectorStore to a JSON byte vector.
///
/// embeddings are NOT serialized — they are rebuilt on load via compute_tfidf.
pub fn to_json(store: &VectorStore) -> Vec<u8> {
    let inner = store.lock_inner();
    let json = serialize_inner(&inner);
    json.into_bytes()
}

fn serialize_inner(inner: &VectorStoreInner) -> String {
    let mut s = String::new();
    s.push('{');

    // vocabulary
    s.push_str("\"vocabulary\":{");
    let mut first = true;
    for (word, idx) in &inner.vocabulary {
        if !first {
            s.push(',');
        }
        first = false;
        write_json_string(&mut s, word);
        s.push(':');
        write_json_u64(&mut s, *idx as u64);
    }
    s.push('}');

    // doc_count
    s.push_str(",\"doc_count\":");
    write_json_u64(&mut s, inner.doc_count as u64);

    // next_id
    s.push_str(",\"next_id\":");
    write_json_u64(&mut s, inner.next_id);

    // df
    s.push_str(",\"df\":[");
    let mut first = true;
    for val in &inner.df {
        if !first {
            s.push(',');
        }
        first = false;
        write_json_u64(&mut s, *val as u64);
    }
    s.push(']');

    // entries (without embeddings)
    s.push_str(",\"entries\":[");
    let mut first = true;
    for entry in &inner.entries {
        if !first {
            s.push(',');
        }
        first = false;
        s.push('{');
        s.push_str("\"id\":");
        write_json_string(&mut s, &entry.id);
        s.push_str(",\"text\":");
        write_json_string(&mut s, &entry.text);
        s.push_str(",\"metadata\":{");
        let mut mfirst = true;
        for (k, v) in &entry.metadata {
            if !mfirst {
                s.push(',');
            }
            mfirst = false;
            write_json_string(&mut s, k);
            s.push(':');
            write_json_string(&mut s, v);
        }
        s.push('}');
        s.push('}');
    }
    s.push(']');
    s.push('}');
    s
}

fn write_json_string(s: &mut String, val: &str) {
    s.push('"');
    for ch in val.chars() {
        match ch {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                // control characters: \uXXXX
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c => s.push(c),
        }
    }
    s.push('"');
}

fn write_json_u64(s: &mut String, val: u64) {
    if val == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = val;
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = (n % 10) as u8 + b'0';
        n /= 10;
    }
    let _ = write!(s, "{}", core::str::from_utf8(&buf[i..]).unwrap_or("0"));
}

// ─── Deserialization ─────────────────────────────────────────────────────────

/// Deserialize a VectorStore from JSON bytes.
///
/// embeddings are rebuilt from TF-IDF using the vocabulary, df, and doc_count.
pub fn from_json(data: &[u8]) -> Result<VectorStore, &'static str> {
    let s = core::str::from_utf8(data).map_err(|_| "invalid utf-8")?;
    let mut parser = JsonParser { data: s, pos: 0 };
    parser.skip_whitespace();
    parser.expect('{')?;

    let mut vocabulary: BTreeMap<String, usize> = BTreeMap::new();
    let mut doc_count: u32 = 0;
    let mut next_id: u64 = 0;
    let mut df: Vec<u32> = Vec::new();
    let mut entries: Vec<VectorEntry> = Vec::new();

    loop {
        parser.skip_whitespace();
        if parser.peek() == Some('}') {
            parser.advance();
            break;
        }
        let key = parser.parse_string()?;
        parser.skip_whitespace();
        parser.expect(':')?;

        match key.as_str() {
            "vocabulary" => {
                vocabulary = parser.parse_string_to_int_map()?;
            }
            "doc_count" => {
                doc_count = parser.parse_u32()?;
            }
            "next_id" => {
                next_id = parser.parse_u64()?;
            }
            "df" => {
                df = parser.parse_u32_array()?;
            }
            "entries" => {
                entries = parser.parse_entries()?;
            }
            _ => {
                // skip unknown keys
                parser.skip_value()?;
            }
        }

        parser.skip_whitespace();
        if parser.peek() == Some(',') {
            parser.advance();
        }
    }

    // Rebuild store
    let store = VectorStore::new();
    {
        let mut inner = store.lock_inner();
        inner.vocabulary = vocabulary;
        inner.doc_count = doc_count;
        inner.next_id = next_id;
        inner.df = df;
        inner.entries = entries;

        // Rebuild embeddings from TF-IDF
        let vocab_size = inner.vocabulary.len();
        let texts: Vec<String> = inner.entries.iter().map(|e| e.text.clone()).collect();
        // Pre-resolve token indices for each entry (immutable pass)
        let all_indices: Vec<Vec<usize>> = texts
            .iter()
            .map(|text| {
                let tokens = crate::tokenize::tokenize(text);
                tokens
                    .iter()
                    .filter_map(|t| inner.vocabulary.get(t).copied())
                    .collect()
            })
            .collect();
        // Update embeddings (mutable pass) — clone df once to avoid borrow conflict
        let df_clone = inner.df.clone();
        let doc_count = inner.doc_count;
        for (entry, indices) in inner.entries.iter_mut().zip(all_indices.iter()) {
            entry.embedding = crate::tfidf::compute_tfidf_from_indices(
                indices,
                &df_clone,
                doc_count,
                vocab_size,
            );
        }
    }
    Ok(store)
}

// ─── Minimal JSON Parser ─────────────────────────────────────────────────────

struct JsonParser<'a> {
    data: &'a str,
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn peek(&self) -> Option<char> {
        self.data[self.pos..].chars().next()
    }

    fn advance(&mut self) {
        if let Some(ch) = self.data[self.pos..].chars().next() {
            self.pos += ch.len_utf8();
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), &'static str> {
        self.skip_whitespace();
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            Err("expected character")
        }
    }

    fn parse_string(&mut self) -> Result<String, &'static str> {
        self.skip_whitespace();
        if self.peek() != Some('"') {
            return Err("expected string");
        }
        self.advance(); // skip opening "
        let mut result = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string"),
                Some('"') => {
                    self.advance();
                    return Ok(result);
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('"') => { result.push('"'); self.advance(); }
                        Some('\\') => { result.push('\\'); self.advance(); }
                        Some('n') => { result.push('\n'); self.advance(); }
                        Some('r') => { result.push('\r'); self.advance(); }
                        Some('t') => { result.push('\t'); self.advance(); }
                        Some('u') => {
                            self.advance();
                            let hex: String = self.data[self.pos..].chars().take(4).collect();
                            if hex.len() < 4 {
                                return Err("invalid unicode escape");
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| "invalid unicode escape")?;
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                            }
                            self.pos += 4;
                        }
                        _ => return Err("invalid escape"),
                    }
                }
                Some(ch) => {
                    result.push(ch);
                    self.advance();
                }
            }
        }
    }

    fn parse_u32(&mut self) -> Result<u32, &'static str> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let s = &self.data[start..self.pos];
        if s.is_empty() {
            return Err("expected number");
        }
        s.parse::<u32>().map_err(|_| "invalid u32")
    }

    fn parse_u64(&mut self) -> Result<u64, &'static str> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let s = &self.data[start..self.pos];
        if s.is_empty() {
            return Err("expected number");
        }
        s.parse::<u64>().map_err(|_| "invalid u64")
    }

    fn parse_string_to_int_map(&mut self) -> Result<BTreeMap<String, usize>, &'static str> {
        self.skip_whitespace();
        self.expect('{')?;
        let mut map = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some('}') {
                self.advance();
                return Ok(map);
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let val = self.parse_u32()?;
            map.insert(key, val as usize);

            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance();
            }
        }
    }

    fn parse_u32_array(&mut self) -> Result<Vec<u32>, &'static str> {
        self.skip_whitespace();
        self.expect('[')?;
        let mut arr = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some(']') {
                self.advance();
                return Ok(arr);
            }
            let val = self.parse_u32()?;
            arr.push(val);

            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance();
            }
        }
    }

    fn parse_entries(&mut self) -> Result<Vec<VectorEntry>, &'static str> {
        self.skip_whitespace();
        self.expect('[')?;
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some(']') {
                self.advance();
                return Ok(entries);
            }
            let entry = self.parse_single_entry()?;
            entries.push(entry);

            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance();
            }
        }
    }

    fn parse_single_entry(&mut self) -> Result<VectorEntry, &'static str> {
        self.skip_whitespace();
        self.expect('{')?;
        let mut id = String::new();
        let mut text = String::new();
        let mut metadata = BTreeMap::new();
        // embedding is NOT serialized; will be rebuilt

        loop {
            self.skip_whitespace();
            if self.peek() == Some('}') {
                self.advance();
                break;
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;

            match key.as_str() {
                "id" => id = self.parse_string()?,
                "text" => text = self.parse_string()?,
                "metadata" => metadata = self.parse_string_map()?,
                _ => { self.skip_value()?; }
            }

            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance();
            }
        }

        Ok(VectorEntry::new(id, text, Vec::new(), metadata))
    }

    fn parse_string_map(&mut self) -> Result<BTreeMap<String, String>, &'static str> {
        self.skip_whitespace();
        self.expect('{')?;
        let mut map = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some('}') {
                self.advance();
                return Ok(map);
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let val = self.parse_string()?;
            map.insert(key, val);

            self.skip_whitespace();
            if self.peek() == Some(',') {
                self.advance();
            }
        }
    }

    fn skip_value(&mut self) -> Result<(), &'static str> {
        self.skip_whitespace();
        match self.peek() {
            Some('"') => { self.parse_string()?; Ok(()) }
            Some('{') => { self.skip_object() }
            Some('[') => { self.skip_array() }
            Some(ch) if ch.is_ascii_digit() || ch == '-' => {
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(())
            }
            Some('t') | Some('f') | Some('n') => {
                // true, false, null
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphabetic() {
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(())
            }
            _ => Err("unexpected value"),
        }
    }

    fn skip_object(&mut self) -> Result<(), &'static str> {
        self.expect('{')?;
        let mut depth = 1u32;
        while depth > 0 {
            match self.peek() {
                None => return Err("unterminated object"),
                Some('{') => { depth += 1; self.advance(); }
                Some('}') => { depth -= 1; self.advance(); }
                Some('"') => { self.parse_string()?; }
                Some(_) => { self.advance(); }
            }
        }
        Ok(())
    }

    fn skip_array(&mut self) -> Result<(), &'static str> {
        self.expect('[')?;
        let mut depth = 1u32;
        while depth > 0 {
            match self.peek() {
                None => return Err("unterminated array"),
                Some('[') => { depth += 1; self.advance(); }
                Some(']') => { depth -= 1; self.advance(); }
                Some('"') => { self.parse_string()?; }
                Some(_) => { self.advance(); }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn demo() -> bool {
        let mut store = crate::VectorStore::new();
        let mut m = BTreeMap::new();
        m.insert("agent".into(), "test".into());
        store.insert("Rust bare metal OS development", m);
        store.insert("Python data science", BTreeMap::new());

        let json = to_json(&store);
        let json_str = core::str::from_utf8(&json).unwrap_or("");

        // Verify it contains key structural elements
        if !json_str.contains("\"vocabulary\"") { return false; }
        if !json_str.contains("\"doc_count\"") { return false; }
        if !json_str.contains("\"entries\"") { return false; }
        if !json_str.contains("\"id\"") { return false; }
        if !json_str.contains("\"text\"") { return false; }

        // Round-trip
        let restored = from_json(&json).unwrap_or_else(|_| crate::VectorStore::new());
        if restored.len() != 2 { return false; }
        if restored.vocab_size() == 0 { return false; }

        // Search in restored
        let results = restored.search("Rust", 1);
        if results.is_empty() { return false; }

        true
    }
}
