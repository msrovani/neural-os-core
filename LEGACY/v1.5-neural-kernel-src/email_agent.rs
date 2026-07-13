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

    /// Envia email via SMTP. Requer um servidor SMTP local ou relay.
    pub fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), &'static str> {
        let cfg = crate::net::NET_CONFIG.lock();
        let dns = cfg.dns_ip;
        drop(cfg);

        let mut msg = Vec::new();
        msg.extend_from_slice(b"HELO aios\r\n");
        msg.extend_from_slice(&alloc::format!("MAIL FROM:<{}>\r\n", self.from_addr).into_bytes());
        msg.extend_from_slice(&alloc::format!("RCPT TO:<{}>\r\n", to).into_bytes());
        msg.extend_from_slice(b"DATA\r\n");
        msg.extend_from_slice(&alloc::format!("From: {}\r\n", self.from_addr).into_bytes());
        msg.extend_from_slice(&alloc::format!("To: {}\r\n", to).into_bytes());
        msg.extend_from_slice(&alloc::format!("Subject: {}\r\n", subject).into_bytes());
        msg.extend_from_slice(b"\r\n");
        msg.extend_from_slice(body.as_bytes());
        msg.extend_from_slice(b"\r\n.\r\n");
        msg.extend_from_slice(b"QUIT\r\n");

        let raw = unsafe { crate::net::http_get_raw(dns, self.smtp_port, &msg) };
        match raw {
            Some(_) => {
                kjson!("EMAIL", "agent", "send", "to", to, "subject", subject);
                Ok(())
            }
            None => {
                kjson!("EMAIL", "agent", "err", "to", to);
                Err("smtp failed")
            }
        }
    }

    pub fn status(&self) -> String {
        alloc::format!("[EMAIL] from: {} via {}:{}", self.from_addr,
            self.smtp_host[0], self.smtp_port)
    }
}
