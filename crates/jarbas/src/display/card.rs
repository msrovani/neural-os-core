//! ADR-0058 S2/S3 — Generative Card Desktop: `UiDeclaration` (cards declarativos)
//! + `UiRenderer` (desenha via `embedded-graphics` no `FbTarget`).
//!
//! Um "card" é a unidade de resposta visual do Jarbas: uma janela com título e
//! um corpo de widgets (Text, KeyValue, Gauge, Bars, List, Divider, Button).
//! É gerado como **dados** (JSON) por Hermes/Trinity/Cortex (constrangido pelo
//! structured decoding ADR-0057 #412) ou por uma skill WASM (RustCoder/Codex).
//!
//! Valores de gauge/bars são inteiros (0..=max) para manter o caminho
//! soft-float-free (o kernel desabilita SSE).

use crate::display::eg::FbTarget;
use crate::display::fb::DoubleBuffer;
use alloc::string::String;
use alloc::vec::Vec;
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_6X10, FONT_9X15_BOLD},
        MonoTextStyle,
    },
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::{Baseline, Text},
};

// Paleta "glass JARVIS" (cyan sobre fundo escuro translúcido pintado).
const C_BG: Rgb888 = Rgb888::new(14, 18, 26);
const C_TITLE_BG: Rgb888 = Rgb888::new(0, 40, 78);
const C_BORDER: Rgb888 = Rgb888::new(0, 120, 180);
const C_TEXT: Rgb888 = Rgb888::new(220, 240, 255);
const C_DIM: Rgb888 = Rgb888::new(150, 175, 200);
const C_ACCENT: Rgb888 = Rgb888::new(0, 200, 255);
const C_GAUGE_BG: Rgb888 = Rgb888::new(30, 40, 55);
const C_BTN: Rgb888 = Rgb888::new(0, 90, 140);

#[derive(Debug, Clone)]
pub enum Widget {
    Text(String),
    KeyValue(String, String),
    Gauge { label: String, value: i32, max: i32, unit: String },
    Bars { label: String, values: Vec<i32> },
    List(Vec<String>),
    Divider,
    Button(String),
    /// Região retangular rotulada (placeholder p/ vídeo/câmera; ADR-0058 S4).
    Panel { label: String, height: i32 },
}

#[derive(Debug, Clone)]
pub struct UiDeclaration {
    pub id: u32,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub body: Vec<Widget>,
    pub closable: bool,
}

/// Rect de um botão clicável (para hit-testing no compositor).
#[derive(Clone, Copy)]
pub struct ButtonHit {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub index: usize,
}

impl UiDeclaration {
    pub fn new(id: u32, title: &str, x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            id,
            title: String::from(title),
            x,
            y,
            w,
            h,
            body: Vec::new(),
            closable: true,
        }
    }
    pub fn push(mut self, wg: Widget) -> Self {
        self.body.push(wg);
        self
    }
    /// Rect da caixa de fechar (X) no canto sup. dir. do título.
    pub fn close_rect(&self) -> (i32, i32, i32, i32) {
        (self.x + self.w - 20, self.y + 2, 16, 16)
    }
}

fn fill(t: &mut FbTarget, x: i32, y: i32, w: i32, h: i32, c: Rgb888) {
    if w <= 0 || h <= 0 {
        return;
    }
    let _ = Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(c))
        .draw(t);
}

fn text(t: &mut FbTarget, s: &str, x: i32, y: i32, c: Rgb888) {
    let style = MonoTextStyle::new(&FONT_6X10, c);
    let _ = Text::with_baseline(s, Point::new(x, y), style, Baseline::Top).draw(t);
}

/// Hit-test puro: retorna os rects dos botões do card SEM renderizar.
/// Mesma fórmula geométrica de `render_card` — use em handlers de clique
/// para evitar side-effect visual.
pub fn hit_test_buttons(d: &UiDeclaration) -> Vec<ButtonHit> {
    let mut hits: Vec<ButtonHit> = Vec::new();
    let pad = 8;
    let mut cy = d.y + 26;
    for wg in &d.body {
        if let Widget::Button(lbl) = wg {
            let bw = (lbl.len() as i32) * 6 + 16;
            let bx = d.x + pad;
            let by = cy;
            hits.push(ButtonHit { x: bx, y: by, w: bw, h: 16, index: hits.len() });
            cy += 22;
        } else {
            // Espelha o avanço de cy do render para manter alinhamento.
            match wg {
                Widget::Text(_) | Widget::KeyValue(_, _) | Widget::List(_) => cy += 13,
                Widget::Gauge { .. } => cy += 26,
                Widget::Bars { .. } => cy += 56,
                Widget::Divider => cy += 8,
                Widget::Panel { height, .. } => {
                    let ph = (*height).clamp(20, d.h - (cy - d.y) - 24);
                    cy += ph + 6;
                }
                _ => {}
            }
        }
        if cy > d.y + d.h - 10 { break; }
    }
    hits
}

/// Renderiza o card e retorna os rects dos botões (para clique).
pub fn render_card(fb: &mut DoubleBuffer, d: &UiDeclaration) -> Vec<ButtonHit> {
    let mut hits: Vec<ButtonHit> = Vec::new();
    let mut t = FbTarget::new(fb);

    // Moldura: borda + corpo + barra de título arredondada.
    let frame = Rectangle::new(Point::new(d.x - 1, d.y - 1), Size::new(d.w as u32 + 2, d.h as u32 + 2));
    let _ = RoundedRectangle::with_equal_corners(frame, Size::new(6, 6))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(C_BG)
                .stroke_color(C_BORDER)
                .stroke_width(1)
                .build(),
        )
        .draw(&mut t);
    fill(&mut t, d.x, d.y, d.w, 20, C_TITLE_BG);

    // Título + botão fechar.
    let tstyle = MonoTextStyle::new(&FONT_9X15_BOLD, C_ACCENT);
    let _ = Text::with_baseline(&d.title, Point::new(d.x + 6, d.y + 3), tstyle, Baseline::Top).draw(&mut t);
    if d.closable {
        let (cx, cy, cw, ch) = d.close_rect();
        fill(&mut t, cx, cy, cw, ch, Rgb888::new(150, 40, 40));
        text(&mut t, "x", cx + 5, cy + 4, C_TEXT);
    }

    // Corpo — cursor vertical.
    let pad = 8;
    let mut cy = d.y + 26;
    let inner_w = d.w - pad * 2;
    for wg in &d.body {
        match wg {
            Widget::Text(s) => {
                text(&mut t, s, d.x + pad, cy, C_TEXT);
                cy += 13;
            }
            Widget::KeyValue(k, v) => {
                text(&mut t, k, d.x + pad, cy, C_DIM);
                let vx = d.x + d.w - pad - (v.len() as i32) * 6;
                text(&mut t, v, vx, cy, C_TEXT);
                cy += 13;
            }
            Widget::Gauge { label, value, max, unit } => {
                text(&mut t, label, d.x + pad, cy, C_DIM);
                cy += 12;
                let gw = inner_w;
                fill(&mut t, d.x + pad, cy, gw, 8, C_GAUGE_BG);
                let m = if *max <= 0 { 1 } else { *max };
                let filled = (gw * value.clamp(&0, &m)) / m;
                fill(&mut t, d.x + pad, cy, filled, 8, C_ACCENT);
                let vtxt = alloc::format!("{}{}", value, unit);
                text(&mut t, &vtxt, d.x + d.w - pad - (vtxt.len() as i32) * 6, cy - 12, C_TEXT);
                cy += 14;
            }
            Widget::Bars { label, values } => {
                text(&mut t, label, d.x + pad, cy, C_DIM);
                cy += 12;
                let n = values.len().max(1) as i32;
                let bw = ((inner_w - (n - 1) * 2) / n).max(2);
                let maxv = values.iter().copied().max().unwrap_or(1).max(1);
                let base = cy + 40;
                for (i, v) in values.iter().enumerate() {
                    let bh = (40 * v.clamp(&0, &maxv)) / maxv;
                    let bx = d.x + pad + (i as i32) * (bw + 2);
                    fill(&mut t, bx, base - bh, bw, bh, C_ACCENT);
                }
                cy = base + 4;
            }
            Widget::List(items) => {
                for it in items {
                    text(&mut t, "•", d.x + pad, cy, C_ACCENT);
                    text(&mut t, it, d.x + pad + 10, cy, C_TEXT);
                    cy += 13;
                }
            }
            Widget::Divider => {
                fill(&mut t, d.x + pad, cy + 3, inner_w, 1, C_BORDER);
                cy += 8;
            }
            Widget::Panel { label, height } => {
                let ph = (*height).clamp(20, d.h - (cy - d.y) - 24);
                fill(&mut t, d.x + pad, cy, inner_w, ph, C_GAUGE_BG);
                // moldura
                let r = Rectangle::new(Point::new(d.x + pad, cy), Size::new(inner_w as u32, ph as u32));
                let _ = r
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .stroke_color(C_BORDER)
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(&mut t);
                text(&mut t, label, d.x + pad + 6, cy + ph / 2 - 5, C_DIM);
                cy += ph + 6;
            }
            Widget::Button(lbl) => {
                let bw = (lbl.len() as i32) * 6 + 16;
                let bx = d.x + pad;
                let by = cy;
                let r = Rectangle::new(Point::new(bx, by), Size::new(bw as u32, 16));
                let _ = RoundedRectangle::with_equal_corners(r, Size::new(4, 4))
                    .into_styled(PrimitiveStyle::with_fill(C_BTN))
                    .draw(&mut t);
                text(&mut t, lbl, bx + 8, by + 4, C_TEXT);
                hits.push(ButtonHit { x: bx, y: by, w: bw, h: 16, index: hits.len() });
                cy += 22;
            }
        }
        if cy > d.y + d.h - 10 {
            break;
        }
    }
    hits
}

// ─── Parser JSON mínimo (no_std) ────────────────────────────────────────────
// Formato: {"id":N,"title":"..","x":N,"y":N,"w":N,"h":N,"body":[
//   {"t":"text","s":".."} | {"t":"kv","k":"..","v":".."}
//   {"t":"gauge","label":"..","value":N,"max":N,"unit":".."}
//   {"t":"bars","label":"..","v":[N,..]} | {"t":"list","items":["..",..]}
//   {"t":"div"} | {"t":"btn","label":".."} ]}

pub fn parse_card(json: &str) -> Option<UiDeclaration> {
    let id = extract_i32(json, "id").unwrap_or(0) as u32;
    let title = extract_str(json, "title").unwrap_or_else(|| String::from("Card"));
    let x = extract_i32(json, "x").unwrap_or(120);
    let y = extract_i32(json, "y").unwrap_or(90);
    let w = extract_i32(json, "w").unwrap_or(300);
    let h = extract_i32(json, "h").unwrap_or(200);
    let mut decl = UiDeclaration::new(id, &title, x, y, w, h);

    if let Some(bstart) = json.find("\"body\"").and_then(|i| json[i..].find('[').map(|j| i + j + 1)) {
        let rest = &json[bstart..];
        let mut depth = 0usize;
        let mut obj_start = None;
        for (i, c) in rest.char_indices() {
            match c {
                '{' => {
                    if depth == 0 {
                        obj_start = Some(i);
                    }
                    depth += 1;
                }
                '}' => {
                    if depth > 0 {
                        depth -= 1;
                        if depth == 0 {
                            if let Some(s) = obj_start {
                                if let Some(wg) = parse_widget(&rest[s..=i]) {
                                    decl.body.push(wg);
                                }
                            }
                        }
                    }
                }
                ']' if depth == 0 => break,
                _ => {}
            }
        }
    }
    Some(decl)
}

fn parse_widget(obj: &str) -> Option<Widget> {
    let t = extract_str(obj, "t")?;
    match t.as_str() {
        "text" => Some(Widget::Text(extract_str(obj, "s").unwrap_or_default())),
        "kv" => Some(Widget::KeyValue(
            extract_str(obj, "k").unwrap_or_default(),
            extract_str(obj, "v").unwrap_or_default(),
        )),
        "gauge" => Some(Widget::Gauge {
            label: extract_str(obj, "label").unwrap_or_default(),
            value: extract_i32(obj, "value").unwrap_or(0),
            max: extract_i32(obj, "max").unwrap_or(100),
            unit: extract_str(obj, "unit").unwrap_or_default(),
        }),
        "bars" => Some(Widget::Bars {
            label: extract_str(obj, "label").unwrap_or_default(),
            values: extract_int_array(obj, "v"),
        }),
        "list" => Some(Widget::List(extract_str_array(obj, "items"))),
        "div" => Some(Widget::Divider),
        "btn" => Some(Widget::Button(extract_str(obj, "label").unwrap_or_default())),
        "panel" => Some(Widget::Panel {
            label: extract_str(obj, "label").unwrap_or_default(),
            height: extract_i32(obj, "h").unwrap_or(80),
        }),
        _ => None,
    }
}

/// ADR-0058 + ADR-0057 #412: dica de schema p/ o LLM gerar um card válido
/// (o structured decoding `cortex::decode` restringe os tokens a esta forma).
pub fn card_json_schema_hint() -> &'static str {
    concat!(
        "Responda SÓ com um card JSON: {\"id\":N,\"title\":\"..\",\"w\":N,\"h\":N,\"body\":[",
        "{\"t\":\"text\",\"s\":\"..\"}|{\"t\":\"kv\",\"k\":\"..\",\"v\":\"..\"}|",
        "{\"t\":\"gauge\",\"label\":\"..\",\"value\":N,\"max\":N,\"unit\":\"..\"}|",
        "{\"t\":\"bars\",\"label\":\"..\",\"v\":[N,..]}|{\"t\":\"list\",\"items\":[\"..\"]}|",
        "{\"t\":\"panel\",\"label\":\"..\",\"h\":N}|{\"t\":\"btn\",\"label\":\"..\"}|{\"t\":\"div\"}]}"
    )
}

// ─── Cards demo (ADR-0058 S4) — provam o pipeline sem modelo/HW ─────────────

/// Card de status do sistema (rótulos + gauges + botão de ação).
pub fn demo_status_card() -> UiDeclaration {
    UiDeclaration::new(1001, "Sistema K3CHJ", 60, 92, 300, 176)
        .push(Widget::KeyValue(String::from("Kernel"), String::from("v1.9.1")))
        .push(Widget::Gauge { label: String::from("CPU"), value: 37, max: 100, unit: String::from("%") })
        .push(Widget::Gauge { label: String::from("MEM"), value: 52, max: 100, unit: String::from("%") })
        .push(Widget::Divider)
        .push(Widget::Button(String::from("Atualizar")))
}

/// Card de clima (demo "clima de amanhã" — dados reais viriam da skill weather).
pub fn demo_weather_card() -> UiDeclaration {
    UiDeclaration::new(1002, "Clima - Amanha", 392, 92, 300, 176)
        .push(Widget::KeyValue(String::from("Condicao"), String::from("Nublado")))
        .push(Widget::KeyValue(String::from("Temp"), String::from("22 C")))
        .push(Widget::KeyValue(String::from("Max/Min"), String::from("25/16")))
        .push(Widget::Bars { label: String::from("24h (C)"), values: alloc::vec![16, 17, 19, 22, 24, 23, 20, 18] })
}

/// Card scaffold de videochamada (interação mouse/teclado; mic/alto-falante/
/// câmera dependem de HDA/UVC existentes — S4 é a UI + roteamento de botões).
pub fn demo_call_card() -> UiDeclaration {
    UiDeclaration::new(1003, "Chamada de Video", 724, 92, 320, 220)
        .push(Widget::Panel { label: String::from("[camera / video feed]"), height: 96 })
        .push(Widget::KeyValue(String::from("Status"), String::from("pronto")))
        .push(Widget::Button(String::from("Atender")))
        .push(Widget::Button(String::from("Microfone")))
        .push(Widget::Button(String::from("Alto-falante")))
        .push(Widget::Button(String::from("Encerrar")))
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

fn extract_int_array(json: &str, key: &str) -> Vec<i32> {
    let mut out = Vec::new();
    let pat = alloc::format!("\"{}\"", key);
    if let Some(idx) = json.find(&pat) {
        let after = &json[idx + pat.len()..];
        if let Some(lb) = after.find('[') {
            if let Some(rb) = after[lb..].find(']') {
                let inner = &after[lb + 1..lb + rb];
                for tok in inner.split(',') {
                    let s = tok.trim();
                    if let Ok(v) = parse_int(s) {
                        out.push(v);
                    }
                }
            }
        }
    }
    out
}

fn extract_str_array(json: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let pat = alloc::format!("\"{}\"", key);
    if let Some(idx) = json.find(&pat) {
        let after = &json[idx + pat.len()..];
        if let Some(lb) = after.find('[') {
            if let Some(rb) = after[lb..].find(']') {
                let mut inner = &after[lb + 1..lb + rb];
                while let Some(s) = inner.find('"') {
                    let rest = &inner[s + 1..];
                    if let Some(e) = rest.find('"') {
                        out.push(String::from(&rest[..e]));
                        inner = &rest[e + 1..];
                    } else {
                        break;
                    }
                }
            }
        }
    }
    out
}

fn parse_int(s: &str) -> Result<i32, ()> {
    let mut n = 0i32;
    let mut neg = false;
    let mut any = false;
    for (i, c) in s.chars().enumerate() {
        if i == 0 && c == '-' {
            neg = true;
            continue;
        }
        if c.is_ascii_digit() {
            n = n * 10 + (c as u8 - b'0') as i32;
            any = true;
        } else {
            break;
        }
    }
    if any {
        Ok(if neg { -n } else { n })
    } else {
        Err(())
    }
}

/// Self-test S2 (sem modelo): parse de um card JSON + valida estrutura.
pub fn self_test() -> bool {
    let json = r#"{"id":1,"title":"Test","x":10,"y":10,"w":200,"h":120,"body":[
        {"t":"kv","k":"A","v":"1"},{"t":"gauge","label":"cpu","value":42,"max":100,"unit":"%"},
        {"t":"bars","label":"h","v":[1,2,3,4]},{"t":"btn","label":"OK"}]}"#;
    let ok = match parse_card(json) {
        Some(d) => d.body.len() == 4 && d.title == "Test",
        None => false,
    };
    if ok {
        k_nano::slog_jarbas!("UI", "info", "UiDeclaration parser self-test PASS (ADR-0058 S2)");
    } else {
        k_nano::slog_jarbas!("UI", "warn", "UiDeclaration parser self-test FAIL");
    }
    ok
}
