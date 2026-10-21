//! cortex::bitfuzzy — LOADER/PARSER do contentor `model_type=3` ("BitFuzzy").
//!
//! O contentor `.bfz` (writer canónico: `tools/export_no_std.py`) estende a
//! família `.bitnet v6` com um tipo NOVO:
//!
//! ```text
//! [0..52)    header v6 canónico (model_type=3)
//! [52..72)   bloco de config model_type=3 (20 bytes)
//! [72..EOF)  body: embed → bloco partilhado → experts → routing → masks
//!                      → step-emb → act-halting → rms_final → unembed
//!                      → [theta] → [lora]
//! ```
//!
//! ## O QUE ISTO É (Etapa A + Etapa B)
//! Parser/loader estrito do contentor: valida o header, o bloco de config e
//! TODAS as secções do body (offsets + tamanhos derived do próprio ficheiro),
//! e materializa os tensores em RAM. `describe()` despeja a config parseada.
//!
//! **Etapa B** acrescenta o FORWARD PASS: `forward_bfz()` corre a recorrência
//! (K passos sobre UM bloco partilhado + experts MoE fuzzy + ACT-halting) e
//! devolve os logits. A matemática espelha linha a linha a referência Python
//! (`tools/bitfuzzy_moe_recurrent.py`) e é provada por PARIDADE DE LOGITS
//! contra `tools/golden_bfz.ref.f32` (golden gerado pela MESMA fonte).
//!
//! ## O QUE ISTO NÃO É (honestidade — ADR-0113)
//! * **O forward é uma PROVA DE MATEMÁTICA NO HOST.** Nunca correu em bare
//!   metal, o modelo do golden é **NÃO TREINADO** (pesos aleatórios) e o
//!   contentor **AINDA NÃO É REGISTÁVEL** no hub (`validate_for_slot`
//!   recusa `model_type=3` em todos os slots; `model_hub::register_bytes`
//!   devolve false) — o boot ainda não sabe USAR um modelo recorrente.
//!   Registo + integração do hub são uma decisão à parte (Etapa C).
//! * O contentor **NÃO é um LLM denso**: tem UM bloco transformer partilhado
//!   pelos pesos aplicado K vezes (recorrente), mais experts MoE, routing
//!   fuzzy (Sugeno) e ACT-halting. Não cabe em `TransformerModel`.
//! * **Sem optimização de VRAM externa.** O loader v6 tem
//!   `vram_external_weights()` (pesos ficam na BAR do GPU em vez de copiados
//!   para RAM). O loader bfz NÃO implementa isso: copia tudo para RAM. Gap
//!   conhecido para o contentor 3B real — os arrays de escala POR LINHA
//!   (`f32[rows]`) precisariam de ponteiros BAR próprios.
//! * `embed_type=1` (Q6_K) é validado em tamanho e guardado em bytes crus; o
//!   forward **recusa** Q6_K (não há dequantizador aqui) em vez de inventar
//!   números. `embed_type=0` (ternário) e `=3` (f32 exacto) correm.
//!
//! ## DESVIOS DO v6 CANÓNICO (contrato do contentor — NÃO "corrigir")
//! 1. Bloco de config de 20 bytes logo a seguir ao header (o v6 não tem
//!    lugar para num_experts/top_k/head_dim/rms_eps/...).
//! 2. Escala **por linha de saída** (`f32[rows]`) e não um escalar.
//! 3. Matrizes ternárias guardadas como **[OUT, IN]** (convenção
//!    `torch.nn.Linear`); os tensores canónicos do kernel v6 são [IN, OUT].
//!    Os campos chamam-se `rows`(=OUT) / `cols`(=IN) para tornar a troca
//!    impossível de ignorar.
//! 4. `embed_type=3` = f32 exacto (a matriz de embedding é real-valued).
//! 5. As tabelas de routing i16 trazem as constantes de dequant por coluna.
//! 6. Cada record de máscara Sugeno traz um prefixo `u16 in_features`.
//! 7. Os parâmetros de ACT-halting (halt_center, halt_width) são guardados.
//! 8. LoRA (se existir) tem secção própria no fim do ficheiro.

use alloc::vec::Vec;

use crate::tensor::PackedTernaryTensor;

// ── Constantes do contentor (tools/export_no_std.py) ───────────────────────

/// Magic do header v6 (`0xBE11BE11`).
pub const BFZ_MAGIC: u32 = 0xBE11BE11;
/// Versão do header (família v6 canónica).
pub const BFZ_VERSION: u16 = 6;
/// `model_type` do BitFuzzy recurrent MoE **no ficheiro**.
pub const BFZ_MODEL_TYPE: u8 = 3;
/// Tamanho do header v6 canónico.
pub const BFZ_HEADER_SIZE: usize = 52;
/// Tamanho do bloco de config `model_type=3`.
pub const BFZ_CONFIG_SIZE: usize = 20;
/// Offset do início do body (header + config).
pub const BFZ_BODY_OFFSET: usize = BFZ_HEADER_SIZE + BFZ_CONFIG_SIZE; // 72

/// feat 0x01 — `rms_inner_attn` presente (f32[hidden]).
pub const BFZ_FEAT_RMS_INNER: u8 = 0x01;
/// feat 0x02 — `rms_ffn_norm` presente (f32[intermediate]).
pub const BFZ_FEAT_RMS_FFN_NORM: u8 = 0x02;
/// feat 0x04 — theta presente (últimos 4 bytes do ficheiro).
pub const BFZ_FEAT_THETA: u8 = 0x04;
/// feat 0x08 — unembed é Q6_K.
pub const BFZ_FEAT_UNEMBED_Q6K: u8 = 0x08;
/// feat 0x10 — experts MoE presentes.
pub const BFZ_FEAT_EXPERTS: u8 = 0x10;
/// feat 0x20 — tabelas de routing presentes (i16 fixed point).
pub const BFZ_FEAT_ROUTING: u8 = 0x20;
/// feat 0x40 — máscaras Sugeno presentes.
pub const BFZ_FEAT_SUGENO: u8 = 0x40;
/// feat 0x80 — tabela de step-embedding presente.
pub const BFZ_FEAT_STEP_EMB: u8 = 0x80;
/// Todos os bits de feat conhecidos — qualquer outro é **rejeitado**.
pub const BFZ_FEAT_KNOWN: u8 = 0xFF;

/// `embed_type=0` — ternário.
pub const BFZ_EMBED_TERNARY: u8 = 0;
/// `embed_type=1` — Q6_K.
pub const BFZ_EMBED_Q6K: u8 = 1;
/// `embed_type=3` — f32 exacto.
pub const BFZ_EMBED_F32: u8 = 3;

/// Bytes por super-bloco Q6_K (256 pesos).
const Q6K_BLOCK: usize = 210;

// ── Erros ───────────────────────────────────────────────────────────────────

/// Erro do parse do contentor bfz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BfzError {
    /// Buffer curto — `needed` é o offset mínimo para continuar o parse.
    Truncated(usize),
    /// Header/config/feat inválido — recusa explícita (nunca "adivinha").
    Invalid(&'static str),
}

// ── Header / config ────────────────────────────────────────────────────────

/// Header v6 canónico com `model_type=3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BfzHeader {
    pub version: u16,
    pub num_params: u64,
    pub model_type: u8,
    pub hidden: usize,
    /// K — número de passos RECORRENTES sobre o bloco partilhado.
    pub k_steps: usize,
    pub num_heads: usize,
    pub vocab: usize,
    pub max_seq: usize,
    pub intermediate: usize,
    pub num_kv_heads: usize,
    pub q_dim: usize,
    pub num_medusa: usize,
    /// `true` ⇒ o unembed é o próprio embedding (zero bytes no ficheiro).
    pub tied: bool,
    pub tok_type: u8,
    pub tok_len: usize,
    /// 0 = silu, 1 = gelu (o writer só emite 0).
    pub act_type: u8,
    /// 0 = ternário, 1 = Q6_K, 3 = f32 exacto (2 = BF16: rejeitado).
    pub embed_type: u8,
    pub feat: u8,
}

/// Bloco de config `model_type=3` (20 bytes, offset 52).
///
/// ⚠ `rms_eps` é f32 ⇒ sem `Eq` (derivado apenas `PartialEq`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BfzConfig {
    pub num_experts: usize,
    pub top_k: usize,
    pub step_embeddings: usize,
    pub head_dim: usize,
    /// flag bit0 do bloco de config.
    pub use_act_halting: bool,
    pub num_masks: usize,
    pub num_lora: usize,
    pub rms_eps: f32,
}

// ── Tensors ─────────────────────────────────────────────────────────────────

/// Matriz ternária 2-bit do contentor bfz.
///
/// ⚠ CONVENÇÃO (diferente da do kernel v6): o contentor guarda **[OUT, IN]**
/// (convenção `torch.nn.Linear`); os tensores canónicos do kernel v6 são
/// `[IN, OUT]`. `rows` = OUT, `cols` = IN — nomeados assim para que a
/// troca não passe despercebida. A escala é `f32[rows]`: **uma por linha de
/// saída** (absmean), não um escalar.
pub struct BfzTernary {
    /// Nº de features de SAÍDA (= linhas da matriz guardada).
    pub rows: usize,
    /// Nº de features de ENTRADA (= colunas da matriz guardada).
    pub cols: usize,
    /// Trits empacotados (4/byte, LSB-first, row-major sobre (rows, cols)).
    pub trits: PackedTernaryTensor,
    /// `f32[rows]` — escala absmean por linha de saída.
    pub row_scales: Vec<f32>,
}

impl BfzTernary {
    /// Escala da linha `row` (`None` se fora de range).
    pub fn row_scale(&self, row: usize) -> Option<f32> {
        self.row_scales.get(row).copied()
    }

    /// Nº de trits (rows*cols).
    pub fn elems(&self) -> usize {
        self.rows.saturating_mul(self.cols)
    }
}

/// Matriz f32 exacta (embed/unembed com `embed_type=3`).
pub struct BfzF32Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f32>,
}

/// Codificação da matriz de embedding / unembedding.
pub enum BfzEmbed {
    /// `embed_type=0` — trits [hidden, vocab] + `f32[hidden]` (absmean/row).
    Ternary(BfzTernary),
    /// `embed_type=1` — Q6_K [hidden, vocab] + 1 f32. Bytes crus: o
    /// dequantizador Q6_K é da Etapa B (não há forward aqui).
    Q6K {
        rows: usize,
        cols: usize,
        data: Vec<u8>,
        scale: f32,
    },
    /// `embed_type=3` — f32 exacto [hidden, vocab] + 1 f32 (1.0, slot
    /// canónico herdado do writer v6).
    F32 {
        rows: usize,
        cols: usize,
        data: Vec<f32>,
        scale: f32,
    },
}

/// Um expert do MoE (gate/up/down).
pub struct BfzExpert {
    /// [intermediate, hidden]
    pub gate: BfzTernary,
    /// [intermediate, hidden]
    pub up: BfzTernary,
    /// [hidden, intermediate]
    pub down: BfzTernary,
}

/// Tabelas de routing fuzzy (i16 fixed point + constantes de dequant).
pub struct BfzRouting {
    /// i16[num_experts * hidden] — centros (row-major por expert).
    pub centers_q: Vec<i16>,
    /// i16[num_experts * hidden] — larguras (row-major por expert).
    pub widths_q: Vec<i16>,
    /// f32[hidden] — offset de dequant por coluna.
    pub centers_offset: Vec<f32>,
    /// f32[hidden] — escala de dequant por coluna.
    pub centers_scale: Vec<f32>,
    /// f32[hidden] — escala de dequant por coluna.
    pub widths_scale: Vec<f32>,
    /// Matriz de routing ternária [num_experts, hidden].
    pub wg: BfzTernary,
}

/// Máscara Sugeno (1 x in_features, ternário).
pub struct BfzMask {
    pub in_features: usize,
    pub trits: PackedTernaryTensor,
}

/// Tabela de step-embedding (o f32 é o **GANHO** por passo, não um absmean).
pub struct BfzStepEmb {
    /// Ternário [step_embeddings, hidden].
    pub trits: PackedTernaryTensor,
    /// f32[step_embeddings] — ganho aplicado ao passo (não é escala).
    pub gain: Vec<f32>,
}

/// Par de fatores low-rank (LoRA) guardado no contentor.
pub struct BfzLora {
    pub a_rows: usize,
    pub a_cols: usize,
    pub a: Vec<f32>,
    pub b_rows: usize,
    pub b_cols: usize,
    pub b: Vec<f32>,
}

// ── Plano (offsets + tamanhos) ─────────────────────────────────────────────

/// Deslocamento + tamanho de uma secção (bytes absolutos no ficheiro).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BfzSection {
    pub offset: usize,
    pub len: usize,
}

/// Secções da tabela de routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BfzRoutingSections {
    pub centers_q: BfzSection,
    pub widths_q: BfzSection,
    pub centers_offset: BfzSection,
    pub centers_scale: BfzSection,
    pub widths_scale: BfzSection,
    pub wg: BfzSection,
}

/// Secção de uma máscara Sugeno (+ os seus `in_features`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BfzMaskSection {
    pub section: BfzSection,
    pub in_features: usize,
}

/// Secção de um par LoRA (+ as suas dimensões).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BfzLoraSection {
    pub section: BfzSection,
    pub a_rows: usize,
    pub a_cols: usize,
    pub b_rows: usize,
    pub b_cols: usize,
}

/// Todas as secções do body, com offset e tamanho.
#[derive(Clone, Debug)]
pub struct BfzSections {
    pub embed: BfzSection,
    pub rms_attn: BfzSection,
    pub rms_ffn: BfzSection,
    pub rms_inner: Option<BfzSection>,
    pub rms_ffn_norm: Option<BfzSection>,
    pub q: BfzSection,
    pub k: BfzSection,
    pub v: BfzSection,
    pub o: BfzSection,
    /// Por expert: [gate, up, down].
    pub experts: Vec<[BfzSection; 3]>,
    pub routing: Option<BfzRoutingSections>,
    pub masks: Vec<BfzMaskSection>,
    pub step_emb: Option<BfzSection>,
    pub act_halting: BfzSection,
    pub rms_final: BfzSection,
    pub unembed: Option<BfzSection>,
    pub theta: Option<BfzSection>,
    pub lora: Vec<BfzLoraSection>,
}

/// Plano completo do contentor (header + config + secções + tamanho total).
#[derive(Clone, Debug)]
pub struct BfzPlan {
    pub header: BfzHeader,
    pub config: BfzConfig,
    /// Derivado: `num_kv_heads * head_dim`.
    pub k_dim: usize,
    pub sections: BfzSections,
    /// Tamanho total do ficheiro (último byte consumido).
    pub total: usize,
}

// ── Modelo carregado ───────────────────────────────────────────────────────

/// Contentor BitFuzzy carregado (config + pesos em RAM).
///
/// **Não corre o modelo** — ver o docstring do módulo (Etapa A: sem forward).
pub struct BitFuzzyModel {
    /// Header + config + offsets/tamanhos de todas as secções.
    pub plan: BfzPlan,
    pub embed: BfzEmbed,
    pub rms_attn: Vec<f32>,
    pub rms_ffn: Vec<f32>,
    pub rms_inner: Option<Vec<f32>>,
    pub rms_ffn_norm: Option<Vec<f32>>,
    /// [q_dim, hidden]
    pub q: BfzTernary,
    /// [k_dim, hidden]
    pub k: BfzTernary,
    /// [k_dim, hidden]
    pub v: BfzTernary,
    /// [hidden, q_dim]
    pub o: BfzTernary,
    pub experts: Vec<BfzExpert>,
    pub routing: Option<BfzRouting>,
    pub masks: Vec<BfzMask>,
    pub step_emb: Option<BfzStepEmb>,
    /// Par de ACT-halting (sempre presente no body).
    pub halt_center: f32,
    pub halt_width: f32,
    pub rms_final: Vec<f32>,
    /// `None` quando o unembed é tied ao embedding (zero bytes no ficheiro).
    pub unembed: Option<BfzEmbed>,
    /// rope theta (últimos 4 bytes do ficheiro) quando feat&0x04.
    pub theta: Option<f32>,
    pub lora: Vec<BfzLora>,
}

impl BitFuzzyModel {
    /// `k_dim` derivado (`num_kv_heads * head_dim`).
    pub fn k_dim(&self) -> usize {
        self.plan.k_dim
    }

    /// Despeja a config parseada no log estruturado.
    ///
    /// Só configuração: **não** é uma prova de execução (não há forward).
    pub fn describe(&self) {
        let h = &self.plan.header;
        let c = &self.plan.config;
        let s = &self.plan.sections;
        k_nano::slog_cortex!(
            "BFZ",
            "ok",
            "bitfuzzy v{} type={} hidden={} K={} heads={} kv={} hd={} q_dim={} k_dim={} vocab={} inter={} max_seq={}",
            h.version,
            h.model_type,
            h.hidden,
            h.k_steps,
            h.num_heads,
            h.num_kv_heads,
            c.head_dim,
            h.q_dim,
            self.plan.k_dim,
            h.vocab,
            h.intermediate,
            h.max_seq
        );
        k_nano::slog_cortex!(
            "BFZ",
            "ok",
            "bitfuzzy experts={} top_k={} step_emb={} masks={} lora={} act_halting={} feat=0x{:02X} embed_type={} act_type={} rms_eps={} bytes={}",
            c.num_experts,
            c.top_k,
            c.step_embeddings,
            c.num_masks,
            c.num_lora,
            c.use_act_halting,
            h.feat,
            h.embed_type,
            h.act_type,
            c.rms_eps,
            self.plan.total
        );
        k_nano::slog_cortex!(
            "BFZ",
            "ok",
            "bitfuzzy secções: embed@{}B rms_attn@{}B rms_ffn@{}B q@{}B k@{}B v@{}B o@{}B experts={} routing={} step_emb={} unembed={}B theta={}",
            s.embed.len,
            s.rms_attn.len,
            s.rms_ffn.len,
            s.q.len,
            s.k.len,
            s.v.len,
            s.o.len,
            s.experts.len(),
            s.routing.is_some(),
            s.step_emb.is_some(),
            s.unembed.map(|x| x.len).unwrap_or(0),
            self.theta.is_some()
        );
    }
}

// ── Helpers de tamanho ─────────────────────────────────────────────────────

/// Bytes dos trits de `n` elementos (4 trits/byte).
fn tern_trit_len(n: usize) -> Option<usize> {
    n.checked_add(3)?.checked_div(4)
}

/// Bytes de uma matriz ternária [rows, cols] + escalas `f32[rows]`.
fn tern_bytes(rows: usize, cols: usize) -> Option<usize> {
    let n = rows.checked_mul(cols)?;
    let trits = tern_trit_len(n)?;
    trits.checked_add(rows.checked_mul(4)?)
}

/// Bytes de uma matriz Q6_K de `n` elementos + 1 f32 de escala.
fn q6k_bytes(n: usize) -> Option<usize> {
    let blocks = n.checked_add(255)?.checked_div(256)?;
    blocks.checked_mul(Q6K_BLOCK)?.checked_add(4)
}

/// Bytes de uma matriz f32 [rows, cols] + 1 f32 (slot canónico).
fn f32_matrix_bytes(rows: usize, cols: usize) -> Option<usize> {
    let n = rows.checked_mul(cols)?;
    n.checked_mul(4)?.checked_add(4)
}

/// Bytes de uma matriz de embedding conforme o `embed_type`.
fn embed_bytes(rows: usize, cols: usize, embed_type: u8) -> Option<usize> {
    match embed_type {
        BFZ_EMBED_TERNARY => tern_bytes(rows, cols),
        BFZ_EMBED_Q6K => q6k_bytes(rows.checked_mul(cols)?),
        BFZ_EMBED_F32 => f32_matrix_bytes(rows, cols),
        _ => None,
    }
}

// ── Cursor com bounds ──────────────────────────────────────────────────────

struct Cur<'a> {
    data: &'a [u8],
    off: usize,
}

impl<'a> Cur<'a> {
    fn new(data: &'a [u8], off: usize) -> Self {
        Cur { data, off }
    }

    fn need(&self, n: usize) -> Result<(), BfzError> {
        match self.off.checked_add(n) {
            Some(end) if end <= self.data.len() => Ok(()),
            _ => Err(BfzError::Truncated(self.off.saturating_add(n))),
        }
    }

    fn skip(&mut self, n: usize) -> Result<(), BfzError> {
        self.need(n)?;
        self.off += n;
        Ok(())
    }

    fn u16(&mut self) -> Result<u16, BfzError> {
        self.need(2)?;
        let b = [self.data[self.off], self.data[self.off + 1]];
        self.off += 2;
        Ok(u16::from_le_bytes(b))
    }

    fn mark(&self) -> usize {
        self.off
    }

    fn section_since(&self, start: usize) -> BfzSection {
        BfzSection {
            offset: start,
            len: self.off - start,
        }
    }
}

/// Consome `len` bytes e devolve a secção (offset + tamanho).
fn take_sec(cur: &mut Cur<'_>, len: usize) -> Result<BfzSection, BfzError> {
    let start = cur.mark();
    cur.skip(len)?;
    Ok(cur.section_since(start))
}

// ── Header / config ────────────────────────────────────────────────────────

fn parse_header(data: &[u8]) -> Result<BfzHeader, BfzError> {
    if data.len() < BFZ_HEADER_SIZE {
        return Err(BfzError::Truncated(BFZ_HEADER_SIZE));
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != BFZ_MAGIC {
        return Err(BfzError::Invalid("bad_magic"));
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != BFZ_VERSION {
        return Err(BfzError::Invalid("bad_version"));
    }
    let num_params = u64::from_le_bytes([
        data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
    ]);
    let model_type = data[14];
    if model_type != BFZ_MODEL_TYPE {
        return Err(BfzError::Invalid("bad_model_type"));
    }
    // reserved[3] tem de ser zero (writer emite b"\x00\x00\x00").
    if data[15] != 0 || data[16] != 0 || data[17] != 0 {
        return Err(BfzError::Invalid("reserved_not_zero"));
    }
    let hidden = u16::from_le_bytes([data[18], data[19]]) as usize;
    let k_steps = u16::from_le_bytes([data[20], data[21]]) as usize;
    let num_heads = u16::from_le_bytes([data[22], data[23]]) as usize;
    let vocab = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let max_seq = u16::from_le_bytes([data[28], data[29]]) as usize;
    let intermediate = u16::from_le_bytes([data[30], data[31]]) as usize;
    let num_kv_heads = u16::from_le_bytes([data[32], data[33]]) as usize;
    let q_dim = u16::from_le_bytes([data[34], data[35]]) as usize;
    let num_medusa = u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let tie = &data[40..44] == b"TIED";
    // tie_flag: só "TIED" ou 4 zeros — qualquer outro é layout ambíguo.
    if !tie && data[40..44] != [0, 0, 0, 0] {
        return Err(BfzError::Invalid("bad_tie_flag"));
    }
    let tok_type = data[44];
    let tok_len =
        u32::from_le_bytes([data[45], data[46], data[47], data[48]]) as usize;
    if tok_len != 0 || tok_type != 0 {
        // O contentor bfz não transporta tokenizer: o body começa em 72.
        return Err(BfzError::Invalid("tok_not_supported"));
    }
    let act_type = data[49];
    if act_type > 1 {
        return Err(BfzError::Invalid("bad_act_type"));
    }
    let embed_type = data[50];
    if embed_type != BFZ_EMBED_TERNARY
        && embed_type != BFZ_EMBED_Q6K
        && embed_type != BFZ_EMBED_F32
    {
        // 2 = BF16 declarado mas NÃO implementado neste contentor.
        return Err(BfzError::Invalid("bad_embed_type"));
    }
    let feat = data[51];
    if feat & !BFZ_FEAT_KNOWN != 0 {
        return Err(BfzError::Invalid("unknown_feat_bits"));
    }
    if num_medusa != 0 {
        // Não há secção de medusa no body bfz.
        return Err(BfzError::Invalid("medusa_not_supported"));
    }
    if hidden == 0 || vocab == 0 || intermediate == 0 || q_dim == 0 {
        return Err(BfzError::Invalid("zero_dim"));
    }
    if k_steps == 0 || num_heads == 0 || num_kv_heads == 0 {
        return Err(BfzError::Invalid("zero_arch"));
    }
    Ok(BfzHeader {
        version,
        num_params,
        model_type,
        hidden,
        k_steps,
        num_heads,
        vocab,
        max_seq,
        intermediate,
        num_kv_heads,
        q_dim,
        num_medusa,
        tied: tie,
        tok_type,
        tok_len,
        act_type,
        embed_type,
        feat,
    })
}

fn parse_config(data: &[u8]) -> Result<BfzConfig, BfzError> {
    let base = BFZ_HEADER_SIZE;
    if data.len() < base + BFZ_CONFIG_SIZE {
        return Err(BfzError::Truncated(base + BFZ_CONFIG_SIZE));
    }
    let g = |i: usize| u16::from_le_bytes([data[base + i], data[base + i + 1]]) as usize;
    let num_experts = g(0);
    let top_k = g(2);
    let step_embeddings = g(4);
    let head_dim = g(6);
    let flags = g(8);
    let num_masks = g(10);
    let num_lora = g(12);
    let pad = g(14);
    if flags & !1 != 0 {
        return Err(BfzError::Invalid("bad_flags"));
    }
    if pad != 0 {
        return Err(BfzError::Invalid("bad_pad"));
    }
    if head_dim == 0 {
        return Err(BfzError::Invalid("zero_head_dim"));
    }
    let rms_eps = f32::from_le_bytes([data[base + 16], data[base + 17], data[base + 18], data[base + 19]]);
    Ok(BfzConfig {
        num_experts,
        top_k,
        step_embeddings,
        head_dim,
        use_act_halting: flags & 1 != 0,
        num_masks,
        num_lora,
        rms_eps,
    })
}

// ── Plano ───────────────────────────────────────────────────────────────────

/// Calcula o plano completo do contentor (offsets + tamanhos + total).
///
/// Única fonte de verdade do layout: o loader e o probe usam ESTE plano, logo
/// não podem divergir. Lê o ficheiro apenas para os campos auto-descritivos
/// que o header não traz (`in_features` das máscaras, dimensões do LoRA).
pub fn plan(data: &[u8]) -> Result<BfzPlan, BfzError> {
    let header = parse_header(data)?;
    let config = parse_config(data)?;

    // q_dim = num_heads * head_dim (contrato do writer).
    if header.num_heads.checked_mul(config.head_dim) != Some(header.q_dim) {
        return Err(BfzError::Invalid("q_dim_mismatch"));
    }
    let k_dim = header
        .num_kv_heads
        .checked_mul(config.head_dim)
        .ok_or(BfzError::Invalid("k_dim_overflow"))?;
    if k_dim == 0 {
        return Err(BfzError::Invalid("zero_k_dim"));
    }
    // feat 0x08 ⇔ embed_type == Q6_K (o writer liga/desliga os dois juntos).
    let q6k_feat = header.feat & BFZ_FEAT_UNEMBED_Q6K != 0;
    if q6k_feat != (header.embed_type == BFZ_EMBED_Q6K) {
        return Err(BfzError::Invalid("embed_feat_mismatch"));
    }
    // Experts: o feat bit e o num_experts têm de concordar.
    let has_experts = header.feat & BFZ_FEAT_EXPERTS != 0;
    if has_experts != (config.num_experts > 0) {
        return Err(BfzError::Invalid("expert_count_mismatch"));
    }
    if has_experts && config.top_k == 0 {
        return Err(BfzError::Invalid("zero_top_k"));
    }
    if has_experts && config.top_k > config.num_experts {
        return Err(BfzError::Invalid("top_k_too_big"));
    }
    // Máscaras: o feat bit e o num_masks têm de concordar.
    let has_masks = header.feat & BFZ_FEAT_SUGENO != 0;
    if has_masks != (config.num_masks > 0) {
        return Err(BfzError::Invalid("mask_count_mismatch"));
    }
    // Step-emb: o feat bit exige pelo menos 1 passo.
    if header.feat & BFZ_FEAT_STEP_EMB != 0 && config.step_embeddings == 0 {
        return Err(BfzError::Invalid("zero_step_embeddings"));
    }

    let h = &header;
    let c = &config;
    let mut cur = Cur::new(data, BFZ_BODY_OFFSET);

    let embed = take_sec(
        &mut cur,
        embed_bytes(h.hidden, h.vocab, h.embed_type).ok_or(BfzError::Invalid("embed_size"))?,
    )?;
    let rms_attn = take_sec(
        &mut cur,
        h.hidden.checked_mul(4).ok_or(BfzError::Invalid("rms"))?,
    )?;
    let rms_ffn = take_sec(
        &mut cur,
        h.hidden.checked_mul(4).ok_or(BfzError::Invalid("rms"))?,
    )?;
    let rms_inner = if h.feat & BFZ_FEAT_RMS_INNER != 0 {
        Some(take_sec(
            &mut cur,
            h.hidden.checked_mul(4).ok_or(BfzError::Invalid("rms"))?,
        )?)
    } else {
        None
    };
    let rms_ffn_norm = if h.feat & BFZ_FEAT_RMS_FFN_NORM != 0 {
        Some(take_sec(
            &mut cur,
            h.intermediate.checked_mul(4).ok_or(BfzError::Invalid("rms"))?,
        )?)
    } else {
        None
    };

    let q = take_sec(
        &mut cur,
        tern_bytes(h.q_dim, h.hidden).ok_or(BfzError::Invalid("q"))?,
    )?;
    let k = take_sec(
        &mut cur,
        tern_bytes(k_dim, h.hidden).ok_or(BfzError::Invalid("k"))?,
    )?;
    let v = take_sec(
        &mut cur,
        tern_bytes(k_dim, h.hidden).ok_or(BfzError::Invalid("v"))?,
    )?;
    let o = take_sec(
        &mut cur,
        tern_bytes(h.hidden, h.q_dim).ok_or(BfzError::Invalid("o"))?,
    )?;

    let mut experts: Vec<[BfzSection; 3]> = Vec::new();
    if has_experts {
        for _ in 0..c.num_experts {
            let gate = take_sec(
                &mut cur,
                tern_bytes(h.intermediate, h.hidden).ok_or(BfzError::Invalid("expert"))?,
            )?;
            let up = take_sec(
                &mut cur,
                tern_bytes(h.intermediate, h.hidden).ok_or(BfzError::Invalid("expert"))?,
            )?;
            let down = take_sec(
                &mut cur,
                tern_bytes(h.hidden, h.intermediate).ok_or(BfzError::Invalid("expert"))?,
            )?;
            experts.push([gate, up, down]);
        }
    }

    let routing = if h.feat & BFZ_FEAT_ROUTING != 0 {
        let i16_len = c
            .num_experts
            .checked_mul(h.hidden)
            .and_then(|n| n.checked_mul(2))
            .ok_or(BfzError::Invalid("routing"))?;
        let f32_len = h.hidden.checked_mul(4).ok_or(BfzError::Invalid("routing"))?;
        let centers_q = take_sec(&mut cur, i16_len)?;
        let widths_q = take_sec(&mut cur, i16_len)?;
        let centers_offset = take_sec(&mut cur, f32_len)?;
        let centers_scale = take_sec(&mut cur, f32_len)?;
        let widths_scale = take_sec(&mut cur, f32_len)?;
        let wg = take_sec(
            &mut cur,
            tern_bytes(c.num_experts, h.hidden).ok_or(BfzError::Invalid("routing"))?,
        )?;
        Some(BfzRoutingSections {
            centers_q,
            widths_q,
            centers_offset,
            centers_scale,
            widths_scale,
            wg,
        })
    } else {
        None
    };

    let mut masks: Vec<BfzMaskSection> = Vec::new();
    if has_masks {
        for _ in 0..c.num_masks {
            let start = cur.mark();
            let in_features = cur.u16()? as usize;
            if in_features == 0 {
                return Err(BfzError::Invalid("zero_mask_features"));
            }
            let tlen = tern_trit_len(in_features).ok_or(BfzError::Invalid("mask_len"))?;
            cur.skip(tlen)?;
            masks.push(BfzMaskSection {
                section: cur.section_since(start),
                in_features,
            });
        }
    }

    let step_emb = if h.feat & BFZ_FEAT_STEP_EMB != 0 {
        Some(take_sec(
            &mut cur,
            tern_bytes(c.step_embeddings, h.hidden).ok_or(BfzError::Invalid("step_emb"))?,
        )?)
    } else {
        None
    };

    // ACT-halting: sempre presente no body.
    let act_halting = take_sec(&mut cur, 8)?;
    let rms_final = take_sec(
        &mut cur,
        h.hidden.checked_mul(4).ok_or(BfzError::Invalid("rms"))?,
    )?;

    // Unembed: tied ⇒ zero bytes; Q6_K (feat 0x08) ⇒ blocos Q6_K crus.
    let unembed = if h.tied {
        None
    } else {
        let len = if q6k_feat {
            q6k_bytes(h.hidden.checked_mul(h.vocab).ok_or(BfzError::Invalid("unembed"))?)
                .ok_or(BfzError::Invalid("unembed"))?
        } else {
            embed_bytes(h.hidden, h.vocab, h.embed_type).ok_or(BfzError::Invalid("unembed"))?
        };
        Some(take_sec(&mut cur, len)?)
    };

    let theta = if h.feat & BFZ_FEAT_THETA != 0 {
        Some(take_sec(&mut cur, 4)?)
    } else {
        None
    };

    let mut lora: Vec<BfzLoraSection> = Vec::new();
    for _ in 0..c.num_lora {
        let start = cur.mark();
        let a_rows = cur.u16()? as usize;
        let a_cols = cur.u16()? as usize;
        let b_rows = cur.u16()? as usize;
        let b_cols = cur.u16()? as usize;
        let a_len = a_rows
            .checked_mul(a_cols)
            .and_then(|n| n.checked_mul(4))
            .ok_or(BfzError::Invalid("lora"))?;
        let b_len = b_rows
            .checked_mul(b_cols)
            .and_then(|n| n.checked_mul(4))
            .ok_or(BfzError::Invalid("lora"))?;
        cur.skip(a_len)?;
        cur.skip(b_len)?;
        lora.push(BfzLoraSection {
            section: cur.section_since(start),
            a_rows,
            a_cols,
            b_rows,
            b_cols,
        });
    }

    let sections = BfzSections {
        embed,
        rms_attn,
        rms_ffn,
        rms_inner,
        rms_ffn_norm,
        q,
        k,
        v,
        o,
        experts,
        routing,
        masks,
        step_emb,
        act_halting,
        rms_final,
        unembed,
        theta,
        lora,
    };
    let total = cur.off;
    Ok(BfzPlan {
        header,
        config,
        k_dim,
        sections,
        total,
    })
}

// ── Leitura dos payloads ───────────────────────────────────────────────────

fn slice_of<'a>(data: &'a [u8], sec: BfzSection) -> Option<&'a [u8]> {
    let end = sec.offset.checked_add(sec.len)?;
    data.get(sec.offset..end)
}

fn read_f32_vec(data: &[u8], sec: BfzSection, count: usize) -> Option<Vec<f32>> {
    if sec.len != count.checked_mul(4)? {
        return None;
    }
    let chunk = slice_of(data, sec)?;
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let b = chunk.get(i * 4..i * 4 + 4)?;
        v.push(f32::from_le_bytes(b.try_into().ok()?));
    }
    Some(v)
}

fn read_i16_vec(data: &[u8], sec: BfzSection, count: usize) -> Option<Vec<i16>> {
    if sec.len != count.checked_mul(2)? {
        return None;
    }
    let chunk = slice_of(data, sec)?;
    let mut v = Vec::with_capacity(count);
    for i in 0..count {
        let b = chunk.get(i * 2..i * 2 + 2)?;
        v.push(i16::from_le_bytes(b.try_into().ok()?));
    }
    Some(v)
}

/// Lê uma matriz ternária [rows=OUT, cols=IN] + escalas f32[rows].
fn read_ternary(data: &[u8], sec: BfzSection, rows: usize, cols: usize) -> Option<BfzTernary> {
    let n = rows.checked_mul(cols)?;
    let trits_len = tern_trit_len(n)?;
    let scales_len = rows.checked_mul(4)?;
    if trits_len.checked_add(scales_len)? != sec.len {
        return None;
    }
    let chunk = slice_of(data, sec)?;
    let packed = chunk.get(..trits_len)?;
    let scales_chunk = chunk.get(trits_len..)?;
    let mut row_scales = Vec::with_capacity(rows);
    for i in 0..rows {
        let b = scales_chunk.get(i * 4..i * 4 + 4)?;
        row_scales.push(f32::from_le_bytes(b.try_into().ok()?));
    }
    Some(BfzTernary {
        rows,
        cols,
        trits: PackedTernaryTensor {
            shape: (rows, cols),
            packed_data: packed.to_vec(),
        },
        row_scales,
    })
}

/// Lê uma matriz de embedding/unembed conforme o encoding.
fn read_embed(data: &[u8], sec: BfzSection, rows: usize, cols: usize, enc: u8) -> Option<BfzEmbed> {
    match enc {
        BFZ_EMBED_TERNARY => {
            let t = read_ternary(data, sec, rows, cols)?;
            Some(BfzEmbed::Ternary(t))
        }
        BFZ_EMBED_Q6K => {
            let n = rows.checked_mul(cols)?;
            let q6k_len = q6k_bytes(n)? - 4;
            if sec.len != q6k_len.checked_add(4)? {
                return None;
            }
            let chunk = slice_of(data, sec)?;
            let raw = chunk.get(..q6k_len)?.to_vec();
            let sb = chunk.get(q6k_len..q6k_len + 4)?;
            let scale = f32::from_le_bytes(sb.try_into().ok()?);
            Some(BfzEmbed::Q6K {
                rows,
                cols,
                data: raw,
                scale,
            })
        }
        BFZ_EMBED_F32 => {
            let n = rows.checked_mul(cols)?;
            if sec.len != n.checked_mul(4)?.checked_add(4)? {
                return None;
            }
            let chunk = slice_of(data, sec)?;
            let mut vals = Vec::with_capacity(n);
            for i in 0..n {
                let b = chunk.get(i * 4..i * 4 + 4)?;
                vals.push(f32::from_le_bytes(b.try_into().ok()?));
            }
            let sb = chunk.get(n * 4..n * 4 + 4)?;
            let scale = f32::from_le_bytes(sb.try_into().ok()?);
            Some(BfzEmbed::F32 {
                rows,
                cols,
                data: vals,
                scale,
            })
        }
        _ => None,
    }
}

// ── Loader ─────────────────────────────────────────────────────────────────

/// Carrega um contentor `.bfz` (model_type=3).
///
/// Devolve `None` (nunca panic) se: magic/versão/model_type errados, feat bits
/// desconhecidos, `embed_type` inválido, `act_type > 1`, reserved != 0,
/// secções inconsistentes ou ficheiro truncado.
///
/// **Não corre o modelo** (Etapa A): devolve config + pesos em RAM.
pub fn load_bitfuzzy_v6(data: &[u8]) -> Option<BitFuzzyModel> {
    let plan = match plan(data) {
        Ok(p) => p,
        Err(BfzError::Truncated(need)) => {
            k_nano::slog_cortex!("BFZ", "warn", "bitfuzzy truncado: preciso de {} bytes", need);
            return None;
        }
        Err(BfzError::Invalid(why)) => {
            k_nano::slog_cortex!("BFZ", "warn", "bitfuzzy recusado: {}", why);
            return None;
        }
    };
    if data.len() < plan.total {
        k_nano::slog_cortex!(
            "BFZ",
            "warn",
            "bitfuzzy truncado: preciso de {} bytes, tenho {}",
            plan.total,
            data.len()
        );
        return None;
    }

    let h = plan.header;
    let c = plan.config;
    let s = &plan.sections;

    let embed = read_embed(data, s.embed, h.hidden, h.vocab, h.embed_type)?;
    let rms_attn = read_f32_vec(data, s.rms_attn, h.hidden)?;
    let rms_ffn = read_f32_vec(data, s.rms_ffn, h.hidden)?;
    let rms_inner = match s.rms_inner {
        Some(sec) => Some(read_f32_vec(data, sec, h.hidden)?),
        None => None,
    };
    let rms_ffn_norm = match s.rms_ffn_norm {
        Some(sec) => Some(read_f32_vec(data, sec, h.intermediate)?),
        None => None,
    };

    let q = read_ternary(data, s.q, h.q_dim, h.hidden)?;
    let k = read_ternary(data, s.k, plan.k_dim, h.hidden)?;
    let v = read_ternary(data, s.v, plan.k_dim, h.hidden)?;
    let o = read_ternary(data, s.o, h.hidden, h.q_dim)?;

    let mut experts = Vec::with_capacity(s.experts.len());
    for e in &s.experts {
        experts.push(BfzExpert {
            gate: read_ternary(data, e[0], h.intermediate, h.hidden)?,
            up: read_ternary(data, e[1], h.intermediate, h.hidden)?,
            down: read_ternary(data, e[2], h.hidden, h.intermediate)?,
        });
    }

    let routing = match s.routing {
        Some(r) => Some(BfzRouting {
            centers_q: read_i16_vec(data, r.centers_q, c.num_experts * h.hidden)?,
            widths_q: read_i16_vec(data, r.widths_q, c.num_experts * h.hidden)?,
            centers_offset: read_f32_vec(data, r.centers_offset, h.hidden)?,
            centers_scale: read_f32_vec(data, r.centers_scale, h.hidden)?,
            widths_scale: read_f32_vec(data, r.widths_scale, h.hidden)?,
            wg: read_ternary(data, r.wg, c.num_experts, h.hidden)?,
        }),
        None => None,
    };

    let mut masks = Vec::with_capacity(s.masks.len());
    for m in &s.masks {
        let trits_len = tern_trit_len(m.in_features)?;
        let chunk = slice_of(data, m.section)?;
        let packed = chunk.get(2..2 + trits_len)?;
        masks.push(BfzMask {
            in_features: m.in_features,
            trits: PackedTernaryTensor {
                shape: (1, m.in_features),
                packed_data: packed.to_vec(),
            },
        });
    }

    let step_emb = match s.step_emb {
        Some(sec) => {
            let n = c.step_embeddings.checked_mul(h.hidden)?;
            let trits_len = tern_trit_len(n)?;
            let scales_len = c.step_embeddings.checked_mul(4)?;
            if trits_len.checked_add(scales_len)? != sec.len {
                return None;
            }
            let chunk = slice_of(data, sec)?;
            let packed = chunk.get(..trits_len)?.to_vec();
            let sc = chunk.get(trits_len..)?;
            let mut gain = Vec::with_capacity(c.step_embeddings);
            for i in 0..c.step_embeddings {
                let b = sc.get(i * 4..i * 4 + 4)?;
                gain.push(f32::from_le_bytes(b.try_into().ok()?));
            }
            Some(BfzStepEmb {
                trits: PackedTernaryTensor {
                    shape: (c.step_embeddings, h.hidden),
                    packed_data: packed,
                },
                gain,
            })
        }
        None => None,
    };

    let halting = read_f32_vec(data, s.act_halting, 2)?;
    let rms_final = read_f32_vec(data, s.rms_final, h.hidden)?;

    let unembed = match s.unembed {
        Some(sec) => {
            let enc = if h.feat & BFZ_FEAT_UNEMBED_Q6K != 0 {
                BFZ_EMBED_Q6K
            } else {
                h.embed_type
            };
            Some(read_embed(data, sec, h.hidden, h.vocab, enc)?)
        }
        None => None,
    };

    let theta = match s.theta {
        Some(sec) => read_f32_vec(data, sec, 1)?.first().copied(),
        None => None,
    };

    let mut lora = Vec::with_capacity(s.lora.len());
    for l in &s.lora {
        let chunk = slice_of(data, l.section)?;
        let a_len = l.a_rows.checked_mul(l.a_cols)?;
        let b_len = l.b_rows.checked_mul(l.b_cols)?;
        if chunk.len() != 8 + a_len.checked_mul(4)? + b_len.checked_mul(4)? {
            return None;
        }
        let mut a = Vec::with_capacity(a_len);
        for i in 0..a_len {
            let b = chunk.get(8 + i * 4..8 + i * 4 + 4)?;
            a.push(f32::from_le_bytes(b.try_into().ok()?));
        }
        let b_off = 8 + a_len.checked_mul(4)?;
        let mut bvals = Vec::with_capacity(b_len);
        for i in 0..b_len {
            let b = chunk.get(b_off + i * 4..b_off + i * 4 + 4)?;
            bvals.push(f32::from_le_bytes(b.try_into().ok()?));
        }
        lora.push(BfzLora {
            a_rows: l.a_rows,
            a_cols: l.a_cols,
            a,
            b_rows: l.b_rows,
            b_cols: l.b_cols,
            b: bvals,
        });
    }

    Some(BfzModel {
        plan,
        embed,
        rms_attn,
        rms_ffn,
        rms_inner,
        rms_ffn_norm,
        q,
        k,
        v,
        o,
        experts,
        routing,
        masks,
        step_emb,
        halt_center: halting.first().copied().unwrap_or(0.0),
        halt_width: halting.get(1).copied().unwrap_or(0.0),
        rms_final,
        unembed,
        theta,
        lora,
    })
}

/// Tipo do modelo carregado — alias curto para `BitFuzzyModel`.
pub type BfzModel = BitFuzzyModel;

/// Probe do tamanho total de um contentor bfz (model_type=3).
///
/// * `Ready(total)` — tamanho exacto do contentor (header + config + body);
/// * `Need(n)` — o buffer precisa de `n` bytes para continuar (as máscaras
///   Sugeno e os headers de LoRA são auto-descritivos ⇒ o probe tem de os
///   ler, ao contrário do LLM v6 onde tudo vem do header);
/// * `NotLlm` — não é um contentor bfz válido (magic/versão/feat/…).
pub fn probe_bfz_size(buf: &[u8]) -> crate::model::LlmSizeProbe {
    match plan(buf) {
        Ok(p) => crate::model::LlmSizeProbe::Ready(p.total),
        // O mínimo útil é header+config (72) — o parse do body só começa
        // depois disso.
        Err(BfzError::Truncated(need)) => {
            crate::model::LlmSizeProbe::Need(need.max(BFZ_BODY_OFFSET))
        }
        Err(BfzError::Invalid(_)) => crate::model::LlmSizeProbe::NotLlm,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// FORWARD PASS (Etapa B)
// ─────────────────────────────────────────────────────────────────────────
//
// FONTE DA VERDADE: `tools/bitfuzzy_moe_recurrent.py` (torch). Cada função
// abaixo cita as linhas da referência que espelha — se a referência mudar,
// as citações são o aviso.

/// Threshold de ACT-halting por omissão.
///
/// ⚠ O contentor `.bfz` NÃO guarda o threshold (guarda `halt_center` e
/// `halt_width`): o valor por omissão é o `DEFAULT_ACT_THRESHOLD` da
/// referência (`tools/bitfuzzy_moe_recurrent.py` L74 = 0.9).
pub const BFZ_DEFAULT_ACT_THRESHOLD: f32 = 0.9;

/// Saída do forward do contentor BitFuzzy.
pub struct BfzForward {
    /// Logits [seq, vocab] row-major.
    pub logits: Vec<f32>,
    /// Nº de passos recorrentes executados (1..=K).
    pub steps_used: usize,
    /// Confiança de halting por passo (escala livre).
    pub halt_conf: Vec<f32>,
    /// Pertinência (membership) de halting por passo — a comparação com o
    /// threshold é DURA: é ela que decide se a recorrência parou.
    pub halt_mu: Vec<f32>,
}

/// Vista de leitura de uma matriz de embedding/unembed.
enum BfzEmbedView<'a> {
    /// f32 exacto [rows, cols] + o f32 de escala final do record.
    F32 {
        cols: usize,
        data: &'a [f32],
        scale: f32,
    },
    /// Ternário [rows, cols] (trits + escala POR LINHA de saída).
    Tern(&'a BfzTernary),
}

impl BfzEmbedView<'_> {
    /// Valor da matriz em [row][col].
    fn get(&self, row: usize, col: usize) -> f32 {
        match self {
            BfzEmbedView::F32 { cols, data, scale } => data[row * cols + col] * *scale,
            BfzEmbedView::Tern(t) => {
                t.trits.get_weight(row * t.cols + col) as f32 * t.row_scales[row]
            }
        }
    }
}

/// Vista de leitura do embedding/unembed; `None` para Q6_K (sem dequantizador
/// no cortex — recusa explícita em vez de inventar números).
fn bfz_embed_view(embed: &BfzEmbed) -> Option<BfzEmbedView<'_>> {
    match embed {
        BfzEmbed::F32 {
            rows,
            cols,
            data,
            scale,
        } => {
            if data.len() != rows * cols {
                return None;
            }
            Some(BfzEmbedView::F32 {
                cols: *cols,
                data,
                scale: *scale,
            })
        }
        BfzEmbed::Ternary(t) => Some(BfzEmbedView::Tern(t)),
        BfzEmbed::Q6K { .. } => {
            k_nano::slog_cortex!(
                "BFZ",
                "warn",
                "forward recusa embed/unembed Q6_K — sem dequantizador no cortex"
            );
            None
        }
    }
}

/// RMSNorm pré-norm — espelha `RMSNorm.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L279-283):
/// `x * rsqrt(mean(x^2) + eps) * weight` (sem subtração de média, sem bias).
fn bfz_rmsnorm(x: &[f32], w: &[f32], eps: f32, out: &mut [f32]) {
    let n = x.len();
    if n == 0 || w.len() < n || out.len() < n {
        return;
    }
    let mut sq = 0.0f32;
    for v in x.iter() {
        sq += v * v;
    }
    let inv = 1.0 / libm::sqrtf(sq / n as f32 + eps);
    for i in 0..n {
        out[i] = x[i] * inv * w[i];
    }
}

/// Linear ternário — espelha `BitLinear.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L411-418):
/// `y = (x @ W.T) * row_scale` (escala POR LINHA de saída, não escalar).
fn bfz_tern_linear(x: &[f32], t: &BfzTernary, out: &mut [f32]) {
    let cols = t.cols;
    if x.len() < cols || out.len() < t.rows {
        return;
    }
    for o in 0..t.rows {
        let base = o * cols;
        let mut acc = 0.0f32;
        for i in 0..cols {
            // trit ∈ {-1,0,+1}: `x * ±1` é exacto em f32, `x * 0` não muda a
            // soma ⇒ o branching é bit-idêntico ao produto e mais barato.
            match t.trits.get_weight(base + i) {
                1 => acc += x[i],
                -1 => acc -= x[i],
                _ => {}
            }
        }
        out[o] = acc * t.row_scales[o];
    }
}

/// Softmax numericamente estável (max-subtract) — o mesmo que o `softmax` do
/// torch: subtrai o máximo, exp, divide pela soma.
fn bfz_softmax(row: &mut [f32]) {
    let mut max = f32::NEG_INFINITY;
    for v in row.iter() {
        if *v > max {
            max = *v;
        }
    }
    let mut sum = 0.0f32;
    for v in row.iter_mut() {
        let e = libm::expf(*v - max);
        *v = e;
        sum += e;
    }
    let inv = 1.0 / sum;
    for v in row.iter_mut() {
        *v *= inv;
    }
}

/// Sigmoide local: `1 / (1 + exp(-x))`.
fn bfz_sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-x))
}

/// RoPE INTERLEAVED — espelha `RotaryEmbedding.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L329-349, ramo "interleaved"):
/// os pares (2d, 2d+1) são rodados com (cos[d], sin[d]) e
/// `inv_freq = theta ** (-2d/head_dim)`. É a convenção do kernel
/// (`cortex.rs::rope_apply_heads`), NÃO a variante half-rotated.
fn bfz_rope(
    data: &mut [f32],
    seq: usize,
    heads: usize,
    head_dim: usize,
    stride: usize,
    theta: f32,
    max_seq: usize,
) {
    let half = head_dim / 2;
    if half == 0 || head_dim % 2 != 0 || max_seq == 0 {
        return;
    }
    let mut cos = crate::tensor::f32_zeros_2d(seq, half);
    let mut sin = crate::tensor::f32_zeros_2d(seq, half);
    if cos.len() != seq * half || sin.len() != seq * half {
        return;
    }
    for d in 0..half {
        let e = (-2.0f32 * d as f32) / head_dim as f32;
        let inv = libm::powf(theta, e);
        for p in 0..seq {
            // Alem da janela pre-computada a referencia REPETE a ultima linha
            // (RotaryEmbedding.forward L338-342) — nao recalcula o angulo.
            let pe = if p < max_seq { p } else { max_seq - 1 };
            let ang = pe as f32 * inv;
            cos[p * half + d] = libm::cosf(ang);
            sin[p * half + d] = libm::sinf(ang);
        }
    }
    for p in 0..seq {
        for hh in 0..heads {
            let base = p * stride + hh * head_dim;
            for d in 0..half {
                let x1 = data[base + 2 * d];
                let x2 = data[base + 2 * d + 1];
                let c = cos[p * half + d];
                let s = sin[p * half + d];
                data[base + 2 * d] = x1 * c - x2 * s;
                data[base + 2 * d + 1] = x1 * s + x2 * c;
            }
        }
    }
}

/// Atenção causal com GQA — espelha `FalconAttention.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L761-779): q/k/v projectados, RoPE,
/// kv repetido `repeat_interleave` (GQA), scores `q·k / sqrt(head_dim)`,
/// máscara causal (j <= i), softmax, `p·v` e finally `o_proj`.
fn bfz_attention(
    model: &BitFuzzyModel,
    a: &[f32],
    seq: usize,
    q: &mut [f32],
    k: &mut [f32],
    v: &mut [f32],
    ctx: &mut [f32],
    attn: &mut [f32],
) {
    let h = &model.plan.header;
    let c = &model.plan.config;
    let hidden = h.hidden;
    let heads = h.num_heads;
    let kvheads = h.num_kv_heads;
    let hd = c.head_dim;
    let q_dim = h.q_dim;
    let k_dim = model.k_dim();
    let theta = match model.theta {
        Some(t) => t,
        None => return,
    };

    // q/k/v projectados (por token).
    for i in 0..seq {
        let row = &a[i * hidden..(i + 1) * hidden];
        bfz_tern_linear(row, &model.q, &mut q[i * q_dim..(i + 1) * q_dim]);
        bfz_tern_linear(row, &model.k, &mut k[i * k_dim..(i + 1) * k_dim]);
        bfz_tern_linear(row, &model.v, &mut v[i * k_dim..(i + 1) * k_dim]);
    }
    // RoPE em q e k (interleaved).
    bfz_rope(q, seq, heads, hd, q_dim, theta, model.plan.header.max_seq);
    bfz_rope(k, seq, kvheads, hd, k_dim, theta, model.plan.header.max_seq);

    // GQA: o kv head g serve os query heads [g*group, (g+1)*group).
    let group = (heads / kvheads).max(1);
    let scale = 1.0 / libm::sqrtf(hd as f32);
    let mut scores = crate::tensor::f32_zeros(seq * seq);
    if scores.len() != seq * seq {
        return;
    }
    for i in 0..seq {
        for hh in 0..heads {
            let kvh = hh / group;
            for j in 0..=i {
                let mut acc = 0.0f32;
                for d in 0..hd {
                    acc += q[i * q_dim + hh * hd + d] * k[j * k_dim + kvh * hd + d];
                }
                scores[i * seq + j] = acc * scale;
            }
            // softmax sobre j <= i (no torch o resto é -inf ⇒ 0 aqui).
            bfz_softmax(&mut scores[i * seq..i * seq + i + 1]);
            for d in 0..hd {
                let mut acc = 0.0f32;
                for j in 0..=i {
                    acc += scores[i * seq + j] * v[j * k_dim + kvh * hd + d];
                }
                ctx[i * q_dim + hh * hd + d] = acc;
            }
        }
    }
    // o_proj sobre o contexto [seq, q_dim].
    for i in 0..seq {
        bfz_tern_linear(
            &ctx[i * q_dim..(i + 1) * q_dim],
            &model.o,
            &mut attn[i * hidden..(i + 1) * hidden],
        );
    }
}

/// Expert MoE — espelha `_TernaryExpert.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L509-510):
/// `down(silu(gate(x)) * up(x))`. A máscara Sugeno aplica-se SÓ À ENTRADA DO
/// GATE (`SugenoBitLinear.forward`, L482-489) — o `up` NÃO é mascarado.
fn bfz_expert(
    model: &BitFuzzyModel,
    e: usize,
    row: &[f32],
    gate_in: &mut [f32],
    gbuf: &mut [f32],
    ubuf: &mut [f32],
    tmp: &mut [f32],
    out: &mut [f32],
) {
    let hidden = model.plan.header.hidden;
    let expert = &model.experts[e];
    for cc in 0..hidden {
        gate_in[cc] = row[cc];
    }
    if let Some(mask) = model.masks.get(e) {
        for cc in 0..hidden {
            gate_in[cc] *= mask.trits.get_weight(cc) as f32;
        }
    }
    bfz_tern_linear(gate_in, &expert.gate, gbuf);
    bfz_tern_linear(row, &expert.up, ubuf);
    for cc in 0..gbuf.len() {
        tmp[cc] = crate::nn::silu(gbuf[cc]) * ubuf[cc];
    }
    bfz_tern_linear(tmp, &expert.down, out);
}

/// MoE fuzzy — espelha `BitFuzzyMoE.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L584-647): membership gaussiana →
/// T-norm produto NORMALIZADA → modulação pela routing ternária → top-k →
/// pesos softmax → mistura ponderada dos experts.
fn bfz_moe(
    model: &BitFuzzyModel,
    m: &[f32],
    seq: usize,
    out: &mut [f32],
    gate_in: &mut [f32],
    gbuf: &mut [f32],
    ubuf: &mut [f32],
    tmp: &mut [f32],
    ebuf: &mut [f32],
) {
    let h = &model.plan.header;
    let c = &model.plan.config;
    let hidden = h.hidden;
    let experts = c.num_experts;
    let top_k = c.top_k;
    let routing = match model.routing.as_ref() {
        Some(r) => r,
        None => return,
    };

    let mut scores = crate::tensor::f32_zeros(experts);
    let mut sel = crate::tensor::f32_zeros(top_k);
    let mut idx: Vec<usize> = (0..experts).collect();
    if scores.len() != experts || sel.len() != top_k {
        return;
    }

    for i in 0..seq {
        let row = &m[i * hidden..(i + 1) * hidden];
        for e in 0..experts {
            // membership: mu = exp(-(x - center)^2 / (2 w^2)), clamp [0,1]
            // (centers/widths dequantizados das tabelas fixed-point).
            let mut logsum = 0.0f32;
            for cc in 0..hidden {
                let center = routing.centers_q[e * hidden + cc] as f32 * routing.centers_scale[cc]
                    + routing.centers_offset[cc];
                let w = (routing.widths_q[e * hidden + cc] as f32 * routing.widths_scale[cc])
                    .abs()
                    .max(1e-3);
                let d = row[cc] - center;
                let mu = libm::expf(-(d * d) / (2.0 * w * w)).clamp(0.0, 1.0);
                logsum += libm::logf(mu.max(1e-6));
            }
            // T-norm produto NORMALIZADA (geomean): exp(sum(log mu) / hidden)
            let base = libm::expf(logsum / hidden as f32);
            // modulação: |projeção ternária W_g · x| + 1e-3
            let mut proj = 0.0f32;
            for cc in 0..hidden {
                match routing.wg.trits.get_weight(e * hidden + cc) {
                    1 => proj += row[cc],
                    -1 => proj -= row[cc],
                    _ => {}
                }
            }
            let mval = proj.abs() + 1e-3;
            scores[e] = base * mval;
        }
        // top-k (descendente; empate ⇒ índice menor primeiro).
        idx.sort_by(|&x, &y| {
            scores[y]
                .partial_cmp(&scores[x])
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(x.cmp(&y))
        });
        for kk in 0..top_k {
            sel[kk] = scores[idx[kk]];
        }
        // softmax sobre os scores SELECCIONADOS (score_temperature = 1.0).
        bfz_softmax(&mut sel[..top_k]);
        // mistura ponderada dos experts seleccionados.
        for kk in 0..top_k {
            let e = idx[kk];
            let w = sel[kk];
            bfz_expert(model, e, row, gate_in, gbuf, ubuf, tmp, ebuf);
            for cc in 0..hidden {
                out[i * hidden + cc] += w * ebuf[cc];
            }
        }
    }
}

/// Confiança + pertenência de halting — espelha
/// `BitFuzzyRecurrent.halt_confidence` (L1007-1026) e
/// `halt_membership` (L1002-1005).
///
/// ⚠ O `mean(dim=(-1,-2))` do torch reduz sobre (T, hidden): a RMS é do
/// estado INTEIRO da amostra, não por token.
fn bfz_halt(
    h: &[f32],
    h_new: &[f32],
    seq: usize,
    hidden: usize,
    center: f32,
    width: f32,
) -> (f32, f32) {
    let n = seq * hidden;
    let mut prev_sq = 0.0f32;
    let mut step_sq = 0.0f32;
    for i in 0..n {
        prev_sq += h[i] * h[i];
        let d = h_new[i] - h[i];
        step_sq += d * d;
    }
    let prev_rms = libm::sqrtf(prev_sq / n as f32);
    let step_rms = libm::sqrtf(step_sq / n as f32);
    let conf = 1.0 / (1.0 + step_rms / (prev_rms + 1e-6));
    let w = width.abs().max(1e-3);
    let mu = bfz_sigmoid((conf - center) / w);
    (conf, mu)
}

/// FORWARD do contentor BitFuzzy (`model_type=3`).
///
/// Espelha `BitFuzzyRecurrent.forward`
/// (`tools/bitfuzzy_moe_recurrent.py` L1035-1081):
///
/// ```text
///   h = embed(ids)
///   para t em 0..K:
///       h_in  = h + gain[t] * P_step(t)        (L1042 + L980-987)
///       h_new = block(h_in)                    (L1043, bloco L851-854)
///       mu    = sigmoid((conf - center)/width) (L1049, L1002-1005)
///       h     = h_new                           (L1066-1070: SEMPRE aplicado)
///       se mu >= act_threshold: break          (L1050, L1077-1078)
///   logits = lm_head(norm(h))                  (L1080)
/// ```
///
/// Devolve `None` (nunca números inventados) quando o contentor usa um
/// caminho NÃO implementado: LoRA (`num_lora > 0`), embed/unembed Q6_K,
/// routing ausente, máscaras Sugeno fora do layout canónico ou K = 0.
pub fn forward_bfz(model: &BitFuzzyModel, ids: &[u32], act_threshold: f32) -> Option<BfzForward> {
    let h = &model.plan.header;
    let c = &model.plan.config;
    let hidden = h.hidden;
    let vocab = h.vocab;
    let seq = ids.len();
    if seq == 0 || hidden == 0 || vocab == 0 || h.k_steps == 0 {
        return None;
    }
    // Fail-closed: caminhos não implementados são RECUSADOS (silêncio = número
    // falso — ADR-0113).
    if c.num_lora != 0 {
        k_nano::slog_cortex!("BFZ", "warn", "forward recusa LoRA (num_lora={})", c.num_lora);
        return None;
    }
    if c.num_experts == 0 || c.top_k == 0 || c.top_k > c.num_experts {
        return None;
    }
    // O bloco de referencia (Falcon3Block) NAO tem inner-attn norm nem ffn
    // norm, e a activacao e silu. Um contentor com esses campos usa
    // semantica que a referencia nao define ⇒ recusa (nada inventado).
    if model.rms_inner.is_some() || model.rms_ffn_norm.is_some() {
        k_nano::slog_cortex!(
            "BFZ",
            "warn",
            "forward recusa contentor com rms_inner/rms_ffn_norm (fora do modelo de referencia)"
        );
        return None;
    }
    if h.act_type != 0 {
        k_nano::slog_cortex!("BFZ", "warn", "forward recusa act_type={} (so silu)", h.act_type);
        return None;
    }
    if h.max_seq == 0 {
        return None;
    }
    if model.experts.len() != c.num_experts || model.routing.is_none() {
        return None;
    }
    // O container NÃO regista a que módulo pertence cada máscara Sugeno (o
    // writer emite-as na ordem de percorrencia expert→gate). Só aceitamos o
    // layout canónico: uma máscara por expert, com in_features == hidden.
    if model.masks.len() != c.num_experts {
        k_nano::slog_cortex!(
            "BFZ",
            "warn",
            "forward recusa {} máscaras Sugeno (esperado {} = 1 por expert)",
            model.masks.len(),
            c.num_experts
        );
        return None;
    }
    for mk in &model.masks {
        if mk.in_features != hidden {
            return None;
        }
    }
    let step_emb = match model.step_emb.as_ref() {
        Some(s) => s,
        None => return None,
    };
    if step_emb.gain.is_empty() {
        return None;
    }
    let embed_view = bfz_embed_view(&model.embed)?;
    // tied ⇒ o unembed É o próprio embedding (zero bytes no ficheiro).
    let unembed_view = match model.unembed.as_ref() {
        Some(u) => bfz_embed_view(u)?,
        None => bfz_embed_view(&model.embed)?,
    };
    if model.theta.is_none() {
        // Sem theta não há RoPE bem definido ⇒ recusa (não assume 1000042).
        k_nano::slog_cortex!("BFZ", "warn", "forward recusa contentor sem theta (RoPE)");
        return None;
    }

    let eps = c.rms_eps;
    let q_dim = h.q_dim;
    let k_dim = model.k_dim();

    // Buffers (via f32_zeros — NUNCA `vec![0.0; n]`: o headroom/overflow vive aí).
    let mut hbuf = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut h_in = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut a = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut x = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut m = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut h_new = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut moe_out = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut q = crate::tensor::f32_zeros_2d(seq, q_dim);
    let mut k = crate::tensor::f32_zeros_2d(seq, k_dim);
    let mut v = crate::tensor::f32_zeros_2d(seq, k_dim);
    let mut ctx = crate::tensor::f32_zeros_2d(seq, q_dim);
    let mut attn = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut final_norm = crate::tensor::f32_zeros_2d(seq, hidden);
    let mut gate_in = crate::tensor::f32_zeros(hidden);
    let mut gbuf = crate::tensor::f32_zeros(h.intermediate);
    let mut ubuf = crate::tensor::f32_zeros(h.intermediate);
    let mut tmp = crate::tensor::f32_zeros(h.intermediate);
    let mut ebuf = crate::tensor::f32_zeros(hidden);
    let mut logits = crate::tensor::f32_zeros_2d(seq, vocab);
    // Alocação recusada (headroom) ⇒ sem números.
    if hbuf.len() != seq * hidden
        || h_in.len() != seq * hidden
        || a.len() != seq * hidden
        || x.len() != seq * hidden
        || m.len() != seq * hidden
        || h_new.len() != seq * hidden
        || moe_out.len() != seq * hidden
        || q.len() != seq * q_dim
        || k.len() != seq * k_dim
        || v.len() != seq * k_dim
        || ctx.len() != seq * q_dim
        || attn.len() != seq * hidden
        || final_norm.len() != seq * hidden
        || gate_in.len() != hidden
        || gbuf.len() != h.intermediate
        || ubuf.len() != h.intermediate
        || tmp.len() != h.intermediate
        || ebuf.len() != hidden
        || logits.len() != seq * vocab
    {
        k_nano::slog_cortex!("BFZ", "fail", "forward sem memória para os buffers");
        return None;
    }

    // h = embedding: h[t][c] = embed[c][id[t]] (embed é [hidden, vocab]).
    for i in 0..seq {
        let id = ids[i] as usize;
        if id >= vocab {
            return None;
        }
        for cc in 0..hidden {
            hbuf[i * hidden + cc] = embed_view.get(cc, id);
        }
    }

    let mut halt_conf: Vec<f32> = Vec::new();
    let mut halt_mu: Vec<f32> = Vec::new();
    let mut steps_used = 0usize;

    for t in 0..h.k_steps {
        // (a) step vector: trit da pool × ganho do passo.
        let sidx = t % step_emb.gain.len();
        let gain = step_emb.gain[sidx];
        for i in 0..seq {
            for cc in 0..hidden {
                let trit = step_emb.trits.get_weight(sidx * hidden + cc) as f32;
                h_in[i * hidden + cc] = hbuf[i * hidden + cc] + trit * gain;
            }
        }
        // (c) bloco partilhado: atenção + MoE, com residuais.
        for i in 0..seq {
            bfz_rmsnorm(
                &h_in[i * hidden..(i + 1) * hidden],
                &model.rms_attn,
                eps,
                &mut a[i * hidden..(i + 1) * hidden],
            );
        }
        bfz_attention(model, &a, seq, &mut q, &mut k, &mut v, &mut ctx, &mut attn);
        for i in 0..seq * hidden {
            x[i] = h_in[i] + attn[i];
        }
        for i in 0..seq {
            bfz_rmsnorm(
                &x[i * hidden..(i + 1) * hidden],
                &model.rms_ffn,
                eps,
                &mut m[i * hidden..(i + 1) * hidden],
            );
        }
        for i in 0..seq * hidden {
            moe_out[i] = 0.0;
        }
        bfz_moe(
            model,
            &m,
            seq,
            &mut moe_out,
            &mut gate_in,
            &mut gbuf,
            &mut ubuf,
            &mut tmp,
            &mut ebuf,
        );
        for i in 0..seq * hidden {
            h_new[i] = x[i] + moe_out[i];
        }
        // (g) ACT halting.
        let (conf, mu) = bfz_halt(
            &hbuf,
            &h_new,
            seq,
            hidden,
            model.halt_center,
            model.halt_width,
        );
        halt_conf.push(conf);
        halt_mu.push(mu);
        // A saída do passo é SEMPRE aplicada — mesmo no passo que parou a
        // recorrência (L1052-1070: o halting só decide se corre OUTRO passo).
        hbuf.copy_from_slice(&h_new);
        steps_used = t + 1;
        if mu >= act_threshold {
            break;
        }
    }

    // (3) final: rmsnorm + unembed.
    for i in 0..seq {
        bfz_rmsnorm(
            &hbuf[i * hidden..(i + 1) * hidden],
            &model.rms_final,
            eps,
            &mut final_norm[i * hidden..(i + 1) * hidden],
        );
    }
    for i in 0..seq {
        for v in 0..vocab {
            let mut acc = 0.0f32;
            for cc in 0..hidden {
                acc += final_norm[i * hidden + cc] * unembed_view.get(cc, v);
            }
            logits[i * vocab + v] = acc;
        }
    }

    Some(BfzForward {
        logits,
        steps_used,
        halt_conf,
        halt_mu,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O contentor golden produzido por `tools/export_no_std.py --golden`.
    const GOLDEN: &[u8] = include_bytes!("../../../tools/golden_bfz.bin");

    #[test]
    fn bfz_golden_parses() {
        let m = load_bitfuzzy_v6(GOLDEN).expect("golden bfz carrega");
        let h = &m.plan.header;
        let c = &m.plan.config;

        // ── header ──
        assert_eq!(h.version, 6);
        assert_eq!(h.model_type, 3);
        assert_eq!(h.hidden, 32);
        assert_eq!(h.k_steps, 3);
        assert_eq!(h.num_heads, 2);
        assert_eq!(h.vocab, 64);
        assert_eq!(h.max_seq, 4096);
        assert_eq!(h.intermediate, 64);
        assert_eq!(h.num_kv_heads, 1);
        assert_eq!(h.q_dim, 32);
        assert_eq!(h.num_medusa, 0);
        assert!(!h.tied);
        assert_eq!(h.tok_type, 0);
        assert_eq!(h.tok_len, 0);
        assert_eq!(h.act_type, 0);
        assert_eq!(h.embed_type, 3);
        assert_eq!(h.feat, 0xF4);
        assert_eq!(h.num_params, 19909);

        // ── config ──
        assert_eq!(c.num_experts, 2);
        assert_eq!(c.top_k, 1);
        assert_eq!(c.step_embeddings, 3);
        assert_eq!(c.head_dim, 16);
        assert!(c.use_act_halting);
        assert_eq!(c.num_masks, 2);
        assert_eq!(c.num_lora, 0);
        assert!((c.rms_eps - 1e-5).abs() < 1e-9, "rms_eps={}", c.rms_eps);

        // ── derivados ──
        assert_eq!(m.plan.k_dim, 16);
        assert_eq!(m.k_dim(), 16);

        // ── secções (offsets absolutos, do writer) ──
        let s = &m.plan.sections;
        assert_eq!((s.embed.offset, s.embed.len), (72, 8196));
        assert_eq!((s.rms_attn.offset, s.rms_attn.len), (8268, 128));
        assert_eq!((s.rms_ffn.offset, s.rms_ffn.len), (8396, 128));
        assert!(s.rms_inner.is_none());
        assert!(s.rms_ffn_norm.is_none());
        assert_eq!((s.q.offset, s.q.len), (8524, 384));
        assert_eq!((s.k.offset, s.k.len), (8908, 192));
        assert_eq!((s.v.offset, s.v.len), (9100, 192));
        assert_eq!((s.o.offset, s.o.len), (9292, 384));
        assert_eq!(s.experts.len(), 2);
        assert_eq!((s.experts[0][0].offset, s.experts[0][0].len), (9676, 768));
        assert_eq!((s.experts[0][1].offset, s.experts[0][1].len), (10444, 768));
        assert_eq!((s.experts[0][2].offset, s.experts[0][2].len), (11212, 640));
        assert_eq!((s.experts[1][0].offset, s.experts[1][0].len), (11852, 768));
        assert_eq!((s.experts[1][1].offset, s.experts[1][1].len), (12620, 768));
        assert_eq!((s.experts[1][2].offset, s.experts[1][2].len), (13388, 640));
        let r = s.routing.expect("routing presente");
        assert_eq!((r.centers_q.offset, r.centers_q.len), (14028, 128));
        assert_eq!((r.widths_q.offset, r.widths_q.len), (14156, 128));
        assert_eq!((r.centers_offset.offset, r.centers_offset.len), (14284, 128));
        assert_eq!((r.centers_scale.offset, r.centers_scale.len), (14412, 128));
        assert_eq!((r.widths_scale.offset, r.widths_scale.len), (14540, 128));
        assert_eq!((r.wg.offset, r.wg.len), (14668, 24));
        assert_eq!(s.masks.len(), 2);
        assert_eq!((s.masks[0].section.offset, s.masks[0].section.len), (14692, 10));
        assert_eq!(s.masks[0].in_features, 32);
        assert_eq!((s.masks[1].section.offset, s.masks[1].section.len), (14702, 10));
        assert_eq!(s.masks[1].in_features, 32);
        let se = s.step_emb.expect("step_emb presente");
        assert_eq!((se.offset, se.len), (14712, 36));
        assert_eq!((s.act_halting.offset, s.act_halting.len), (14748, 8));
        assert_eq!((s.rms_final.offset, s.rms_final.len), (14756, 128));
        let un = s.unembed.expect("unembed presente");
        assert_eq!((un.offset, un.len), (14884, 8196));
        let th = s.theta.expect("theta presente");
        assert_eq!((th.offset, th.len), (23080, 4));
        assert!(s.lora.is_empty());

        // ── total ──
        assert_eq!(m.plan.total, 23084);
        assert_eq!(m.plan.total, GOLDEN.len());

        // ── payloads ──
        match &m.embed {
            BfzEmbed::F32 {
                rows,
                cols,
                data,
                scale,
            } => {
                assert_eq!((*rows, *cols), (32, 64));
                assert_eq!(data.len(), 32 * 64);
                assert_eq!(*scale, 1.0);
            }
            _ => panic!("embed inesperado (esperado f32 exacto)"),
        }
        assert_eq!(m.rms_attn.len(), 32);
        assert_eq!(m.rms_ffn.len(), 32);
        assert_eq!(m.rms_final.len(), 32);
        assert_eq!((m.q.rows, m.q.cols), (32, 32));
        assert_eq!(m.q.row_scales.len(), 32);
        assert_eq!((m.k.rows, m.k.cols), (16, 32));
        assert_eq!(m.k.row_scales.len(), 16);
        assert_eq!((m.v.rows, m.v.cols), (16, 32));
        assert_eq!((m.o.rows, m.o.cols), (32, 32));
        assert_eq!(m.o.row_scales.len(), 32);
        assert_eq!(m.experts.len(), 2);
        assert_eq!((m.experts[0].gate.rows, m.experts[0].gate.cols), (64, 32));
        assert_eq!((m.experts[0].down.rows, m.experts[0].down.cols), (32, 64));
        let ro = m.routing.as_ref().expect("routing");
        assert_eq!(ro.centers_q.len(), 2 * 32);
        assert_eq!(ro.widths_q.len(), 2 * 32);
        assert_eq!(ro.centers_offset.len(), 32);
        assert_eq!((ro.wg.rows, ro.wg.cols), (2, 32));
        assert_eq!(m.masks.len(), 2);
        assert_eq!(m.masks[0].in_features, 32);
        let ste = m.step_emb.as_ref().expect("step_emb");
        assert_eq!(ste.gain.len(), 3);
        assert!((m.halt_center - 0.8).abs() < 1e-6);
        assert!((m.halt_width - 0.05).abs() < 1e-6);
        assert!(m.unembed.is_some());
        assert!((m.theta.expect("theta") - 1000042.0).abs() < 1.0);
        assert!(m.lora.is_empty());

        // ── o probe vê o mesmo total ──
        assert_eq!(
            crate::model::probe_llm_file_size(GOLDEN),
            crate::model::LlmSizeProbe::Ready(23084)
        );
    }

    #[test]
    fn bfz_negative_rejects() {
        // 1 byte a menos ⇒ None (nunca panic).
        let mut short = GOLDEN.to_vec();
        short.truncate(GOLDEN.len() - 1);
        assert!(load_bitfuzzy_v6(&short).is_none());

        // reserved[3] != 0 ⇒ rejeitado.
        let mut bad_reserved = GOLDEN.to_vec();
        bad_reserved[15] = 1;
        assert!(load_bitfuzzy_v6(&bad_reserved).is_none());

        // embed_type=2 (BF16 declarado, não implementado) ⇒ rejeitado.
        let mut bad_embed = GOLDEN.to_vec();
        bad_embed[50] = 2;
        assert!(load_bitfuzzy_v6(&bad_embed).is_none());

        // act_type > 1 ⇒ rejeitado.
        let mut bad_act = GOLDEN.to_vec();
        bad_act[49] = 2;
        assert!(load_bitfuzzy_v6(&bad_act).is_none());

        // magic / model_type errados ⇒ rejeitados.
        let mut bad_magic = GOLDEN.to_vec();
        bad_magic[0] = 0x00;
        assert!(load_bitfuzzy_v6(&bad_magic).is_none());
        let mut bad_type = GOLDEN.to_vec();
        bad_type[14] = 2;
        assert!(load_bitfuzzy_v6(&bad_type).is_none());

        // feat inconsistente com a config ⇒ rejeitado.
        // (a) feat experts (0x10) desligado mas num_experts=2.
        let mut no_experts = GOLDEN.to_vec();
        no_experts[51] = 0xF4 & !0x10;
        assert!(load_bitfuzzy_v6(&no_experts).is_none());
        // (b) feat sugeno (0x40) desligado mas num_masks=2.
        let mut no_masks = GOLDEN.to_vec();
        no_masks[51] = 0xF4 & !0x40;
        assert!(load_bitfuzzy_v6(&no_masks).is_none());
        // (c) feat 0x08 (Q6_K) ligado com embed_type=3 ⇒ inconsistente.
        let mut bad_feat = GOLDEN.to_vec();
        bad_feat[51] = 0xF4 | 0x08;
        assert!(load_bitfuzzy_v6(&bad_feat).is_none());

        // Cabeçalho truncado ⇒ None (sem panic).
        assert!(load_bitfuzzy_v6(&GOLDEN[..40]).is_none());
        assert!(load_bitfuzzy_v6(&[]).is_none());

        // Bytes extras no fim ⇒ o loader aceita (>= total), o validador não.
        let mut longer = GOLDEN.to_vec();
        longer.push(0);
        assert!(load_bitfuzzy_v6(&longer).is_some());
        assert_eq!(
            crate::model::validate_bitnet_blob(&longer),
            Err("size_mismatch")
        );
        // O golden intacto valida.
        assert_eq!(crate::model::validate_bitnet_blob(GOLDEN), Ok(()));
    }

    /// O contentor golden produzido por `tools/export_no_std.py --golden`.
    const GOLDEN_FWD: &[u8] = include_bytes!("../../../tools/golden_bfz.bin");
    /// Logits de referência da FONTE (torch), raw LE f32, [1, 5, 64].
    const GOLDEN_REF: &[u8] = include_bytes!("../../../tools/golden_bfz.ref.f32");

    fn bfz_ref_logits() -> Vec<f32> {
        let mut v = Vec::with_capacity(GOLDEN_REF.len() / 4);
        for ch in GOLDEN_REF.chunks_exact(4) {
            v.push(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]));
        }
        v
    }

    /// PARIDADE DE LOGITS: o forward Rust (lendo SÓ o contentor) tem de
    /// reproduzir os logits da referência Python.
    ///
    /// Fonte da referência: `tools/golden_bfz.ref.f32` (320 floats = [1,5,64],
    /// ids [11,5,59,38,21], passos = 3 sem early halt).
    #[test]
    fn bfz_forward_matches_python_reference() {
        let m = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        let ids = [11u32, 5, 59, 38, 21];
        let out = forward_bfz(&m, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward bfz");
        let reference = bfz_ref_logits();
        assert_eq!(reference.len(), 320, "golden = 5 tokens x 64 vocab");
        assert_eq!(out.logits.len(), 320);
        assert_eq!(
            out.steps_used, 3,
            "o golden foi produzido com steps_used=3 (sem early halt)"
        );

        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        let mut worst_rel = 0usize;
        let mut worst_abs = 0usize;
        // Diagnóstico: o erro RELATIVO é inflamado por logits de referência
        // quase nulos (o erro ABSOLUTO é o ruído f32). Mede também o erro
        // relativo só nos logits com magnitude útil.
        let mut max_rel_useful = 0.0f32;
        for i in 0..reference.len() {
            let d = (out.logits[i] - reference[i]).abs();
            let r = d / reference[i].abs().max(1e-12);
            if d > max_abs {
                max_abs = d;
                worst_abs = i;
            }
            if r > max_rel {
                max_rel = r;
                worst_rel = i;
            }
            if reference[i].abs() > 1e-2 {
                let ru = d / reference[i].abs();
                if ru > max_rel_useful {
                    max_rel_useful = ru;
                }
            }
        }
        println!(
            "[BFZ] parity vs golden_bfz.ref.f32: max_abs={:.6e} (idx {}) max_rel={:.6e} (idx {}, ref={:.6e})",
            max_abs,
            worst_abs,
            max_rel,
            worst_rel,
            reference[worst_rel]
        );
        println!(
            "[BFZ] max_rel sobre logits com |ref|>1e-2 = {:.6e} (min |ref| = {:.6e})",
            max_rel_useful,
            reference
                .iter()
                .map(|v| v.abs())
                .fold(f32::INFINITY, f32::min)
        );
        for (i, mu) in out.halt_mu.iter().enumerate() {
            println!(
                "[BFZ] passo {}: conf={:.9} mu={:.9} |mu-0.9|={:.9}",
                i,
                out.halt_conf[i],
                mu,
                (mu - BFZ_DEFAULT_ACT_THRESHOLD).abs()
            );
        }
        assert!(
            max_rel < 1e-4,
            "paridade de logits falhou: max_rel={:.6e} max_abs={:.6e}",
            max_rel,
            max_abs
        );
    }

    /// A paridade NÃO É VACUOSA: mexer nos pesos parseados muda os logits.
    ///
    /// Também cobre a negativa da Etapa A (contentor truncado ⇒ None).
    #[test]
    fn bfz_forward_parity_check_is_live() {
        let base = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        let ids = [11u32, 5, 59, 38, 21];
        let ref_out = forward_bfz(&base, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward");

        let delta = |a: &BfzForward, b: &BfzForward| -> f32 {
            a.logits
                .iter()
                .zip(b.logits.iter())
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max)
        };

        // (1) um trit da q projectada (mutação mínima).
        let mut m_trit = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        let byte = &mut m_trit.q.trits.packed_data[0];
        let cur = *byte & 0b11;
        *byte = (*byte & !0b11) | if cur == 0b01 { 0b10 } else { 0b01 };
        let out_trit = forward_bfz(&m_trit, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward");
        let d_trit = delta(&ref_out, &out_trit);
        println!("[BFZ] mutação trit q[0][0]: max_abs_delta={:.6e}", d_trit);
        assert!(d_trit > 0.0, "a paridade é VACUOSA: mexer num trit não mudou nada");

        // (2) a escala POR LINHA de uma linha da q (a materialidade aqui é
        //     atenuada de propósito: a atenção contribui ~1% do residual).
        let mut m_scale = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        m_scale.q.row_scales[0] *= 64.0;
        let out_scale = forward_bfz(&m_scale, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward");
        let d_scale = delta(&ref_out, &out_scale);
        println!("[BFZ] mutação q.row_scales[0] *= 64: max_abs_delta={:.6e}", d_scale);
        assert!(d_scale > 0.0, "a paridade é VACUOSA: a escala da q não influenceia");

        // (3) as escalas de um expert (material: > 1e-3). A contribuição do
        //     expert ao estado é pequena face ao residual ⇒ uma linha só
        //     mexe pouco; o conjunto das linhas mexe materialmente.
        let mut m_exp = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        for s in m_exp.experts[0].gate.row_scales.iter_mut() {
            *s *= 2.0;
        }
        let out_exp = forward_bfz(&m_exp, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward");
        let d_exp = delta(&ref_out, &out_exp);
        println!("[BFZ] mutação expert0.gate.row_scales[*] *= 2: max_abs_delta={:.6e}", d_exp);
        assert!(d_exp > 1e-3, "mutação material não movou os logits ({:.6e})", d_exp);

        // (4) o embedding (material: > 1e-3).
        let mut m_emb = load_bitfuzzy_v6(GOLDEN_FWD).expect("golden bfz carrega");
        if let BfzEmbed::F32 { data, .. } = &mut m_emb.embed {
            for cc in 0..m_emb.plan.header.hidden {
                data[cc * m_emb.plan.header.vocab + 11] *= 2.0;
            }
        }
        let out_emb = forward_bfz(&m_emb, &ids, BFZ_DEFAULT_ACT_THRESHOLD).expect("forward");
        let d_emb = delta(&ref_out, &out_emb);
        println!("[BFZ] mutação embed[:, 11] *= 2: max_abs_delta={:.6e}", d_emb);
        assert!(d_emb > 1e-3, "mutação material não movou os logits ({:.6e})", d_emb);

        // Cobertura negativa da Etapa A: contentor truncado ⇒ None (nunca panic).
        let mut short = GOLDEN_FWD.to_vec();
        short.truncate(GOLDEN_FWD.len() - 1);
        assert!(load_bitfuzzy_v6(&short).is_none());

        // O contentor bfz (model_type=3) tem de ser RECUSADO pelos loaders
        // de LLM denso: `load_model_v6` (dispatch por model_type) e o loader
        // legado `load_model` (guard fail-closed no offset 14) — senão um
        // bfz seria lido como LLM v1..v4 com shapes erradas (corrupção).
        assert!(crate::cortex::load_model_v6(GOLDEN_FWD).is_none());
        assert!(crate::cortex::load_model(GOLDEN_FWD).is_none());
    }
}
