extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use crate::kjson;

pub struct EmailAgent {
    pub smtp_host: [u8; 4],
    pub smtp_port: u16,
    pub from_addr: String,
}

impl EmailAgent {
    pub fn new(host: [u8; 4], port: u16, from: &str) -> Self {
        EmailAgent { smtp_host: host, smtp_port: port, from_addr: String::from(from) }
    }

    /// SMTP real ainda residual — não finge sucesso via http_get no DNS.
    pub fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), &'static str> {
        let _ = (to, subject, body);
        kjson!(
            "EMAIL",
            "agent",
            "err",
            "msg",
            "smtp_dialogue_unwired",
            "host",
            alloc::format!(
                "{}.{}.{}.{}:{}",
                self.smtp_host[0],
                self.smtp_host[1],
                self.smtp_host[2],
                self.smtp_host[3],
                self.smtp_port
            )
        );
        Err("smtp_dialogue_unwired")
    }

    pub fn status(&self) -> String {
        alloc::format!("[EMAIL] from: {} via {}:{}", self.from_addr,
            self.smtp_host[0], self.smtp_port)
    }
}
