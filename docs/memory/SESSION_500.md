# SESSION_500 — Auditoria de aderência: uso de Laya/Jev (ADR-0078 §2.2 + ADR-0110)

**Sprint:** v1.9.99-s500 TEST
**Data:** 2026-10-21
**Pedido:** "revise o uso da laya/jev, como previsto na adr" — escopo escolhido pelo
maintainer (ask_questions): **auditoria de aderência**, referência = **ambas as ADRs**
(ADR-0078 §2.2 + ADR-0110). Correções mínimas de doc; **0 linhas Rust alteradas**.

## 1. Método

- Varredura repo-inteiro (`grep -i` com word-boundary em `.rs`/`.py`/docs — o substring
  "laya" dentro de "Displ**aya**gent" é falso-positivo e foi filtrado) + leitura
  cláusula-por-cláusula das duas ADRs.
- **Falsificação por ausência**: cada item da coluna REJEITADA da ADR-0110 virou um grep
  que tem de dar **vazio no código**; cada item ACEITO virou um grep que tem de dar
  **presente**. Resultado abaixo.
- Sem feature nova: a ADR-0110 §7/§10.4 bloqueia scorer/triagem atrás do gate **#611**
  (dataset + gate ≥65% + ECE + evidência) — implementar o uso "previsto e ainda gated"
  violaria a própria ADR. O uso **liberado** é o auditado.

## 2. ADR-0078 §2.2 — "Laya / Jev / AgentJev = Intent/router (`decide_intent`, ROUTER) — não slot gerador"

| Cláusula | Evidência | Veredito |
|---|---|---|
| Não aparece como slot/alias gerador | `model_hub.rs` e `mkfat32.py`: **zero** ocorrências de laya/jev (único hit "laya" = `model_hub.rs:475 "DisplayAgent"`, falso-positivo) | ✅ |
| Papel intent via `decide_intent` | `intent_decide.rs:153` — `decide_intent(&str) -> Decision<Intent,15>`, 15 arms score-por-braço, θ_conf=110/θ_margin=12, `source=Heuristic` (honesto, não finge Neural) | ✅ |
| Wired no fluxo real | `cortex.rs:6257` (`think`) e `:6266` (`decide`) → `hermes/agents.rs:2256` `self.cortex.decide(msg)` → `:2257` `resolve_intent` | ✅ |
| Papel ROUTER | `trinity.rs:228-229` `router_trained`, `:240` encode do `ROUTER.BITNET`, `resolve_route` (`trinity.rs:121`) → dispatch (`cortex.rs:5834`) | ✅ |
| Docs dizem "intent, não slot" | `README.md:252`, `STATE.md:136`, `SESSION_INDEX.md:21` (s491) | ✅ |

## 3. ADR-0110 — REJEITADOS ausentes · ACEITOS presentes · gates intactos

**REJEITADOS (grep de ausência no código):**

| Item rejeitado (§2.1/§10.1) | Evidência | Veredito |
|---|---|---|
| `hw_expert_v6` como classificador de intenção | único caller de `pack_vid_did` = `cortex.rs:674` dentro de `predict_hw_v4` (PnP: vid/did → família de device) — nenhum caminho de intenção/pacote | ✅ ausente |
| Gate `HEAP_FREE_MB > 200` | **0** ocorrências em `crates/`/`tools/` (só a própria ADR descrevendo-o como símbolo inexistente + a nota refutada do #544, corrigida abaixo) | ✅ ausente |
| TickV-TTL efêmero | `tickv.rs`: sem TTL/evict; `mount_scan_expired` (`:82`) é **budget de parede 3 s do scan de mount** (`MOUNT_SCAN_BUDGET_US`, `:72`), não expiração de entrada | ✅ ausente |
| Skip de detectores / consenso-drop / `Intent::Normal` de fallback | `hermes/security.rs`: **zero** `intent`/`laya`/`Normal`; só drop-oldest do buffer de alerts (`:174/:222/:504`) | ✅ ausente |
| Garantia "<35ms forward" / RAM 150–300MB pinned | `grep "35ms\|35 ms" crates/*.rs` = **vazio** | ✅ ausente |
| Dependência TypeSafe/Jev (nuvem) | `Cargo.toml` + `crates/*/Cargo.toml`: **zero** `typesafe\|jev` | ✅ ausente |
| `k_ai::laya::classify` / triagem antes do gate | `grep "mod laya\|k_ai::laya\|laya::classify"` = **vazio** | ✅ ausente (correto: gated #611) |

**ACEITOS (grep de presença):**

| Item aceito (§2.2/§9) | Evidência | Veredito |
|---|---|---|
| `TaskType::LayaIntent = 8` (protocolo só) | `noproto.rs:60` + teste `:300-302`; `grep LayaIntent crates/*.rs` = **exatamente 2 hits** (enum + teste) — sem produtores/consumidores | ✅ |
| `Decision<T>` + `abstain(ScorerAbsent)` | `decision.rs:58` (reason), `:159` (`abstain()`), `:253` (absent) | ✅ |
| Abstenção ≠ `Intent::Normal` (fail-closed) | `intent_decide.rs:171-174` Unknown→`choice=None`; `resolve_intent` → `Unknown` (comentário "Abstenção → LLM path; não escala HITL", `cortex.rs:6258`) | ✅ |
| BOOT_AI extensível (§9.4) | `boot_report.rs:32` (linha `BOOT_AI observe=…`) e `:238` (`publish_boot_ai`) | ✅ |
| Evidência obrigatória antes de ACCEPTED | `docs/evidence/laya-zero/` **AUSENTE** (exigido pelo §8) | ✅ correto |
| Lifecycle coerente | header da ADR = `PROPOSED (revisada)` / `por_fazer` = linha do `INDEX.md:118` | ✅ |
| Gates #611/#615/#616 | IDEA_BANK: #611 ⏳, #615 💡, #616 ⏳ — nenhum marcado feito | ✅ |

## 4. Desvios encontrados e corrigidos (3, todos de doc)

1. **IDEA_BANK #544** — a nota "Integração ADR-110" era **pré-revisão (SESSION_407)** e
   ainda afirmava: (a) que a ADR-110 reusa `hw_expert_v6` (chamado ainda de "modelo 3B
   1.58-bit"; o artefato real é **265.620 B / ~1M params**) como classificador System 1,
   e (b) o gate `TOTAL_RAM_MB >= 8192 && HEAP_FREE_MB > 200` — **ambos os dois items
   REJEITADOS pela ADR-0110 rev.** Corrigido: nota marcada REFUTADA com a rota vigente
   (#611, evidência ausente).
2. **IDEA_BANK #588** — gloss "110 decide o que classificar System 1 (Laya como
   **classificador hardware**)" pré-revisão; corrigido para "classificador de
   **intenção/triagem** fail-closed (§2.2/§9.5 rev.)", registrando que o reuso de
   `hw_expert_v6` foi rejeitado como erro de categoria.
3. **TECNOLOGIAS.md** — a superfície System-1 Laya/Jev (contrato `Decision<T>` +
   `decide_intent` + slot `LayaIntent`) **não tinha registro**, violando a regra 6 do
   AGENTS.md ("toda tecnologia nova registrada com ADR, IDEA, arquivo e sprint").
   Inserida a linha **9.7** com ADRs 0106/0110/0078 §2.2, IDEA #611, arquivos e status
   honesto (ADR-0110 segue PROPOSED/por_fazer; evidência ausente).

Registro do resultado: esta SESSION_500 + linha 500 no SESSION_INDEX + entrada
`[1.9.99-s500]` no CHANGELOG + `python tools/update_tecnologias.py` (regra 6 pós-Tec).

## 5. Verificações executadas

- Greps ausência/presença da §3 — todos nos valores esperados (exit 0 nas buscas com hit;
  buscas vazias = ausência confirmada).
- Pós-edição: `grep -c` = 1 para "REFUTADA pela revisão da ADR-0110" (IDEA_BANK),
  `| 9.7 |` (TECNOLOGIAS), `^| 500 |` (SESSION_INDEX), `1.9.99-s500` (CHANGELOG).
- `cargo test -p k-nano --lib net::noproto` — ver §6 (rodado após as edições).
- `python tools/update_tecnologias.py` — ver §6.
- Nenhum `.rs` alterado → nenhum build de kernel é pré-requisito desta auditoria;
  a working tree paralela (s499, BitFuzzy) foi **intocada**.

## 6. Honestidade e limitações

- **Classe = VERIFIED por inspeção estática**, não por runtime: nenhuma propriedade de
  comportamento foi medida porque **0 linhas de código mudaram** (não há o que medir).
  O teste `noproto` abaixo só reconfirma o já commitado em SESSION_408.
- **Fora de escopo**: os white papers externos (Laya-Zero fonte, spec JEV §5) — o
  alinhamento da spec §5 é a IDEA **#616** (⏳, "docs JEV" externo, não existe no repo).
- **n-sgdb**: sync MCP neural-sgdb indisponível neste cliente (mesma limitação da s490).
- **Metal/runtime**: N/A — sem mudança de código.
- **Pendências que a auditoria NÃO fecha**: #611 (destilação + dataset + gate ≥65% +
  ECE + `docs/evidence/laya-zero/`), #615 (AgentJev-0.6B como backend), #616.
  ADR-0110 só sai de `por_fazer` com evidência registrada (§7.4).
