use alloc::string::String;
use alloc::vec::Vec;

const PATTERNS: &[&[u8]] = &[
    b"api_key", b"api-secret", b"token", b"sk-", b"ghp_",
    b"password", b"passwd", b"secret", b"credential", b"bearer ",
    b"authorization", b"x-api-key", b"private_key", b"-----BEGIN",
];

pub struct PrivacyFilter;

impl PrivacyFilter {
    pub fn filter(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut i = 0;
        while i < data.len() {
            let mut matched = false;
            for pat in PATTERNS {
                if data[i..].starts_with(pat) {
                    let end = data[i..].iter().position(|&c| c == b'\n' || c == b'\r')
                        .map(|p| i + p).unwrap_or(data.len());
                    result.extend_from_slice(b"[REDACTED]");
                    i = end;
                    matched = true;
                    break;
                }
            }
            if !matched { result.push(data[i]); i += 1; }
        }
        result
    }
}
