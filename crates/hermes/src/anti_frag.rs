//! s442 — Anti-fragmentação do TALC: **IA-observa → IA-age**, com HITL e
//! verificação. Fecha o ciclo que a s435 abriu pela metade.
//!
//! A s435 deu à IA o NÚMERO (`talc_free`/`largest`/`gaps`/`partial`) e a s433
//! deu o canal LLM, mas o elo que faltava era o do meio: a fragmentação era
//! `Observe` (correto — lição s429-lab: escalar sintoma = loop de feedback) e a
//! "ação" que o LLM produzia era TEXTO LIVRE, que nunca chegava a nenhum
//! executor. Este módulo é o elo:
//!
//! 1. **Vocabulário FECHADO** (`AntiFragCmd`): o LLM escolhe uma chave de um
//!    catálogo com efeito REAL medido, nunca escreve o comando. Texto livre é
//!    a APENAS o rótulo humano do toast. Isso é a propriedade de segurança
//!    central e tem teste próprio: uma proposta de LLM não é comando.
//! 2. **HITL obrigatório**: a ação só existe depois de `Escalate` no
//!    `ApprovalGate` (`/approve <id>` / `/deny <id>`, overlay Jarbas). Sem
//!    humano, a fragmentação continua observe-only.
//! 3. **Efeito medido, não prometido**: guardamos a amostra do TALC ANTES,
//!    executamos, e no pump seguinte comparamos com a amostra nova
//!    (`verify`). Se não melhorou, isso é logado como `nochange`/`worse` —
//!    a ação que não ajuda não se repete (cooldown), e a telemetria continua
//!    sendo a fonte da verdade (regra 419: só é "feito" quando o slog prova).
//!
//! Sem LLM carregado, sem headroom, ou com amostra `partial=1`, o módulo
//! deliberadamente NÃO age: degradação de metadado não se repara agindo em
//! cima dela (s441 deixou isso explícito).

use alloc::format;
use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicU64, Ordering};

use event_bus::Receiver;

use crate::hub_triage::HubTriageInputs;

/// Tópico de reply do job LLM de anti-fragmentação (o InferQueue publica aqui).
pub const TOPIC_ANTIFRAG_LLM: &str = "ANTIFRAG_LLM";
/// Skill registrada no ApprovalGate — o rótulo que o humano lê no `/pending`.
pub const ANTIFRAG_SKILL: &str = "hub.antifrag";
/// Agente que pede (aparece no card HITL).
pub const ANTIFRAG_AGENT: &str = "hub_triage";

/// Cadência da bomba anti-fragmentação: 60 s (mesma escala do triage — a foto
/// de fragmentação muda devagar; agir rápido aqui seria tiro em cego).
pub const ANTIFRAG_PERIOD_TICKS: u64 = 3600;
/// Silêncio após agir: 5 min. Uma ação whose efeito não foi medido ainda não
/// pode ser seguida de outra (senão a lane vira metralhadora de `drop_kv`).
pub const ANTIFRAG_QUIET_TICKS: u64 = 18000;
/// TTL do pedido HITL: 10 min. Passou disso, o pedido é esquecido (humano
/// ignorou). Um id reaprovado muito depois de o snapshot mudar age sobre dado
/// velho — pior que não agir.
pub const ANTIFRAG_HITL_TTL_TICKS: u64 = 36000;
/// Timeout da resposta do LLM: 60 s.
pub const ANTIFRAG_LLM_TIMEOUT_TICKS: u64 = 3600;
/// Espera antes de medir o efeito: 3 s (180 ticks @60Hz).
///
/// Não é arbitrário: `talc_refresh_usage` só corre a 2 Hz, então a amostra do
/// pump seguinte é LITERALMENTE a mesma que colhemos antes de agir. Julgar
/// "sem efeito" sobre ela seria um falso negativo manufactured — o caminho
/// mais fácil de mentir sobre a própria ação.
pub const ANTIFRAG_MEASURE_DELAY_TICKS: u64 = 180;

/// Free TALC (MB) a partir do qual `largest * 4 < free` conta como fragmentado.
/// Mesma régua da s435 (`hub_triage::triage_from_inputs`) — uma só verdade.
pub const FRAG_FREE_MB: u64 = 256;

/// `recent` do H2O: janela recente preservada (tokens). O LLM pode pedir,
/// a política limita — evicting demais é mini-drop-kv com complexidade.
pub const EVICT_RECENT_MIN: u16 = 128;
pub const EVICT_RECENT_MAX: u16 = 2048;
pub const EVICT_RECENT_DEFAULT: u16 = 512;
/// `heavy` do H2O: quantos heavy-hitters antigos sobrevivem.
pub const EVICT_HEAVY_MIN: u16 = 64;
pub const EVICT_HEAVY_MAX: u16 = 4096;
pub const EVICT_HEAVY_DEFAULT: u16 = 1024;

/// Vocabulário FECHADO de ações anti-fragmentação. Cada variante tem um seam
/// REAL (nenhuma é no-op decorativo) e um efeito verificável na telemetria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiFragCmd {
    /// H2O no KV global: mantém recent+heavy, devolve o resto ao TALC.
    EvictKv { recent: u16, heavy: u16 },
    /// Descarta o KV global inteiro (pior caso, maior ganho).
    DropKv,
    /// Libera o cache de experts MoE da arena (memória de longa vida).
    ResetMoeCache,
    /// Declínio explícito do LLM: "não há ação segura agora".
    NoAction,
}

impl AntiFragCmd {
    /// Chave estável do comando (contrato de wire com o prompt).
    pub fn key(&self) -> &'static str {
        match self {
            AntiFragCmd::EvictKv { .. } => "evict_kv",
            AntiFragCmd::DropKv => "drop_kv",
            AntiFragCmd::ResetMoeCache => "reset_moe_cache",
            AntiFragCmd::NoAction => "no_action",
        }
    }

    /// Texto curto pro toast HITL (rótulo humano — NÃO é o comando).
    pub fn label(&self) -> &'static str {
        match self {
            AntiFragCmd::EvictKv { .. } => "eviction H2O do KV cache (p/ o TALC)",
            AntiFragCmd::DropKv => "descartar o KV cache inteiro (p/ o TALC)",
            AntiFragCmd::ResetMoeCache => "liberar cache de experts MoE",
            AntiFragCmd::NoAction => "nenhuma acao segura",
        }
    }
}

/// Catálogo mostrado ao LLM. O texto É o contrato: o modelo escolhe uma
/// `cmd` deste catálogo e os args dentro das faixas.
const ANTIFRAG_PROMPT_HEADER: &str = "Voce e a IA de memoria do kernel AIOS. O allocator \
TALC esta fragmentado: ha memoria livre, mas em pedacos pequenos demais para as \
alocacoes grandes. Escolha UMA acao do catalogo abaixo, de acordo com a \
fragmentacao observada. Responda SOMENTE com JSON: \
{\"title\":\"<curto>\",\"cmd\":\"<chave>\",\"recent\":<n>,\"heavy\":<n>}. \
Catalogo de cmd: evict_kv (mantem recent+heavy tokens do KV e devolve o resto \
ao TALC; args recent 128..2048, heavy 64..4096), drop_kv (descarta o KV cache \
inteiro; use so se evict_kv nao resolve), reset_moe_cache (libera experts MoE \
da arena; so se o KV nao e o problema), no_action (nada seguro agora). \
Nao escreva comandos em texto livre. Nao explique.\nSnapshot: ";

/// Monta o prompt (snapshot JSON do triage + catálogo).
pub fn antifrag_prompt(snapshot_json: &str) -> String {
    format!("{}{}", ANTIFRAG_PROMPT_HEADER, snapshot_json)
}

/// Amostra do TALC no instante da ação (para medir o efeito depois).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TalcSample {
    pub used_mb: u64,
    pub free_mb: u64,
    pub largest_mb: u64,
    pub gaps: u64,
    pub partial: u8,
}

impl TalcSample {
    pub fn of(i: &HubTriageInputs) -> Self {
        TalcSample {
            used_mb: i.talc_used_mb,
            free_mb: i.talc_free_mb,
            largest_mb: i.talc_largest_mb,
            gaps: i.talc_gaps,
            partial: i.talc_partial,
        }
    }
}

/// O que a telemetria diz sobre o efeito de uma ação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// O maior gap cresceu — a fragmentação era mesmo o problema.
    Improved { largest_gain_mb: u64, free_gain_mb: u64 },
    /// Nada mudou de relevante: a hipótese estava errada (ou o KV estava em
    /// voo). Não repete a ação.
    NoChange,
    /// Piorou (uso subiu / maior gap caiu) — a ação custou e não devolveu.
    Worse { used_delta_mb: u64 },
}

/// Julga o efeito a partir de duas amostras. Puro e testável.
///
/// Regra honesta: só é `Improved` se o MAIOR GAP cresceu — free subindo com
/// largest parado é ruído de amostra (ex.: outro cache liberou e o TALK ainda
/// não coalesceu). `Worse` exige uso de TALC maior, que é o custo real.
pub fn verify(before: TalcSample, after: TalcSample) -> Effect {
    if after.partial == 1 {
        // Metadado ilegível: não afirma nada (mesma honestidade da s441).
        return Effect::NoChange;
    }
    if after.largest_mb > before.largest_mb {
        return Effect::Improved {
            largest_gain_mb: after.largest_mb - before.largest_mb,
            free_gain_mb: after.free_mb.saturating_sub(before.free_mb),
        };
    }
    if after.used_mb > before.used_mb {
        return Effect::Worse {
            used_delta_mb: after.used_mb - before.used_mb,
        };
    }
    Effect::NoChange
}

/// Pedido HITL em voo (aguardando `/approve` ou `/deny`).
#[derive(Debug, Clone)]
pub struct Pending {
    /// id do ApprovalGate — é o que o humano digita.
    pub approval_id: u64,
    pub cmd: AntiFragCmd,
    pub requested_tick: u64,
    /// Amostra do TALC no pedido: é contra ela que o efeito é julgado.
    pub before: TalcSample,
    pub title: String,
}

/// Resposta do job LLM, esperando parse.
#[derive(Debug, Clone)]
pub struct LlmAsk {
    pub submitted_tick: u64,
}

/// O que a bomba deve fazer neste tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nada (explica por quê, para o slog).
    Idle(&'static str),
    /// Deve submeter o snapshot ao LLM.
    AskLlm,
    /// O LLM escolheu `cmd`: abrir HITL com ele.
    OpenHitl(AntiFragCmd),
    /// O HITL respondeu `approve`: executar.
    Apply(AntiFragCmd),
    /// O HITL respondeu `deny`.
    Denied(&'static str),
    /// Medir o efeito de uma ação já executada.
    Verify(Effect),
}

impl Default for AntiFragState {
    fn default() -> Self {
        AntiFragState {
            ask: None,
            pending: None,
            verify: None,
            next_tick: 0,
            quiet_until: 0,
        }
    }
}

/// Estado da lane (separado do agente para teste sem statics).
pub struct AntiFragState {
    /// Job LLM em voo (reserva o ciclo; resposta/timeout resolve).
    pub ask: Option<LlmAsk>,
    /// Pedido HITL em voo.
    pub pending: Option<Pending>,
    /// Ação aplicada aguardando medição (`before` + tick do pedido).
    pub verify: Option<(AntiFragCmd, TalcSample, u64)>,
    next_tick: u64,
    /// Ticks até onde a lane cala (após agir).
    quiet_until: u64,
}

// ── contadores (evidência de wire, regra 419) ─────────────────────────────────
pub static ANTIFRAG_LLM_SUBMITTED: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_LLM_FALLBACKS: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_HITL_OPENED: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_APPLIED: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_DENIED: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_NOCHANGE: AtomicU64 = AtomicU64::new(0);
pub static ANTIFRAG_IMPROVED: AtomicU64 = AtomicU64::new(0);

/// Fragmentação com a régua da s435 (uma só verdade com o triage).
pub fn is_fragmented(free_mb: u64, largest_mb: u64) -> bool {
    free_mb >= FRAG_FREE_MB && largest_mb.saturating_mul(4) < free_mb
}

/// s444: QUE TIPO de fragmentação é — a forma decide se a ação compensa.
///
/// Totais (regra da s435) dizem QUE; o histograma (s443) diz DE QUE JEITO, e as
/// duas formas pedem ações OPOSTAS:
/// - `Dust`: a maioria dos gaps é pequena — aloc médio falha apesar de haver
///   memória. É o caso em que liberar/cortar consumers ajuda.
/// - `Benign`: poucos gaps, todos grandes — o sintoma estrutural não existe e
///   qualquer "ação anti-fragmentação" é trabalho jogado fora (e um HITL gasto).
/// - `Mixed`: sem dominante — deixa o LLM decidir com o snapshot na mão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragKind {
    Dust,
    Benign,
    Mixed,
}

/// Classifica a FORMA da fragmentação pelo histograma de gaps.
///
/// Limiares: >=60% dos gaps abaixo de 8KB = poeira; <=20% = benigno. São
/// regras puras e testáveis — e o número que elas usam é o que o kernel mede
/// de verdade, não uma estimativa.
pub fn frag_kind(hist: &[u32; k_nano::allocator::TALC_HIST_BUCKETS]) -> FragKind {
    let total: u32 = hist.iter().sum();
    if total == 0 {
        // Sem histograma (log pré-s443) => não afirma forma: Mixed deixa a
        // decisão com o LLM, que tem o snapshot inteiro.
        return FragKind::Mixed;
    }
    let dust = hist[0];
    if dust * 100 >= total * 60 {
        FragKind::Dust
    } else if dust * 100 <= total * 20 {
        FragKind::Benign
    } else {
        FragKind::Mixed
    }
}

/// Calibra os args do H2O dentro das faixas (a IA pede, a política limita).
pub fn clamp_args(recent: u32, heavy: u32) -> (u16, u16) {
    let r = recent.clamp(EVICT_RECENT_MIN as u32, EVICT_RECENT_MAX as u32) as u16;
    let h = heavy.clamp(EVICT_HEAVY_MIN as u32, EVICT_HEAVY_MAX as u32) as u16;
    (r, h)
}

/// Extrai um inteiro simples de um campo JSON (sem serde; CAP de dígitos).
fn json_int_field(text: &str, field: &str) -> Option<i64> {
    let needle = format!("\"{}\"", field);
    let start = text.find(&needle)? + needle.len();
    let rest = &text.as_bytes()[start..];
    let mut idx = 0;
    while idx < rest.len() && matches!(rest[idx], b' ' | b'\t' | b'\n' | b'\r') {
        idx += 1;
    }
    if idx >= rest.len() || rest[idx] != b':' {
        return None;
    }
    idx += 1;
    while idx < rest.len() && matches!(rest[idx], b' ' | b'\t' | b'\n' | b'\r') {
        idx += 1;
    }
    let num_start = idx;
    let mut digits = 0;
    while idx < rest.len() && matches!(rest[idx], b'-' | b'0'..=b'9') {
        idx += 1;
        digits += 1;
        if digits > 12 {
            return None; // número desproporcional = resposta malformada
        }
    }
    if digits == 0 {
        return None;
    }
    let tok = core::str::from_utf8(&rest[num_start..idx]).ok()?;
    tok.parse::<i64>().ok()
}

/// Extrai a chave `cmd` (string curta) do JSON de resposta.
fn json_cmd_key(text: &str) -> Option<&str> {
    let needle = "\"cmd\"";
    let start = text.find(needle)? + needle.len();
    let rest = &text.as_bytes()[start..];
    let mut idx = 0;
    while idx < rest.len() && matches!(rest[idx], b' ' | b'\t' | b'\n' | b'\r') {
        idx += 1;
    }
    if idx >= rest.len() || rest[idx] != b':' {
        return None;
    }
    idx += 1;
    while idx < rest.len() && matches!(rest[idx], b' ' | b'\t' | b'\n' | b'\r') {
        idx += 1;
    }
    if idx >= rest.len() || rest[idx] != b'"' {
        return None;
    }
    idx += 1;
    let mut end = idx;
    while end < rest.len() && rest[end] != b'"' && rest[end] != b'\\' {
        end += 1;
        if end - idx > 32 {
            return None; // chave longa = não é uma chave do catálogo
        }
    }
    if end >= rest.len() {
        return None;
    }
    core::str::from_utf8(&rest[idx..end]).ok()
}

/// O que o LLM respondeu (classificação pura, testável sem statics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmReply {
    /// Comando do catálogo, com args já calibrados.
    Cmd(AntiFragCmd),
    /// "não há ação segura" (decisão honesta; não abre HITL).
    Decline,
    /// Resposta inutilizável (gibberish, marcador do InferQueue, JSON inválido).
    Fallback(&'static str),
}

/// Decide o que fazer com a resposta do LLM.
///
/// **Propriedade de segurança**: `LlmReply::Cmd` só existe para chave do
/// catálogo. Texto livre ("action": "rm -rf ...") cai em `Fallback` — não há
/// caminho de código que transforme texto do modelo em comando.
pub fn classify_reply(text: &str) -> LlmReply {
    let t = text.trim();
    if t.is_empty() {
        return LlmReply::Fallback("resposta vazia");
    }
    if t.starts_with('[') {
        // Marcadores de controle do InferQueue (fail-closed s430).
        return LlmReply::Fallback("marcador de controle do InferQueue");
    }
    let Some(key) = json_cmd_key(t) else {
        // Contrato s433: `{}` = "nada a fazer agora" — declínio HONESTO do
        // LLM (não é falha de parse). Qualquer OUTRO texto sem `cmd` é lixo
        // (gibberish ou texto livre) → fallback, nunca execução.
        let b = t.as_bytes();
        if b.len() >= 2 && b[0] == b'{' && b[b.len() - 1] == b'}' && t[1..t.len() - 1].trim().is_empty() {
            return LlmReply::Decline;
        }
        return LlmReply::Fallback("sem chave cmd no JSON");
    };
    match key {
        "no_action" => LlmReply::Decline,
        "drop_kv" => LlmReply::Cmd(AntiFragCmd::DropKv),
        "reset_moe_cache" => LlmReply::Cmd(AntiFragCmd::ResetMoeCache),
        "evict_kv" => {
            let r = json_int_field(t, "recent").unwrap_or(EVICT_RECENT_DEFAULT as i64);
            let h = json_int_field(t, "heavy").unwrap_or(EVICT_HEAVY_DEFAULT as i64);
            // Negativo/ausente cai no default (clamp trata); i64 → u32 com
            // saturate para não virar número gigante em debug.
            let (recent, heavy) = clamp_args(r.clamp(0, u32::MAX as i64) as u32, h.clamp(0, u32::MAX as i64) as u32);
            LlmReply::Cmd(AntiFragCmd::EvictKv { recent, heavy })
        }
        // Chave desconhecida ou 'action' em texto livre: recusado.
        _ => LlmReply::Fallback("cmd fora do catalogo"),
    }
}

/// Botão de cooldown ativo?
pub fn in_quiet(s: &AntiFragState, now_tick: u64) -> bool {
    now_tick < s.quiet_until
}

impl AntiFragState {
    fn arm(&mut self, now_tick: u64) {
        self.next_tick = now_tick.saturating_add(ANTIFRAG_PERIOD_TICKS);
    }

    /// Cancela o job LLM em voo e devolve a razão (fallback).
    fn abort_ask(&mut self, reason: &'static str) -> Step {
        self.ask = None;
        Step::Idle(reason)
    }

    /// Decide o passo de CIMA, sem tocar em statics/bus (a unidade da lógica).
    ///
    /// `model_ok`/`headroom_ok` são gates de classe (mesmo contrato da s433:
    /// o submit recusaria de qualquer forma; recusar aqui evita ruído).
    pub fn decide(
        &mut self,
        inputs: &HubTriageInputs,
        model_ok: bool,
        headroom_ok: bool,
        now_tick: u64,
    ) -> Step {
        // 1. Medição pendente tem prioridade: verificar antes de propor de novo.
        if let Some((cmd, before, applied_tick)) = self.verify {
            // A amostra do TALC é cacheada a 2 Hz: medir antes disso é comparar
            // a amostra com ela mesma (falso "sem efeito"). Espera a amostra nova.
            if now_tick.saturating_sub(applied_tick) < ANTIFRAG_MEASURE_DELAY_TICKS {
                return Step::Idle("aguardando amostra nova");
            }
            let effect = verify(before, TalcSample::of(inputs));
            self.verify = None;
            // Cooldown SEMPRE depois de agir (mesmo sem efeito: repetir a mesma
            // hipótese a cada minuto é como a lane vira metralhadora).
            self.quiet_until = applied_tick.saturating_add(ANTIFRAG_QUIET_TICKS);
            self.arm(now_tick);
            match effect {
                Effect::Improved { .. } => {
                    ANTIFRAG_IMPROVED.fetch_add(1, Ordering::Relaxed);
                }
                Effect::NoChange | Effect::Worse { .. } => {
                    ANTIFRAG_NOCHANGE.fetch_add(1, Ordering::Relaxed);
                }
            }
            let _ = cmd; // logado pelo agente (o estado não o perde)
            return Step::Verify(effect);
        }
        // 2. HITL em voo: só esperamos (decisão chega pelo resolution()).
        if self.pending.is_some() {
            return Step::Idle("hitl em voo");
        }
        // 3. Job LLM em voo: idem (resposta/timeout chegam por fora).
        if self.ask.is_some() {
            if now_tick.saturating_sub(self.ask.as_ref().unwrap().submitted_tick)
                >= ANTIFRAG_LLM_TIMEOUT_TICKS
            {
                self.ask = None;
                self.arm(now_tick);
                ANTIFRAG_LLM_FALLBACKS.fetch_add(1, Ordering::Relaxed);
                return Step::Idle("llm timeout (sem acao)");
            }
            return Step::Idle("llm em voo");
        }
        // 4. Cooldown pós-efeito.
        if in_quiet(self, now_tick) {
            return Step::Idle("cooldown pos-efeito");
        }
        // 5. Cadência.
        if self.next_tick != 0 && now_tick < self.next_tick {
            return Step::Idle("fora da cadencia");
        }
        // 6. Sem amostra honesta: não se decide sobre nada.
        if inputs.talc_cap_mb == 0 {
            return Step::Idle("talc sem claim");
        }
        if inputs.talc_partial == 1 {
            return Step::Idle("amostra parcial (nao decide)");
        }
        // 7. Só fragmentação interessa aqui (heap é do triage).
        if !is_fragmented(inputs.talc_free_mb, inputs.talc_largest_mb) {
            return Step::Idle("talc nao fragmentado");
        }
        // 7b. s444: fragmentação BENIGNA (poucos gaps, todos grandes) não é
        //     alvo de ação. Gastar HITL + derrubar o KV cache para consertar um
        //     sintoma que não existe é a pior versão da loop de feedback da
        //     s429-lab: trabalho que não devolve nada. O histograma diz isso
        //     ANTES de chamar a IA — e é por isso que ele precisa existir.
        let kind = frag_kind(&inputs.talc_hist);
        if kind == FragKind::Benign {
            self.arm(now_tick);
            return Step::Idle("fragmentacao benigna (poucos gaps grandes) — acao seria desperdicio");
        }
        // 8. Gates de classe → LLM.
        if !model_ok {
            self.arm(now_tick);
            return Step::Idle("modelo ausente (observe-only)");
        }
        if !headroom_ok {
            self.arm(now_tick);
            return Step::Idle("headroom baixo (observe-only)");
        }
        self.ask = Some(LlmAsk {
            submitted_tick: now_tick,
        });
        Step::AskLlm
    }

    /// O LLM respondeu: classifica e (se for comando) abre o HITL.
    /// `open_hitl` é injetado para o teste poder provar que a decisão é
    /// HITL-gated sem tocar no ApprovalGate real.
    pub fn on_llm_reply<F>(
        &mut self,
        text: &str,
        inputs: &HubTriageInputs,
        now_tick: u64,
        mut open_hitl: F,
    ) -> Step
    where
        F: FnMut(AntiFragCmd, &str) -> u64,
    {
        if self.ask.is_none() {
            return Step::Idle("resposta sem job em voo");
        }
        self.ask = None;
        match classify_reply(text) {
            LlmReply::Cmd(cmd) => {
                // TÍTULO vem do LLM quando ele é utilizável; o comando é o
                // enum, nunca o texto. (parse best-effort, len CAP.)
                let title = crate::hub_triage::parse_llm_proposal(text)
                    .map(|(t, _)| t)
                    .unwrap_or_else(|| cmd.label().to_string());
                let before = TalcSample::of(inputs);
                let id = open_hitl(cmd, &title);
                self.pending = Some(Pending {
                    approval_id: id,
                    cmd,
                    requested_tick: now_tick,
                    before,
                    title,
                });
                self.arm(now_tick);
                ANTIFRAG_HITL_OPENED.fetch_add(1, Ordering::Relaxed);
                Step::OpenHitl(cmd)
            }
            LlmReply::Decline => {
                self.arm(now_tick);
                Step::Idle("llm declinou (observe-only)")
            }
            LlmReply::Fallback(reason) => {
                self.arm(now_tick);
                ANTIFRAG_LLM_FALLBACKS.fetch_add(1, Ordering::Relaxed);
                Step::Idle(reason)
            }
        }
    }

    /// O HITL respondeu. `None` = ainda pendente.
    pub fn on_hitl_resolution(
        &mut self,
        approved: Option<bool>,
        inputs: &HubTriageInputs,
        now_tick: u64,
    ) -> Step {
        let Some(p) = self.pending.clone() else {
            return Step::Idle("resolucao sem pedido");
        };
        if now_tick.saturating_sub(p.requested_tick) >= ANTIFRAG_HITL_TTL_TICKS {
            self.pending = None;
            self.arm(now_tick);
            return Step::Idle("hitl expirado (ignorado)");
        }
        match approved {
            None => Step::Idle("hitl pendente"),
            Some(false) => {
                self.pending = None;
                self.arm(now_tick);
                ANTIFRAG_DENIED.fetch_add(1, Ordering::Relaxed);
                Step::Denied("hitl negado")
            }
            Some(true) => {
                self.pending = None;
                // Guarda a amostra ANTES de agir: é contra ela que o efeito
                // será julgado no próximo pump (se agirmos depois de coletar,
                // a "antes" já é o "depois" e a medição vira teatro).
                let before = TalcSample::of(inputs);
                self.verify = Some((p.cmd, before, now_tick));
                self.arm(now_tick);
                Step::Apply(p.cmd)
            }
        }
    }
}



/// Executa a ação no seam REAL. Retorna o que aconteceu (honesto).
///
/// Regra: **esta função é a única que muta o sistema**, e só é alcançável a
/// partir de `Step::Apply`, que só existe depois de HITL aprovado.
pub fn apply(cmd: AntiFragCmd) -> Step {
    match cmd {
        AntiFragCmd::EvictKv { recent, heavy } => {
            match cortex::cortex::kv_h2o_evict_global(recent as usize, heavy as usize) {
                Some(dropped) => {
                    ANTIFRAG_APPLIED.fetch_add(1, Ordering::Relaxed);
                    k_nano::slog_hermes!(
                        "AntiFrag", "ok",
                        "evict_kv recent={} heavy={} dropped={} tokens",
                        recent, heavy, dropped
                    );
                }
                None => {
                    // KV global em voo (take) ou vazio: fail-closed honesto.
                    k_nano::slog_hermes!(
                        "AntiFrag", "warn",
                        "evict_kv sem efeito (KV global em voo/vazio) — no-op honesto"
                    );
                }
            }
        }
        AntiFragCmd::DropKv => {
            let pages = cortex::cortex::kv_global_pages();
            cortex::cortex::kv_cache_reset();
            if pages > 0 {
                ANTIFRAG_APPLIED.fetch_add(1, Ordering::Relaxed);
                k_nano::slog_hermes!(
                    "AntiFrag", "ok", "drop_kv liberou {} paginas p/ o TALC", pages
                );
            } else {
                k_nano::slog_hermes!(
                    "AntiFrag", "warn", "drop_kv sem efeito (KV global vazio/em voo)"
                );
            }
        }
        AntiFragCmd::ResetMoeCache => {
            let (used, _cap) = cortex::global_arena::arena_stats();
            cortex::global_arena::reset_moe_cache();
            ANTIFRAG_APPLIED.fetch_add(1, Ordering::Relaxed);
            k_nano::slog_hermes!(
                "AntiFrag", "ok", "reset_moe_cache (arena usada {}MB antes)", used / (1024 * 1024)
            );
        }
        AntiFragCmd::NoAction => {}
    }
    Step::Idle("aplicado")
}

// ── wiring IMPURO (bus + approval gate + infer queue) ────────────────────────
// A lógica toda é `AntiFragState` (pura, testada acima). Aqui só o que precisa
// do mundo: subscrever o reply, submeter o job, abrir o HITL, executar.

/// Abre o pedido HITL (Escalate = aprovação explícita obrigatória) e devolve o
/// id, ou 0 se o gate não abriu (fail-closed: sem gate, não há id para aprovar).
///
/// `try_lock` COM LIMITE (nunca `lock()`): o `TicketLock` do hermes não é
/// reentrante e o `request()` publica no bus — segurar o lock atravessando
/// publicação é como o s438 morreu (EventBus de IRQ pegando o mesmo lock).
fn open_hitl(cmd: AntiFragCmd, title: &str) -> u64 {
    let reason = format!(
        "TALC fragmentado. Proposta da IA: {} ({}). Se aprovar, o kernel EXECUTA a acao.",
        cmd.label(),
        title
    );
    for _ in 0..HITL_LOCK_TRIES {
        match crate::globals::APPROVAL_GATE.try_lock() {
            Some(mut g) => {
                return g.request(
                    ANTIFRAG_SKILL,
                    ANTIFRAG_AGENT,
                    &reason,
                    crate::approval::ApprovalLevel::Escalate,
                )
            }
            None => core::hint::spin_loop(),
        }
    }
    k_nano::slog_hermes!("AntiFrag", "warn", "approval gate ocupado — HITL nao aberto");
    0
}

/// Tentativas de aquisição do APPROVAL_GATE (best-effort, nunca blocking).
const HITL_LOCK_TRIES: usize = 64;

impl AntiFragState {
    /// Há trabalho pendente (resposta do LLM / HITL / medição de efeito)?
    pub fn busy(&self) -> bool {
        self.ask.is_some() || self.pending.is_some() || self.verify.is_some()
    }

    /// `approval_id == 0` = o HITL não abriu; o pedido é descartado.
    fn close_if_unopened(&mut self, now_tick: u64) {
        if self.pending.as_ref().map(|p| p.approval_id == 0).unwrap_or(false) {
            self.pending = None;
            self.arm(now_tick);
        }
    }
}

/// Bomba da lane (chamada pelo `HubTriageAgent::tick`). Devolve o passo para
/// o agente logar (String::new() = nada a dizer).
pub fn pump(state: &mut AntiFragState, llm_rx: &Receiver, now_tick: u64) -> Step {
    // 1. Resposta do LLM (chega fora da cadência — drena tudo que houver).
    while let Some(ev) = llm_rx.try_receive() {
        let inputs = HubTriageInputs::gather();
        let text = core::str::from_utf8(&ev.payload).unwrap_or("");
        let step = state.on_llm_reply(text, &inputs, now_tick, open_hitl);
        state.close_if_unopened(now_tick);
        match step {
            Step::OpenHitl(cmd) => {
                if state.pending.is_none() {
                    return Step::Idle("hitl nao aberto (gate ocupado)");
                }
                k_nano::slog_hermes!(
                    "AntiFrag", "ok",
                    "IA escolheu {} — HITL #{} ({}). /approve aplica, /deny recusa.",
                    cmd.key(),
                    state.pending.as_ref().map(|p| p.approval_id).unwrap_or(0),
                    cmd.label()
                );
                return Step::OpenHitl(cmd);
            }
            other => return other,
        }
    }
    // 2. Resolução do HITL em voo (bounded try_lock; None = ainda pendente).
    if let Some(p) = state.pending.clone() {
        let res = crate::globals::APPROVAL_GATE
            .try_lock()
            .and_then(|g| g.resolution(p.approval_id));
        let step = state.on_hitl_resolution(res, &HubTriageInputs::gather(), now_tick);
        if let Step::Apply(cmd) = step {
            apply(cmd);
            return step;
        }
        match step {
            Step::Verify(e) => return Step::Verify(e),
            Step::Idle(reason) if reason == "hitl pendente" => {}
            other => return other,
        }
    }
    // 3. Decide: medir efeito pendente / pedir / ficar quieto.
    let inputs = HubTriageInputs::gather();
    let model_ok = cortex::cortex::model_is_loaded();
    let headroom_ok = !k_nano::allocator::heap_headroom_low();
    match state.decide(&inputs, model_ok, headroom_ok, now_tick) {
        Step::AskLlm => {
            let json = crate::hub_triage::triage_snapshot_json(&inputs);
            let kind = frag_kind(&inputs.talc_hist);
            match cortex::infer_queue::submit(
                antifrag_prompt(&json),
                cortex::infer_queue::InferMode::Plain,
                TOPIC_ANTIFRAG_LLM,
            ) {
                Ok(id) => {
                    ANTIFRAG_LLM_SUBMITTED.fetch_add(1, Ordering::Relaxed);
                    k_nano::slog_hermes!(
                        "AntiFrag", "ok",
                        "fragmentacao {:?} detectada (free={}M largest={}M gaps={} hist={}/{}/{}/{}/{}/{}) — LLM id={}",
                        kind,
                        inputs.talc_free_mb, inputs.talc_largest_mb, inputs.talc_gaps,
                        inputs.talc_hist[0], inputs.talc_hist[1], inputs.talc_hist[2],
                        inputs.talc_hist[3], inputs.talc_hist[4], inputs.talc_hist[5], id
                    );
                    Step::AskLlm
                }
                Err(e) => {
                    state.ask = None;
                    ANTIFRAG_LLM_FALLBACKS.fetch_add(1, Ordering::Relaxed);
                    k_nano::slog_hermes!(
                        "AntiFrag", "warn",
                        "submit LLM falhou ({:?}) — fragmentacao segue observe-only", e
                    );
                    Step::Idle("submit falhou (observe-only)")
                }
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> HubTriageInputs {
        HubTriageInputs {
            heap_used_mb: 1000,
            heap_window_mb: 2030,
            heap_pressure: 0,
            talc_cap_mb: 6911,
            talc_used_mb: 1200,
            talc_free_mb: 5700,
            talc_largest_mb: 200, // fragmentado: largest*4=800 < 5700
            talc_gaps: 64,
            talc_partial: 0,
            talc_hist: [40, 12, 8, 3, 1, 0],
            arena_used_mb: 10,
            arena_cap_mb: 256,
            posture_sev: 0,
            posture_line: String::from("a1 x0 e0 L0"),
            machine_json: None,
            sched_violations: 0,
        }
    }

    #[test]
    fn fragmented_predicate_matches_s435_rule() {
        assert!(is_fragmented(5700, 200));
        assert!(!is_fragmented(200, 200)); // free < 256MB
        assert!(!is_fragmented(5700, 2000)); // largest*4 = 8000 >= 5700
    }

    // ── s444: a FORMA da fragmentacao (o histograma decide se a acao compensa) ─

    #[test]
    fn forma_poeira_dominante_e_acao_justificada() {
        // 48 de 71 gaps abaixo de 8KB = 67% => Dust.
        assert_eq!(frag_kind(&[48, 13, 7, 2, 1, 0]), FragKind::Dust);
    }

    #[test]
    fn limiares_sao_exatos() {
        // 60% exato => Dust (>= 60); 20% exato => Benign (<= 20).
        assert_eq!(frag_kind(&[60, 40, 0, 0, 0, 0]), FragKind::Dust);
        assert_eq!(frag_kind(&[20, 80, 0, 0, 0, 0]), FragKind::Benign);
        // 30% fica NO MEIO: e' o caso que distingue os dois limiares (sem ele,
        // mover 60->20 nao quebraria nada — mutation M10 CIROU por aqui).
        assert_eq!(frag_kind(&[30, 40, 10, 10, 5, 5]), FragKind::Mixed);
    }

    #[test]
    fn forma_benigna_e_acao_que_nao_compensa() {
        // Poucos gaps, todos grandes: o sintoma estrutural nao existe.
        assert_eq!(frag_kind(&[1, 2, 3, 4, 5, 6]), FragKind::Benign);
        assert_eq!(frag_kind(&[0, 0, 0, 0, 0, 9]), FragKind::Benign);
    }

    #[test]
    fn forma_mixed_e_o_caso_sem_dominante() {
        assert_eq!(frag_kind(&[10, 10, 5, 3, 1, 1]), FragKind::Mixed);
        // Sem histograma (log pre-s443) => nao afirma forma.
        assert_eq!(frag_kind(&[0, 0, 0, 0, 0, 0]), FragKind::Mixed);
    }

    #[test]
    fn fragmentacao_benigna_nao_chega_ao_llm() {
        // fragmented = true, porem a forma e' benigna: a lane NAO deve gastar
        // HITL nem sub-meter nada.
        let mut s = AntiFragState::default();
        let mut i = inputs();
        i.talc_hist = [0, 0, 0, 0, 0, 9]; // 9 gaps, todos >= 16MB
        i.talc_gaps = 9;
        match s.decide(&i, true, true, 10) {
            Step::Idle("fragmentacao benigna (poucos gaps grandes) — acao seria desperdicio") => {}
            other => panic!("esperava benigno/observe-only, veio {:?}", other),
        }
        assert!(s.ask.is_none(), "nao pode ter submetido LLM em fragmentacao benigna");
    }

    #[test]
    fn forma_poeira_segue_para_o_llm() {
        let mut s = AntiFragState::default();
        let mut i = inputs();
        i.talc_hist = [48, 13, 7, 2, 1, 0];
        assert_eq!(s.decide(&i, true, true, 10), Step::AskLlm);
    }

    #[test]
    fn fragmentado_com_modelo_chega_ao_llm() {
        let mut s = AntiFragState::default();
        match s.decide(&inputs(), true, true, 10) {
            Step::AskLlm => {}
            other => panic!("esperava AskLlm, veio {:?}", other),
        }
    }

    #[test]
    fn sem_fragmentacao_nao_age() {
        let mut s = AntiFragState::default();
        let mut i = inputs();
        i.talc_largest_mb = 6000;
        match s.decide(&i, true, true, 10) {
            Step::Idle("talc nao fragmentado") => {}
            other => panic!("esperava idle, veio {:?}", other),
        }
    }

    #[test]
    fn amostra_parcial_nunca_decide() {
        let mut s = AntiFragState::default();
        let mut i = inputs();
        i.talc_partial = 1;
        match s.decide(&i, true, true, 10) {
            Step::Idle("amostra parcial (nao decide)") => {}
            other => panic!("esperava idle, veio {:?}", other),
        }
        assert!(s.ask.is_none(), "nao pode ter submetido LLM");
    }

    #[test]
    fn talc_sem_claim_nao_age() {
        let mut s = AntiFragState::default();
        let mut i = inputs();
        i.talc_cap_mb = 0;
        match s.decide(&i, true, true, 10) {
            Step::Idle("talc sem claim") => {}
            other => panic!("esperava idle, veio {:?}", other),
        }
    }

    #[test]
    fn sem_modelo_ou_headroom_fica_observe_only() {
        let mut s = AntiFragState::default();
        assert!(matches!(s.decide(&inputs(), false, true, 10), Step::Idle("modelo ausente (observe-only)")));
        let mut s2 = AntiFragState::default();
        assert!(matches!(s2.decide(&inputs(), true, false, 10), Step::Idle("headroom baixo (observe-only)")));
    }

    // ── propriedade de segurança: texto livre do LLM NUNCA vira comando ────────

    #[test]
    fn texto_livre_do_llm_nao_vira_cmd() {
        // "action" em vez de "cmd" — o texto é rótulo, não comando.
        let r = classify_reply(r#"{"title":"x","action":"format c: elimine tudo"}"#);
        assert!(matches!(r, LlmReply::Fallback(_)), "veio {:?}", r);
        // Chave fora do catálogo.
        let r2 = classify_reply(r#"{"title":"x","cmd":"rm_rf"}"#);
        assert!(matches!(r2, LlmReply::Fallback("cmd fora do catalogo")), "veio {:?}", r2);
    }

    #[test]
    fn cmd_do_catalogo_vira_comando_calibrado() {
        let r = classify_reply(r#"{"title":"KV","cmd":"evict_kv","recent":300,"heavy":900}"#);
        assert_eq!(r, LlmReply::Cmd(AntiFragCmd::EvictKv { recent: 300, heavy: 900 }));
        assert_eq!(classify_reply(r#"{"cmd":"drop_kv"}"#), LlmReply::Cmd(AntiFragCmd::DropKv));
        assert_eq!(classify_reply(r#"{"cmd":"reset_moe_cache"}"#), LlmReply::Cmd(AntiFragCmd::ResetMoeCache));
        assert_eq!(classify_reply("{}"), LlmReply::Decline);
        assert_eq!(classify_reply(r#"{"cmd":"no_action"}"#), LlmReply::Decline);
    }

    #[test]
    fn args_fora_da_faixa_sao_clampados() {
        let r = classify_reply(r#"{"cmd":"evict_kv","recent":999999,"heavy":1}"#);
        assert_eq!(r, LlmReply::Cmd(AntiFragCmd::EvictKv { recent: EVICT_RECENT_MAX, heavy: EVICT_HEAVY_MIN }));
        let r2 = classify_reply(r#"{"cmd":"evict_kv","recent":-5,"heavy":-5}"#);
        assert_eq!(r2, LlmReply::Cmd(AntiFragCmd::EvictKv { recent: EVICT_RECENT_MIN, heavy: EVICT_HEAVY_MIN }));
    }

    #[test]
    fn gibberish_e_marcadores_de_controle_sao_fallback() {
        assert!(matches!(classify_reply("andsfaqt zzz"), LlmReply::Fallback(_)));
        assert!(matches!(classify_reply("[cancelled]"), LlmReply::Fallback(_)));
        assert!(matches!(classify_reply(""), LlmReply::Fallback(_)));
        assert!(matches!(classify_reply("{\"cmd\":\"evict_kv"), LlmReply::Fallback(_)));
    }

    // ── HITL: nada executa sem aprovação ──────────────────────────────────────

    #[test]
    fn comando_do_llm_so_existe_apos_hitl() {
        let mut s = AntiFragState::default();
        let mut opened = 0;
        s.decide(&inputs(), true, true, 10);
        let step = s.on_llm_reply(
            r#"{"title":"KV alto","cmd":"drop_kv"}"#,
            &inputs(),
            11,
            |_c, _t| {
                opened += 1;
                4242
            },
        );
        assert_eq!(step, Step::OpenHitl(AntiFragCmd::DropKv));
        assert_eq!(opened, 1);
        let p = s.pending.as_ref().unwrap();
        assert_eq!(p.approval_id, 4242);
        // Enquanto pendente, decide() não abre novo pedido.
        assert_eq!(s.decide(&inputs(), true, true, 12), Step::Idle("hitl em voo"));
    }

    #[test]
    fn denied_nao_executa() {
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 11, |_c, _t| 7);
        let step = s.on_hitl_resolution(Some(false), &inputs(), 12);
        assert_eq!(step, Step::Denied("hitl negado"));
        assert!(s.pending.is_none());
        assert!(s.verify.is_none(), "denied nao pode agendar medicao");
    }

    #[test]
    fn approved_agenda_verificacao_e_executa_seam_real() {
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 11, |_c, _t| 7);
        let step = s.on_hitl_resolution(Some(true), &inputs(), 12);
        assert_eq!(step, Step::Apply(AntiFragCmd::DropKv));
        // verify agendado com a amostra do instante do pedido.
        let (cmd, before, _t) = s.verify.unwrap();
        assert_eq!(cmd, AntiFragCmd::DropKv);
        assert_eq!(before.largest_mb, 200);
        // O agente chama `apply` — seam real (no-op honesto em host sem KV).
        apply(cmd);
    }

    #[test]
    fn hitl_expirado_e_ignorado() {
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 11, |_c, _t| 7);
        let late = 11 + ANTIFRAG_HITL_TTL_TICKS;
        let step = s.on_hitl_resolution(Some(true), &inputs(), late);
        assert_eq!(step, Step::Idle("hitl expirado (ignorado)"));
        assert!(s.verify.is_none(), "aprovacao tardia nao pode agir");
    }

    #[test]
    fn resposta_sem_job_em_voo_e_ignorada() {
        let mut s = AntiFragState::default();
        let step = s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 10, |_c, _t| 1);
        assert_eq!(step, Step::Idle("resposta sem job em voo"));
        assert!(s.pending.is_none());
    }

    #[test]
    fn llm_timeout_nao_deixa_job_pendurado() {
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        let t = 10 + ANTIFRAG_LLM_TIMEOUT_TICKS;
        assert_eq!(s.decide(&inputs(), true, true, t), Step::Idle("llm timeout (sem acao)"));
        assert!(s.ask.is_none());
    }

    // ── verificação de efeito (o ciclo fecha com número, não com promessa) ───

    #[test]
    fn verify_julga_melhora_pelo_maior_gap() {
        let before = TalcSample { used_mb: 1200, free_mb: 5700, largest_mb: 200, gaps: 64, partial: 0 };
        let after = TalcSample { used_mb: 400, free_mb: 6500, largest_mb: 6400, gaps: 2, partial: 0 };
        assert_eq!(
            verify(before, after),
            Effect::Improved { largest_gain_mb: 6200, free_gain_mb: 800 }
        );
    }

    #[test]
    fn verify_nao_chama_de_melhora_de_largest_parado() {
        let before = TalcSample { used_mb: 1200, free_mb: 5700, largest_mb: 200, gaps: 64, partial: 0 };
        let after = TalcSample { used_mb: 1200, free_mb: 6000, largest_mb: 200, gaps: 40, partial: 0 };
        assert_eq!(verify(before, after), Effect::NoChange);
    }

    #[test]
    fn verify_piora_quando_uso_sobe() {
        let before = TalcSample { used_mb: 1200, free_mb: 5700, largest_mb: 200, gaps: 64, partial: 0 };
        let after = TalcSample { used_mb: 1300, free_mb: 5600, largest_mb: 200, gaps: 64, partial: 0 };
        assert_eq!(verify(before, after), Effect::Worse { used_delta_mb: 100 });
    }

    #[test]
    fn verify_nao_afirma_nada_com_amostra_parcial() {
        let before = TalcSample { used_mb: 1200, free_mb: 5700, largest_mb: 200, gaps: 64, partial: 0 };
        let after = TalcSample { used_mb: 400, free_mb: 6500, largest_mb: 6400, gaps: 2, partial: 1 };
        assert_eq!(verify(before, after), Effect::NoChange);
    }

    #[test]
    fn pos_efeito_entra_em_cooldown_e_verifica_uma_vez() {
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 11, |_c, _t| 7);
        s.on_hitl_resolution(Some(true), &inputs(), 12);
        let mut after = inputs();
        after.talc_largest_mb = 6400;
        after.talc_free_mb = 6500;
        after.talc_used_mb = 400;
        after.talc_gaps = 2;
        // Pump seguinte: ainda NÃO julga (amostra do TALC é cache 2 Hz).
        assert_eq!(
            s.decide(&after, true, true, 13),
            Step::Idle("aguardando amostra nova")
        );
        // Passado o delay: verifica de verdade.
        let t = 12 + ANTIFRAG_MEASURE_DELAY_TICKS;
        match s.decide(&after, true, true, t) {
            Step::Verify(Effect::Improved { .. }) => {}
            other => panic!("esperava Verify(Improved), veio {:?}", other),
        }
        assert!(s.verify.is_none(), "verificacao nao se repete");
        // E o cooldown segura a metralhadora.
        assert_eq!(s.decide(&after, true, true, t + 1), Step::Idle("cooldown pos-efeito"));
    }

    #[test]
    fn medir_antes_do_delay_queimaria_a_unica_verificacao() {
        // Sem o delay, o primeiro pump mediria a MESMA amostra (cache 2 Hz) e
        // registraria "sem efeito" — e o cooldown de 5 min impediria nova
        // medição. O guard é o que mantém a verificação honesta.
        let mut s = AntiFragState::default();
        s.decide(&inputs(), true, true, 10);
        s.on_llm_reply(r#"{"cmd":"drop_kv"}"#, &inputs(), 11, |_c, _t| 7);
        s.on_hitl_resolution(Some(true), &inputs(), 12);
        for t in 13..(12 + ANTIFRAG_MEASURE_DELAY_TICKS) {
            assert_eq!(s.decide(&inputs(), true, true, t), Step::Idle("aguardando amostra nova"));
        }
        assert!(s.verify.is_some(), "a medicao continua pendente ate ter amostra nova");
    }
}