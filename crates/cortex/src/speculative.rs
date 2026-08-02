//! ADR-0081 C3: DSD — Speculative Decoder distribuído (draft/verify/rejection).
//!
//! ## Visão
//! O nó Worker gera um draft (tokens especulativos) e o Master verifica o
//! draft contra o modelo real (speculative decoding clássico: aceita o prefixo
//! verificado, rejeita o resto, o LLM não re-computa os tokens aceitos).
//!
//! ## Estado (SESSION_237) — honesto
//! A verificação REAL é distribuída via MW/MR-style (enviar o draft ao Master
//! e verificar lá) — mas isso exige um verifier MLP no Master (o draft vem do
//! modelo draft, a verificação do modelo alvo). Enquanto o verifier MLP não
//! existe, o DSD roda em modo LOCAL: `draft_verify` verifica o draft token a
//! token com a `verify_fn` fornecida pelo chamador, conta accepted/rejected, e
//! `mesh_tick` loga as stats (o gate de mesh só muda o log).
//!
//! ## Self-test
//! `self_test()`: draft de 8 tokens + verify_fn identidade → accepted = 8.

use spin::Mutex;

/// Speculative Decoder — mede a taxa de aceitação de drafts especulativos.
pub struct SpeculativeDecoder {
    /// Ativo quando o mesh P2P está vivo (role != Undecided).
    active: bool,
    /// Comprimento do último draft verificado.
    draft_len: u32,
    /// Tokens aceitos acumulados.
    verified: u64,
    /// Tokens rejeitados acumulados.
    rejected: u64,
}

impl SpeculativeDecoder {
    /// Cria um decoder (lazy; active = mesh role != Undecided).
    pub fn new() -> Self {
        let role = k_nano::net::mesh::local_role();
        Self {
            active: role != k_nano::net::mesh::NodeRole::Undecided,
            draft_len: 0,
            verified: 0,
            rejected: 0,
        }
    }

    /// Verifica um draft token a token: para cada prefixo `draft[..=i]`, a
    /// `verify_fn` prediz o próximo token; se bate com `draft[i]`, o token é
    /// aceito; no primeiro mismatch, o resto do draft é rejeitado.
    ///
    /// Retorna `(accepted_prefix, rejected_count)`.
    pub fn draft_verify(&mut self, draft: &[u32], verify_fn: impl Fn(&[u32]) -> u32) -> (u32, usize) {
        self.draft_len = draft.len() as u32;
        let mut accepted: u32 = 0;
        for (i, &tok) in draft.iter().enumerate() {
            let predicted = verify_fn(&draft[..=i]);
            if predicted == tok {
                accepted += 1;
            } else {
                self.rejected += 1;
                break;
            }
        }
        self.verified += accepted as u64;
        let rejected = draft.len().saturating_sub(accepted as usize);
        (accepted, rejected)
    }

    /// (draft_len, verified, rejected) — stats para telemetria/serial.
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.draft_len as u64, self.verified, self.rejected)
    }

    /// Zera as stats (mantém `active`).
    pub fn reset(&mut self) {
        self.draft_len = 0;
        self.verified = 0;
        self.rejected = 0;
    }

    /// Integração com mesh: a cada ~200 ticks, loga as stats do DSD.
    /// ponytail: verificação distribuída real (draft → Master via MW/MR-style,
    /// verifier MLP no Master) fica para quando o verifier existir — hoje o
    /// caminho é local e o log indica se o mesh está vivo.
    pub fn mesh_tick(&mut self, tick: u64) {
        if tick % 200 != 0 {
            return;
        }
        let role = k_nano::net::mesh::local_role();
        self.active = role != k_nano::net::mesh::NodeRole::Undecided;
        k_nano::slog_cortex!(
            "DSD", "info",
            "draft={} verified={} rejected={} (mesh: {} role={:?})",
            self.draft_len, self.verified, self.rejected, self.active, role
        );
    }
}

// ─── Singleton global + wrappers (wiring no bei_tick do bin) ───────────────

lazy_static::lazy_static! {
    static ref DSD: Mutex<SpeculativeDecoder> = Mutex::new(SpeculativeDecoder::new());
}

/// Tick global do DSD — chamado pelo bin (bei_tick) a cada tick do scheduler.
pub fn dsd_tick(tick: u64) {
    DSD.lock().mesh_tick(tick);
}

/// Verifica um draft (global) — `(accepted_prefix, rejected_count)`.
pub fn draft_verify(draft: &[u32], verify_fn: impl Fn(&[u32]) -> u32) -> (u32, usize) {
    DSD.lock().draft_verify(draft, verify_fn)
}

/// Stats globais do DSD.
pub fn stats() -> (u64, u64, u64) {
    DSD.lock().stats()
}

/// Zera as stats globais.
pub fn reset() {
    DSD.lock().reset();
}

// ─── Self-test ────────────────────────────────────────────────────────────

/// Self-test: draft de 8 tokens + verify_fn identidade (devolve o último token
/// do prefixo) → todos aceitos.
pub fn self_test() -> bool {
    let mut d = SpeculativeDecoder::new();
    let draft = [1u32, 2, 3, 4, 5, 6, 7, 8];
    let (accepted, rejected) = d.draft_verify(&draft, |pfx: &[u32]| *pfx.last().unwrap_or(&0));
    if accepted != 8 || rejected != 0 {
        return false;
    }
    let (len, verified, rejected_total) = d.stats();
    len == 8 && verified == 8 && rejected_total == 0
}
