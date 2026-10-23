# 📋 TODO — neural-os-core

**Versão:** v1.9.99-s501 TEST
**Data:** 2026-10-22
**Fonte:** SESSION_501 / SESSION_500 / STATE.md + ADR-0117/0118 + forum HUMAN-0010
**Retomada:** docs/memory/RESUME.md (runbook único — comandos do F1.5, decisões D1–D5, mapa de donos da working tree, armadilhas medidas, UNKNOWN)
**Legenda:** ✅ feito | 🟡 em andamento | [~] parcial | 🔴 bloqueado | ⏳ agendado | ▶️ AWAITING_HW | [ ] pendente

---

## ✅ s501 — A1 claims/drift/governança + NixAIOS bancada (HUMAN-0010)

| item | status |
|------|--------|
| A1 escrita docs/tools autorizada (sem crates/, sem commit auto) | ✅ |
| NixAIOS = bancada (não produto) até nova decisão HUMAN | ✅ |
| `CLAIMS.yaml` + `tools/check_claims.py` (gate + plant demo) | ✅ |
| `measure_repo` markers 758/232910 | ✅ |
| `check_duplication.py` cobre nixaios (exclusões bancada) | ✅ |
| SESSION_488 recriada + SESSION_501 | ✅ |
| TODO sem cópia mojibake s451 | ✅ |
| gitignore `store/` `volume/` `models/*.misname_*` | ✅ |
| P0 funil: F1.5 recall em dev / #PF determinístico | [ ] (não é A1) |

---

## 🟡 s499 — BitFuzzy-MoE Recurrent Engine (ADR-0117): pipeline host-side de 6 fases + loader/forward kernel (host-side)

| item | IDEA/ADR | criterio de aceite | status |
|------|----------|--------------------|--------|
| F1 — pruning angular (ShortGPT) do teacher denso | #706 / ADR-0117 | exercitado em modelo REAL (hoje só modelo SINTÉTICO de 2 camadas) | 🟡 |
| F2 — recorrência de profundidade (1 bloco compartilhado, K passos) | #706 / ADR-0117 | gradiente do bloco vivo (bug do ACT halting — que descartava a saída do passo halting — já corrigido) | 🟡 |
| F3 — MoE fuzzy (pertinência + T-norm + top-k + roteamento ternário) | #706 / ADR-0117 | `--selftest` verde + comportamento de roteamento medido | 🟡 |
| F4 — fuzzy STE + Sugeno BitLinear | #706 / ADR-0117 | paridade de forward com o STE padrão (mesma aritmética) | 🟡 |
| F5 — destilação (KL+CE+esparsidade) + latent thought | #706 / ADR-0117 | corrida REAL em CUDA (VRAM estimada 23,84 GB) | 🔴 sem GPU neste host (torch 2.13 CPU-only) |
| F6 — low-rank A×B nos experts + congelamento ternário | #706 / ADR-0117 | roda sobre student treinada; footprint medido (<500 MB só fecha aqui) | 🔴 depende de F5 |
| Exportador bare-metal `tools/export_no_std.py` (v6 `model_type=3` + header C) | #706 / ADR-0117 | roundtrip + paridade de logits | ✅ **feito (host-side)** — `--selftest` 7/7 (paridade byte-a-byte do header vs writer v6, roundtrip, logits max\|delta\| 2,384e-07, header C monotônico, roteamento int16); header C compila limpo em `gcc -fsyntax-only -Wall -Wextra -std=c99` |
| Loader Rust / suporte `cortex` para a arquitetura nova (`model_type=3`) | #706 / ADR-0117 | loader que parseia header v6 (52 B) + bloco de config (20 B) + corpo sequencial e bate com a referência Python no mesmo container (mesmos logits) | ✅ **feito (host-side)** — `crates/cortex/src/bitfuzzy.rs` (loader/parser por UM `plan()` **+ forward** recorrente: K passos sobre UM bloco compartilhado + MoE fuzzy + Sugeno + halting ACT); `model.rs` = `ModelKind::BitFuzzy`/`ModelView::BitFuzzy`/`as_bitfuzzy()`/braço `3 =>`; guard fail-closed no `load_model` legado (recusa `version>=6` com `data[14]!=0`); `model_hub` **descreve mas não registra**. **Aceite = paridade VERIFIED host-side**: max_abs 2,533197e-07 / max_rel 8,959963e-05 (tolerância 1e-04 **não aliviada**; o max_rel estrito é dominado por 1 logit de referência quase-zero `\|ref\|=1,256407e-03` — a métrica significativa sobre `\|ref\|>1e-2` é 6,333679e-06); mutações materiais provam que o gate não é vazio (expert gate 1,279962e-02; embed 1,953993e-01); `cargo check --release -p cortex` 0 erros; `cargo build --release` 0 erros no membro bare-metal. ⚠️ **NÃO usável pelo boot** (hub recusa) e **nunca rodou em bare metal** |
| Integração hub/boot — como o boot consome um modelo **recorrente** (e não um `TransformerModel`) | #706 / ADR-0117 | decisão de design (slot, agente, política de inferência) + bfz registável sem mentir sobre o tipo | [ ] pendente |
| Student **treinado** + medição de qualidade (suite/perplexidade/KL) | #706 / ADR-0117 | corrida real em GPU (F5/F6) e números medidos sobre o student treinado (hoje o golden tem pesos aleatórios) | 🔴 depende de F5 (sem GPU neste host) |
| Validação do forward bfz em QEMU/metal | #706 / ADR-0117 | boot que carrega um bfz e logits medidos no guest (hoje a prova é **só host-side**) | [ ] pendente |
| Caminho de VRAM externa para o contentor de escala 3B (pesos + escalas por linha em BAR) | #706 / ADR-0117 | loader que não copia tudo para a RAM (ponteiros BAR próprios, alinhado com a ADR-0112) | [ ] pendente |
| Decisão do maintainer: onde roda o treino real (GPU própria vs cloud) | #706 / ADR-0117 | decisão registrada (orçamento; exige CUDA ≥ 24 GB, preferir ≥ 40 GB) | ⏳ decisão do maintainer |
| Decisão do maintainer: formato de saída (v6 `model_type=3` vs engine C externo) | #706 / ADR-0117 | decisão registrada (define o custo de kernel) | ⏳ decisão do maintainer |
| Decisão do maintainer: orçamento de perda de qualidade na ternarização sem calibração QAT | #706 / ADR-0117 | número de suite/perplexidade aceito | ⏳ decisão do maintainer |
| Eixo de memória de KV (604 MB > 544 MB, SESSION_413) | — | ADR própria (SSM/Falcon-H1) — **fora** deste pipeline (recorrência reduz parâmetros, não KV) | ⏳ frente separada |

---

## 🟢 s468 — BEI hardening + DynamicMoE GGUF + AP offload + traits + property tests + typed AFFECT channel

- [x] **P0.1/P0.2**: Watchpoint DR0 armed on ffect_regulator (s452); BeiState pinned to fixed HHDM frame via PMM — eliminates REMAP race in map_page_direct.
- [x] **P1**: 
ecord_result real metrics — CortexAgent stores LAST_LLM_SUBMIT_TICK on submit; HermesAgent computes latency (ticks) + heuristic confidence on LLM_RESPONSE, calls ei_state().record_result().
- [x] **P2**: connect_components documented honestly — lock order 
et→pc→mem→affect→supervisor→dmoe→lifecycle→budget; "PARTIAL/noop" warning removed.
- [x] **P3**: DynamicMoE loads GGUF experts — load_expert_from_gguf() parses GGUF, dequantizes tensor by name, converts to ternary via 32_to_ternary_packed, registers BitLinear in MoELayer + ExpertLifecycleManager (bits=2). AutoLearnAgent loads expert GGUF from FAT32 on learn_topic().
- [x] **P4**: BeiHeapPolicy centralizes thresholds (low=128MB, critical=48MB); ThrottledLog trait replaces 7+ ad-hoc statics (LAST_LOG, LAST_BUDGET, etc.).
- [x] **P5/Item 10**: ei_tick split — mesh/FL/CRDT offloaded to AP workers via k_nano::smp::ap_work::enqueue; new p_jobs.rs with mesh_fl_crdt_tick_job.
- [x] **Item 6**: op_ir_schema_hint() expanded with real ops + examples; model_skill_prompt uses it.
- [x] **Item 11**: ffect_regulator sync verified — Arc<Mutex> shared, BEI decay+snapshot under lock, Hermes/Display incorporate_affect, compositor reads snapshot.
- [x] **Item 12**: Traits for testability — k_ai::traits (ExpertLifecycle, BudgetManager), cortex::traits (PlasticityController).
- [x] **Item 13**: DynamicMoE property tests (8) — expert count ≤ max, merge/split/birth counts, metadata sync, entropy ≥ 0, confidence ∈ [0,1]; lush_births guards max_experts.
- [x] **Item 14**: AFFECT_SNAPSHOT typed EventBus channel — ffect_channel.rs with serialize/deserialize, typed publish/subscribe, backward-compat sync_affect_snapshot_both.
- [x] **Item 15**: ei_tick contract documented in ei.rs header — lock order, heap gates, fail-closed, mesh integration, metrics.
- [x] **Build**: cargo check --release 0 errors; cargo test -p cortex property_tests 8/8 passed.
- [x] **Image**: 	arget\usb_hw.img (8319 MB, kernel sha 2fe1c24c0ab1680) ready for metal flash.
- [ ] **Metal validation**: Flash 	arget\usb_hw.img → boot Dell (Secure Boot OFF) → capture console + HUD photos → report results.

---
