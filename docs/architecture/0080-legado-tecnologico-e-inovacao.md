# ADR-0080: Legado Tecnológico e Inovação — Síntese de 4 Documentos Históricos

**Status:** Proposed  
**Lifecycle:** planejamento  
**Criação:** 2026-07-30  
**Fontes:** `docs/archive/notes/AIOS K2CHJ v2 Rodamap.txt`, `docs/archive/sprints/sprint-plan-fs-v2.md`, `docs/archive/sessions/SESSION_080.md`, `docs/archive/sessions/SESSION_082.md`  
**Tags:** infrastructure, storage, performance, ml-pipeline, golden-path

---

## Contexto

O projeto neural-os-core acumulou 4 documentos históricos com planos, análises e especificações que nunca foram formalizados como ADRs. Alguns foram parcialmente implementados, outros permanecem como oportunidades não exploradas. Esta ADR sintetiza os 4 documentos adaptando-os ao estado atual do código (v1.9.9-test).

---

## 1. Rodamap K²CHJ v2 — Visão vs Realidade

**Fonte:** `docs/archive/notes/AIOS K2CHJ v2 Rodamap.txt` (61 linhas)

### O que o documento propunha

| Proposta | Status v1.9.9-test |
|----------|-------------------|
| Workspace K²CHJ com 5 crates | ✅ **Feito** — 11 crates no workspace |
| "Golden Path" demo (voz → HTTP → LLM → WASM → widget) | 🟡 **Parcial** — fluxo existe mas loop não fechado |
| GitHub Release v1.2.0 | ⏳ Release automático não implementado |

### Análise de Gap

A proposta mais relevante é o **Golden Path**: "JARVIS, como está o clima?" → HTTP GET → LLM → WASM app → widget desktop. O autor estima ~600 LOC. O fluxo já existe parcialmente (Hermes→WwwAgent→LLM) mas o **tracker de frequência + auto-compilação WASM** nunca foi fechado.

**Decisão:** Adotar o Golden Path como demo canônica do projeto. Implementar o closed-loop: (1) usuário pergunta → (2) Hermes roteia → (3) LLM gera resposta → (4) após 3 repetições, AutoSkillGen gera WASM skill → (5) skill registrada → (6) próximas consultas usam a skill em vez do LLM.

---

## 2. Ecossistema de Armazenamento — Plano FS v2

**Fonte:** `docs/archive/sprints/sprint-plan-fs-v2.md` (8.558 bytes)

### O que o documento propunha

| Componente | Status | Nota |
|-----------|--------|------|
| FAT32 R/W | ✅ Feito | Leitura/escrita funcional |
| exFAT R/W | 🟡 Parcial | Leitura funciona, escrita gated (`EXFAT_WRITE=1`) |
| GPT write | 🟡 Parcial | Funções existem mas não em produção |
| EXT2/NTFS read | ❌ Não iniciado | Stubs existem em LEGACY |
| NeuralFS (CoW B-tree) | 🟡 Parcial | Volume RAM funciona, sem persistência em disco |
| Active MHI com DMA ring | ❌ Não iniciado | MHI existe como soft-MVP (metadados + memcpy) |
| ARC cache / I/O scheduler | ❌ Não iniciado | Apenas stubs |
| Network mounts (iSCSI/NFS/WebDAV) | ❌ Não iniciado | smoltcp OK, sem protocolo de mounts |
| GPU Direct Storage | ❌ Não iniciado | AWAITING_HW |
| SelfHeal FS consistency | ❌ Não iniciado | Apenas conceito |

### Análise de Gap

O ecossistema de armazenamento é o maior gap de especificação do projeto. Nenhuma ADR atual (nem 0063-SGDB, nem 0064-RAG) cobre a visão completa deste documento.

**Decisão:** Adotar como roadmap de storage pós-v2.0. Prioridades imediatas:
1. desbloquear `EXFAT_WRITE=1` (testar + estabilizar)
2. NeuralFS persistente em disco (checkpoint L0/L1 para SGDB)
3. MHI ativo com DMA ring (NVMe→VRAM)

---

## 3. Performance WHPX — AVX2 Emulation Cost

**Fonte:** `docs/archive/sessions/SESSION_080.md`

### Descoberta Crítica

Cada instrução VEX (AVX2) sob WHPX gera uma VM exit de ~10K+ ciclos. Resultado: **AVX2 é 2× MAIS LENTO que scalar** no WHPX:

| Modo | Ticks/layer | Relativo |
|------|-------------|----------|
| WHPX AVX2 | 4.443 | 2,0× |
| WHPX scalar | 2.218 | 1,0× (base) |
| Bare-metal AVX2 | ~44 | ~50× mais rápido |

**Impacto:** O BitNet 2B (30 layers) leva ~133K ticks em modo AVX2 WHPX vs ~66K em scalar. Em bare-metal com AVX2 real, seriam ~1.320 ticks — **50× mais rápido**.

### Decisões

1. `platform_probe::build_gate()` já desabilita AVX2 sob WHXP (HypervisorKind::MicrosoftHv → `allow_avx2=false`). **Confirmado funcionando.**
2. Para desenvolvimento LLM, recomendar `-accel tcg` (que permite AVX2 real, sem VM exits).
3. Para produção, bare-metal é o único caminho viável.
4. Documentar no `HOWTO.md` e na saída do `run-qemu-whpx.ps1`.

---

## 4. Pipeline de Treino ML — Quirks e Lições

**Fonte:** `docs/archive/sessions/SESSION_082.md`

### Problemas Conhecidos

| Problema | Impacto | Workaround |
|----------|---------|------------|
| CUDA 13.0 dropped sm_61 (GTX 1050) | PyTorch sem suporte GPU | Treinar em CPU ou usar torch-2.8.x |
| Python 3.14 incompatível com PyTorch | Erro de instalação | Usar Python 3.12 ou 3.13 |
| Export .bitnet v4: vocab_size como u16 ao invés de u32 | `load_model()` retorna `None` | `fix_bitnet_header.py` repara |
| Export .bitnet: num_medusa como u16 ao invés de u32 | `load_model()` retorna `None` | Mesmo script repara |

### Decisões

1. Adotar `tools/convert_*.py` como pipeline canônico de conversão (GGUF → .bitnet, safetensors → .bitnet)
2. Manter `fix_bitnet_header.py` como ferramenta de reparo pós-export
3. Documentar toolchain: Python 3.13, PyTorch 2.9.0+cu126, CUDA 12.6
4. Treinar experts em CPU (k_nano não depende de GPU) ou bare-metal com GPU real

---

## Plano de Implementação

### Fase 1 — Imediata (ADR + LEGACY restoration)
- [ ] Esta ADR (0080)
- [ ] CDC Rabin chunking — portar de LEGACY/k_ia/src/chunker.rs (~100 LOC)
- [ ] PhysicalBuffer DMA — portar de LEGACY/v1.5-dead-k2chj/hermes/wifi_dma.rs (~27 LOC)
- [ ] HAL Architecture trait — portar de LEGACY/v1.5-dead-k2chj/k_ia/hal.rs (~80 LOC)

### Fase 2 — Curto Prazo (Golden Path + Storage)
- [ ] Golden Path closed-loop: frequência → auto-WASM → skill
- [ ] Desbloquear EXFAT_WRITE=1
- [ ] NeuralFS persistente (checkpoint L0/L1 para disco)
- [ ] MHI ativo com DMA ring (NVMe→VRAM via BAR)

### Fase 3 — Médio Prazo (Armazenamento + Performance)
- [ ] Network mounts (WebDAV via smoltcp)
- [ ] ARC cache / I/O scheduler
- [ ] GPU Direct Storage (NVMe→VRAM via PCIe P2P)
- [ ] SelfHeal FS consistency checks

### Fase 4 — Longo Prazo (Armazenamento Avançado)
- [ ] EXT2/NTFS leitura (stubs existem)
- [ ] exFAT/NTFS escrita
- [ ] iSCSI initiator
- [ ] NeuralFS completo em disco (CoW B-tree, journal)

---

## Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| WHPX + AVX2 lento para sempre | Alta | Desenvolvimento LLM inviável em Windows | Usar TCG para LLM, bare-metal para produção |
| Pipeline ML quebra com novas libs | Média | Experts não treináveis | Fixar toolchain (Python 3.13, PyTorch 2.9.0) |
| exFAT write corrompe disco | Baixa | Perda de dados | Testar em QEMU, validar checksums antes de habilitar |
| NeuralFS sem journal corrompe | Média | Perda de memória do SGDB | WAL-style log antes de checkpoint |

---

## Referências

- `docs/archive/notes/AIOS K2CHJ v2 Rodamap.txt` — Visão K²CHJ e Golden Path
- `docs/archive/sprints/sprint-plan-fs-v2.md` — Ecossistema de armazenamento
- `docs/archive/sessions/SESSION_080.md` — WHPX AVX2 emulation cost
- `docs/archive/sessions/SESSION_082.md` — ML training pipeline quirks
- `docs/architecture/INDEX.md` — Lifecycle e conflitos de ADRs
- `LEGACY/` — Código legado com tecnologias a portar
