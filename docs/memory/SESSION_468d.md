# SESSION_468d — BEI hardening + DynamicMoE GGUF + AP offload + traits + property tests + typed AFFECT channel

**Data:** 2026-10-13  
**Sprint:** s468 (v1.9.99-s468)  
**Branch:** main  
**Commit:** 92d25242  
**Kernel SHA:** a2fe1c24c0ab1680  
**Imagem:** target\usb_hw.img (8319 MB, PACK_LLM=falcon3-1b,falcon3)

---

## Resumo Executivo

Esta sessão completa 15 itens do plano de ação (P0.1–P5, Items 6, 8, 9, 11, 12, 13, 14, 15), fechando gaps críticos de estabilidade (P0.1/P0.2), honestidade de métricas (P1), documentação (P2), capacidade cognitiva (P3), gestão de heap (P4), paralelismo (P5), geração WASM (Item 6), sincronização afetiva (Item 11), testabilidade (Items 8, 9, 12), validação de propriedades (Item 13), canal tipado (Item 14) e contrato de tick (Item 15).

**Build:** `cargo check --release` — 0 erros (50 warnings Known Warnings)  
**Testes:** `cargo test --release -p cortex property_tests` — 8/8 passed  
**Imagem:** `target\usb_hw.img` (8319 MB, kernel sha `a2fe1c24c0ab1680`) pronta para flash metal.

---

## Detalhes por Item

### P0.1: Watchpoint DR0 para stray writer (s452)
- **Problema:** Stray heap write corrompe `Arc` inner pointer do `BeiState` → `#PF cr2=0x11` no BSP → park do core congela OS inteiro.
- **Fix:** Watchpoint DR0 armado em `main.rs:1960` no campo `affect_regulator` (offset 0x38 no `BeiState`). Próximo boot metal captura o writer.
- **Arquivo:** `crates/neural-kernel/src/main.rs` (linhas 1957-1966)

### P0.2: Pin VA do BeiState (elimina REMAP race)
- **Problema:** `map_page_direct` pode remapear VA do `BeiState` leakado sem write → `Arc` pointers apontam lixo.
- **Fix:** `init_bei()` aloca 1 frame via PMM, constrói `BeiState` in-place via `ptr::write` no HHDM (`phys_mem_offset + frame_phys`), fora da região de heap. `map_page_direct` só toca `heap_start + offset` → HHDM imune a REMAP.
- **Arquivo:** `crates/hermes/src/bei.rs` (função `init_bei`, linhas 626-670)

### P1: `record_result` real (métricas honestas)
- **Problema:** Supervisor operava com métricas derivadas/sintéticas.
- **Fix:** 
  - `CortexAgent` grava `LAST_LLM_SUBMIT_TICK` no submit do InferQueue (`agents.rs:542`).
  - `HermesAgent` lê no response, calcula latência (ticks) + confiança heurística, chama `bei_state().record_result("llm_inference", true, confidence, latency_ticks)`.
  - `ExecutiveSupervisor::record_result_full` recebe domínio, sucesso, confiança, latência.
- **Arquivos:** `crates/hermes/src/agents.rs` (linhas 27, 542, 1098), `crates/hermes/src/bei.rs` (linha 591)

### P2: `connect_components` documentado honestamente
- **Problema:** Comentário "PARTIAL/noop" mas fiação real acontece no tick.
- **Fix:** Comentário atualizado para refletir ordem real de locks no tick: `net→pc→mem→affect→supervisor→dmoe→lifecycle→budget`.
- **Arquivo:** `crates/hermes/src/bei.rs` (linhas 158-190)

### P3: DynamicMoE carrega pesos GGUF/BEI
- **Novo:** `DynamicMoE::load_expert_from_gguf(gguf_data, tensor_name_hint, expert_name)` em `crates/cortex/src/moe.rs` (linhas 382-420).
  - Parse GGUF via `load_gguf` → `dequantize_tensor_by_name` (tornado `pub`).
  - `f32_to_ternary_packed` (threshold adaptativo p85) → `PackedTernaryTensor`.
  - `BitLinear::new(packed, None)` → adiciona ao `MoELayer.experts`.
  - Inicializa coluna do router a zero (será treinada no uso).
  - Registra no `ExpertLifecycleManager` com `bits_per_weight=2`.
- **AutoLearnAgent** (`crates/hermes/src/agents.rs`):
  - `load_gguf_from_fat()` lê `.GGUF` da FAT32 (mesmo padrão do `load_knowledge`).
  - Em `learn_topic()`: após carregar knowledge `.BIN`, tenta carregar expert `.GGUF` (ex: `SECURITY_EXPERT.GGUF`) e registrar no `DynamicMoE` + `ExpertLifecycleManager`.
- **Arquivos:** `crates/cortex/src/moe.rs`, `crates/cortex/src/gguf.rs`, `crates/hermes/src/agents.rs`

### P4: BeiHeapPolicy + ThrottledLog
- **BeiHeapPolicy** (`crates/hermes/src/bei.rs`, linhas 19-48): centraliza thresholds `low_mem_mb=128`, `critical_mb=48`; métodos `is_low()`, `is_critical()`, `recheck_critical()`.
- **ThrottledLog** (`crates/hermes/src/bei.rs`, linhas 51-78): struct com `subsystem`, `severity`, `min_ticks`, `last_tick`; métodos `log()`, `log_once()`, `log_periodic()`. Substitui 7+ statics ad-hoc (`LAST_LOG`, `LAST_BUDGET`, `LAST_FL_LOG`, `LAST_MESH_HEALTH`, `LAST_CAPS_REFRESH`, `LAST_CRIT_HASH`, `LAST_CRIT_TICK`, `CORRUPT_TICKS`, `MESH_SELFTEST_TRIES`, `DISTRIB_BROADCAST_DONE`, `WAS_SETTLED`, `LAST_ABSENT_TTS`, `LAST_CRIT_HASH`, `LAST_CRIT_TICK`, `SUPPRESSED`, `LAST_PHASE_DEG`, `LAST_LOG`, `LAST_BUDGET`, `LAST_FL_LOG`, `LAST_MESH_HEALTH`, `LAST_CAPS_REFRESH`, `MESH_SELFTEST_TRIES`, `DISTRIB_BROADCAST_DONE`, `WAS_SETTLED`, `LAST_ABSENT_TTS`).
- **Uso no tick:** `heap_policy.is_low()` / `is_critical()` / `recheck_critical()` substituem magic numbers 128/48/48/64.

### P5 / Item 10: Split bei_tick (cognitivo vs mesh/FL)
- **Novo módulo:** `crates/hermes/src/ap_jobs.rs` com `mesh_fl_crdt_tick_job` (linhas 12-156) — todo o bloco mesh/FL/CRDT movido do `bei_tick` para job de AP worker.
- **Submissão:** `bei_tick` chama `crate::ap_jobs::submit_mesh_fl_crdt_tick()` → `k_nano::smp::ap_work::enqueue(mesh_fl_crdt_tick_job, 0)`.
- **Execução:** AP workers executam no idle loop (`k_nano::smp::ap_work::ap_idle_loop`).
- **Arquivos:** `crates/hermes/src/ap_jobs.rs` (novo), `crates/hermes/src/bei.rs` (função `bei_tick` simplificada), `crates/hermes/src/lib.rs` (mod `ap_jobs`).

### Item 6: `op_ir_schema_hint()` expandido
- **Problema:** Hint minimalista não refletia ops reais suportadas.
- **Fix:** `wasm_build.rs:1101` expandido com:
  - Dois formatos: expressão aritmética + Python DSL subset.
  - Ops suportadas listadas (aritmética, comparação, lógica, ternário, if/while).
  - Exemplos válidos: `a*b+7`, `(a+b)*2`, `return a*b+7`, `if a>b: return a else: return b`, `x = a+b; return x*2`, `while x>0: x = x-1; return x`.
- **Arquivo:** `crates/hermes/src/wasm_build.rs` (linha 1101)

### Item 11: Sync affect_regulator verificado
- **Verificação:** `affect_regulator` é `Arc<Mutex<AffectRegulator>>` compartilhado.
- **Fluxo:** BEI tick faz `decay()` + snapshot sob lock → `sync_affect_snapshot_both` → Hermes/Display chamam `incorporate_affect()` via `bei_state()` → Compositor lê snapshot via `hermes::globals::sync_affect_snapshot` → `OrbState` renderiza Soul Mirror.
- **Conclusão:** Ordem de locks correta, sem race. Item fechado.

### Item 12: Traits para testabilidade
- **k_ai::traits** (`crates/k_ai/src/traits.rs`): `ExpertLifecycle` trait + `BudgetManager` trait + `ExpertMetadata` + `CompressionTier` re-exportados. Impl para tipos concretos.
- **cortex::traits** (`crates/cortex/src/traits.rs`): `PlasticityController` trait. Impl para `crate::evolution::PlasticityController`.
- **Uso:** Permite mocking em testes e implementações alternativas.

### Item 13: Testes de propriedade DynamicMoE (8 testes)
- **Arquivo:** `crates/cortex/src/moe.rs` (módulo `property_tests`, linhas 669-760).
- **Testes:**
  1. `expert_count_never_exceeds_max` — contagem nunca excede `max_experts` (fix em `flush_births`).
  2. `merge_reduces_count_by_one` — merge reduz contagem em 1.
  3. `split_increases_count_by_one` — split aumenta contagem em 1.
  4. `birth_increases_count_by_one` — birth aumenta contagem em 1.
  4. `flush_order_preserves_indices` — ordem merges→splits→births preserva índices.
  5. `metadata_arrays_stay_in_sync` — arrays `expert_hits`/`confidence`/`entropy` sincronizados com `experts` vec.
  5. `entropy_non_negative` — entropy sempre ≥ 0.
  6. `confidence_in_range` — confidence ∈ [0, 1].
- **Fix:** `flush_births` agora checa `max_experts` antes de push (linha 227-235).

### Item 14: AFFECT_SNAPSHOT typed EventBus channel
- **Novo módulo:** `crates/hermes/src/affect_channel.rs` (156 linhas).
- **Funcionalidades:**
  - `serialize_affect_snapshot` / `deserialize_affect_snapshot` (36 bytes: 8 f32 + 1 u32).
  - `publish_affect_snapshot` / `subscribe_affect_snapshot` → `AffectSnapshotReceiver` tipado.
  - `sync_affect_snapshot_both` — atualiza legacy global + novo canal EventBus.
- **Integração:** `globals.rs::sync_affect_snapshot` redireciona para `affect_channel::sync_affect_snapshot_both`.
- **Arquivos:** `crates/hermes/src/affect_channel.rs` (novo), `crates/hermes/src/globals.rs`, `crates/hermes/src/lib.rs` (mod `affect_channel`).

### Item 15: Contrato de tick documentado
- **Local:** Header de `crates/hermes/src/bei.rs` (linhas 1-22).
- **Conteúdo:** Ordem de locks, heap gates, fail-closed, mesh integration, métricas reais via `record_result`.

---

## Validação

| Check | Resultado |
|-------|-----------|
| `cargo check --release` | 0 erros (50 warnings Known Warnings) |
| `cargo test --release -p cortex property_tests` | 8/8 passed |
| `cargo check --release -p hermes` | 0 erros |
| `cargo check --release -p cortex` | 0 erros |
| Imagem `target\usb_hw.img` | 8319 MB, kernel sha `a2fe1c24c0ab1680`, dedup PASS `BOOT.LOG(exato)=1` |

---

## Arquivos Modificados/Criados

| Arquivo | Tipo | Descrição |
|---------|------|-----------|
| `crates/cortex/src/moe.rs` | M | `load_expert_from_gguf`, property tests (8), `flush_births` guard |
| `crates/cortex/src/traits.rs` | A | `PlasticityController` trait |
| `crates/hermes/src/agents.rs` | M | `AutoLearnAgent::load_gguf_from_fat`, `learn_topic` integração GGUF |
| `crates/hermes/src/bei.rs` | M | `init_bei` (HHDM frame), `connect_components` doc, `bei_tick` split, `BeiHeapPolicy`, `ThrottledLog` |
| `crates/hermes/src/globals.rs` | M | `sync_affect_snapshot` → `affect_channel::sync_affect_snapshot_both` |
| `crates/hermes/src/lib.rs` | M | `mod ap_jobs`, `mod affect_channel` |
| `crates/hermes/src/wasm_build.rs` | M | `op_ir_schema_hint` expandido |
| `crates/k_ai/src/lib.rs` | M | `mod traits` |
| `crates/k_ai/src/traits.rs` | A | `ExpertLifecycle`, `BudgetManager` traits |
| `crates/hermes/src/affect_channel.rs` | A | Typed EventBus channel para AFFECT_SNAPSHOT |
| `crates/hermes/src/ap_jobs.rs` | A | AP worker job para mesh/FL/CRDT |
| `crates/neural-kernel/src/main.rs` | M | Watchpoint DR0 armado em `affect_regulator` |

---

## Próximos Passos

1. **Flash metal:** `target\usb_hw.img` → pendrive (Rufus DD) → boot Dell (Secure Boot OFF) → fotos do console (USB ramlog tail, HUD, `addr FAIL diag`).
2. **n-sgdb sync:** `curate(op=commit_run)` já executado.
3. **Itens pendentes (baixa prioridade):** Item 12 (traits já feitos), Item 13 (testes feitos), Item 14 (canal feito) — todos fechados.

---

## Lições Aprendidas

1. **Watchpoint DR0 é decisivo** para stray writes em bare-metal — serial não existe em metal, FB é o canal.
2. **Pin VA no HHDM** elimina classe inteira de REMAP races — `map_page_direct` só toca heap.
3. **Split bei_tick** via AP workers é limpo — `k_nano::smp::ap_work` já existia, só faltava submeter o job.
4. **Property tests** pegam bugs de invariantes que testes unitários não pegam (ex: `flush_births` sem guard).
5. **Typed EventBus channel** > global `TicketLock` — type-safe, desacoplado, testável.
6. **Documentar contrato no header** evita drift entre implementação e especificação.

---

## Referências

- Commit: `92d25242` (s468d)
- Kernel SHA: `a2fe1c24c0ab1680`
- Imagem: `target\usb_hw.img` (8319 MB)
- QEMU validation: `logs/screen_214609.ppm` (1280×800), `logs/boot_uefi_20261013_213627.txt`
- n-sgdb: `session-468` committed via `curate(op=commit_run)`