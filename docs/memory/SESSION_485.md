# SESSION_485 — Passo 3 do hub: evict explícito Active ↔ (Pro|Agent) sob `heap_headroom_low()`

**Data:** 2026-10-17
**Continua:** s484 (Passo 0, validate_bitnet_blob + guard 8.3). Este é o **Passo 3** do plano @oracle
(evict explícito de 1 slot com `heap_headroom_low()`).

## 1. O que o Passo 3 é

O hub tem 5 slots pequenos residentes (~644MB) + o Active (falcon3 3B, ~1GB de tensores no heap).
Sob pressão de heap (`heap_headroom_low()` = headroom < 128MB), carregar um nicho grande
(GeneratorPro até 1.9GB / Agent 752MB) não cabe — e o padrão sem evict é o conhecido
OOM → alloc NULL → #PF → park (s415-420). O plano manda **um evict explícito** em vez do storm:
liberar o Active (maior residente) para o nicho entrar.

## 2. Descoberta que simplificou o passo

`clear_model()` (cortex.rs:5648) **já era o primitivo de evict do Active**: `CURRENT_MODEL=None` +
`MODEL_LOADED=false` + `EMBED_DIM=0` + `clear_model_header()` + `mark_active(false)`. O que faltava
era: footprint por slot (s482) voltar a `None`, o alias Pro não virar mentira, e o gate no ponto
de carga.

E `heap_headroom_bytes()` (allocator.rs:1043-1048) mede headroom **COMBINADO** (bump + TALC free
medido nos gap-nodes, s435) — ou seja, o evict libera exatamente o que o gate mede. Coerente.

## 3. Feito

1. **`evict_slot(slot) -> bool`** em `cortex/model_hub.rs` (primitivo público):
   - `Active` → `clear_model()` + `note_footprint(Active, 0)` + limpa o bit alias de
     GeneratorPro **se** Pro não tem blob próprio (senão o `hub_status` mostraria "alias"
     apontando para um Active morto). Sempre `warn`.
   - Slots do HUB (Vision/Pro/Reranker/Learner/Agent) → `HUB.lock().slots[i].take()` + `drop`
     (libera os tensores AGORA) + `mark(false)` + `note_footprint(0)` + `warn` com `freed~MB`.
   - `RustCoder`/`HwExpert` → `false` (statics legados com consumers próprios no MoE routing —
     sem evict, YAGNI, fora do plano).
   - Evict de slot vazio = `false` (nada a liberar; não finge sucesso).
   - Evict é DEGRADAÇÃO: sempre `warn`, nunca `ok` (mesma regra do fallback s480).
2. **Gate no bin** (`try_hub_slot_fat`, junto ao gate do Reranker do s483, onde `sz` é conhecido):
   `slot ∈ {GeneratorPro, Agent}` && `heap_headroom_low()` && `model_is_loaded()` →
   `evict_slot(Active)` com warn `hub evict Active p/ slot={} sz={}KB (headroom_low)`.
3. **Coerência do swap:** após o evict, o branch já existente "Active vazio e slot pro/fast vira
   primary" (main.rs ~5040) faz o Pro recém-carregado virar o CURRENT_MODEL — o swap
   Active↔Pro fica completo e o sistema continua com um gerador primário.

## 4. Direção única (honestidade)

O plano diz "Active ↔ Pro|Agent", mas **recarregar o Active em runtime não tem caminho hoje**
(o Active só carrega no boot). A direção implementada é a única real: sob pressão, o nicho
grande custa o Active. Depois do evict, `generate_via_model` cai no fallback dos geradores do
hub (Pro → Reranker) — degradação com warn explícito, não silêncio.

## 5. Prova

- `cargo test -p cortex --lib model_hub` → **1/1** (`evict_slot_libera_blob_e_limpa_estado`:
  register dummy no Agent → `slot_real` true → evict true → `slot_real` false + footprint `None`
  + evict de vazio false + experts false). DummyModel implementa os 4 métodos obrigatórios da
  trait (`generate`/`embed_dim`/`vocab_size`/`max_seq`); slot Agent escolhido porque nenhum
  outro teste toca `HUB[7]` (grep em cortex/src).
- `cargo check --release` → **exit=0** (0 erros).
- Crash `0xc0000096` da suite completa: pré-existente (provado 3× no s484, stash).

## 6. Runtime = UNKNOWN (mesmo bloqueio do s483/s484)

Sob QEMU `qemu_dev_skip_models` pula o bloco do hub inteiro — o gate nunca dispara em sandbox.
Aceite de runtime exige HW real ou gate liberado pelo dono. `IMPLEMENTED`, não `VERIFIED`
(ADR-0113).

## 7. Pendentes

- Aceite de runtime do evict (HW real/gate).
- Passo 5-6 do plano s481: podar `fat_names_for`/`slot_from_bitnet_bytes`/`load_models_multi`
  + `MICROPY.WASM` (peso morto, zero leitores).
- `#PF storm` sob falcon3 (pré-existente, IP migrante — watchpoint, não whack-a-mole).
- Direção B do evict (recarregar Active sob demanda): só quando existir caminho runtime para
  o Active — hoje é boot-only.
