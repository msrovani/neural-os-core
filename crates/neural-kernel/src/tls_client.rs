//! ADR-0016 N4 — cliente TLS 1.3 (`embedded-tls`) sobre NETSTACK.
//! Trust: HybridProvider (pins + TOFU) — ver tls_trust.rs.
//! Soft crypto: `polyval_force_soft` + `aes_force_soft` + `sha2/force-soft` (.cargo/config).
//! Alinhamento 16-byte para AES-NI via `alloc_aligned_buf()`.

use alloc::string::String;
use alloc::vec::Vec;
use core::num::NonZeroU32;

use embedded_io::{ErrorType, Read, Write};
use embedded_tls::blocking::TlsConnection;
use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsContext, TlsError};
use rand_core::{CryptoRng, RngCore};
use smoltcp::iface::SocketHandle;

use crate::hw_rng::HardwareRandom;
use crate::netstack::NetStack;
use crate::tls_trust::HybridProvider;

/// Aloca buffer com alinhamento 16-byte para AES-NI (evita #GP/#DF).
/// Retorna Vec com capacidade `size` e ponteiro alinhado a 16 bytes.
pub fn alloc_aligned_buf(size: usize) -> Vec<u8> {
    let align = 16;
    let layout = core::alloc::Layout::from_size_align(size, align).expect("alloc_aligned_buf layout");
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        alloc::alloc::handle_alloc_error(layout);
    }
    unsafe { Vec::from_raw_parts(ptr, 0, size) }
}

/// RNG kernel → `rand_core` / `embedded-tls`.
pub struct KernelRng;

impl RngCore for KernelRng {
    fn next_u32(&mut self) -> u32 {
        HardwareRandom::next_u64_retry(16).unwrap_or(0) as u32
    }

    fn next_u64(&mut self) -> u64 {
        HardwareRandom::next_u64_retry(16).unwrap_or(0)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let _ = HardwareRandom::fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        HardwareRandom::fill_bytes(dest)
            .map_err(|_| rand_core::Error::from(NonZeroU32::new(1).unwrap()))
    }
}

impl CryptoRng for KernelRng {}

#[derive(Debug)]
pub struct TlsIoError;

impl core::fmt::Display for TlsIoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("tls_io")
    }
}

impl core::error::Error for TlsIoError {}

impl embedded_io::Error for TlsIoError {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

pub struct NetTcpIo<'a> {
    stack: &'a mut NetStack,
    handle: SocketHandle,
    now: u64,
}

impl<'a> NetTcpIo<'a> {
    pub fn new(stack: &'a mut NetStack, handle: SocketHandle, now: u64) -> Self {
        Self {
            stack,
            handle,
            now,
        }
    }
}

impl ErrorType for NetTcpIo<'_> {
    type Error = TlsIoError;
}

impl Read for NetTcpIo<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.stack
            .tcp_session_recv(self.handle, buf, self.now)
            .map_err(|_| TlsIoError)
    }
}

impl Write for NetTcpIo<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.stack
            .tcp_session_send(self.handle, buf, self.now)
            .map_err(|_| TlsIoError)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.stack.tcp_session_poll(self.now);
        Ok(())
    }
}

fn tls_err_reason(e: TlsError) -> &'static str {
    match e {
        TlsError::ConnectionClosed => "tls_conn_closed",
        TlsError::IoError | TlsError::Io(_) => "tls_io",
        TlsError::InvalidHandshake => "tls_bad_hs",
        TlsError::InvalidCipherSuite => "tls_cipher",
        TlsError::InvalidCertificate | TlsError::InvalidCertificateEntry => "tls_cert",
        TlsError::CryptoError => "tls_crypto",
        TlsError::UnableToInitializeCryptoEngine => "tls_crypto_init",
        TlsError::OutOfMemory | TlsError::InsufficientSpace => "tls_oom",
        TlsError::HandshakeAborted(_, _) | TlsError::AbortHandshake(_, _) => "tls_alert",
        TlsError::InvalidRecord => "tls_record",
        _ => "tls_handshake",
    }
}

/// HTTPS GET sobre TLS 1.3. Caller strippa headers HTTP.
pub fn https_get_on_stack(
    stack: &mut NetStack,
    ip: [u8; 4],
    port: u16,
    host: &str,
    path: &str,
    now: u64,
) -> Result<Vec<u8>, &'static str> {
    let _ = stack.prime_neighbor_for_http();
    k_nano::slog_bin!("TLS", "info", "step=tcp_connect");
    let handle = stack
        .tcp_session_connect(ip, port, now)
        .ok_or("tls_tcp_connect")?;
    k_nano::slog_bin!("TLS", "info", "step=tcp_ok");

    // Buffers alinhados a 16-byte para AES-NI (ClaudioOS pattern)
    let mut read_buf = alloc_aligned_buf(16_384);
    let mut write_buf = alloc_aligned_buf(16_384);

    let result = (|| {
        let io = NetTcpIo::new(stack, handle, now);
        let config = TlsConfig::new()
            .with_server_name(host)
            .enable_rsa_signatures();
        let mut tls: TlsConnection<'_, _, Aes128GcmSha256> =
            TlsConnection::new(io, &mut read_buf, &mut write_buf);
        let provider = HybridProvider::new();
        k_nano::slog_bin!("TLS", "info", "step=handshake");
        tls.open(TlsContext::new(&config, provider))
            .map_err(tls_err_reason)?;
        k_nano::slog_bin!(
            "TLS",
            "info",
            "step=handshake_ok trust={}",
            crate::tls_trust::last_trust().as_str()
        );

        let req = alloc::format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path,
            host
        );
        tls.write_all(req.as_bytes()).map_err(|_| "tls_write")?;
        tls.flush().map_err(|_| "tls_flush")?;
        k_nano::slog_bin!("TLS", "info", "step=http_sent");

        let mut body = Vec::new();
        let mut chunk = [0u8; 4096];
        for _ in 0..64 {
            match tls.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&chunk[..n]),
                Err(_) => {
                    if body.is_empty() {
                        return Err("tls_read");
                    }
                    break;
                }
            }
            if body.len() > 512 * 1024 {
                break;
            }
        }
        if body.is_empty() {
            Err("tls_empty")
        } else {
            Ok(body)
        }
    })();

    stack.tcp_session_close(handle, now);
    result
}

pub fn parse_https_url(url: &str) -> Result<(String, u16, String), &'static str> {
    let u = url.trim();
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("HTTPS://"))
        .ok_or("bad_https_url")?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if hostport.is_empty() {
        return Err("bad_https_url");
    }
    let (host, port) = if let Some(i) = hostport.rfind(':') {
        let maybe_port = &hostport[i + 1..];
        if maybe_port.chars().all(|c| c.is_ascii_digit()) {
            let p: u16 = maybe_port.parse().map_err(|_| "bad_port")?;
            (&hostport[..i], p)
        } else {
            (hostport, 443u16)
        }
    } else {
        (hostport, 443u16)
    };
    Ok((String::from(host), port, String::from(path)))
}
