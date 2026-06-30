use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

#[derive(Debug, Clone)]
pub struct SchemeOpen {
    pub id: u64,
    pub path: String,
    pub flags: u8,
}

pub trait SchemeHandler: Send {
    fn name(&self) -> &str;
    fn open(&mut self, path: &str, flags: u8) -> Result<u64, &str>;
    fn read(&mut self, id: u64, buf: &mut [u8]) -> Result<usize, &str>;
    fn write(&mut self, id: u64, data: &[u8]) -> Result<usize, &str>;
    fn close(&mut self, id: u64);
}

pub struct SchemeRegistry {
    schemes: Vec<Box<dyn SchemeHandler>>,
}

impl SchemeRegistry {
    pub fn new() -> Self { SchemeRegistry { schemes: Vec::new() } }

    pub fn register(&mut self, s: Box<dyn SchemeHandler>) {
        self.schemes.push(s);
    }

    pub fn open(&mut self, url: &str, flags: u8) -> Result<u64, &str> {
        let (scheme, path) = url.split_once("://").ok_or("invalid scheme url")?;
        for s in &mut self.schemes {
            if s.name() == scheme { return s.open(path, flags); }
        }
        Err("scheme not found")
    }

    pub fn read(&mut self, id: u64, buf: &mut [u8]) -> Result<usize, &str> {
        for s in &mut self.schemes {
            if let Ok(n) = s.read(id, buf) { return Ok(n); }
        }
        Err("id not found")
    }

    pub fn close(&mut self, id: u64) {
        for s in &mut self.schemes { s.close(id); }
    }

    pub fn list(&self) -> String {
        let mut out = String::from("Schemes:\n");
        for s in &self.schemes { let _ = write!(out, "  {}://\n", s.name()); }
        out
    }
}
