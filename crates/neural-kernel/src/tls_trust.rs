//! ADR-0016 N4 + ADR-0071 — trust híbrido: pins (hosts conhecidos) + TOFU (resto).
//! Identidade = SHA-256(leaf X.509 DER). CertificateVerify = ECDSA P-256 + RSA-PSS (L12/L23).
//! Persistência: RAM + SGDB (primário) + FAT `TLSPINS.BIN` (fallback, non-fatal sem disco).

use core::sync::atomic::{AtomicU8, Ordering};

use digest::Digest;
use embedded_tls::blocking::{
    CertificateEntryRef, CertificateRef, CertificateVerifyRef, CryptoProvider, TlsCipherSuite,
    TlsVerifier,
};
use embedded_tls::{Aes128GcmSha256, SignatureScheme, TlsError};
use p256::ecdsa::{signature::Verifier as EcdsaVerifier, Signature as EcdsaSignature, VerifyingKey};
use rand_core::CryptoRngCore;
use rsa::pss::{Signature as RsaPssSignature, VerifyingKey as RsaPssVerifyingKey};
use rsa::signature::Verifier as RsaVerifier;
use rsa::{BigUint, RsaPublicKey};
use sha2::Sha256;
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
const PIN_FILE: &str = "TLSPINS.BIN";
const PIN_MAGIC: &[u8; 4] = b"TLSP";
const PIN_VERSION: u8 = 1;

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
static LAST_CERTVERIFY: AtomicU8 = AtomicU8::new(0);

#[repr(u8)]
enum CertVerifyResult {
    Unset = 0,
    OkEcdsa = 1,
    DenyScheme = 2,
    DenySig = 3,
    DenyNoLeaf = 4,
    OkRsaPss = 5,
}

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

fn set_certverify(r: CertVerifyResult) {
    LAST_CERTVERIFY.store(r as u8, Ordering::Relaxed);
}

pub fn last_certverify_ok() -> bool {
    matches!(
        LAST_CERTVERIFY.load(Ordering::Relaxed),
        x if x == CertVerifyResult::OkEcdsa as u8 || x == CertVerifyResult::OkRsaPss as u8
    )
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

    fn serialize(&self) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec::Vec::with_capacity(8 + MAX_ENTRIES * (1 + HOST_MAX + 32 + 1));
        out.extend_from_slice(PIN_MAGIC);
        out.push(PIN_VERSION);
        let count = self.entries.iter().flatten().count() as u8;
        out.push(count);
        for e in self.entries.iter().flatten() {
            out.push(e.host_len);
            out.extend_from_slice(&e.host);
            out.extend_from_slice(&e.fp);
            out.push(if e.known { 1 } else { 0 });
        }
        out
    }

    fn load_bytes(&mut self, data: &[u8]) -> usize {
        if data.len() < 6 || &data[0..4] != PIN_MAGIC || data[4] != PIN_VERSION {
            return 0;
        }
        let count = data[5] as usize;
        let mut off = 6usize;
        let mut loaded = 0usize;
        for _ in 0..count.min(MAX_ENTRIES) {
            if off + 1 + HOST_MAX + 32 + 1 > data.len() {
                break;
            }
            let host_len = data[off] as usize;
            off += 1;
            if host_len == 0 || host_len > HOST_MAX {
                break;
            }
            let mut host = [0u8; HOST_MAX];
            host.copy_from_slice(&data[off..off + HOST_MAX]);
            off += HOST_MAX;
            let mut fp = [0u8; 32];
            fp.copy_from_slice(&data[off..off + 32]);
            off += 32;
            let known = data[off] != 0;
            off += 1;
            let host_str = core::str::from_utf8(&host[..host_len]).unwrap_or("");
            if self.store(host_str, fp, known) {
                loaded += 1;
            }
        }
        loaded
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

/// Extrai SEC1 P-256 (65 B: 0x04||X||Y) do leaf X.509 via scan BIT STRING típico.
fn extract_p256_sec1(der: &[u8]) -> Option<[u8; 65]> {
    let n = der.len();
    if n < 68 {
        return None;
    }
    for i in 0..n.saturating_sub(67) {
        if der[i] == 0x03 && der[i + 1] == 0x42 && der[i + 2] == 0x00 && der[i + 3] == 0x04 {
            let mut out = [0u8; 65];
            out.copy_from_slice(&der[i + 3..i + 68]);
            return Some(out);
        }
    }
    None
}

/// Scan BIT STRING RSA: SEQUENCE { INTEGER n, INTEGER e } após OID rsaEncryption.
fn extract_rsa_ne(der: &[u8]) -> Option<(BigUint, BigUint)> {
    // OID 1.2.840.113549.1.1.1 = 06 09 2A 86 48 86 F7 0D 01 01 01
    let oid = [0x06u8, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
    let n = der.len();
    let mut oid_at = None;
    for i in 0..n.saturating_sub(oid.len()) {
        if &der[i..i + oid.len()] == oid {
            oid_at = Some(i);
            break;
        }
    }
    let start = oid_at? + oid.len();
    // Find BIT STRING 0x03 after OID
    let mut i = start;
    while i + 4 < n {
        if der[i] == 0x03 {
            let (blen, hdr) = read_asn1_len(&der[i + 1..])?;
            let content = i + 1 + hdr;
            if content >= n || der[content] != 0x00 {
                i += 1;
                continue;
            }
            let seq = content + 1;
            if seq >= n || der[seq] != 0x30 {
                i += 1;
                continue;
            }
            let (_slen, shdr) = read_asn1_len(&der[seq + 1..])?;
            let mut p = seq + 1 + shdr;
            if p >= n || der[p] != 0x02 {
                return None;
            }
            let (nlen, nhdr) = read_asn1_len(&der[p + 1..])?;
            let nstart = p + 1 + nhdr;
            let nend = nstart + nlen;
            if nend > n {
                return None;
            }
            let mut nbytes = &der[nstart..nend];
            if !nbytes.is_empty() && nbytes[0] == 0 {
                nbytes = &nbytes[1..];
            }
            p = nend;
            if p >= n || der[p] != 0x02 {
                return None;
            }
            let (elen, ehdr) = read_asn1_len(&der[p + 1..])?;
            let estart = p + 1 + ehdr;
            let eend = estart + elen;
            if eend > n {
                return None;
            }
            let ebytes = &der[estart..eend];
            let nb = BigUint::from_bytes_be(nbytes);
            let eb = BigUint::from_bytes_be(ebytes);
            if blen > 0 {
                return Some((nb, eb));
            }
        }
        i += 1;
    }
    None
}

fn read_asn1_len(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }
    let b0 = data[0];
    if b0 & 0x80 == 0 {
        return Some((b0 as usize, 1));
    }
    let n = (b0 & 0x7f) as usize;
    if n == 0 || n > 3 || data.len() < 1 + n {
        return None;
    }
    let mut len = 0usize;
    for i in 0..n {
        len = (len << 8) | data[1 + i] as usize;
    }
    Some((len, 1 + n))
}

fn tls13_certverify_msg(transcript: Sha256) -> ([u8; 130], usize) {
    let ctx = b"TLS 1.3, server CertificateVerify\x00";
    let th = transcript.finalize();
    let mut msg = [0u8; 130];
    for b in &mut msg[..64] {
        *b = 0x20;
    }
    msg[64..64 + ctx.len()].copy_from_slice(ctx);
    msg[64 + ctx.len()..64 + ctx.len() + 32].copy_from_slice(th.as_slice());
    (msg, 64 + ctx.len() + 32)
}

/// Carrega pins de SGDB (primário) ou FAT (fallback). Non-fatal.
pub fn load_pins_from_fat() {
    // Try SGDB first
    if load_pins_from_sgdb() {
        return;
    }
    // Fall back to FAT32
    let Some(data) = crate::gguf::read_fat_range(PIN_FILE, 0, 4096) else {
        k_nano::slog_bin!("TLS", "info", "pins=FAT miss file={}", PIN_FILE);
        return;
    };
    let n = PINS.lock().load_bytes(&data);
    k_nano::slog_bin!(
        "TLS",
        "info",
        "pins=FAT load n={} file={} bytes={}",
        n,
        PIN_FILE,
        data.len()
    );
}

/// Persiste pins em FAT. Non-fatal.
pub fn persist_pins_to_fat() {
    let blob = PINS.lock().serialize();
    // Always write to SGDB
    let _ = k_ai::sgdb::put_kv("sys/tls_pins", &blob);
    // Also write to FAT32 (fallback)
    match crate::gguf::write_fat_file(PIN_FILE, &blob) {
        Ok(()) => {
            k_nano::slog_bin!(
                "TLS",
                "info",
                "pins=FAT save OK file={} bytes={}",
                PIN_FILE,
                blob.len()
            );
        }
        Err(e) => {
            k_nano::slog_bin!(
                "TLS",
                "info",
                "pins=FAT save SKIP reason={} file={}",
                e,
                PIN_FILE
            );
        }
    }
}

/// Carrega pins de SGDB. Non-fatal.
fn load_pins_from_sgdb() -> bool {
    if !k_ai::sgdb::ready() {
        return false;
    }
    match k_ai::sgdb::get_kv("sys/tls_pins") {
        Ok(Some(data)) => {
            let n = PINS.lock().load_bytes(&data);
            k_nano::slog_bin!("TLS", "info", "pins=SGDB load n={} bytes={}", n, data.len());
            n > 0
        }
        Ok(None) => {
            k_nano::slog_bin!("TLS", "info", "pins=SGDB miss");
            false
        }
        Err(e) => {
            k_nano::slog_bin!("TLS", "info", "pins=SGDB load error={:?}", e);
            false
        }
    }
}

pub struct HybridVerifier {
    host: [u8; HOST_MAX],
    host_len: u8,
    /// Transcript até Certificate (clone do Hash da cipher suite).
    transcript: Option<Sha256>,
    /// SEC1 leaf para CertificateVerify ECDSA.
    leaf_sec1: Option<[u8; 65]>,
    /// Cópia leaf DER (RSA-PSS L23); max 2 KiB.
    leaf_der: Option<alloc::vec::Vec<u8>>,
}

impl HybridVerifier {
    pub fn new() -> Self {
        Self {
            host: [0u8; HOST_MAX],
            host_len: 0,
            transcript: None,
            leaf_sec1: None,
            leaf_der: None,
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
        transcript: &<Aes128GcmSha256 as TlsCipherSuite>::Hash,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        let der = leaf_der(&cert)?;
        let fp = crate::tpm::sha256(der);
        let host = alloc::string::String::from(self.host_str());
        if host.is_empty() {
            set_last_trust(TrustClass::Deny);
            return Err(TlsError::InvalidCertificate);
        }
        let known = is_known_host(&host);

        // Guarda material para CertificateVerify (ADR-0071 / L23).
        self.transcript = Some(transcript.clone());
        self.leaf_sec1 = extract_p256_sec1(der);
        self.leaf_der = Some(alloc::vec::Vec::from(der));
        if self.leaf_sec1.is_none() {
            k_nano::slog_bin!(
                "TLS",
                "info",
                "certverify=WARN reason=no_p256_sec1 leaf={} (try_rsa_pss=1)",
                der.len()
            );
        }

        let mut pins = PINS.lock();
        if let Some(stored) = pins.lookup(&host) {
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

        if !pins.store(&host, fp, known) {
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
        drop(pins);
        persist_pins_to_fat();
        Ok(())
    }

    fn verify_signature(&mut self, verify: CertificateVerifyRef) -> Result<(), TlsError> {
        let scheme = verify.signature_scheme;
        let is_ecdsa = matches!(scheme, SignatureScheme::EcdsaSecp256r1Sha256);
        // 0x0804 = rsa_pss_rsae_sha256 (RFC 8446)
        let is_rsa_pss = scheme.as_u16() == 0x0804;

        if !is_ecdsa && !is_rsa_pss {
            set_certverify(CertVerifyResult::DenyScheme);
            k_nano::slog_bin!(
                "TLS",
                "info",
                "certverify=DENY scheme=0x{:04x} residual=unsupported",
                scheme.as_u16()
            );
            return Err(TlsError::InvalidSignature);
        }

        let Some(transcript) = self.transcript.take() else {
            set_certverify(CertVerifyResult::DenyNoLeaf);
            return Err(TlsError::InvalidSignature);
        };
        let (msg, msg_len) = tls13_certverify_msg(transcript);

        if is_ecdsa {
            let Some(sec1) = self.leaf_sec1.take() else {
                set_certverify(CertVerifyResult::DenyNoLeaf);
                k_nano::slog_bin!("TLS", "info", "certverify=DENY reason=no_leaf_sec1");
                return Err(TlsError::InvalidSignature);
            };
            let vk = VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| {
                set_certverify(CertVerifyResult::DenySig);
                TlsError::InvalidSignature
            })?;
            let sig = EcdsaSignature::from_der(verify.signature).map_err(|_| {
                set_certverify(CertVerifyResult::DenySig);
                TlsError::InvalidSignature
            })?;
            return match EcdsaVerifier::verify(&vk, &msg[..msg_len], &sig) {
                Ok(()) => {
                    set_certverify(CertVerifyResult::OkEcdsa);
                    k_nano::slog_bin!(
                        "TLS",
                        "info",
                        "certverify=OK scheme=ecdsa_secp256r1_sha256"
                    );
                    Ok(())
                }
                Err(_) => {
                    set_certverify(CertVerifyResult::DenySig);
                    k_nano::slog_bin!("TLS", "info", "certverify=DENY reason=ecdsa_bad_sig");
                    Err(TlsError::InvalidSignature)
                }
            };
        }

        // RSA-PSS SHA256 (Labor 23)
        let Some(der) = self.leaf_der.take() else {
            set_certverify(CertVerifyResult::DenyNoLeaf);
            k_nano::slog_bin!("TLS", "info", "certverify=DENY reason=no_leaf_der_rsa");
            return Err(TlsError::InvalidSignature);
        };
        let Some((n, e)) = extract_rsa_ne(&der) else {
            set_certverify(CertVerifyResult::DenyNoLeaf);
            k_nano::slog_bin!("TLS", "info", "certverify=DENY reason=no_rsa_spki");
            return Err(TlsError::InvalidSignature);
        };
        let pubkey = RsaPublicKey::new(n, e).map_err(|_| {
            set_certverify(CertVerifyResult::DenySig);
            TlsError::InvalidSignature
        })?;
        let vk = RsaPssVerifyingKey::<Sha256>::new(pubkey);
        let sig = RsaPssSignature::try_from(verify.signature).map_err(|_| {
            set_certverify(CertVerifyResult::DenySig);
            TlsError::InvalidSignature
        })?;
        match RsaVerifier::verify(&vk, &msg[..msg_len], &sig) {
            Ok(()) => {
                set_certverify(CertVerifyResult::OkRsaPss);
                k_nano::slog_bin!("TLS", "info", "certverify=OK scheme=rsa_pss_sha256");
                Ok(())
            }
            Err(_) => {
                set_certverify(CertVerifyResult::DenySig);
                k_nano::slog_bin!("TLS", "info", "certverify=DENY reason=rsa_pss_bad_sig");
                Err(TlsError::InvalidSignature)
            }
        }
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

/// Labor 33: CA chain mínima — leaf + issuer pin (SHA-256 do DER CA).
static CA_PINS: Mutex<[[u8; 32]; 4]> = Mutex::new([[0u8; 32]; 4]);
static CA_N: AtomicU8 = AtomicU8::new(0);

pub fn pin_ca_der(ca_der: &[u8]) -> bool {
    if ca_der.is_empty() {
        return false;
    }
    let fp = crate::tpm::sha256(ca_der);
    let n = CA_N.load(Ordering::Relaxed) as usize;
    if n >= 4 {
        return false;
    }
    CA_PINS.lock()[n] = fp;
    CA_N.store((n + 1) as u8, Ordering::Relaxed);
    true
}

/// Verifica se `issuer_der` está no pin set (leaf+1).
pub fn issuer_pinned(issuer_der: &[u8]) -> bool {
    let fp = crate::tpm::sha256(issuer_der);
    let n = CA_N.load(Ordering::Relaxed) as usize;
    let pins = CA_PINS.lock();
    pins[..n].iter().any(|p| *p == fp)
}

pub fn ca_chain_boot_smoke() -> bool {
    // Synthetic CA DER = ASCII label (smoke only — not a real X.509)
    let ca = b"NEURAL-OS-TEST-CA-DER-V1";
    let leaf_issuer = ca;
    let ok = pin_ca_der(ca) && issuer_pinned(leaf_issuer);
    k_nano::slog_bin!(
        "TLS",
        "info",
        "step=ca_chain status={} pins={} VERDICT={}",
        if ok { "OK" } else { "FAIL" },
        CA_N.load(Ordering::Relaxed),
        if ok { "PASS" } else { "FAIL" }
    );
    ok
}
