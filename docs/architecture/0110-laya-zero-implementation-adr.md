# ADR-0110: Avaliação e Decisões sobre o White Paper "Laya-Zero"

**Status**: PROPOSED | Accepted  
**Date**: 2026-09-22  
**Author**: Workflow Manager  
**Tags**: laya-zero, architecture, viability, phase-1

---

## 1. Context

Este documento registra as decisões de arquitetura resultantes da análise do white paper "Laya-Zero: O Paradigma Neural-OS-Core" (não incluso no repo, fornecido como referência externa) mais uma análise comprehensiva das áreas críticas do `neural-os-core` desde o boot (scheduler, bootloader, memória, event bus/security). O white paper propõe reestruturar o `neural-os-core` com o modelo Laya como motor Ring 0 (Sistema Operacional Cognitivo, Preditivo e Efêmero em Rust Bare-Metal).

**Pré-requisitos conhecidos do `neural-os-core`:**
- v1.9.99-s392 TEST: ~148K LOC, ~671 arquivos Rust, 41 agentes nativos
- Premissa AIOS-First (ADR-0088): IA é modo de operar desde o boot
- Sprint atual: v1.9.99-s392 TEST — Boot/Limine bughunt + canvas
- Plataforma de desenvolvimento: QEMU/WHPX

**Análises adicionais realizadas (sessions exp-1/2, lib-1/2, fix-1/2):**
- **Scheduler/k-nano** (exp-1/2): scheduling cooperativo + tick-based; `AgentTickBusy` via `consecutive_pending` + `goal_urgency`; rate-limiting `urgency==0 && consecutive > 50 && tick_id % 5 != 0` (80% skip após 50 Pending); watchdog `consecutive_pending > 10000 → Crashed` apenas para non-interativos (urgency==0); `has_pending()` trait para EventDriven agents; `sleep_us`/`busy_wait_us` via TSC calibration; `TIMER_TICKS ≈ 18.2 Hz` (PIT); `consecutive_pending` watchdog needs `watchdog_should_crash(urgency, consecutive) = urgency==0 && consecutive>10000` para não crashar agentes interativos
- **Bootloader protocols** (exp-1/2): Limine 2.0 wireado com `.requests` section no linker; bootflow 8 phases testadas (WHPX+OVMF); UEFI/OVMF funciona, BIOS dá triple-fault; estruturas chave: `HhdmRequest`, `MemmapRequest`, `FramebufferRequest`, `KernelAddressRequest`; `init_from_usable_ranges` para frame allocator; `TOTAL_RAM_MB` derivado de usable ranges; `KernelAddressRequest` marca região kernel como OCUPADA no allocator (SESSION_252/ora-1)
- **Memory management** (lib-1/2): `BitmapFrameAllocator` static array `[u8; BITMAP_SIZE]` (64GiB cap) com ops inline `#[under]` under `TicketLock`; `HEAP_BUFFER` 512MB em `.bss` requer fix `.data` section (SESSION_233) para evitar bump heap overwrite; `TOTAL_RAM_MB` AtomicU64 detectado no boot; `heap_budget_mb(ram_mb)` → `min(75% RAM, RAM−keep)` com floor enforcement; `grow_bump_auto` cresce 256MB/step com `heap_pte_present` guard — NÃO eager 6GB em TCG (exaure frames → reboot loop); `reserve_range()` marca frames ocupados sem `delivered` flag; `deallocate_frame` recusa non-delivered (IDEA #526) — previne DMA corrupção; `needs_airllm(params, file_mb)` quando modelo + heap > 75% RAM → AirLLM (layer streaming) em vez de carregar todo modelo residente
- **Event bus / NoProto** (lib-1/2): `AiosTaskPacket` `repr(C,packed)` 36 bytes (magic=0x41494F53="AIOS", clock u64, `TaskType` enum 8 values, priority, tensor_len u32, param_len u32, `PacketFlags` com persist/require_ack/compressed/encrypted); `NoProtoParser` zero-copy slice-overlay unsafe parse; `serialize_header`/`validate_packet` completam round-trip; bounded channels: `DEFAULT_QUEUE_DEPTH=64` (control)/`STREAM_QUEUE_DEPTH=8` (audio); `push_bounded` drop oldest quando full; `Receiver::has_pending()` sem consumir; zombie prune via `Arc::strong_count==1`; `SecurityAgent` subscreve `NET_EVENT`+`SYSTEM_EVENT`; `correlate` → 3+ alerts janela curta → publica `SECURITY_ALERT`; `TaskType` values: Unknown=0, Inference=1, Training=2, Sync=3, ModelUpdate=4, Heartbeat=5, Error=6, Shutdown=7
- **Security desde o boot** (fix-1/2): `SecurityAgent` estrutura com 5 detectores (PORT_SCAN, ARP_SPOOF, PingFlood, DhcpStarvation, TimerAnomaly); `feed_net_event` routes por tipo; `correlate` → 3+ alerts → publica `SECURITY_ALERT` no EventBus + Hermes `TOPIC_HERMES_RESPONSE`; cadeia de confiança HITL: `SystemAgent`, `MonitorAgent`, `SelfHealAgent`; `seed_embedded_agents()` com trust compilation; `record_access` sem callers → política ruído; `NoProto` + `SecurityAgent` integração: `SecurityAgent::tick()` lê `NET_EVENT`; classificação de intenção antes do `cortex`; short-circuit: publish `SECURITY_ALERT` e skip detectores pesados; se classifier não inicializado → retorna `Intent::Normal` fallback seguro

---

## 2. Decisões Principais

### 2.1 Pontos Descartados (❌)

| Aspecto | Decisão | Racional |
|---------|---------|----------|
| **Bloquear 150-300MB RAM pinned no boot** | **REJECTED** como requisito absoluto | Reduz margem de segurança em HW real (notebooks 8-16GB). O `TOTAL_RAM_MB` atual é derivado de `init_from_usable_ranges`; bloquear mais memória entraria em conflito com `grow_bump_auto` e riscaria OOM em máquinas menores. Mantido como "melhor esforço" com gate condicional: `if TOTAL_RAM_MB >= 8192 && HEAP_FREE_MB > 200 { embed_model() } else { fall_back_to_scalar() }`. |
| **Garantir forward pass < 35ms** | **REJECTED** como garantia absoluta | Depende de TSC estável, cache alignment e número de núcleos. Em TCG QEMU já é instável; em HW real com variações de clock, não há garantia absoluta. Mantido como "meta desejável" com medição empírica. TTR (Time-to-Routing Semântico) medido empiricamente em cada build, não garantido absolutamente. |

### 2.2 Pontos Aceitos (✅)

| Aspecto | Decisão | Implementação |
|---------|---------|---------------|
| **Laya como classificador System 1 no k-ai** | **ACEITO** | Reutilizar `hw_expert_v6` já treinado como "intuição" do kernel, em vez de criar modelo do zero. Entrega ~60% do valor conceitual. Forward pass usa intrínsecas SIMD existentes (`bitnet_avx2.rs`) ou fallback scalar. |
| **NoProto + TickV para dados efêmeros** | **ACEITO** | Estender writer `TickV` existente para serializar blocos de dados (logs, estado UI) em vez de files. Apps morrem após uso, RAM não incha. NoProto `TaskType` pode ser estendido com `LayaIntent`; TickV circular buffer evict após TTL (~60s). |
| **Triagem de segurança no kernel (Laya como filtro)** | **ACEITO** | Usar `SecurityAgent` existente + classificação de intenção antes do `cortex`. `SecurityAgent::tick()` no topo chama `k_ai::laya::classify(packet_payload)`; se `Intent::Malicious` → publish `SECURITY_ALERT` e skip remaining detectores (eles são pesados). Fallback: classifier não inicializado → `Intent::Normal`. Consenso: exigir tanto Laya quanto um detector existente antes de dropar pacote. |

---

## 3. Racional de Design

### 3.1 Por que reutilizar `hw_expert_v6`?

O modelo `hw_expert_v6` já existe com:
- 128 heads / 6 layers / 8 KV heads
- Hidden size 3072, vocab 64
- Treinado com 43K devices PCI+USB
- Loss convergido 0.389 (SESSION_255)

**Vantagens:**
- Já está no códigobase (SESSION_255)
- Non-training forward pass é determinístico em Rust
- Gates `allow_avx2()` já existem em `bitnet_avx2.rs`
- Evita "not invented here" — aproveita investimento prévio
- Se encaixa no gate condicional de RAM (`TOTAL_RAM_MB >= 8192`)

**Limitações:**
- Não é um "System 1" puro — é um classificador de devices
- Precisa de conversão de grafo para matrizes Rust puras (já feito em `cortex` para BitLinear)
- Intrínsecas SIMD já cobertas por `bitnet_avx2.rs` dispatch

### 3.2 Por que não bloquear 150-300MB RAM?

O custo arquitetural original do white paper:

```
Imposto de RAM inegociável: 150-300MB bloqueadas permanentemente
```

**Por que rejeitar (consolidado das sessões lib-1/2):**
1. O kernel atual reserva ~512MB heap via TALC (`HEAP_BUFFER` de 512MB em `.bss`)
2. Em QEMU 6GB: ainda sobra ~4GB — ok para development
3. Em HW real notebooks (8-16GB): crítico; `grow_bump_auto` (256MB/passo) poderia OOM
4. O `TOTAL_RAM_MB` é derivado de `init_from_usable_ranges` (memory.rs:56-98) — já considera RAM disponível
5. `HEAP_BUFFER` em `.bss` seguido por outras statics — conflito se HEAP_LIMIT estendido além HEAP_SIZE (SESSION_233 fix: section `.data` para statics críticos)

**Alternativa aceita:**
- Gate condicional: `if TOTAL_RAM_MB >= 8192 && HEAP_FREE_MB > 200 { embed_model() } else { fall_back_to_scalar() }`
- `heap_budget_mb(ram_mb)` política: `min(75% RAM, RAM − keep)` com floor enforcement
- `heap_piso_mb(ram_mb)` piso inicial do boot — AIOS mede RAM, não hardcode 512 em 1G
- `grow_bump_auto` cresce preguiçosamente 256MB/passo com verificação `heap_pte_present` pós-mapeamento

### 3.3 Por que não garantir <35ms forward pass?

O custo original:

```
Meta: < 35 milissegundos (limiar da percepção humana)
```

**Por que rejeitar (consolidado das sessões exp-1/2):**
1. Em TCG QEMU: variável, depende da sobrecarga do host
2. Em HW real: clock TSC drift, variações de carga de CPU
3. O scheduler atual não tem preemptivo perfeito para outros núcleos quando `AgentTickBusy` está ativo
4. O `consecutive_pending > 10000 → Crashed` (SESSION_258) já mata agentes passivos; conflitar-ia com forward pass contínuo
5. O scheduler é cooperativo + tick-based — não há preemptivo verdadeiro; o "preemptivo" é o próprio tick

**Meta alternativa:**
```
TTR (Time-to-Routing Semântico): medição empírica em cada build
Meta desejável: < 35ms na maioria dos builds, mas não garantida absolutamente
```

---

## 4. Plano de Implementação

### 4.1 Fase 1 — Imediata (1-2 sprints)

**Tasks principais (integrando findings das 6 sessions):**
1. **@explorer** — Discovery: mapear pontos de integração atuais (`hw_expert_v6`, `TickV`, `NoProto`, `Jarbas`, `SecurityAgent`, `limine_boot` entries) e localizar onde inserir o classificador Laya. *Findings*: scheduler pode reutilizar hooks `urgency`/`has_pending`; bootflow Limine entry point já existe; pode embedar model via `kernel_phys` marking.
2. **@fixer** — Implementation: converter grafo do modelo para matrizes Rust puras (já existente em `cortex`/BitLinear), escrever intrínsecas SIMD/NEON para multiplicações densas (já coberto por `bitnet_avx2.rs` gates), gates de memória estática. *Findings*: modelo cabe em 512MB HEAP_BUFFER com gate condicional; SIMD já disponível; fallback scalar suficiente para PoC.
3. **@librarian** — Research: verificar padrões de tokenizer no_std (já decidido: NÃO necessário para Phase 1 — classifier trabalha em raw byte slices); docs de NoProto/ARQ (já coberto: `AiosTaskPacket 36 bytes`, `TaskType` enum, bounded channels); exemplos de intrinsics em bare-metal (já coberto: `allow_avx2()` gates, `core::arch` limitations).

**Resultados esperados (integrando todas as sessions):**
- `k-ai` capaz de classificar intenções de hardware em < 50ms (meta empírica, não garantida) — usando forward pass existente ou scalar fallback
- Dados efêmeros serializados no `TickV` em vez de files no sistema — NoProto `TaskType::LayaIntent` + TickV writer com TTL ~60s
- `SecurityAgent` filtra pacotes de rede por intenção antes do `cortex` — classifier no topo de `tick()` com consenso Laya+detector existente
- Scheduler não quebrado: uso de `urgency` exemption + `has_pending()` trait para agents EventDriven
- Bootflow não quebrado: embedding model via `limine_boot` entries existentes; `kernel_phys` marking para marcar região como OCUPADA no frame allocator

### 4.2 Fase 2 — Curto Prazo (2-3 sprints)

**Tasks principais:**
1. Microagentes WASM sob demanda no `Jarbas`
2. `AgentRegistry::evict_older_than(60s)` baseado em `TickV` timestamp
3. Extender `NoProto` para broadcast simples + TOFU básico — adicionar `TaskType::LayaIntent = 8` (próximo disponível após Shutdown=7)

### 4.3 Fase 3 — Longo Prazo (3-4 sprints, alto risco)

**Tasks principais:**
1. Novo crate `mesh-transport` no `k-hal` para protocolo P2P neurológico — baseado em ADR-0081 mesh (16 slots reassembly + ACK seletivo FRAG\0→FRACK\0, stop-and-wait; `peer_p99_rtt` via `(count*99+99)/100` sem `f32::ceil`; token bucket rate limiting 1/tick burst 20; TOFU anti-replay `PK\0+pk` no heartbeat)
2. Laya como dispatcher de carga excedente via mesh P2P
3. Garantias de TTR < 35ms (requer HW específico; meta desejável, não garantida)

---

## 5. Evidências e Referências

| Referência | Conteúdo |
|------------|----------|
| **SESSION_255** | `hw_expert_v6` já treinado, 128h/6L/8heads, loss 0.389 |
| **SESSION_293** | `TRINITY` vazio → `populate_trinity_from_bin()` no boot |
| **SESSION_233** | `HEAP_BUFFER` de 512MB em `.bss`; `resize_bump_heap` hardcoded; fix .data section |
| **SESSION_252** | `TOTAL_RAM_MB` derivado de `init_from_usable_ranges`; `grow_bump_auto` (256MB/passo); `HEAP_BUFFER` conflito statics |
| **SESSION_258** | `consecutive_pending > 10000 → Crashed`; `set_urgency` isenção; `watchdog_should_crash(urgency, consecutive) = urgency==0 && consecutive>10000`; `has_pending()` trait |
| **SESSION_293** | `TRINITY` populado no boot via `populate_trinity_from_bin()` |
| **SESSION_152** | `NoProto` + `SecurityAgent` integração; `TOPIC_NET_EVENT` + `TOPIC_SYSTEM_EVENT` subscrição |
| **SESSION_280** | Mesh P2P: 16 slots reassembly + ACK seletivo FRAG\0→FRACK\0; `peer_p99_rtt` `(count*99+99)/100`; token bucket 1/tick burst 20; TOFU `PK\0+pk` heartbeat |
| **SESSION_252/ora-1** | `KernelAddressRequest` + `LIMINE_KERNEL_ADDR` — marca região kernel como OCUPADA no allocator |
| **ADR-0059** | App Factory wasmi; seletor A/B/C; F7 arena W^X |
| **ADR-0088** | Premissa AIOS-First: IA desde o boot |
| **White Paper "Laya-Zero"** | Documento referência externa (non-repo) |

---

## 6. Decisões Conflitantes Resolvidas

| Conflito | Resolução |
|----------|-----------|
| Laya vs Trinity MoE | Laya = System 1 (classificador rápido no k-ai); Trinity = System 2 (MoE pesado, dorme 95% do tempo). Não unificar — servem propósitos diferentes. |
| RAM pinned vs HEAP_BUFFER | HEAP_BUFFER mantido em `.bss` (512MB); Laya usa gate condicional baseado em `TOTAL_RAM_MB`/`HEAP_FREE_MB`. Não bloqueia RAM permanentemente. Gate: `if TOTAL_RAM_MB >= 8192 && HEAP_FREE_MB > 200 { embed_model() } else { fall_back_to_scalar() }`. |
| Forward pass < 35ms vs realidade hardware | Meta desejável, não garantida. Medição empírica por build. O scheduler é cooperativo + tick-based; não há preemptivo verdadeiro. TTR medido empiricamente, não garantido absolutamente. |
| NoProto mesh vs IPC interno | NoProto existente é point-to-point. Fase 1 estende para broadcast simples + TOFU (adicionar `TaskType::LayaIntent`). Fase 3 requer novo crate `mesh-transport` baseado em ADR-0081. |
| Scheduler stall vs Laya classification | Scheduler já tem `urgency` exemption + `has_pending()` trait para EventDriven agents. Laya classification no `tick()` hook não quebra estrutura se usar gates condicionais e não bloquear o tick por tempo excessivo. |

---

## 7. Próximos Passos

1. **Dispatch de especialistas** já realizado (sessions exp-1/2, lib-1/2, fix-1/2 completas e reconciliadas)
2. **Criar tasks** no sistema de tracking com os task IDs correspondentes (já feito nas 6 sessions)
3. **Reconciliar resultados** após conclusão da Fase 1 e decidir sobre Fase 2/3
4. **Atualizar INDEX** com lifecycle `por_fazer` → `fazendo` → `completa` conforme progresso
5. **Implementar Fase 1** com as 3 tasks principais já identificadas e reconciliadas

---

## 8. Metadados

| Campo | Valor |
|-------|-------|
| **ID** | 0110 |
| **Substitui** | Nenhum (nova ADR) |
| **Substituído por** | — |
| **Conflito ID** | — |
| **Ideias relacionadas** | ADR-0088 (AIOS-First), ADR-0059 (App Factory), ADR-0063 (SGDB), ADR-0082 (HardwareInfo) |
| **Sprints impactados** | Sprints 106-110 (Fase 1), 111-113 (Fase 2), 114-117 (Fase 3) |
| **Evidence directory** | `docs/evidence/laya-zero/` (criar se necessário) |

---

**Assinatura**: Workflow Manager — 2026-09-22  
**Revisão solicitada por**: Sistema de scheduler (Ponytail full mode)  
**Próxima revisão**: Após conclusão da Fase 1 (estimated 2 sprints, após dispatch das 3 tasks principais já reconciliadas)