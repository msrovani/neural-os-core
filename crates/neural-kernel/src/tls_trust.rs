//! ADR-0016 N4 — trust híbrido: pins (hosts conhecidos) + TOFU (resto).
//! Identidade = SHA-256(leaf X.509 DER). CertificateVerify crypto = residual N+1.
//! Persistência: RAM (FAT opcional depois).

use core::sync::atomic::{AtomicU8, Ordering};

use embedded_tls::blocking::{
    CertificateEntryRef, CertificateRef, CertificateVerifyRef, CryptoProvider, TlsCipherSuite,
    TlsVerifier,
};
use embedded_tls::{Aes128GcmSha256, TlsError};
use rand_core::CryptoRngCore;
use spin::Mutex;

use crate::tls_client::KernelRng;

/// Hosts com política “root-class”: 1ª observação grava pin sticky; mismatch = deny.
const KNOWN_HOSTS: &[&str] = &[
    "www.google.com",
    "google.com",
    "example.com",
    "www.example.com",
];

const MAX_ENTRIES: usize = 16;
const HOST_MAX: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrustClass {
    Unset = 0,
    RootPin = 1,
    RootLearn = 2,
    Tofu = 3,
    TofuLearn = 4,
    Deny = 5,
}

impl TrustClass {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustClass::Unset => "unset",
            TrustClass::RootPin => "root_pin",
            TrustClass::RootLearn => "root_learn",
            TrustClass::Tofu => "tofu",
            TrustClass::TofuLearn => "tofu_learn",
            TrustClass::Deny => "deny",
        }
    }
}

static LAST_TRUST: AtomicU8 = AtomicU8::new(0);

pub fn last_trust() -> TrustClass {
    match LAST_TRUST.load(Ordering::Relaxed) {
        1 => TrustClass::RootPin,
        2 => TrustClass::RootLearn,
        3 => TrustClass::Tofu,
        4 => TrustClass::TofuLearn,
        5 => TrustClass::Deny,
        _ => TrustClass::Unset,
    }
}

fn set_last_trust(t: TrustClass) {
    LAST_TRUST.store(t as u8, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
struct PinEntry {
    host: [u8; HOST_MAX],
    host_len: u8,
    fp: [u8; 32],
    known: bool,
}

struct PinTable {
    entries: [Option<PinEntry>; MAX_ENTRIES],
}

impl PinTable {
    const fn new() -> Self {
        Self {
            entries: [None; MAX_ENTRIES],
        }
    }

    fn lookup(&self, host: &str) -> Option<[u8; 32]> {
        let hb = host.as_bytes();
        for e in self.entries.iter().flatten() {
            if e.host_len as usize == hb.len() && &e.host[..hb.len()] == hb {
                return Some(e.fp);
            }
        }
        None
    }

    fn store(&mut self, host: &str, fp: [u8; 32], known: bool) -> bool {
        let hb = host.as_bytes();
        if hb.is_empty() || hb.len() > HOST_MAX {
            return false;
        }
        for e in self.entries.iter_mut() {
            if let Some(ent) = e {
                if ent.host_len as usize == hb.len() && &ent.host[..hb.len()] == hb {
                    ent.fp = fp;
                    ent.known = known;
                    return true;
                }
            }
        }
        for e in self.entries.iter_mut() {
            if e.is_none() {
                let mut h = [0u8; HOST_MAX];
                h[..hb.len()].copy_from_slice(hb);
                *e = Some(PinEntry {
                    host: h,
                    host_len: hb.len() as u8,
                    fp,
                    known,
                });
                return true;
            }
        }
        false
    }
}

static PINS: Mutex<PinTable> = Mutex::new(PinTable::new());

fn is_known_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.');
    for k in KNOWN_HOSTS {
        if eq_ignore_ascii(h, k) {
            return true;
        }
    }
    false
}

fn eq_ignore_ascii(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

fn leaf_der<'a>(cert: &CertificateRef<'a>) -> Result<&'a [u8], TlsError> {
    match cert.entries.first() {
        Some(CertificateEntryRef::X509(der)) if !der.is_empty() => Ok(*der),
        _ => Err(TlsError::InvalidCertificate),
    }
}

pub struct HybridVerifier {
    host: [u8; HOST_MAX],
    host_len: u8,
}

impl HybridVerifier {
    pub fn new() -> Self {
        Self {
            host: [0u8; HOST_MAX],
            host_len: 0,
        }
    }

    fn host_str(&self) -> &str {
        core::str::from_utf8(&self.host[..self.host_len as usize]).unwrap_or("")
    }
}

impl TlsVerifier<Aes128GcmSha256> for HybridVerifier {
    fn set_hostname_verification(&mut self, hostname: &str) -> Result<(), TlsError> {
        let hb = hostname.as_bytes();
        if hb.len() > HOST_MAX {
            return Err(TlsError::InsufficientSpace);
        }
        self.host = [0u8; HOST_MAX];
        self.host[..hb.len()].copy_from_slice(hb);
        self.host_len = hb.len() as u8;
        Ok(())
    }

    fn verify_certificate(
        &mut self,
        _transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        let der = leaf_der(&cert)?;
        let fp = crate::tpm::sha256(der);
        let host = self.host_str();
        if host.is_empty() {
            set_last_trust(TrustClass::Deny);
            return Err(TlsError::InvalidCertificate);
        }
        let known = is_known_host(host);

        let mut pins = PINS.lock();
        if let Some(stored) = pins.lookup(host) {
            if stored == fp {
                let t = if known {
                    TrustClass::RootPin
                } else {
                    TrustClass::Tofu
                };
                set_last_trust(t);
                k_nano::slog_bin!(
                    "TLS",
                    "info",
                    "trust={} host={} fp={:02x}{:02x}",
                    t.as_str(),
                    host,
                    fp[0],
                    fp[1]
                );
                return Ok(());
            }
            set_last_trust(TrustClass::Deny);
            k_nano::slog_bin!(
                "TLS",
                "info",
                "trust=deny host={} reason=fp_mismatch known={}",
                host,
                known as u8
            );
            return Err(TlsError::InvalidCertificate);
        }

        if !pins.store(host, fp, known) {
            set_last_trust(TrustClass::Deny);
            return Err(TlsError::OutOfMemory);
        }
        let t = if known {
            TrustClass::RootLearn
        } else {
            TrustClass::TofuLearn
        };
        set_last_trust(t);
        k_nano::slog_bin!(
            "TLS",
            "info",
            "trust={} host={} fp={:02x}{:02x} leaf={}",
            t.as_str(),
            host,
            fp[0],
            fp[1],
            der.len()
        );
        Ok(())
    }

    fn verify_signature(&mut self, _verify: CertificateVerifyRef) -> Result<(), TlsError> {
        // Residual: pin/TOFU do leaf; ECDSA/RSA CertificateVerify = N+1.
        Ok(())
    }
}

pub struct HybridProvider {
    rng: KernelRng,
    verifier: HybridVerifier,
}

impl HybridProvider {
    pub fn new() -> Self {
        Self {
            rng: KernelRng,
            verifier: HybridVerifier::new(),
        }
    }
}

impl CryptoProvider for HybridProvider {
    type CipherSuite = Aes128GcmSha256;
    type Signature = p256::ecdsa::DerSignature;

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}
