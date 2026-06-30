//! Span tracing — execution graph do Hermes Cognitive.
//! RagaAI Catalyst-inspired: cada skill call vira um span.
use alloc::string::String;
use alloc::vec::Vec;

const MAX_SPANS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpanStatus { Open, Ok, Error, Timeout }

#[derive(Debug, Clone)]
pub struct Span {
    pub id: u64,
    pub parent: Option<u64>,
    pub agent: String,
    pub skill: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub status: SpanStatus,
}

pub struct Tracer {
    spans: Vec<Span>,
    next_id: u64,
}

impl Tracer {
    pub fn new() -> Self { Tracer { spans: Vec::with_capacity(MAX_SPANS), next_id: 1 } }

    pub fn start_span(&mut self, parent: Option<u64>, agent: &str, skill: &str, tick: u64) -> u64 {
        let id = self.next_id; self.next_id += 1;
        if self.spans.len() >= MAX_SPANS { self.spans.remove(0); }
        self.spans.push(Span {
            id, parent, agent: String::from(agent), skill: String::from(skill),
            start_tick: tick, end_tick: 0, status: SpanStatus::Open,
        });
        id
    }

    pub fn end_span(&mut self, id: u64, tick: u64, status: SpanStatus) {
        if let Some(s) = self.spans.iter_mut().find(|s| s.id == id) {
            s.end_tick = tick; s.status = status;
        }
    }

    pub fn trace_tree(&self) -> String {
        let mut out = String::new();
        for s in &self.spans {
            if s.parent.is_none() {
                self.format_span(&mut out, s, 0);
            }
        }
        out
    }

    fn format_span(&self, out: &mut String, span: &Span, depth: usize) {
        let prefix = core::iter::repeat("  ").take(depth).collect::<String>();
        let status = match span.status {
            SpanStatus::Ok => "OK", SpanStatus::Error => "ERR",
            SpanStatus::Timeout => "TMO", SpanStatus::Open => "...",
        };
        let dur = if span.end_tick > span.start_tick { span.end_tick - span.start_tick } else { 0 };
        let _ = core::fmt::write(out, format_args!("{}{} {} {} ticks [{}]\n", prefix, span.agent, span.skill, dur, status));
        for child in &self.spans {
            if child.parent == Some(span.id) { self.format_span(out, child, depth + 1); }
        }
    }
}
