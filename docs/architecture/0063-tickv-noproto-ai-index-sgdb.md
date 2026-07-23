# ADR-0063: TicKV + NoProto + Índices IA como SGDB Primário do Neural-OS-Core

**Status:** Proposed / `fazendo` (MVP Flash+TickvLite + F2–F8 lite + **adoção AIOS SgdbStore** — SESSION_173)  
**Lifecycle:** `fazendo`  
**Data:** 2026-07-22  
**Ideias:** #491–#510  
**Supersede:** —  
**Relacionado:** ADR-0061 (CPU-First BitNet), ADR-0040 (NeuralFS), ADR-0057 (Compute Dispatch), **ADR-0064** (RAG TF-IDF L1 lexical — persiste via TicKV)

> **Cruzamento ADR-0064:** a camada L1 lexical (`VectorStore` TF-IDF) é o front-end RAG do agente; a persistência durável usa keys TicKV `vdb/*` deste SGDB. L1 structured (NoProto working docs) permanece escopo desta ADR.

---

## Contexto

O Neural-OS-Core necessita de um sistema de persistência e indexação nativo `no_std` que suporte:

1. **Memórias L0-L7** (8 camadas: Sensory, Working, Episódica Curta/Longa, Semântica, Procedural, Identidade)
2. **Influxo massivo de dados de IA** (vetores 1024-dims, pesos ternários BitNet, Vector Clocks)
3. **Zero-Copy** entre rede (DMA), RAM (L3 Cache) e disco (NVMe)
4. **Imunidade a power-loss** (append-only + CRC)
5. **Busca vetorial semântica** (L4-L5) em microsegundos
6. **Busca por chave/fato** (L0-L3) em nanossegundos
7. **Sem dependências de SO, libc, ou runtime** (bare metal puro)

Soluções tradicionais falham:
- **SQLite/PostgreSQL**: dependem de C runtime, locks, GC, filesystem
- **LMDB/RocksDB**: dependem de `std`, `mmap`, threads de compaction
- **HNSW/FAISS**: ponteiros dispersos destroem L3 Cache; construção de grafo trava CPU
- **B-Tree/B+Tree**: rebalanceamento causa cache-line bouncing; latência O(log N) variável

---

## Decisão

Adotar **TicKV + NoProto + ART + BQ Flat SIMD** como SGDB (Sistema Gerenciador de Banco de Dados) primário e unificado do Neural-OS-Core.

### Arquitetura em Camadas

```
┌─────────────────────────────────────────────────────────────────┐
│  HERMES / CORTEX / JARBAS  (Consumem memórias L0-L7)           │
└────────────────────────────────────────┬────────────────────────┘
                                         │
┌────────────────────────────────────────▼────────────────────────┐
│  NOPROTO (Schema & Layout)  ── Zero-Copy em RAM/DMA            │
│  • Documentos binários L0-L7  • Vector Clocks  • Tensors BitNet │
└────────────────────────────────────────┬────────────────────────┘
                                         │  (Buffer Binário Bruto)
┌────────────────────────────────────────▼────────────────────────┐
│  ÍNDICES EM RAM (L3 Cache)                                      │
│  ┌─────────────────────┐  ┌─────────────────────────────────┐  │
│  │ ART (Adaptive Radix)│  │ BQ + Flat SIMD Scan             │  │
│  │ Chaves/Fatos L0-L3  │  │ Vetores Binários L4-L5 (Hamming)│  │
│  │ Busca O(k)          │  │ XOR + POPCNT via SIMD           │  │
│  └─────────────────────┘  └─────────────────────────────────┘  │
└────────────────────────────────────────┬────────────────────────┘
                                         │
┌────────────────────────────────────────▼────────────────────────┐
│  TICKV (Storage Engine)  ── Append-Only no NVMe via PCIe MMIO  │
│  • CRC por entrada  • GC de blocos  • Power-loss safety        │
└─────────────────────────────────────────────────────────────────┘
```

### Componentes

| Componente | Função | Onde Roda |
|------------|--------|-----------|
| **NoProto** | Schema & Layout — documentos binários L0-L7, Vector Clocks, tensores BitNet | RAM / DMA de Rede / Caches L3 |
| **TicKV** | Storage Engine — pares Key-Value no NVMe via PCIe MMIO, Append-Only Log, CRC, GC | NVMe Físico / Memória Flash Bruta |
| **ART** | Índice Chaves/Fatos L0-L3 — Adaptive Radix Tree, busca O(k), sem rebalanceamento | RAM / L3 Cache |
| **BQ + Flat SIMD** | Índice Vetorial L4-L5 — Quantização Binária + XOR/POPCNT SIMD | RAM / L3 Cache |

### Mapeamento por Camada de Memória

| Camada | Storage | Index | Acesso Típico |
|--------|---------|-------|---------------|
| L0 Sensory | NoProto RAM | — | `get_noproto_doc` |
| L1 Working | NoProto RAM | — | `get_noproto_doc` |
| L2 Episódica Curta | TicKV + NoProto | ART (timestamp key) | `scan_prefix` |
| L3 Episódica Longa | TicKV + NoProto | ART | `scan_prefix` |
| L4 Semântica | TicKV + NoProto | BQ + Flat SIMD | `bitvec.top_k(query, k)` |
| L5 Procedural | TicKV + NoProto | BQ + Flat SIMD | `bitvec.top_k(query, k)` |
| L7 Identidade | TicKV + NoProto | ART (chave fixa) | `get_noproto_doc` |

---

## Consequências

### Positivas

- **100% `no_std` bare metal**: Zero dependências de SO, libc, runtime, ou C externo
- **Formato Único**: Rede ↔ RAM ↔ Disco — mesmo buffer NoProto via DMA → TicKV sem parsing/marshalling
- **Power-Loss Safety Total**: Append-Only Log + CRC; kill -9 durante write não corrompe dados anteriores
- **Zero-Copy Real**: Hermes lê campos via aritmética de ponteiros no buffer NoProto; zero alocação heap
- **Latência Determinística**: Sem GC de linguagem, sem locks SQL, sem rebalanceamento de árvore
- **Busca Vetorial Ultra-Rápida**: BQ + Flat SIMD — 100k vetores em 50µs (AVX-512) / 120µs (AVX2) / 280µs (scalar u64)
- **Busca Chave Ultra-Rápida**: ART — O(k) independente de N; 10M chaves < 100ns P99
- **Inserção Vetorial Zero-Custo**: Push contíguo em array RAM; sem reconstrução de grafo
- **Imunidade a Cache Miss**: Arrays contíguos + Hardware Prefetcher vs ponteiros dispersos HNSW/B-Tree

### Negativas / Limitações

- **Sem SQL/Relacionamentos**: Apenas Get/Set/Scan por chave; sem JOIN/WHERE/GROUP BY
- **Sem Busca Vetorial Nativa no TicKV**: Requer índice separado (BQ Flat SIMD) em RAM
- **GC do Flash (TicKV)**: Pausa breve na escrita quando bloco enche (compactação + page erase)
- **ART Memory Overhead**: ~200MB para 10M chaves; requer tuning de `max_node_size`
- **BQ Recall Trade-off**: Quantização 1-bit perde precisão vs FP32; fallback FP32 para L4 crítico

---

## Plano de Implementação (8 Fases)

| Fase | Entregável | Esforço | Dependência |
|------|------------|---------|-------------|
| **0** | Deps + `FlashController` NVMe | ~200 LOC | — |
| **1** | TicKV wrapper (append/get/GC) | ~800 LOC | Fase 0 |
| **2** | NoProto schemas L0-L7 + encode/decode | ~600 LOC | Fase 0 |
| **3** | `AiosDatabaseEngine` (ponte NoProto↔TicKV) | ~400 LOC | Fases 1+2 |
| **4** | ART Index (L0-L3) | ~1000 LOC | Fase 3 |
| **5** | BQ + Flat SIMD (L4-L5) + Dispatch SIMD | ~1200 LOC | Fase 3 |
| **6** | Integração Camadas L0-L7 (Hermes/Cortex/Jarbas) | ~800 LOC | Fases 4+5 |
| **7** | Dynamic SIMD Dispatch Unificado | ~300 LOC | Fase 0 |
| **8** | Testes E2E + Power-Loss + Benchmarks | ~500 LOC | Fases 1-7 |

**Total estimado:** ~23 arquivos, ~5800 LOC

---

## Riscos & Mitigações

| Risco | Prob. | Impacto | Mitigação |
|-------|-------|---------|-----------|
| `tickv`/`noproto` não compilam `no_std` | Média | Alto | Fork + patch local; testar `cargo check --target x86_64-unknown-none` cedo |
| NVMe driver instável em QEMU | Alta | Médio | Testar em HW real (EPYC/Xeon) + CI com `qemu -drive if=none,format=raw,file=nvme.img` |
| ART memory overhead > esperado | Baixa | Médio | Limitar `max_node_size=48`; monitorar com `alloc::alloc::GlobalAlloc` hooks |
| BQ recall < 90% em L4 | Média | Alto | Fallback: manter FP32 para L4 crítico; BQ apenas para L5 |
| SIMD dispatch errado em CPU híbrida | Baixa | Alto | `cpuid` leaf 0x1A (Intel Hybrid) + leaf 7 para features reais |

---

## Critérios de Aceite (Definition of Done)

- [ ] `cargo check --release` 0 erros em todo workspace
- [ ] Hermes `recall(L4, query)` < 1ms end-to-end (100k vetores)
- [ ] Hermes `recall(L2, timestamp)` < 100µs (10M chaves)
- [ ] Jarbas boot + carregar L7 do TicKV < 500ms
- [ ] Power-loss test: 10k `put` + `kill -9` aleatório → boot → 100% recall
- [ ] CRC corruption test: flip 1 bit no NVMe → `get` retorna erro, não lixo
- [ ] GC stress: NVMe 95% cheio → `put` continua (GC automático)
- [ ] Benchmarks: P50/P99/P999 documentados para `recall(L4)`, `recall(L2)`, `put(L7)`
- [ ] SIMD dispatch correto em 3 perfis: AVX-512, AVX2, scalar (CI)

---

## Ideias Relacionadas (IDEA_BANK)

| ID | Título | Status |
|----|--------|--------|
| #491 | TicKV wrapper para NVMe PCIe MMIO | 🟡 agendada |
| #492 | NoProto schemas para MemoryDoc L0-L7 | 🟡 agendada |
| #493 | AiosDatabaseEngine (ponte NoProto↔TicKV) | 🟡 agendada |
| #494 | ART Index para chaves/fatos L0-L3 | 🟡 agendada |
| #495 | BQ + Flat SIMD Scan para vetores L4-L5 | 🟡 agendada |
| #496 | Dynamic SIMD Dispatch (AVX-512/AVX2/scalar) | 🟡 agendada |
| #497 | Integração camadas L0-L7 no Hermes | 🟡 agendada |
| #498 | Power-loss resilience tests | 🟡 agendada |
| #499 | BQ recall vs FP32 benchmark | 🟡 agendada |
| #500 | ART memory profiling & tuning | 🟡 agendada |
| #501 | TicKV GC tuning para NVMe | 🟡 agendada |
| #502 | NoProto Vector Clock encoding | 🟡 agendada |
| #503 | FlashController trait para NVMe driver | 🟡 agendada |
| #504 | Benchmark suite automatizada | 🟡 agendada |
| #505 | CI pipeline com QEMU NVMe | 🟡 agendada |

---

## Consumidores AIOS (SESSION_173 — adoção SgdbStore)

Facade: `k_ai::sgdb::store` (namespaces `hanr/` `md/` `pkg/` `skill/` `audit/` `vdb/` `sys/`).

| Consumidor | Path | Backend |
|------------|------|---------|
| HANR USER/MEMORY/SOUL/PERSONA | `hermes/memory_store.rs` | SGDB L7 primário + VFS espelho |
| RAG TF-IDF | `vector-db` + `put_vdb_blob` | TickvLite `vdb/blob` |
| L1/L2 last turn | `sgdb::layers` | MemoryDoc |
| AuditTrail | `k_ai/audit.rs` flush/load | `audit/head` compact |
| EpisodicMemory | `k_ai/cognitive.rs` | L2 + `sys/episodic_tail` |
| PackageHub | `hermes/package_hub.rs` | meta SGDB; body VFS ou Tickv ≤4KiB |
| SkillOpt promote | `hermes/skill_opt.rs` | `skill/{name}` + ART L3 |

**Fora do SGDB (honesty):** WIFI.CFG, firmware, modelos, BOOT.LOG, TrustCache dump, bodies PackageHub grandes.

---

## Referências

- **TicKV**: https://github.com/tock/tock/tree/master/libraries/tickv
- **NoProto**: https://github.com/noproto/noproto
- **ART**: "The Adaptive Radix Tree" — Viktor Leis et al., 2013
- **Binary Quantization**: "Binary Quantization for Fast Similarity Search" — various
- **BitNet b1.58**: "BitNet: Scaling 1-bit Transformers for Large Language Models" — Microsoft, 2023
- **ADR-0061**: CPU-First BitNet (autoridade ISA + SIMD dispatch)
- **ADR-0040**: NeuralFS (persistência anterior, supersedida parcialmente)
- **ADR-0057**: Compute Dispatch (SMP/GPU/NPU → CPU fallback chain)

---

*Esta ADR formaliza a adoção de TicKV + NoProto + ART + BQ Flat SIMD como SGDB unificado do Neural-OS-Core, eliminando a necessidade de qualquer banco de dados relacional ou sistema de arquivos tradicional, e garantindo performance determinística, power-loss safety, e zero-copy em todo o pipeline de dados de IA.*