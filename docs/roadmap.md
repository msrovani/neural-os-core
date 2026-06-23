# Roadmap — neural-os-core

**Última atualização:** 2026-06-22  
**Documento vivo:** Alinhado ao ADR-0010 (Estratégico), ADR-0013 (Estado da Arte 2026) e ADR-0013 (Design System).

A ordem de engenharia abaixo segue a dependência física do bare-metal: primeiro a memória, depois o kernel, depois a comunicação entre agentes, depois o runtime de skills, e por último o planejador cognitivo.

---

## 1. Memória Física e Virtual (Bitmap Allocator & Huge Pages)

**Sprints:** 11–13  
**Target:** Q3 2026  
**Depende de:** Fase 2 (OffsetPageTable, BootInfoFrameAllocator)

### Justificativa

Sem um alocador de frames não-monotônico, não podemos reutilizar memória física. Sem Huge Pages (2 MiB / 1 GiB), o TLB será saturado pelas sessões de inferência longas do BitNet.

### Metas

- [ ] **Bitmap Frame Allocator** — alocação/liberação O(1) com bitmap de 4 KiB frames. Substitui `BootInfoFrameAllocator` monotônico e `EmptyFrameDeallocator` stub.
- [ ] **Suporte a Huge Pages (2 MiB)** — mapper no `OffsetPageTable` para `PageSize::Size2MiB`. Redução de TLB misses em ~512× vs 4 KiB pages.
- [ ] **Suporte a Huge Pages (1 GiB)** — para sessões de inferência que exigem >512 MiB contíguos. Opcional: verificar suporte via CPUID.
- [ ] **Slab Allocator para heap** — reduzir fragmentação do `LockedHeap` (100 KB fixo). Alocar `Vec`, `Tensor` e `Message` em slabs de tamanho fixo.

### Alinhamento SotA

- MerlionOS: mapeamento de GGUF diretamente em Huge Pages confirmou redução de TLB misses em 40% durante inferência.
- FairyFuse: TL/I2_S exige DWORD-aligned weight buffers (32 bits) — Huge Pages eliminam fragmentação externa.

---

## 2. Kernel Abstraction (Agent Scheduler)

**Sprints:** 14–15  
**Target:** Q4 2026  
**Depende de:** Fase 1 (Bitmap Allocator, slabs)

### Justificativa

O `loop { hlt() }` atual não escala. Precisamos de um scheduler que gerencie múltiplos `AgentProcess`, cada um com seu contexto, prioridade e fila de skills.

### Metas

- [ ] **`AgentProcess` trait + struct** — id, priority, embedding, skill_queue, tick()
- [ ] **Agent Scheduler** — round-robin com prioridade. A cada tick, percorre a lista de agentes e chama `tick()`. O scheduler consulta o Intent Router (MLP) para decisões de prioridade.
- [ ] **Criação/Destruição de agentes via EventBus** — `Topic::AgentCreated`, `Topic::AgentDestroyed`
- [ ] **Budget de execução** — limite de `tokens_consumed` por agente por ciclo. Agentes que excedem são rebaixados de prioridade.

### Alinhamento SotA

- ASA / Neural eBPF: validam que ML leve (MLP 4→2) para decisões de scheduler sub-µs é viável e superior a heurísticas fixas.
- Mixture-of-Schedulers: nosso Intent Router (MLP 3→2) evolui para rotear políticas de sistema em tempo real.

---

## 3. Event Bus & IPC (Capability Tokens)

**Sprints:** 16–17  
**Target:** Q1 2027  
**Depende de:** Fase 2 (Agent Scheduler), slabs para mensagens

### Justificativa

O sistema não pode ter syscalls tradicionais. Toda comunicação entre Ring 0, Ring 1 e Ring 2 passa pelo `EventBus` com tokens de capacidade verificados em cada mensagem.

### Metas

- [ ] **`EventBus` trait + implementação concreta** — `Vec<Vec<Message>>` indexada por `AgentId` no kernel; ring-buffer lock-free em Huge Pages na versão otimizada.
- [ ] **`CapabilityToken`** — struct com `agent_id` e `permissions: u64` bitmap. Verificado pelo `EventBus::authorize()` antes de cada publish/delivery.
- [ ] **`Topic` enum completo** — AgentCreated, AgentDestroyed, SkillRequest, SkillOutput, CortexDecision, WatchdogTick, MemoryPressure.
- [ ] **Roteamento baseado em ML** — para tópicos de alta frequência (ex: WatchdogTick), o EventBus consulta o Intent Router para decidir quais assinantes recebem a mensagem (evita flooding).

### Alinhamento SotA

- Capability-based IPC é padrão em seL4 e Fuchsia. Nosso diferencial: o token é avaliado pelo MLP, não por ACL estática.
- Zero-copy entre Ring 0 e Ring 2: mensagens são `&[u8]` fat pointers sobre páginas compartilhadas (preparação para SFS Fase 4).

---

## 4. Skill Registry & MCP

**Sprints:** 18–20  
**Target:** Q2 2027  
**Depende de:** Fase 3 (EventBus), Fase 1 (Huge Pages)

### Justificativa

Agentes precisam de habilidades executáveis. Em vez de syscalls, skills são módulos WASM carregados sob demanda, com ciclo de vida gerenciado pelo `SkillRegistry`.

### Metas

- [ ] **`Skill` trait + registry** — lookup por `SkillId`, ciclo de vida (load → execute → drop), `CapabilitySet` validation.
- [ ] **WASM embedder (`wasmi`)** — runtime WASM em `no_std` (ou `std` para ferramentas de host). Host functions: `tensor.matmul`, `nn.silu`, `sfs.read`.
- [ ] **MCP (Model Context Protocol)** — mensagens estruturadas entre skill e Córtex. Formato: `{ "type": "skill_request", "skill_id": "...", "input_tensor": [...], "token": "...". }`.
- [ ] **Linear memory pool** — slabs pré-alocados de 256 KB por skill, alocados do alocador de slabs (Fase 1).

### Alinhamento SotA

- WASM Component Model: skills como módulos efêmeros, sem instalação persistente.
- MCP: skills se comunicam com o Córtex via mensagens no EventBus (não syscalls diretos).

---

## 5. Cognitive Runtime (Intent Planner)

**Sprints:** 21–23  
**Target:** Q3 2027  
**Depende de:** Fase 4 (Skill Registry), Fase 3 (EventBus)

### Justificativa

O Intent Router atual (MLP 3→2) decide apenas a próxima ação imediata. O Cognitive Runtime mantém um plano de múltiplas etapas — uma sequência de skills a executar para satisfazer a intenção do usuário.

### Metas

- [ ] **Intent Planner** — recebe uma embedding de intenção e produz uma sequência de `SkillCommand`s. Usa o MLP atual como política gulosa, evoluindo para beam search.
- [ ] **Success Engine** — feedback loop: se uma skill retorna `success: false`, o planner ajusta a política (pesos do MLP) online.
- [ ] **Neural Cache** — cache de decisões do planner com ~50 ns de latência para intenções repetidas. Implementado como lookup table em Huge Pages.
- [ ] **MatMul-free LM (meta futura)** — substituir self-attention por pooling ternário ou estados recorrentes (RWKV, Mamba). Eliminar **todas** as multiplicações FPU do pipeline.

### Alinhamento SotA

- ASA: scheduling adaptativo com ML. O Cognitive Runtime leva o conceito ao nível de planejamento de intenções.
- Neural Kernel (eBPF + RL): políticas de kernel aprendidas online. Nosso Success Engine faz o mesmo com feedback de execução de skills.

---

## Timeline Consolidada

| Ordem | Componente | Sprints | Target | Depende de |
|---|---|---|---|---|
| 1 | Memória (Bitmap + Huge Pages + Slabs) | 11–13 | Q3 2026 | Fase 2 (page tables) |
| 2 | Kernel Abstraction (Agent Scheduler) | 14–15 | Q4 2026 | Fase 1 (memória) |
| 3 | Event Bus & IPC (Capability Tokens) | 16–17 | Q1 2027 | Fase 2 (scheduler) |
| 4 | Skill Registry & MCP | 18–20 | Q2 2027 | Fase 3 (EventBus) + Fase 1 (Huge Pages) |
| 5 | Cognitive Runtime (Intent Planner) | 21–23 | Q3 2027 | Fase 4 (skills) |
| — | MatMul-free LM (meta futura) | 24+ | Q4 2027+ | Fase 3–5 |

---

*Consulte ADR-0010 (Strategic Roadmap) e ADR-0013 (Executive Summary / Estado da Arte 2026 + Design System) para fundamentação arquitetural completa.*
