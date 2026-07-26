# ADR-IDEIAS-V2: Catálogo de Ideias Pós-v1.9 — Viabilidade, Aderência, Custo e Resultado Esperado

**Data:** 2026-07-26
**Status:** Proposed — coleção de ideias do IDEA_BANK auditadas, classificadas por viabilidade e aderência ao gate v2.0.0
**Origem:** Auditoria completa IDEA_BANK.md (336+ ideias, 1786 linhas) + ADR-PRE-V2 (itens não implementados)
**Sessão:** SESSION_220

---

## 0. Metodologia de Classificação

Cada ideia foi classificada em 4 dimensões:

| Dimensão | Escala | Descrição |
|----------|--------|-----------|
| **Viabilidade** | 🔵 Trivial · 🟢 Fácil · 🟡 Média · 🔴 Difícil · ⚫ Inviável | Complexidade técnica pura |
| **Aderência** | 🎯 Core · 📐 Arcabouço · 🧩 Extensão · 🌌 Visão | Quão central é para o produto K³CHJ |
| **Custo** | ☕ 1h · 🍕 4h · 🗓️ 1-3d · 📆 1-2sem · 🏗️ 1m+ | Esforço estimado (LOC/homem) |
| **Resultado** | Descrição objetiva do que se ganha | |

**Destino:**
- **v2.0** = deve estar pronto para o gate v2.0.0
- **v2.1+** = pós-gate, mas planejado
- **📌 pre-v2** = já no ADR-PRE-V2
- **🗑️ descartar** = não aderente/obsoleto
- **💤 hibernar** = viável mas sem HW/dependência

---

## 1. Itens Já no ADR-PRE-V2 (Transportados da Auditoria ADR)

| # | Ideia | Origem | Viab | Ader | Custo | Resultado |
|---|-------|--------|------|------|-------|-----------|
| 486 | **VectorStore TF-IDF** (RAG in-kernel) | ADR-0064 | 🟢 | 🎯 | 🗓️ 1-3d | RAG on-device sem MCP externo. 1.750 LOC |
| A.3 | **Dynamic MoE** (birth/merge/split) | ADR-0060 | 🟡 | 🎯 | 📆 1-2sem | Experts que nascem/morrem em runtime. 1.600 LOC |
| — | **GPU golden HW validation** | ADR-48/49/50 | 🟡 | 🎯 | 📆 1-2sem | Validação NVIDIA/AMD/Intel em silício real |
| P09 | **Ring3/SFI produção** | ADR-0041 | 🔴 | 🎯 | 🏗️ 1m+ | Isolamento Ring3 real. 3.000 LOC |
| WS-E | **NPU driver full** | ADR-0057 | ⚫ | 🧩 | 💰 Sponsor | Dependente de HW AMD XDNA |
| — | **Perci/Bitwork integration** | ADR-0054 | 🟡 | 🧩 | 🗓️ 1-3d | Pesquisa; sem wire |

---

## 2. Ideias Órfãs de Alto Valor — Viáveis e Aderentes

Ideias 🟡/⏳ do IDEA_BANK que NÃO têm dono claro nem ADR temática, mas são viáveis e alinhadas ao core do produto.

### 2.1 — Self-Optimization / Workflow Learning (IDEA #157–#163)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 157 | Usage Pattern Analyzer — LLM detecta workflow do usuário | 🟡 | 🎯 | 🗓️ 1-3d | Scheduler adaptativo por padrão de uso |
| 158 | Workflow Predictor — pré-carrega recursos por hora | 🟡 | 🎯 | 🗓️ 1-3d | MHI tiers adaptativos |
| 160 | Dynamic Resource Scaling — MHI auto-ajuste | 🟡 | 🎯 | 🗓️ 1-3d | Memória auto-gerenciada |
| 161 | Self-Optimizing Scheduler — prioriza por workflow | 🟡 | 🎯 | 🗓️ 1-3d | UI mais responsiva durante LLM |
| 162 | Workflow Profile — perfil exportável | 🟡 | 🧩 | 🍕 4h | Perfis "dev"/"escritório" |
| 163 | Hardware Config Learning — SystemArchitecture evolve | 🟡 | 🎯 | 🗓️ 1-3d | HW detection que melhora com uso |
| **Total** | | | | **~6-12 dias** | **~1.500 LOC** |

### 2.2 — Self-Learning OS (IDEA #313)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 313a | DataCollector — coleta EventBus/logs/SMART como dataset | 🟢 | 🎯 | 🍕 4h | Dados de treino do próprio sistema |
| 313b | Pipeline LogAgent→DataCollector→TrainingAgent→.bitnet | 🟡 | 🎯 | 🗓️ 1-3d | Ciclo auto-aprendizado fechado |
| 313c | Melhoria contínua — boot usa modelo treinado anterior | 🟡 | 🎯 | 🗓️ 1-3d | Sistema melhora sozinho |
| **Total** | | | | **~4-8 dias** | **~800 LOC** |

### 2.3 — Success Engine — Feedback Loop (IDEA #149–#152)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 149 | Feedback loop — 👍/👎 nas respostas do Hermes | 🟢 | 🎯 | 🍕 4h | Respostas melhores com uso |
| 150 | Ternary weight update on-device | 🔴 | 🎯 | 🗓️ 1-3d | Aprendizado real online |
| 151 | Experience replay buffer (últimas N interações) | 🟢 | 🎯 | 🍕 4h | SleepCycle REPLAY phase |
| 152 | Weight consolidation — export modelo atualizado | 🔴 | 🎯 | 🗓️ 1-3d | Persistência do aprendizado |
| **Total** | | | | **~4-10 dias** | **~800 LOC** |

### 2.4 — Security Pipeline (IDEA #260–#264)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 260 | Event→Detector→Response Pipeline (5 detectores) | 🟡 | 🎯 | 🗓️ 1-3d | Port scan/ARP spoof/DoS detection |
| 261 | Decision Review + Human Escalation | 🟢 | 🎯 | 🍕 4h | HITL para baixa confiança |
| 262 | Hash Chain Audit Trail | 🟢 | 🎯 | 🍕 4h | Merkle chain de eventos |
| 263 | Knowledge Graph para eventos de segurança | 🟡 | 🧩 | 🗓️ 1-3d | Correlação cross-evento |
| 264 | Cross-Layer Correlation Rules | 🟡 | 🧩 | 🗓️ 1-3d | 5 regras iniciais |
| **Total** | | | | **~6-12 dias** | **~1.100 LOC** |

### 2.5 — GPU Foundations + Compute (IDEA #326–#332, Bloco 21c/21d)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 326 | GPU BAR0/BAR1 mapping UC | 🟡 | 🎯 | 🍕 4h | MMIO direto GPU |
| 327 | GPU doorbell + SPSC job ring | 🟡 | 🎯 | 🗓️ 1-3d | Submissão de jobs GPU |
| 328 | VRAM buddy allocator | 🟢 | 🎯 | 🗓️ 1-3d | Gerenciamento VRAM |
| 329 | Agent.xpu prefill/decode split | 🔴 | 🧩 | 📆 1-2sem | LLM híbrida CPU+GPU |
| 330 | GPU matmul kernel ternário | 🔴 | 🎯 | 📆 1-2sem | 10-25× speedup |
| 331 | CPU→GPU KV cache DMA | 🔴 | 🧩 | 🗓️ 1-3d | Swap KV cache GPU |
| **Total** | | | | **~3-6 semanas** | **~2.500 LOC** |

### 2.6 — JARVIS Features (IDEA #315.1–.20, Blocos 24-27)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 315.1 | SOUL.md Personality Engine | 🟢 | 🎯 | 🍕 4h | Persona adaptativa |
| 315.4 | Notification Gate — 4 urgências, rate limiting | 🟢 | 🎯 | 🍕 4h | Notificações inteligentes |
| 315.6 | Emotion Analysis — 7 emoções via BitNet | 🟡 | 🧩 | 🗓️ 1-3d | UI afetiva |
| 315.11 | Persona Pipeline — 16 stages | 🟡 | 🎯 | 🗓️ 1-3d | Voz+texto+expressão |
| 315.14 | Proactive Heartbeats — JARVIS inicia conversa | 🟢 | 🎯 | 🍕 4h | Proatividade |
| 315.15 | Tool-State Save Game — snapshot/rollback | 🟢 | 🧩 | 🍕 4h | Debug de skills |
| 315.18 | Fail-Closed Safety Invariant — 4 invariantes | 🟡 | 🎯 | 🍕 4h | Segurança por design |
| 315.19 | Merkle Audit Trail — Ed25519 chain ring buffer | 🟢 | 🎯 | 🍕 4h | Audit trail |
| **Total** | | | | **~6-15 dias** | **~1.500 LOC** |

### 2.7 — Agents/Skills Evolution (IDEA A-001–A-020)

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| A-003 | AgentScheduler multicore (CFS) | 🟡 | 🎯 | 📆 1-2sem | Fairness entre agents |
| A-007 | Capability-Based Routing | 🟡 | 🎯 | 🗓️ 1-3d | Routing dinâmico |
| A-014 | Agent Budget + Watchdog | 🟢 | 🎯 | 🍕 4h | Previne runaway agents |
| A-015 | Agent Hooks — Pre/Post tick hooks | 🟢 | 🧩 | 🍕 4h | Sistema de plugins |
| A-016 | Multi-Agent Orchestration (graph-based) | 🟡 | 🎯 | 🗓️ 1-3d | Composição de agentes |
| **Total** | | | | **~2-4 semanas** | **~1.800 LOC** |

### 2.8 — Cross-OS Compatibility (IDEA #306a-d) — **Alto Valor via ClaudioOS**

ClaudioOS (ADR-0062) tem implementação real e compilável de:
- **PE32+ loader** completo (kernel32/user32/gdi32/ntdll/ws2_32/advapi32/ole32/msvcrt + DirectWrite/D2D/WASAPI/XInput/WIC)
- **ELF x86-64 loader** com tradução de syscalls
- **Win32 compat layer** funcional

O neural-os-core já tem **PE loader parcial** e **ELF loader parcial**. Com o código do ClaudioOS como referência (MIT/Apache, não AGPL), podemos portar a camada de compatibilidade cross-OS com esforço **muito menor** que desenvolver do zero.

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 306a | **PE32+ loader + Win32 compat** — rodar .exe Windows no AIOS | 🟡 (ClaudioOS tem) | 🧩 | 📆 1-2sem | Legacy Windows apps no bare-metal |
| 306b | **ELF loader + Linux syscall translation** — rodar Linux binaries | 🟡 (ClaudioOS tem) | 🧩 | 📆 1-2sem | Linux toolchain no AIOS |
| 306c | Mach-O loader (macOS) — stub | 🔴 | 🌌 | 📆 1-2sem | macOS compat (investigação) |
| 306d | Android APK compat — ART runtime como skill | ⚫ | 🌌 | 🏗️ 1m+ | Fora de escopo atual |
| 307 | **Syscall-to-Skill Translation Layer** — unifica NT/Linux/XNU | 🔴 | 🎯 | 📆 1-2sem | Camada única de tradução |
| **Total** | | | | **~4-8 semanas** | **~3.000 LOC** |

> **Nota:** O Win32 compat layer do ClaudioOS é particularmente valioso porque destrava ferramentas Windows (editores, compiladores, depuradores) no AIOS. A viabilidade é 🟡 (não 🔴) porque o código de referência existe e compila — o esforço é de portabilidade/adaptação, não invenção.

### 2.9 — Developers / Tooling

| # | Ideia | Viab | Ader | Custo | Resultado |
|---|-------|------|------|-------|-----------|
| 394 | BitNet IDE with HOWTO feature | 🟡 | 🧩 | 📆 1-2sem | IDE on-device |
| 395 | Marketplace/App Store agent | 🟡 | 🧩 | 🗓️ 1-3d | App store neural |
| 172 | MCP Server support (JSON-RPC 2.0) | 🟡 | 🧩 | 🗓️ 1-3d | Compatibilidade MCP |
| 236 | Plugin Hub / MCP Index com AI security scan | 🟡 | 🧩 | 🗓️ 1-3d | Catálogo de plugins |
| **Total** | | | | **~2-5 semanas** | **~1.500 LOC** |

---

## 3. Ideias de Viabilidade Baixa / Hibernar / Descartar

### 🔴 Viabilidade Difícil (requer pesquisa/infraestrutura)

| # | Ideia | Motivo | Destino |
|---|-------|--------|---------|
| 43-52 | NPU AMD XDNA driver | Sem HW, firmware fechado | 💰 Sponsor / hibernar |
| 307 | Syscall-to-Skill Translation Layer | Requer PE/ELF pleno (ver §2.8) | 🟡 v2.1+ (após 306a/b) |
| 123 | TLS 1.3 CertVerify full | CertVerify/FAT residual | 🟡 v2.1+ |
| 416 | **Boa JS Engine** — interpretador JavaScript 100% Rust (~180K LOC, ~5MB). Não-no_std, pesado p/ kernel; já temos WASM (wasmi). | 💤 hibernar |
| 399-400 | Candle Trainer sidecar + ELF loader | Treino GPU via ELF | 💤 hibernar |
| 413 | **PagedAttention** (vLLM, SOSP 2023) — KV cache paginado estilo memória virtual; COW entre prefixos. **Ganho marginal single-user** (nosso cenário: 1 LLM por vez). Complexidade não justifica para v2.0. | 💤 hibernar pós-v2.0 |
| 414 | FlashAttention — IO-aware exact attention com tiling L1. Aplica-se ao BitNet CPU. ~3-5× speedup p/ seq >256 tokens. | 💤 pós-v2.0 (Alta viabilidade, valor real) |

### ⚫ Inviáveis / Descartados

| # | Ideia | Motivo |
|---|-------|--------|
| 136 | LLM decide memory tier | Anti-pattern. Substituído por BudgetManager determinístico (ADR-0060) |
| 453 | NeuOS ISA plena / LatentBus adapter / H3 diffusion | Descartado na ADR-0047 |
| 360 | Kokoro-82M TTS | Supersedido por Piper VITS (ADR-0045) |
| 343-347 | AMD ROCm/KFD, Intel Level Zero, NPU Intel/AMD/AMX | Incompatível com HW atual (i5-6400) |
| 116 | Port ARM/RISC-V | Novo projeto, fora de escopo |
| 115 | Sponsor NPU AMD XDNA | Sem parceria |

---

## 4. Mapa de Calor Atualizado (Estimado)

Com base na auditoria, o mapa de calor do IDEA_BANK evoluiu:

| Status | Quantidade (est.) | % | Descrição |
|--------|------------------|---|-----------|
| ✅ Implementado | ~180 | 54% | Código existe e compila |
| 🟡 Agendado (v2.0) | ~45 | 13% | Nos itens 2.1-2.8 acima |
| 🟡 Agendado (v2.1+) | ~30 | 9% | GPU compute, Security Pipeline |
| ⏳ Pós-MVP / defer | ~40 | 12% | Self-Optimization pleno, FlashAttention |
| 📌 pre-v2 (transportado) | ~8 | 2% | VectorStore, Dynamic MoE, GPU golden |
| 💤 Hibernar | ~15 | 4% | Requer HW/parceria |
| 💰 Sponsor | ~8 | 2% | NPU XDNA, ARM/RISC-V |
| ❌ Descartado | ~10 | 3% | Supersedido/inviável |

**Total:** ~336 ideias

---

## 5. Recomendações por Prioridade

### 🏆 v2.0 Gate — Essenciais (deve estar pronto para declarar v2.0.0)

| Prioridade | Item | Esforço | Impacto |
|-----------|------|---------|---------|
| 1 | VectorStore TF-IDF (RAG on-device) | 🗓️ 1-3d | 🔴 Elimina dependência MCP externo |
| 2 | Usage Pattern Analyzer (#157) | 🗓️ 1-3d | 🟡 Scheduler adaptativo |
| 3 | Feedback loop (#149) | 🍕 4h | 🟢 Melhora qualidade respostas |
| 4 | DataCollector (#313a) | 🍕 4h | 🟢 Auto-aprendizado |
| 5 | Notification Gate (#315.4) | 🍕 4h | 🟡 UX de notificações |

### 📋 v2.1+ — Planejado

| Prioridade | Item | Esforço | Impacto |
|-----------|------|---------|---------|
| 6 | Dynamic MoE (birth/merge/split) | 📆 1-2sem | 🔴 Trinity adaptativo |
| 7 | GPU golden HW validation | 📆 1-2sem | 🔴 Compute GPU real |
| 8 | Security Pipeline (#260-264) | 🗓️ 1-3d | 🟡 Segurança do kernel |
| 9 | AgentScheduler multicore (A-003) | 📆 1-2sem | 🟡 Performance SMP |
| 10 | SOUL.md Personality Engine (#315.1) | 🍕 4h | 🟢 Persona JARVIS |

### 💤 Hibernar (revisar quando HW disponível)

| Item | Gatilho |
|------|---------|
| GPU BAR0/BAR1 mapping + doorbell + VRAM | Quando GPU golden HW validado |
| GPU matmul kernel ternário | Quando GPU compute pipeline estável |
| NPU XDNA driver | Quando HW AMD APU disponível |
| Ring3/SFI produção | Pós-v2.0.0, quando wasmi B/C exigirem |

---

## 6. Resumo de Esforço Total (v2.0 + v2.1)

| Pacote | Itens | Dias (est.) | LOC (est.) |
|--------|-------|-------------|------------|
| Self-Optimization (#157-163) | 6 | 6-12 | ~1.500 |
| Self-Learning OS (#313) | 3 | 4-8 | ~800 |
| Success Engine (#149-152) | 4 | 4-10 | ~800 |
| Security Pipeline (#260-264) | 5 | 6-12 | ~1.100 |
| GPU Foundations (#326-332) | 6 | 15-30 | ~2.500 |
| JARVIS Features (#315.x) | 8 | 6-15 | ~1.500 |
| Agents Evolution (A-xxx) | 5 | 10-20 | ~1.800 |
| Developer Tooling (#394,395,172,236) | 4 | 10-25 | ~1.500 |
| **PRE-V2 itens** | 8 | 15-30 | ~3.500 |
| **Total** | **~49** | **~75-162 dias** | **~15.000 LOC** |

---

## 7. Próximos Passos

1. Revisar este documento com o maintainer
2. Priorizar os itens do §5 para v2.0 gate
3. Mover itens "📌 pre-v2" para sprints ativas
4. Atualizar IDEA_BANK.md com links cruzados para este ADR
5. Fechar IDEAS descartadas com ❌ definitivo

---

## Referências

- `docs/memory/IDEA_BANK.md` — fonte completa (336+ ideias, 1786 linhas)
- `docs/architecture/INDEX.md` — lifecycle das ADRs
- `docs/architecture/pre-v2-residuals.md` — itens não implementados
- SESSION_220 — auditoria completa ADR + IDEA_BANK
