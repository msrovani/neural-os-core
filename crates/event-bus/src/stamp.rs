//! E3 (OPCODE-0088): kernel-stamped provenance ring for the `EventBus`.
//!
//! Cada evento publicado ganha um `Stamp` encadeado (hash do stamp anterior +
//! campos + digest do payload) num anel bounded. O carimbo é best-effort: se o
//! lock bounded do anel falhar, o stamp é DROPADO (`AUDIT_SKIPPED`) mas o evento
//! é entregue — nunca bloqueia o publish nem aninha o lock do anel no lock dos
//! subscribers.
//!
//! Hasher: registrado por fn-pointer bridge (`register_audit_hooks`); sem
//! registro cai no FNV-1a-64 padded (algo=0) — honesto, NÃO finge sha256.

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const STAMP_RING_CAP: usize = 512;
pub const HASH_PREFIX_MAX: usize = 256;

/// Carimbo de proveniência de um evento. `Clone + Copy` (lido/copiado no verify).
#[derive(Clone, Copy)]
pub struct Stamp {
    pub seq: u64,
    pub event_id: u64,
    pub token_id: u64,
    pub publisher: u64,
    pub causal_parent_id: u64,
    pub taint_label: u32,
    pub algo: u8,
    pub digest_len: u32,
    pub payload_digest: [u8; 32],
    pub chain_hash: [u8; 32],
}

/// Anel bounded de stamps com encadeamento. `base_hash` = chain_hash do stamp
/// IMEDIATAMENTE antes do entry mais antigo retido — base da verificação após
/// truncamento (evicção).
pub struct StampRing {
    entries: VecDeque<Stamp>,
    last_hash: [u8; 32],
    base_hash: [u8; 32],
    next_seq: u64,
    dropped: u64,
}

impl StampRing {
    pub fn new() -> Self {
        StampRing {
            entries: VecDeque::with_capacity(STAMP_RING_CAP),
            last_hash: [0u8; 32],
            base_hash: [0u8; 32],
            next_seq: 0,
            dropped: 0,
        }
    }

    /// Anexa; se cheio, evicta o mais antigo (o chain_hash dele vira `base_hash`)
    /// e conta `dropped`.
    pub fn push(&mut self, st: Stamp) {
        if self.entries.len() >= STAMP_RING_CAP {
            if let Some(ev) = self.entries.pop_front() {
                self.base_hash = ev.chain_hash;
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        self.last_hash = st.chain_hash;
        self.next_seq = st.seq.wrapping_add(1);
        self.entries.push_back(st);
    }

    pub fn last_hash(&self) -> [u8; 32] {
        self.last_hash
    }

    pub fn base_hash(&self) -> [u8; 32] {
        self.base_hash
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for StampRing {
    fn default() -> Self {
        Self::new()
    }
}

/// Hasher injetável (fn-pointer bridge). Ex.: `k_nano::tpm::sha256`.
pub type HashFn = fn(&[u8]) -> [u8; 32];
/// Identidade do publicador (sem contexto no `publish`, é um fn global).
pub type PublisherFn = fn() -> u64;

/// FNV-1a 64 (padded a 32 bytes, `algo=0`). Fallback HONESTO — NÃO é sha256.
pub fn fnv1a64_pad32(data: &[u8]) -> [u8; 32] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&h.to_le_bytes());
    out
}

fn unknown_publisher() -> u64 {
    0
}

// Fix 5b (OPCODE-0093): fn pointers em `AtomicUsize` (load Relaxed) — o publish
// path NÃO pega mais um TicketLock bloqueante para ler os hooks.
static AUDIT_HASHER: AtomicUsize = AtomicUsize::new(0);
static AUDIT_PUBLISHER: AtomicUsize = AtomicUsize::new(0);

/// Registra o hasher (ex.: `k_nano::tpm::sha256`) e o publisher. Fn-pointer
/// bridge — event-bus NÃO depende de k_nano (sem ciclo de Cargo).
pub fn register_audit_hooks(hasher: HashFn, publisher: PublisherFn) {
    AUDIT_HASHER.store(hasher as usize, Ordering::Release);
    AUDIT_PUBLISHER.store(publisher as usize, Ordering::Release);
}

/// Hasher ativo (registrado) ou o fallback FNV-1a. Load Relaxed, sem lock.
pub fn active_hasher() -> HashFn {
    let p = AUDIT_HASHER.load(Ordering::Relaxed);
    if p == 0 {
        fnv1a64_pad32
    } else {
        // Safety: só `register_audit_hooks` escreve, sempre um `HashFn` válido.
        unsafe { core::mem::transmute::<usize, HashFn>(p) }
    }
}

/// Publisher ativo (registrado) ou 0 (desconhecido). Load Relaxed, sem lock.
pub fn active_publisher() -> PublisherFn {
    let p = AUDIT_PUBLISHER.load(Ordering::Relaxed);
    if p == 0 {
        unknown_publisher
    } else {
        unsafe { core::mem::transmute::<usize, PublisherFn>(p) }
    }
}

/// `algo=0` para o fallback FNV; `algo=1` para um hasher registrado (sha256).
fn algo_for(hasher: HashFn) -> u8 {
    let fallback: HashFn = fnv1a64_pad32;
    if hasher as usize == fallback as usize {
        0
    } else {
        1
    }
}

/// Copia `src` para `dst` a partir de `*pos` (capa no espaço restante). Zero alloc.
#[inline]
fn copy_into(dst: &mut [u8], pos: &mut usize, src: &[u8]) {
    let room = dst.len().saturating_sub(*pos);
    let n = src.len().min(room);
    dst[*pos..*pos + n].copy_from_slice(&src[..n]);
    *pos += n;
}

#[inline]
fn put_u64(dst: &mut [u8], pos: &mut usize, v: u64) {
    copy_into(dst, pos, &v.to_le_bytes());
}

#[inline]
fn put_u32(dst: &mut [u8], pos: &mut usize, v: u32) {
    copy_into(dst, pos, &v.to_le_bytes());
}

/// Encadeia um stamp ao `prev_hash`. Recomponível a partir dos campos do
/// `Stamp` (o verify não precisa do evento original).
///
/// Fix 4 (OPCODE-0093): buffer de STACK (`[u8; 128]`; o layout usa 113 bytes) —
/// zero heap alloc no publish path.
#[allow(clippy::too_many_arguments)]
fn chain_of(
    prev_hash: [u8; 32],
    seq: u64,
    event_id: u64,
    token_id: u64,
    publisher: u64,
    causal_parent_id: u64,
    taint_label: u32,
    algo: u8,
    digest_len: u32,
    payload_digest: &[u8; 32],
    hasher: HashFn,
) -> [u8; 32] {
    let mut cbuf = [0u8; 128];
    let mut p = 0usize;
    copy_into(&mut cbuf, &mut p, &prev_hash);
    put_u64(&mut cbuf, &mut p, seq);
    put_u64(&mut cbuf, &mut p, event_id);
    put_u64(&mut cbuf, &mut p, token_id);
    put_u64(&mut cbuf, &mut p, publisher);
    put_u64(&mut cbuf, &mut p, causal_parent_id);
    put_u32(&mut cbuf, &mut p, taint_label);
    copy_into(&mut cbuf, &mut p, &[algo]);
    put_u32(&mut cbuf, &mut p, digest_len);
    copy_into(&mut cbuf, &mut p, payload_digest);
    hasher(&cbuf[..p])
}

/// Constrói um `Stamp`.
///
/// `payload_digest` compromete `topic || payload[..prefix_len] || payload_len_le`
/// (prefix_len = min(len, HASH_PREFIX_MAX); 0 para tópicos `AUDIO_*`). O topic
/// entra no digest porque o `Stamp` NÃO retém os bytes do tópico — sem isso o
/// `verify_stamps(ring, hasher)` não teria como recomputar a cadeia a partir do
/// anel. `chain_hash` encadeia `prev_hash || campos || payload_digest`.
pub fn build_stamp(
    seq: u64,
    event_id: u64,
    token_id: u64,
    publisher: u64,
    prev_hash: [u8; 32],
    topic: &[u8],
    payload: &[u8],
    hasher: HashFn,
) -> Stamp {
    let prefix_len = if topic.starts_with(b"AUDIO_") {
        0
    } else {
        payload.len().min(HASH_PREFIX_MAX)
    };
    let algo = algo_for(hasher);
    let digest_len = prefix_len as u32;
    // Sem contexto causal/taint no publish atual — defaults honestos.
    let causal_parent_id = 0u64;
    let taint_label = 0u32;

    // Fix 4 (OPCODE-0093): buffer de STACK — 256 (payload prefix) + 64 (topic,
    // capado) + 8 (len). Zero heap alloc no publish path. O topic é truncado a
    // 64B no digest (tópicos reais do bus são curtos).
    let topic_n = topic.len().min(64);
    let mut dbuf = [0u8; 256 + 64 + 8];
    let mut dp = 0usize;
    copy_into(&mut dbuf, &mut dp, &topic[..topic_n]);
    copy_into(&mut dbuf, &mut dp, &payload[..prefix_len]);
    put_u64(&mut dbuf, &mut dp, payload.len() as u64);
    let payload_digest = hasher(&dbuf[..dp]);

    let chain_hash = chain_of(
        prev_hash,
        seq,
        event_id,
        token_id,
        publisher,
        causal_parent_id,
        taint_label,
        algo,
        digest_len,
        &payload_digest,
        hasher,
    );

    Stamp {
        seq,
        event_id,
        token_id,
        publisher,
        causal_parent_id,
        taint_label,
        algo,
        digest_len,
        payload_digest,
        chain_hash,
    }
}

/// Verifica a cadeia retida. `Ok(n)` = `n` stamps íntegros; `Err(seq)` =
/// primeiro stamp cujo `chain_hash` recomputado não bate. Começa de
/// `ring.base_hash` (suporta truncamento por evicção).
///
/// Fix 1 (OPCODE-0093): hasher POR ENTRY a partir de `st.algo` — stamps antigos
/// (algo=0, FNV) continuam verificáveis depois que `register_audit_hooks` troca
/// o hasher global (sha256); o hasher passado só vale para `algo != 0`.
pub fn verify_stamps(ring: &StampRing, hasher: HashFn) -> Result<u64, u64> {
    let mut prev = ring.base_hash;
    let mut n = 0u64;
    for st in ring.entries.iter() {
        let entry_hasher: HashFn = if st.algo == 0 { fnv1a64_pad32 } else { hasher };
        let want = chain_of(
            prev,
            st.seq,
            st.event_id,
            st.token_id,
            st.publisher,
            st.causal_parent_id,
            st.taint_label,
            st.algo,
            st.digest_len,
            &st.payload_digest,
            entry_hasher,
        );
        if want != st.chain_hash {
            return Err(st.seq);
        }
        prev = st.chain_hash;
        n += 1;
    }
    Ok(n)
}

// Nota: `AUDIT_SKIPPED` (contador de stamps dropados por lock bounded) vive no
// bus (é lá que a aquisição falha); o módulo stamp é puro.

#[cfg(test)]
mod tests {
    use super::*;

    /// Publica N stamps encadeados a partir do hash zero (sem evicção).
    fn fill_ring(ring: &mut StampRing, n: u64, hasher: HashFn) {
        let mut prev = [0u8; 32];
        for i in 0..n {
            let st = build_stamp(
                i,
                1000 + i,
                1,
                42,
                prev,
                b"TEST_TOPIC",
                b"hello-payload",
                hasher,
            );
            prev = st.chain_hash;
            ring.push(st);
        }
    }

    #[test]
    fn chain_verifies_and_detects_tamper() {
        let hasher: HashFn = fnv1a64_pad32;
        let mut ring = StampRing::new();
        let n = 8u64;
        fill_ring(&mut ring, n, hasher);
        assert_eq!(verify_stamps(&ring, hasher), Ok(n));

        // Tamper no chain_hash do stamp seq=3 → verify para no seq=3.
        let mut bad = ring.entries[3];
        bad.chain_hash[0] ^= 0xFF;
        ring.entries[3] = bad;
        assert_eq!(verify_stamps(&ring, hasher), Err(3));

        // Tamper num campo encadeado (event_id) também é detectado.
        let mut ring2 = StampRing::new();
        fill_ring(&mut ring2, n, hasher);
        let mut bad2 = ring2.entries[5];
        bad2.event_id ^= 0xDEAD;
        ring2.entries[5] = bad2;
        assert_eq!(verify_stamps(&ring2, hasher), Err(5));
    }

    #[test]
    fn evict_keeps_chain_and_counts_dropped() {
        let hasher: HashFn = fnv1a64_pad32;
        let mut ring = StampRing::new();
        let total = STAMP_RING_CAP as u64 + 5;
        fill_ring(&mut ring, total, hasher);
        assert!(ring.dropped() >= 5, "dropped={}", ring.dropped());
        assert_eq!(ring.len(), STAMP_RING_CAP);
        // Mesmo truncado, a cadeia retida verifica (base_hash ancora o head).
        assert_eq!(verify_stamps(&ring, hasher), Ok(STAMP_RING_CAP as u64));
    }

    /// Fix 1 (OPCODE-0093): verify escolhe o hasher POR ENTRY pelo `st.algo`
    /// (0 → FNV). Assim stamps FNV antigos continuam válidos mesmo depois que o
    /// hasher global muda (sha256) — o bug mixed-algo do `audit_verify`.
    #[test]
    fn verify_selects_hasher_per_entry_algo() {
        fn other_hasher(data: &[u8]) -> [u8; 32] {
            // Hasher diferente (não-FNV) só para provar a seleção por algo.
            let mut out = [0xABu8; 32];
            for (i, &b) in data.iter().enumerate() {
                out[i % 32] ^= b;
            }
            out
        }
        let mut ring = StampRing::new();
        let mut prev = [0u8; 32];
        for i in 0..3u64 {
            let st = build_stamp(i, i, 1, 7, prev, b"T", b"p", fnv1a64_pad32);
            prev = st.chain_hash;
            ring.push(st);
        }
        for i in 3..5u64 {
            let st = build_stamp(i, i, 1, 7, prev, b"T", b"p", other_hasher);
            prev = st.chain_hash;
            ring.push(st);
        }
        assert_eq!(ring.entries[0].algo, 0);
        assert_eq!(ring.entries[3].algo, 1);
        // FNV entries usam FNV; other entries usam `other_hasher` (passado).
        assert_eq!(verify_stamps(&ring, other_hasher), Ok(5));
        // Passando FNV puro, falha no 1º entry algo=1 (seq=3).
        assert_eq!(verify_stamps(&ring, fnv1a64_pad32), Err(3));
    }
}
