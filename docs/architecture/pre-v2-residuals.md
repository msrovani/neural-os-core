# ADR-PRE-V2: Resíduos Pós-v1.9 — Itens Não Implementados Transportados das ADRs

**Data:** 2026-07-26
**Status:** Proposed — coleção de itens residual de ADRs auditadas que não foram implementados ou estão incompletos, transferidos aqui para planejamento pós-v2.0.0 gate.
**Origem:** Auditoria ADR decrescente SESSION_220 (0075 → 0001)

---

## 1. ADR-0064 — VectorStore TF-IDF (RAG in-kernel)

**Fonte:** `docs/architecture/0064-rag-db-in-kernel.md`
**Lifecycle original:** `por_fazer`

Item não implementado. A crate `crates/vector-db` nunca foi criada. O VectorStore TF-IDF com cosine similarity (inspirado no ClaudioOS `vectordb.rs`) seria a camada L1 lexical de RAG on-device, persistindo via ADR-0063 TicKV `vdb/*`.

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| F1 | crate `vector-db` — VectorStore, VectorEntry, tokenize EN+PT-BR, tfidf, cosine, ln_f32, sqrt_f32, demo() | ~1.000 LOC |
| F2 | Serialização JSON + persistência VFS FAT32 | ~300 LOC |
| F3 | Integração Cortex/Trinity — search antes de LLM, insert pós-resposta | ~200 LOC |
| F4 | Integração Hermes — RAG para skills | ~150 LOC |
| F5 | Thread-safety spin::Mutex + multi-store | ~100 LOC |
| F6 | Embeddings neurais (MiniLM) gated por VRAM — residual do residual | — |

**Total estimado:** ~1.750 LOC

---

## 2. ADR-0060 A.3 — Dynamic MoE (Birth/Merge/Split)

**Fonte:** `docs/architecture/0060-bitnet-cognitivo-bei.md` §A.3
**Lifecycle original:** `fazendo`

O Trinity MoE atual (`cortex::moe`, `trinity.rs`) é estático — 3 experts fixos (hw_control, generator, etc.) com router_weight treinável. Dynamic MoE exigiria:

| Subitem | Descrição | Esforço |
|---------|-----------|---------|
| Birth | Criar novo expert em runtime baseado em necessidade detectada | ~500 LOC |
| Merge | Fundir dois experts similares (similaridade > threshold) | ~400 LOC |
| Split | Dividir expert sobrecarregado em dois especialistas | ~400 LOC |
| Lifecycle | Registrar/monitorar birth/death de experts, budget-aware | ~300 LOC |

**Total estimado:** ~1.600 LOC

---

## 3. ADR-0048/0049/0050 — GPU Compute Golden HW

**Fonte:** `docs/architecture/0048-nvidia-compute-multigeracao.md`, `0049-amd-compute-multigeracao.md`, `0050-intel-compute-multigeracao.md`
**Lifecycle original:** `fazendo` (todos)

Código dos 3 vendors implementado (ACR/NKP/Discovery/GuC) mas **não validado em HW real**:

| Vendor | Status | Pendente |
|--------|--------|----------|
| NVIDIA Pascal | ACR ✅, D2-D4 ✅, dispatch CUBIN ✅ | Validação HW real GTX 1050 |
| AMD RDNA | Discovery ✅, PSP ✅, KIQ/MES ✅ | Validação HW real gfx1030+ |
| Intel Gen9/Arc | Ring alive ✅, GuC boot ✅, COMPUTE_WALKER ✅ | Validação HW real iGPU/dGPU |

**Bloqueio:** AWAITING_HW — todos dependem de silício real para canário final.

---

## 4. ADR-0041 §Ring3/SFI — Isolamento de Produção

**Fonte:** `docs/architecture/0041-k2chj-capability-rings.md` §P09
**Lifecycle original:** `fazendo`

Ring3/SFI PoC existe (P09) mas não é isolamento de produção. Necessário para B/C do ADR-0059 (Cranelift JIT, Rust-subset nativo).

| Subitem | Descrição |
|---------|-----------|
| Ring3 iretq | syscall handler com stack switch |
| Paging UVM | User page tables + #PF demand-page |
| CapGate HW | MMIO/DMA bloqueado em Ring3 |

**Bloqueio:** Pós-v2.0.0 — item mais complexo (est. ~3.000 LOC).

---

## 5. ADR-0057 Layer S/HW — NPU + GPU Full Dispatch

**Fonte:** `docs/architecture/0057-compute-dispatch-smp-gpu-npu.md` §WS-D/WS-E
**Lifecycle original:** `completa` (gated honestamente)

GPU dispatch existe mas é **gated** — só executa se canário Ready PASS. NPU é detection-only.

| Subitem | Descrição |
|---------|-----------|
| GPU full | GPU como backend primário de matmul (não fallback) |
| NPU XDNA/Intel | Driver + dispatch real (não só detection) |

**Bloqueio:** HW real (GPU Ready + NPU detection).

---

## 6. ADR-0033 — On-Device Micro-Learning (Dynamic MoE)

**Fonte:** `docs/architecture/0033-on-device-micro-learning.md`
**Lifecycle original:** `modernizada`

AutoLearnAgent existe (SESSION_108) e faz fine-tuning on-device. Mas falta o **MoE dinâmico propriamente dito** — birth/merge/split de especialistas em runtime (mesmo que item 2 acima).

---

## 7. ADR-0054 — Perci/Bitwork Integration

**Fonte:** `docs/architecture/0054-perci-bitwork-integration.md`
**Lifecycle original:** `pesquisa` (adiada)

Perci Bitwork é arquitetura associativa binária (não transformer). Viabilidade depende de OK do maintainer pós-Net RX estável.

---

## Resumo

| Prioridade | Item | ADR | Esforço est. | Bloqueio |
|-----------|------|-----|-------------|----------|
| 🔴 Alta | VectorStore TF-IDF | 0064 | ~1.750 LOC | Nenhum (puro algoritmo) |
| 🔴 Alta | Dynamic MoE birth/merge/split | 0060 A.3 | ~1.600 LOC | Arquitetural |
| 🟡 Média | GPU golden HW validation | 0048/49/50 | — | AWAITING_HW |
| 🟡 Média | Ring3/SFI produção | 0041 P09 | ~3.000 LOC | Pós-v2.0.0 |
| 🟢 Baixa | NPU driver full | 0057 WS-E | — | HW real |
| 🟢 Baixa | Perci/Bitwork | 0054 | pesquisa | Maintainer OK |

---

## Referências

- SESSION_220 — auditoria ADR decrescente
- Commit `5ea319a` (BitNet recommendations)
- Commit `7a5e0a7` (UI WM fixes)
- Commit `0fdf20e` (ADR-0065 FASE 2.2+3.2)
- Commit `289339c` (ADR-0065 FASE 1.1-3.1)
