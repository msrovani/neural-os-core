//! Generative UI JSON PoC (ADR-0047-HMI H1) — WindowSpec mínimo → compositor.

use alloc::string::String;
use alloc::vec::Vec;

pub const TOPIC_UI_SPEC: &str = "UI_SPEC";

#[derive(Clone)]
pub struct WidgetSpec {
    pub kind: String, // "label" | "button" | "rect"
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone)]
pub struct WindowSpec {
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub widgets: Vec<WidgetSpec>,
}

/// Minimal parser for: {"title":"...","x":N,"y":N,"w":N,"h":N,"widgets":[{"kind":"label","text":"...","x":N,"y":N,"w":N,"h":N}]}
pub fn parse_window_spec(json: &str) -> Option<WindowSpec> {
    let title = extract_str(json, "title").unwrap_or_else(|| String::from("UI"));
    let x = extract_i32(json, "x").unwrap_or(80);
    let y = extract_i32(json, "y").unwrap_or(80);
    let w = extract_i32(json, "w").unwrap_or(320);
    let h = extract_i32(json, "h").unwrap_or(200);
    let mut widgets = Vec::new();
    // Single-widget convenience: top-level "text"
    if let Some(text) = extract_str(json, "text") {
        widgets.push(WidgetSpec {
            kind: String::from("label"),
            text,
            x: 8,
            y: 28,
            w: w - 16,
            h: 24,
        });
    }
    Some(WindowSpec {
        title,
        x,
        y,
        w,
        h,
        widgets,
    })
}

fn extract_str(json: &str, key: &str) -> Option<String> {
    let pat = alloc::format!("\"{}\"", key);
    let idx = json.find(&pat)?;
    let after = &json[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    Some(String::from(&rest[start..end]))
}

fn extract_i32(json: &str, key: &str) -> Option<i32> {
    let pat = alloc::format!("\"{}\"", key);
    let idx = json.find(&pat)?;
    let after = &json[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let mut n = 0i32;
    let mut neg = false;
    let mut started = false;
    for c in rest.chars() {
        if !started && c == '-' {
            neg = true;
            started = true;
            continue;
        }
        if c.is_ascii_digit() {
            started = true;
            n = n * 10 + (c as u8 - b'0') as i32;
        } else if started {
            break;
        } else if c == ' ' {
            continue;
        } else {
            break;
        }
    }
    if started {
        Some(if neg { -n } else { n })
    } else {
        None
    }
}

/// Demo spec published at boot for H1 gate.
pub fn demo_ui_json() -> &'static str {
    r#"{"title":"ADR-0047 UI","x":100,"y":100,"w":360,"h":220,"text":"Generative window PoC"}"#
}

static UI_OK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static AVATAR_TELEM: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn mark_ui_ok() {
    UI_OK.store(true, core::sync::atomic::Ordering::Relaxed);
}

pub fn mark_avatar_telem() {
    AVATAR_TELEM.store(true, core::sync::atomic::Ordering::Relaxed);
}

pub fn gate_status() -> (&'static str, &'static str) {
    let ui = if UI_OK.load(core::sync::atomic::Ordering::Relaxed) {
        "OK"
    } else {
        "ABSENT"
    };
    let av = if AVATAR_TELEM.load(core::sync::atomic::Ordering::Relaxed) {
        "OK"
    } else {
        "ABSENT"
    };
    (ui, av)
}
