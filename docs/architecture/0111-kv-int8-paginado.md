# ADR-0111: KV-INT8 / paginado — cortar a memoria do KV e destravar contexto longo

**Data:** 2026-09-26
**Status:** Implemented P0-P3 (codigo + testes host, SESSION_414); medicao no lab = residual
**Lifecycle (INDEX):** `fazendo`
**IDEA:** **#613**
**Sprint / enquadramento:** s413 (aberto). Cadeia: ADR-0085 (formato v6) -> ADR-0101 (lab 3B) -> ADR-0057 WS-D/E (compute) -> #617 (Bonsai-8B como alvo Pro) -> este.
**Evidencia (medida):**
- KV do Falcon3-1B em ctx 4096 = `18L x 4 KV heads x 256 head_dim x 2 (K+V) x 4096 x 4 B` = **604 MB** — *maior que o proprio modelo* (`target1/FALCON3_1B.V6` = 544 MB). No 3B (22L) = ~738 MB.
- A atencao NAO e o gargalo do prefill de hoje: `LayerDiag` (s413, Falcon3-1B, prefill m=8) = `attn=0,29-0,44 s` de ~12,7-22,6 s (~2%), contra `mlp=9-11,7 s` (~55%). O KV pesa por **memoria** (e por contexto longo, onde `attn` cresce), nao pela velocidade do prefill curto.
- Ja existe base: `crates/cortex/src/kv_h2o.rs` tem `KvPages` (L112, `from_len`/`status`) e `h2o_evict` (L24, eviccao H2O). **Nao ha quantizacao.**

**Nao substitui:** ADR-0085 (formato dos pesos), ADR-0057 (compute), ADR-0101 (lab).
**Compoe com:** `kv_h2o.rs` (eviccao) e #617 (o Bonsai-8B em 16K exige KV <= ~5,5 GiB — sem INT8/paginacao nao cabe).

---

## 0. Decisao (ler primeiro)

1. **KV quantizado INT8 por grupo**, com escala f32 por bloco — a granularidade (por head / por 64 valores / por pagina) e decidida pelo teste de paridade (P1), nao por palpite.
2. **KV paginado** em blocos de tamanho fixo (alvo 64 B alinhados, estendendo o `KvPages` existente), para (a) alocacao nao-contigua, (b) eviccao por pagina (H2O ja existe), (c) contexto longo sem realocar.
3. **Dequant sob demanda no caminho da atencao** — nunca materializar o KV f32 inteiro de volta (senao o ganho de memoria evapora).
4. **Paridade e gate:** a atencao INT8 tem de bater com a f32 dentro de tolerancia **medida** (teste host com vetores reais), e o ganho de memoria tem de ser **medido**, nao estimado.
5. **Nao muda a matematica** do forward — e uma mudanca de **representacao + alocacao**; prefill/decode continuam os mesmos.
6. **Ordem:** memoria primeiro (o KV excede o modelo), velocidade depois. A atencao e ~2% do prefill de hoje — nao vender este ADR como ganho de tok/s.

## 1. Por que agora
O KV do 1B (604 MB) **excede o modelo** (544 MB) — e ele que limita o contexto e que compete com o modelo na janela bump. #617 (Bonsai-8B, 16K) exige KV <= ~5,5 GiB: sem INT8 + paginacao isso nao cabe.

## 2. Fases
| Fase | Entrega | Aceite |
|---|---|---|
| P0 | medir o KV **real alocado** (nao estimado) + `attn` por ctx (1K/2K/4K) | numeros registrados no SESSION |
| P1 | `KvCache` INT8 (quant/dequant + escala por bloco) | paridade host: `max abs(attn_int8 - attn_f32)` dentro da tolerancia; nenhum NaN |
| P2 | paginacao (blocos fixos) integrada ao `KvPages` | alocacao nao-contigua OK; eviccao por pagina (H2O) sem quebrar |
| P3 | integracao no prefill/decode + medicao | memoria do KV ~4x menor; `attn` sem regressao >5% |

## 3. Alternativas descartadas
- **f16 no KV:** 2x — insuficiente para o Bonsai 16K, e nao ajuda a paginacao.
- **So H2O (eviccao):** ja existe; nao corta a representacao, so **descarta** contexto — perde informacao.
- **Nada:** bloqueia #617 e limita o ctx.

## 4. Riscos
- **Dequant no hot path** pode custar CPU (alvo soft-float): medir antes de aceitar. O alvo deste ADR e memoria, nao velocidade.
- **Escala por bloco grosso** pode degradar a atencao em ctx longo: a tolerancia de P1 decide a granularidade.
- O `KvPages` atual e **contagem** de paginas, nao storage paginado — P2 estende o tipo, nao o substitui.

## 5. Referencias
- `crates/cortex/src/kv_h2o.rs` (`KvPages`, `h2o_evict`), `crates/cortex/src/infer_queue.rs` (`KvCache`), `crates/cortex/src/cortex.rs`
- ADR-0085 (v6), ADR-0101 (lab 3B), ADR-0057 (compute), IDEA #613/#614/#617
