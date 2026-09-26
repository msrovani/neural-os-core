# ADR-0110: Avaliação e Decisões sobre o White Paper "Laya-Zero" (REVISADA)

**Status**: PROPOSED (revisada) | Lifecycle: `por_fazer`
**Date**: 2026-09-22 · **Revisão**: 2026-09-25 (SESSION_407)
**Author**: Workflow Manager
**Tags**: laya-zero, architecture, viability, phase-1
**Lifecycle (INDEX)**: `por_fazer` — ver §7 para gatilho de `fazendo`

> **⚠️ Aviso de revisão (2026-09-25, SESSION_407)**: A versão original auto-declarava
> ACCEPTED/`fazendo` com decisões que contradiziam evidência medida do próprio repo
> (SESSION_248, IDEA #492/#496), o contrato ADR-0106 (`ScorerAbsent` ≠ `Normal`), e a
> realidade do código (`HEAP_FREE_MB` inexistente; TickvLite sem TTL; SecurityAgent sem
> caminho de drop; BOOT_AI já implementado). O status ACCEPTED foi **ilegítimo**
> (sem IDEA, sem SESSION, sem evidência, em contradição com o INDEX). Esta revisão
> restaura PROPOSED/`por_fazer` e corrige os fatos. A decisão central original
> ("reutilizar `hw_expert_v6` como classificador de intenção de rede") foi
> **REJEITADA** — erro de categoria, refutado abaixo.

---

## 1. Context

Este documento registra as decisões de arquitetura resultantes da análise do white paper "Laya-Zero: O Paradigma Neural-OS-Core" (referência externa, não incluso no repo) mais análise das áreas críticas do `neural-os-core` (scheduler, bootloader, memória, event bus/security). O white paper propõe reestruturar o `neural-os-core` com o modelo Laya como motor Ring 0 (Sistema Operacional Cognitivo, Preditivo e Efêmero em Rust Bare-Metal).

**Pré-requisitos conhecidos do `neural-os-core`:**
- v1.9.99-s392 TEST: ~148K LOC, ~671 arquivos Rust, 41 agentes nativos
- Premissa AIOS-First (ADR-0088): IA é modo de operar desde o boot
- Plataforma de desenvolvimento: QEMU/WHPX

**Análises de suporte (lanes de sessão, não sessions numeradas):**
- **Scheduler/k_nano**: scheduling cooperativo + tick-based (PIT ~18,2 Hz); rate-limit `urgency==0 && consecutive > 50`; watchdog `watchdog_should_crash(urgency, consecutive) = urgency==0 && consecutive>10000` (SESSION_258); `has_pending()` trait para EventDriven; `sleep_us`/`busy_wait_us` via TSC calibrado.
- **Bootloader**: Limine 12.9.0 UEFI (`.requests` no linker); 8 fases de bootflow; `init_from_usable_ranges` popula `TOTAL_RAM_MB` (`crates/k_nano/src/memory.rs:87-139`); `KernelAddressRequest` marca região do kernel como OCUPADA (SESSION_252/ora-1).
- **Memória**: `BitmapFrameAllocator` 64GiB cap; `HEAP_BUFFER` 512MB (lição em AGENTS.md/SESSION_233 — o arquivo SESSION_233.md NÃO existe no repo); `heap_budget_mb` (`memory.rs:472`) = `min(75% RAM, RAM−keep)`; `heap_piso_mb` (`memory.rs:491`); `grow_bump_auto` 256MB/passo com `heap_pte_present` (`allocator.rs:137,223`); janela do bump ~2GB — `BUMP_MAX_OFFSET` (SESSION_339); **não existe** `HEAP_FREE_MB`.
- **NoProto**: `AiosTaskPacket` `repr(C,packed)` — **37 bytes medidos** (`size_of` no toolchain pinado; o "36" só aparece em `p2p_sim.rs`, código morto gated); `TaskType` = Unknown0..Shutdown7 (`crates/k_nano/src/net/noproto.rs:42-59`); slot 8 livre; bounded channels `DEFAULT_QUEUE_DEPTH=64` drop-oldest; `Receiver::has_pending()`.
- **Security desde o boot**: `SecurityAgent` (`crates/hermes/src/security.rs:162-336`) com 5 detectores (PortScan/ArpSpoof/PingFlood/DhcpStarvation/TimerAnomaly de `k_ai::security_detectors`); `tick()` drena `NET_EVENT`/`SYSTEM_EVENT`; `correlate` 3+ alerts → `SECURITY_ALERT` + Hermes. **É um observador de resumos de texto do barramento — não tem acesso a pacotes nem caminho de drop.** A integração SecurityAgent↔NET_EVENT é SESSION_310 (não 152).
- **TickvLite** (`crates/k_nano/src/storage/tickv.rs`): KV **persistente** append-log + CRC + GC por watermark (`HIGH_WATER=256KB`); GC suspenso no mount (K33[28]/SESSION_354); **sem TTL, sem evict por timestamp, sem circular buffer**.

---

## 2. Decisões Principais (revisadas)

### 2.1 Pontos Descartados (❌)

| Aspecto | Decisão | Racional |
|---------|---------|----------|
| **Bloquear 150-300MB RAM pinned no boot** | **REJECTED** | Conflita com `grow_bump_auto` e risco OOM em 8-16GB. Além disso, o ativo real em questão (`hw_expert_v6`) tem **265KB** — o "imposto de RAM" do white paper nunca se aplicou ao modelo existente. |
| **Garantir forward pass < 35ms** | **REJECTED** como garantia | TCG/HW têm clock drift; scheduler cooperativo sem preemptivo real. Meta desejável com TTR medido empiricamente por build. |
| **Reutilizar `hw_expert_v6` como classificador de intenção de rede (System 1 de tráfego)** | **REJECTED (revisão 2026-09-25)** | **Erro de categoria**: input do modelo é `pack_vid_did(vid,did)` → 4 tokens vocab-64; output são 5 cabeças de **família de device** (family/fw/agent/caps/next) — não intenções de rede. Bytes crus de pacote não entram nesse modelo e device-family não nomeia `Intent::Malicious`. **Evidência contrária registrada**: SESSION_248 mediu o transformer como ferramenta errada para HW identification (fp32 60,58% ≈ majoritário 60,67%; teto de sinal ~59-63%); IDEA #492 exige ≥65% específico para qualquer NN de device. "60% do valor conceitual" era retórica, não medição. |
| **TickV como store de dados efêmeros com TTL ~60s** | **REJECTED (revisão)** | TickvLite é persistente (append-log + GC); churn de TTL de 60s encheria o `HIGH_WATER` continuamente (write amplification + GC em boot, lição K33[28]) e dados sobreviveriam a reboot — o oposto de efêmero. Doutrina registrada (SESSION_352): stream de alta taxa = **ring SPSC de RAM de consumidor único**; o barramento fica com controle. |
| **Triagem "Laya Malicious → skip detectores" + "consenso antes de drop"** | **REJECTED (revisão)** | Auto-contraditório: se Malicious pula os detectores, o consenso exigido nunca ocorre; e **não existe caminho de drop** no SecurityAgent (observador de texto, não inline com a pilha de rede). Skip degradaria os detectores existentes sem evidência de que o classificador os substitui. |

### 2.2 Pontos Aceitos (✅)

| Aspecto | Decisão | Implementação |
|---------|---------|---------------|
| **Conceito Laya = System 1 no k-ai** | **ACEITO como direção, SEM implementação aprovada** | Um classificador de intenção real exige: dataset rotulado (pacote→intenção), pipeline de treino, gate de aceitação ≥65% específico no protocolo honesto (padrão IDEA #492) e evidência em `docs/evidence/laya-zero/`. Registrado como **IDEA #611** (`por_fazer`). Reuso de `hw_expert_v6` para isto está **fora** (§2.1). |
| **NoProto `TaskType::LayaIntent = 8`** | **ACEITO** (extensão trivial de protocolo) | Slot 8 está livre (`noproto.rs:42-59`). Só passa a ter conteúdo quando o classificador real existir. |
| **Triagem de segurança fail-closed e aditiva** | **ACEITO (reescrito)** | Quando um classificador real existir: `SecurityAgent::tick()` chama `k_ai::laya::classify()`; saída é `Decision<Intent>` (ADR-0106). **Classifier ausente/não-inicializado → `abstain(ScorerAbsent)`, NUNCA `Intent::Normal`** (ADR-0106: "n/a ≠ 0"). Ação = **alerta adicional** no pipeline de `correlate` existente (vota junto, não substitui). Drop exige chokepoint novo na pilha de rede, a especificar em ADR própria. |

---

## 3. Racional de Design (revisado)

### 3.1 O que `hw_expert_v6` REALMENTE é (medido 2026-09-25)

Header de `target1/hw_expert_v6.bitnet` (offsets do loader `crates/cortex/src/cortex.rs:399-436`):
**hidden=128 · layers=6 · heads=4 · kv_heads=4 · q_dim=32 · vocab=64 · intermediate=256 · ~1M params · 265.620 bytes.**

- A versão original desta ADR dizia "128 heads / hidden 3072 / 8 KV heads" — **falso**: 3072 é o hidden do **Falcon3-3B**; "128h/6L/8heads" (descrição da v3 no AGENTS.md) foi lido como "128 heads". Loss 0,389 é da v3 (Sprint 102), não da SESSION_255 (que é a conversão de formato v5→v6).
- Veredicto registrado (SESSION_248, `docs/evidence/hwexpert-architecture-verdict-20260804.md`): "o transformer é a ferramenta errada para HW identification"; runtime medido ~5-63%; tabela curada > NN para devices conhecidos.
- Consequência: o modelo serve para o que já serve hoje (hint de família de device no PnP, gated pelo gate #492). Não serve como System 1 de tráfego sem re-treino completo em dataset novo — e aí seria outro modelo, não "reuso".

### 3.2 Por que não bloquear RAM / por que o gate original não faz sentido

- `TOTAL_RAM_MB` (`memory.rs:32`) derivado de `init_from_usable_ranges` (`memory.rs:87-139`); política real: `heap_budget_mb` + `heap_piso_mb` + `grow_bump_auto` (SESSION_287/290).
- O gate proposto `HEAP_FREE_MB > 200` referenciava **símbolo inexistente**; o equivalente real é `heap_observe().headroom_mb` (`allocator.rs:285`). Para um artefato de 265KB, nenhum gate de RAM é necessário; a restrição que importa é a **janela do bump (~2GB, `BUMP_MAX_OFFSET`, SESSION_339)** — relevante para modelos grandes, não para este.
- `embed_model()` vs `fall_back_to_scalar()` conflui **residência** com **dispatch de ISA**: o fallback scalar (soft-float é o alvo do kernel) ainda precisa do modelo residente. São eixos independentes.

### 3.3 TTR (Time-to-Routing Semântico)

Meta desejável < 35ms, **medida empiricamente por build**, nunca garantida. Riscos concretos a endereçar quando existir classificador (todos são bugs já registrados no repo):
1. **Budget no tick**: forward dentro de `tick()` sob `AGENT_TICK_BUSY` congela Display (SESSION_328) → exigir budget TSC (`now_us`) com corte honesto (SESSION_354: timeout que retorna sucesso é pior que hang).
2. **Lock do modelo**: nunca segurar lock de modelo através do forward (padrão `HWEXPERT_V4_MODEL`, SESSION_316).
3. **Fila NET_EVENT**: `DEFAULT_QUEUE_DEPTH=64` drop-oldest (SESSION_252/352) — tick lento do SecurityAgent silencia eventos de segurança; contabilizar drops.

---

## 4. Plano de Implementação (revisado)

### 4.1 Fase 1 — sem tasks aprovadas até existir evidência

A Fase 1 original ("converter hw_expert_v6 para classificador de intenção") está **cancelada** (§2.1). O que resta:
1. **IDEA #611** — definir dataset de intenção (fonte de labels não-circular; lição SESSION_248 "labels circulares produzem números falsos"), protocolo de validação com holdout disjunto, gate ≥65% específico.
2. Só após o gate: ADR de implementação (número próprio ou reabertura desta com lifecycle `fazendo`) com o contrato `Decision<Intent>` (ADR-0106) + budget TSC no tick + contabilização de drops.
3. NoProto `TaskType::LayaIntent = 8`: uma linha, sem dependência do classificador — pode ir direto.

### 4.2 Fase 2 — Curto Prazo (após Fase 1 real)

1. Microagentes WASM sob demanda no Jarbas (ADR-0059 já cobre o runtime wasmi).
2. ~~`AgentRegistry::evict_older_than(60s)` via TickV~~ — **não existe e não é extensão de TickV**: efêmeros vivem em rings de RAM (SESSION_352); persistente vive em TickvLite sem TTL.
3. Broadcast simples + TOFU sobre NoProto (base ADR-0081).

### 4.3 Fase 3 — Longo Prazo (alto risco, inalterado)

1. Crate `mesh-transport` no k-hal para protocolo P2P neurológico.
2. Laya como dispatcher de carga via mesh.
3. Garantias de TTR < 35ms (HW específico; meta desejável, não garantida).

---

## 5. Evidências e Referências (corrigidas 2026-09-25)

| Referência | Conteúdo |
|------------|----------|
| **SESSION_248** | ⚠️ **Faltava na ADR original e é a decisiva**: transformer = ferramenta errada p/ HW identification; fp32 60,58% ≈ majoritário; teto de sinal 59-63%; tabela curada > NN |
| **IDEA #492 / #496** | Gate ≥65% específico p/ NN de device; teto medido ✅ |
| **SESSION_255** | Conversão hw_expert v5→v6; h=128 L=6 q_dim=32; 265.620B (NÃO contém "loss 0,389" nem "128 heads") |
| **SESSION_273** | Fonte única `cortex::trinity::TRINITY`; `router_trained` gate (a ADR original citava 293, que é o card de instalação de disco) |
| **AGENTS.md (lição SESSION_233)** | `HEAP_BUFFER` `.bss` + statics em `.data` (arquivo SESSION_233.md não existe) |
| **SESSION_252** | `TOTAL_RAM_MB` de usable ranges; `grow_bump_auto`; fila bounded drop-oldest |
| **SESSION_258** | `watchdog_should_crash`; `set_urgency` por nome do manifest; `has_pending()` |
| **SESSION_310** | Integração SecurityAgent↔`NET_EVENT` (a ADR original citava 152) |
| **SESSION_242** | Mesh: 16 slots reassembly + FRACK seletivo; `peer_p99_rtt`; token bucket; TOFU (a ADR original citava 280) |
| **SESSION_339** | Janela do bump ~2GB; `BUMP_MAX_OFFSET`; recusa antes de mapear |
| **SESSION_352** | Stream de alta taxa = ring SPSC de RAM; publish clona por assinante |
| **SESSION_354** | Timeout que retorna sucesso é pior que hang; GC suspenso no mount Tickv |
| **ADR-0106** | `Decision<T>` EXISTE (`crates/cortex/src/decision.rs:149-156`): choice/confidence/margin/source/abstain; `ScorerAbsent` ≠ 0 |
| **ADR-0100** | `BOOT_AI` **já implementado** (`crates/k_nano/src/boot_report.rs:32`, T-001..T-004 de 0100 — não 0092) |
| **ADR-0088 / 0059 / 0063 / 0082** | AIOS-First; App Factory wasmi; SGDB; HardwareInfo |
| **White Paper "Laya-Zero"** | Referência externa (non-repo) |

---

## 6. Decisões Conflitantes Resolvidas

| Conflito | Resolução |
|----------|-----------|
| Laya vs Trinity MoE | Laya = System 1 (rápido, k-ai); Trinity = System 2 (router MoE wired no boot, SESSION_273). Não unificar. (A formulação original "Trinity dorme 95% do tempo" era má-atribuição da política de downgrade Continuous→EventDriven.) |
| Classificador ausente | `abstain(ScorerAbsent)` (ADR-0106), nunca `Intent::Normal` — "fallback Normal" é fail-open com nome de segurança. |
| Triagem vs detectores existentes | Aditiva: Laya vota no `correlate`; **não pula** detectores; drop exige chokepoint inexistente → fora de escopo até ADR própria. |
| RAM gate | Removido: artefato de 265KB não precisa de gate; política real = `heap_budget_mb`/`grow_bump_auto`/`BUMP_MAX_OFFSET`. |
| NoProto mesh vs IPC interno | Point-to-point hoje; broadcast+TOFU na Fase 2; `mesh-transport` na Fase 3. |
| Scheduler | Hot path só com budget TSC + sem lock através do forward + contabilidade de drop na fila (itens 1-3 da §3.3). |

---

## 7. Próximos Passos

1. ~~Dispatch de especialistas já realizado~~ — **corrigido**: `exp-1/2, lib-1/2, fix-1/2` eram labels de lanes dentro de sessions numeradas (237/252/316/392...), não sessions reconciliadas. A revisão real ocorreu em **2026-09-25 (SESSION_407)** via @explorer (verificação de claims) + @oracle (julgamento arquitetônico); complementada no mesmo dia por @librarian (modelo Laya real + agent-jev) + @explorer (superfície System-1 no repo) — ver §10.
2. Executar IDEA #611 — **rota escolhida no adendo §10: destilação do contrato Laya sobre o `decide_intent`/router BitNet existentes** (não rodar o modelo Laya no metal). Dataset + gate ≥65% + ECE **antes** de qualquer implementação de classificador.
3. Extensão trivial permitida desde já: `TaskType::LayaIntent = 8` no NoProto.
4. Lifecycle só sai de `por_fazer` quando existir evidência em `docs/evidence/laya-zero/` e a decisão de escopo for re-aprovada (GOVERNANCE.md: status espelha verificação registrada).

---

## 8. Metadados

| Campo | Valor |
|-------|-------|
| **ID** | 0110 |
| **Substitui** | Nenhum (nova ADR) |
| **Substituído por** | — |
| **Conflito ID** | — |
| **Ideas** | **#611** (Laya-Zero: dataset+gate p/ System 1 real); relacionadas #492/#496 |
| **Sprints impactados** | Fase 1: TBD (após #611); Fase 2/3 inalterados |
| **Evidence directory** | `docs/evidence/laya-zero/` — **não existe ainda**; obrigatório antes de ACCEPTED |

**Assinatura**: Workflow Manager — 2026-09-22 · Revisão: SESSION_407 — 2026-09-25
**Próxima revisão**: quando IDEA #611 produzir dataset + medição de gate.

---

## 9. Integração com Infraestrutura Existente (revisada)

### 9.1 Contrato `Decision<T>` (ADR-0106) — VERIFICADO, existe
`crates/cortex/src/decision.rs:149-156`: `Decision<T: Copy, const N: usize> { choice: Option<T>, confidence: Confidence(u8 Q8), margin: Confidence, source: DecisionSource, abstain: Option<AbstainReason>, dist: Q8Dist<N> }`. O futuro classificador Laya deve retornar `Decision<Intent>` com calibração (ECE/θ) — abstenção é resultado de primeira classe, não `else`.
**Convergência verificada (adendo §10):** as primitivas typed-decision do Laya real (boolean/choice/score + margin + softmax por questão, zero tokens) são **o mesmo contrato** que `Decision<T>` já implementa — o repo fala a língua do System 1 sem mudança de interface.

### 9.2 NoProto `TaskType::LayaIntent = 8` — IMPLEMENTADO (protocolo apenas, SESSION_408)
`noproto.rs:60`: `LayaIntent = 8` após `Shutdown = 7`; teste `laya_intent_discriminant_and_header_size` (discriminante + header 37B). Sem produtores/consumidores: variante ignorada por padrão, zero mudança de fio. Cabeçalho real do pacote: **37 bytes** medidos (não 36). Triagem runtime (§9.5) segue bloqueada por #611 (sem scorer, sem tipo de decisão do domínio de segurança).

### 9.3 Gate de RAM
**Removido desta ADR.** Se um dia o System 1 real for um artefato de centenas de MB, aplicar a política vigente: `heap_budget_mb`/`heap_observe().headroom_mb` + recusa **antes de mapear** contra a janela do bump (`BUMP_MAX_OFFSET`, SESSION_339) — não o gate inventado `TOTAL_RAM_MB>=8192 && HEAP_FREE_MB>200`.

### 9.4 BOOT_AI
**Já existe** (`boot_report.rs:32`, `publish_boot_ai()` :238; atribuição correta: ADR-0100 T-001..T-004). Quando o classificador existir: **estender** a linha com contadores `observe/plan/act/verify` da triagem Laya, respeitando o contrato de severidade ADR-0092 e o canal FB-vs-serial (SESSION_289/330).

### 9.5 SecurityAgent Triage (reescrito fail-closed)
`SecurityAgent::tick()` → `k_ai::laya::classify(payload)`:
- `Intent` com confiança ≥ θ → **vota** no `correlate` existente (soma ao pipeline, não o substitui; sem skip de detectores).
- Classifier ausente/não-inicializado → `abstain(ScorerAbsent)` — **não** `Intent::Normal`.
- Abstenção baixa/margin de empate → `abstain(Tie/LowConfidence)`, sem efeito.
- **Sem drop**: o SecurityAgent consome resumos de texto do EventBus, não pacotes; enforcement inline exige chokepoint na pilha de rede a especificar em ADR própria.
- Forward sob budget TSC por tick; lock do modelo nunca segurado através do forward; drops de `NET_EVENT` contabilizados (§3.3).

---

## 10. ADENDO — Fatos verificados do modelo Laya e rota de destilação (2026-09-25, SESSION_407)

Fonte: estudo do white paper original (texto-fonte desta ADR) + verificação externa (@librarian: huggingface.co/convaiinnovations/laya, github.com/NandhaKishorM/laya, github.com/malevrigns/agent-jev) + mapeamento da superfície System-1 no repo (@explorer).

### 10.1 O Laya real (não o Laya do paper)
- **Convai Innovations** (não "lab chinês"; AXERA-TECH/Laya é só pacote de deploy NPU do mesmo modelo). **Apache-2.0**.
- Arquitetura: **ModernBERT-large 395M fine-tuned + head de decisão (2 layers + option-marker scorer + act/escalate) = 421M params; encoder não-autorregressivo, zero tokens gerados** — a premissa conceitual do paper **confirma-se**; model card usa literalmente "System 1".
- Checkpoints: `laya` (EN, ctx 512) · `laya-multilingual` (mmBERT-base 322M, ctx 1024–8192) · `laya-typed-decisions` (especialista). Primitivas de saída: **boolean / choice (2–255, com margin + distribuição) / score (ordinal)** — **contrato idêntico ao `Decision<T>` da ADR-0106 (§9.1)**.
- **Números do paper refutados**: tamanho real **842MB fp16** (não 150–300MB; INT8 não-oficial ~320–420MB); latência **39,5ms p50 em GPU T4 / 193–464ms em CPU** (não "<30ms" — em nosso metal soft-float, o caso CPU vale: **4–9 ticks PIT de forward congelariam o scheduler cooperativo**, S328/AGENT_TICK_BUSY); **zero-shot quase ao acaso** (0,362 vs 0,318 random; ECE cru 0,466; model card: *"a fast base to specialise, not a zero-shot decision engine"*); checkpoint EN colapsa em scripts não-latinos (Khmer 0,000 acc @ 0,952 conf).
- Consequência: **rodar o Laya real no Ring 0 está REJEITADO** — exige motor denso novo (o repo só tem BitNet/W2A8 ternário), 842MB residentes, latência 200–460ms e acurácia nula sem fine-tune. O "Laya de 150–300MB <30ms" do paper é um modelo que não existe.

### 10.2 agent-jev (a análise que o paper fabricou)
`malevrigns/agent-jev` = **AgentJev-0.6B** (Apache-2.0): Qwen3-0.6B **decoder** sem LM head reconvertido em classifier (head permutação-equivariante, KV-reuse de prefixo), primitivas boolean/choice/score, 79,25% no typed-decisions vs 77,00% do Laya especialista (ambos fine-tunados no mesmo split — não é superioridade zero-shot). É um terceiro competidor da mesma categoria, não um "daemon Python sobre Linux" como o paper imaginou. Lição absorvível: o gate `PreToolUse` deles (bloqueia só se score=3 E boolean=unsafe; **fail-open se o serviço cai** — nosso padrão é o oposto: `abstain(ScorerAbsent)`, §9.5) e a recusa honesta de input over-length (nunca truncar silenciosamente — alinha com S252/Range-truncado).

### 10.3 O que o repo JÁ tem (a visão do paper é parcialmente implementada, em versão artesanal)
- **System-1 wired em duas camadas**: `cortex::intent_decide::decide_intent` (15 arms, Q8 dist, θ_conf=110/θ_margin=12, abstain→LLM — `intent_decide.rs:153`, no tick Hermes `agents.rs:1949`) e Trinity `classify_keywords` + ROUTER.BITNET treinável (`trinity.rs:267/473`, counters neural/keyword/fallback). **O "reflexo à frente do córtex" existe** — é hand-weighted, não aprendido.
- `Decision<T>` = contrato typed-decision do Laya (§9.1); SelfHeal `FailureClass::classify` = triagem de anomalia sem LLM; difficulty_gate = budget tier determinístico; CapGate/HITL ≈ o gate de syscall do paper.
- Ausente de verdade vs. paper: classifier de intenção **aprendido**; LLM→op-IR (stub `evolve.rs:133`); fast-path sub-tick orientado a evento (tudo é ~55ms tick — o "mesh <5ms" e o "N-IRQ em µs" do paper são incompatíveis com o transporte atual, heartbeat real ~6s); lifecycle de *agente* WASM efêmero (execução efêmera existe via wasmi+fuel).

### 10.4 Rota aprovada para #611: DESTILAÇÃO, não execução
O valor do Laya para o AIOS é o **contrato e o método**, não o artefato de 421M:
1. **Promover `decide_intent` de hand-weighted → trained**: mesmas 15+ arms de intenção como cabeçalho de decisão; backbone BitNet ternário pequeno (classe hw_expert, ~1M params — não BERT denso), export v6, loader existente. Zero motor novo.
2. **Treino no host** com forward Rust-exato + validação do ARTEFATO exportado (doutrina S247/248), dataset pacote/intenção→rótulo com labels NÃO-circulares, holdout disjunto (S346).
3. **Gate inegociável** (#611): ≥65% específico + ECE calibrado (o Laya cru tinha ECE 0,466 — mesmo destino esperaria nosso router; refit de temperatura é parte do entregável) + evidência em `docs/evidence/laya-zero/`.
4. Saída sempre `Decision<Intent>` (§9.1); triagem aditiva fail-closed (§9.5); `TaskType::LayaIntent=8` liberado desde já.
5. Se um dia um encoder denso pequeno (não o Laya de 421M) se justificar, reabrir com gate de residência pela política real de heap (§9.3) — hoje: nenhum artefato >~1MB é elegível para residente r0 sem nova ADR.

**Em uma linha:** o paper acertou o destino (reflexo aprendido à frente do córtex) e errou o veículo (rodar um encoder 421M fp16 no metal); o veículo certo é destilar o contrato dele sobre o BitNet router que o repo já carrega, sob o gate #611.
