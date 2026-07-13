//! #307 SearchAgent — busca HTTP via serial tunnel ou E1000.
//! Usa http_get() real para consultar DuckDuckGo/Lite e extrair resultados.

use alloc::string::String;
use alloc::vec::Vec;
use crate::kjson;

pub struct SearchAgent;

impl SearchAgent {
    pub fn new() -> Self { SearchAgent }

    /// Busca online via DuckDuckGo Lite (HTML simples, sem JS)
    pub fn search(&self, query: &str, max_results: usize) -> Vec<(String, String)> {
        let mut results = Vec::new();
        let query_encoded: String = query.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
        let path = alloc::format!("/lite/?q={}&ia=web", query_encoded);

        let cfg = crate::net::NET_CONFIG.lock();
        let dns = cfg.dns_ip;
        drop(cfg);

        let html = unsafe { crate::net::http_get(dns, 80, &path) };
        if let Some(data) = html {
            let text = core::str::from_utf8(&data).unwrap_or("");
            let mut count = 0;
            for line in text.lines() {
                if count >= max_results { break; }
                if let Some(title) = extract_between(line, "<a rel=\"nofollow\" href=\"", "\"") {
                    let url = title;
                    let rest = &line[line.find("\">").map(|i| i+2).unwrap_or(0)..];
                    let desc = extract_between(rest, ">", "</a>").unwrap_or("");
                    if !url.is_empty() && count < max_results {
                        results.push((String::from(url), String::from(desc)));
                        count += 1;
                    }
                }
            }
            kjson!("SEARCH", query, "results", "n", results.len());
        } else {
            kjson!("SEARCH", query, "err", "msg", "\"http_get failed\"");
        }
        results
    }

    pub fn status(&self) -> String {
        String::from("[SEARCH] DuckDuckGo Lite via http_get")
    }
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)?;
    let j = s[i + start.len()..].find(end)?;
    Some(&s[i + start.len()..i + start.len() + j])
}
