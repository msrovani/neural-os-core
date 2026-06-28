use alloc::string::String;
use alloc::vec::Vec;

pub struct TokenJuice;

impl TokenJuice {
    pub fn compress(data: &[u8]) -> Vec<u8> {
        let s = core::str::from_utf8(data).unwrap_or("");
        let mut out = Vec::new();
        let mut i = 0;
        let bytes = s.as_bytes();
        while i < bytes.len() {
            if bytes[i] == b'<' && s[i..].starts_with("<html") { i = Self::skip_tag(s, i, "</html>"); }
            else if bytes[i] == b'<' { i = Self::skip_tag(s, i, ">"); }
            else if bytes[i..].starts_with(b"http") {
                let end = s[i..].find(|c: char| c.is_whitespace() || c == '>').map(|p| i+p).unwrap_or(bytes.len());
                let url = &s[i..end];
                if url.len() > 60 {
                    out.extend_from_slice(b"[URL]");
                } else {
                    out.extend_from_slice(url.as_bytes());
                }
                i = end;
            }
            else if bytes[i] == b' ' && out.last() == Some(&b' ') { i += 1; }
            else { out.push(bytes[i]); i += 1; }
        }
        out
    }

    fn skip_tag(s: &str, mut i: usize, closer: &str) -> usize {
        if let Some(end) = s[i..].find(closer) { i + end + closer.len() } else { s.len() }
    }
}
