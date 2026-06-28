use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct Metric {
    pub key: String,
    pub value: String,
    pub status: &'static str,
}

#[derive(Debug)]
pub struct Alert {
    pub severity: u8,
    pub message: String,
}

#[derive(Debug)]
pub struct Dashboard {
    pub title: String,
    pub metrics: Vec<Metric>,
    pub alerts: Vec<Alert>,
    pub tick: u64,
}

impl Dashboard {
    pub fn new(title: &str) -> Self {
        Dashboard { title: String::from(title), metrics: Vec::new(), alerts: Vec::new(), tick: 0 }
    }

    pub fn metric(&mut self, key: &str, value: &str, status: &'static str) {
        self.metrics.push(Metric { key: String::from(key), value: String::from(value), status });
    }

    pub fn alert(&mut self, severity: u8, msg: &str) {
        self.alerts.push(Alert { severity, message: String::from(msg) });
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.title); s.push('\n');
        for m in &self.metrics {
            s.push_str(&alloc::format!("  {}: {} [{}]\n", m.key, m.value, m.status));
        }
        for a in &self.alerts {
            s.push_str(&alloc::format!("  ! [{}] {}\n", a.severity, a.message));
        }
        s
    }
}
