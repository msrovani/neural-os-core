extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::str::from_utf8;
use k_nano::kjson;

pub struct RssAgent;

impl RssAgent {
    pub fn new() -> Self { RssAgent }

    pub fn fetch(&self, feed_url: &str, max_items: usize) -> Vec<(String, String)> {
        let cfg = crate::net::NET_CONFIG.lock();
        let dns = cfg.dns_ip;
        drop(cfg);

        let path = if let Some(p) = feed_url.find("://") {
            let after_scheme = &feed_url[p + 3..];
            let slash = after_scheme.find('/');
            let path_only = if let Some(s) = slash { &after_scheme[s..] } else { "/" };
            String::from(path_only)
        } else { String::from("/") };

        let html = unsafe { crate::net::http_get(dns, 80, &path) };
        if let Some(data) = html {
            let text = from_utf8(&data).unwrap_or("");
            let items = parse_feed(text, max_items);
            kjson!("RSS", "agent", "fetch", "url", feed_url, "items", items.len());
            items
        } else {
            kjson!("RSS", "agent", "err", "url", feed_url);
            Vec::new()
        }
    }
}

fn parse_feed(xml: &str, max: usize) -> Vec<(String, String)> {
    let mut items = Vec::new();
    let mut in_item = false;
    let mut t = String::new();
    let mut l = String::new();
    for line in xml.lines() {
        if items.len() >= max { break; }
        let tr = line.trim();
        if tr.contains("<item>") || tr.contains("<entry>") { in_item = true; t.clear(); l.clear(); }
        if tr.contains("</item>") || tr.contains("</entry>") {
            if in_item && !t.is_empty() { items.push((core::mem::take(&mut t), core::mem::take(&mut l))); }
            in_item = false;
        }
        if in_item {
            if let Some(v) = tag_val(tr, "title") { t = v; }
            if let Some(v) = tag_val(tr, "link") { if v.find('<').is_none() { l = v; } }
            if l.is_empty() { if let Some(v) = attr_val(tr, "href") { l = v; } }
        }
    }
    items
}

fn tag_val(text: &str, tag: &str) -> Option<String> {
    let os = text.find(&alloc::format!("<{}", tag))?;
    let cs = text[os..].find('>')? + os + 1;
    let ce = text[cs..].find(&alloc::format!("</{}>", tag))?;
    Some(text[cs..cs + ce].trim().to_string())
}

fn attr_val(text: &str, attr: &str) -> Option<String> {
    let p = alloc::format!("{}=\"", attr);
    let s = text.find(&p)? + p.len();
    let e = text[s..].find('"')?;
    Some(text[s..s + e].to_string())
}
