//! Framing length-prefix + protocolo CapGate sobre Unix socket / TCP loopback.

#![forbid(unsafe_code)]

use nos_core::CapBits;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_SOCKET: &str = "/run/nosd.sock";

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("short read")]
    Short,
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame too large")]
    TooLarge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    /// Cap bits held by caller (daemon re-valida).
    pub caps: u32,
    pub cap_gen: u64,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub result: serde_json::Value,
}

impl Request {
    pub fn required_cap(&self) -> CapBits {
        match self.method.as_str() {
            "health" | "score" | "model.list" => CapBits::NONE,
            "infer.submit" | "chat" => CapBits::INFER,
            "agent.spawn" | "factory.promote" => CapBits::AGENT_SPAWN,
            "memory.remember" | "memory.recall" | "memory.curate" => CapBits::MEMORY,
            "nix.propose" => CapBits::NIX_WRITE,
            _ => CapBits::ALL,
        }
    }
}

pub fn encode(msg: &impl Serialize) -> Result<Vec<u8>, IpcError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > 16 * 1024 * 1024 {
        return Err(IpcError::TooLarge);
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, IpcError> {
    Ok(serde_json::from_slice(body)?)
}

/// Extrai um frame completo de um buffer acumulado; devolve (msg_body, rest).
pub fn take_frame(buf: &[u8]) -> Result<Option<(Vec<u8>, usize)>, IpcError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let n = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    if n > 16 * 1024 * 1024 {
        return Err(IpcError::TooLarge);
    }
    if buf.len() < 4 + n {
        return Ok(None);
    }
    Ok(Some((buf[4..4 + n].to_vec(), 4 + n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_frame() {
        let req = Request {
            id: 1,
            method: "health".into(),
            caps: 0,
            cap_gen: 0,
            params: serde_json::json!({}),
        };
        let enc = encode(&req).unwrap();
        let (body, consumed) = take_frame(&enc).unwrap().unwrap();
        assert_eq!(consumed, enc.len());
        let back: Request = decode_body(&body).unwrap();
        assert_eq!(back.method, "health");
    }
}
