# Roadmap — neural-os-core

**Última atualização:** 2026-06-22  
**Documento vivo:** Alinhado ao ADR-0010 (Estratégico), ADR-0013 (Estado da Arte 2026) e ADR-0013 (Design System).

A ordem de engenharia abaixo segue a dependência física do bare-metal: primeiro a memória, depois o kernel, depois a comunicação entre agentes, depois o runtime de skills, e por último o planejador cognitivo.

> **Nota:** Sprints 11–15 concluídos. Phase 1 (Bitmap Allocator) e Phase 2 (Async Executor) entregues. Phase 3 (EventBus IPC), Phase 4 (Skill Registry) e Phase 2.5 (Hardware Neural Routing) adiantados.

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

## 2. Kernel Abstraction (Async Neural Executor)

**Sprints:** 12  
**Target:** Q3 2026  
**Depende de:** Fase 1 (Bitmap Allocator)

### Metas

- [x] **`NeuralExecutor`** — `VecDeque<AgentTask>` cooperative polling com `DummyWaker` (`RawWakerVTable` em `no_std`)
- [x] **`AgentTask`** — `{ id: u64, future: Pin<Box<dyn Future>> }` com `AtomicU64` ID generation
- [ ] **Agent Scheduler** — round-robin com prioridade. A cada tick, percorre a lista de agentes e chama `tick()`. O scheduler consulta o Intent Router (MLP) para decisões de prioridade.
- [ ] **Budget de execução** — limite de `tokens_consumed` por agente por ciclo.

---

## 2.5. Hardware Neural Routing

**Sprint:** 15  
**Target:** Q3 2026  
**Depende de:** Phase 3 (EventBus), Phase 2 (Executor)

### Justificativa

Antes de construir um scheduler completo, precisamos provar que o kernel pode rotear I/O de hardware bruta para agentes via EventBus — validando a arquitetura Top-Half/Bottom-Half e a segregação Ring 0/Ring 2.

### Metas

- [x] **`keyboard_interrupt_handler` (IDT[33])** — Top-Half: lê porta 0x60 via `x86_64::Port`, armazena em `LAST_SCANCODE: AtomicU8` (Release), EOI raw (`out 0x20, 0x20`). Sem alocações, sem spinlocks.
- [x] **`hw_bridge_daemon`** — Bottom-Half: `swap(0, Acquire)` do atômico → `EventBus::publish("RAW_HW_IRQ1", [scancode])`. Executa em contexto normal (user).
- [x] **`input_daemon`** — Subscribe "RAW_HW_IRQ1" → log scancode → infer key (0x1E = 'A').
- [x] **Validação QEMU** — 4 tasks spawnadas, 500+ ticks PIT sem Double Fault. ADR-0013 validado: kernel roteia bytes, daemons interpretam.

### Arquitetura

```
Interrupt HW → Top-Half (µs) → AtomicU8 → Bottom-Half (Daemon) → EventBus → Agent
```

### Alinhamento SotA

- ASA / Microkernel design: separação entre mechanism (interrupt → atomic) e policy (daemon → EventBus → agent).

---

## 2. Kernel Abstraction (Agent Scheduler — futuro)

**Sprints:** 16–17  
**Target:** Q4 2026  
**Depende de:** Fase 1 (slabs), Fase 2.5 (HW routing)

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

**Sprints:** 13  
**Target:** Q3 2026  
**Depende de:** Fase 2 (Async Executor)

### Justificativa

O sistema não pode ter syscalls tradicionais. Toda comunicação entre Ring 0, Ring 1 e Ring 2 passa pelo `EventBus` com tokens de capacidade verificados em cada mensagem.

### Metas

- [x] **`EventBus` struct** — `BTreeMap<String, Vec<Arc<Mutex<VecDeque<Event>>>>>`, publish/subscribe com `Receiver`
- [x] **`CapabilityToken(pub u64)`** — validação `is_valid()` (token > 0), verificado no `publish()`
- [x] **`Event` struct** — `{ id: u64, topic: String, payload: Vec<u8>, token: CapabilityToken }`, ID gerado automaticamente
- [x] **IPC Flow** — `hardware_monitor_daemon` publish → `system_daemon` receive → EchoSkill execute → complete
- [ ] **`Topic` enum completo** — AgentCreated, AgentDestroyed, SkillRequest, SkillOutput, CortexDecision, WatchdogTick, MemoryPressure
- [ ] **Roteamento baseado em ML** — para tópicos de alta frequência, EventBus consulta Intent Router para filtrar assinantes

### Alinhamento SotA

- Capability-based IPC é padrão em seL4 e Fuchsia. Nosso diferencial: o token é avaliado pelo MLP, não por ACL estática.
- Zero-copy entre Ring 0 e Ring 2: mensagens são `&[u8]` fat pointers sobre páginas compartilhadas (preparação para SFS Fase 4).

---

## 4. Skill Registry & MCP

**Sprints:** 14  
**Target:** Q3 2026  
**Depende de:** Fase 3 (EventBus)

### Justificativa

Agentes precisam de habilidades executáveis. Em vez de syscalls, skills são módulos WASM carregados sob demanda, com ciclo de vida gerenciado pelo `SkillRegistry`.

### Metas

- [x] **`Skill` trait (Send+Sync)** — `manifest() -> McpManifest`, `execute(&[u8]) -> Result<Vec<u8>>`
- [x] **`McpManifest` struct** — `{ name, description, required_tokens }`
- [x] **`SkillRegistry`** — `BTreeMap<String, Box<dyn Skill>>`, register + `execute_skill` com Zero-Trust `CapabilityToken` validation
- [x] **`EchoSkill`** — skill de demonstração (reversão de payload), registrada no boot
- [x] **Invocation flow** — `system_daemon` recebe SYSTEM_READY via EventBus → `SkillRegistry::execute_skill("echo", ...)` → output `[3,2,1]` confirmado em QEMU
- [ ] **WASM embedder (`wasmi`)** — runtime WASM em `no_std`. Host functions: `tensor.matmul`, `nn.silu`, `sfs.read`.
- [ ] **Linear memory pool** — slabs pré-alocados de 256 KB por skill.

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

| Ordem | Componente | Sprints | Target | Status |
|---|---|---|---|---|
| 1 | Memória (Bitmap Allocator) | 11 | Q3 2026 | ✅ Concluído |
| 2 | Kernel — Async Neural Executor | 12 | Q3 2026 | ✅ Concluído |
| 3 | Event Bus & IPC (Capability Tokens) | 13 | Q3 2026 | ✅ Concluído |
| 4 | Skill Registry & MCP Layer | 14 | Q3 2026 | ✅ Concluído |
| 2.5 | Hardware Neural Routing (IRQ1 → EventBus) | 15 | Q3 2026 | ✅ Concluído |
| 5 | Memória — Slab Allocator | 16 | Q4 2026 | Pendente |
| 6 | Agent Scheduler (Round-Robin) | 17 | Q4 2026 | Depende de Slab |
| 7 | Cognitive Runtime (Intent Planner) | 18+ | Q1 2027+ | Depende de Agent Scheduler |
| — | MatMul-free LM (meta futura) | 19+ | Q2 2027+ | Fase 5+ |

---

*Consulte ADR-0010 (Strategic Roadmap) e ADR-0013 (Executive Summary / Estado da Arte 2026 + Design System) para fundamentação arquitetural completa.*
