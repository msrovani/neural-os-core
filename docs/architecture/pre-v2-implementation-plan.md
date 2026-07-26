# Plano de Implementação — PRE-V2 + Residuals

**Data:** 2026-07-26
**Status:** Proposed — aguardando autorização para execução
**Estimativa total:** 15 itens · ~22.000 LOC · 8-12 semanas · 5 engenheiros simultâneos
**Gate:** v2.0.0 = todos os itens Tier 0-1 completos + boot QEMU 8 fases + `[TIMER] tick=`

---

## Estrutura de Dependências

```
Tier 0 (sem bloqueios, paralelizável)
├── §1  VectorStore TF-IDF
├── §4  Self-Learning OS
├── §10 Success Engine
├── §11 Security Pipeline
│
Tier 1 (depende de infra existente, sem blocker externo)
├── §2  Dynamic MoE
├── §8  Self-Optimization
├── §13 JARVIS Features
├── §14 Agents Evolution
├── §16 Developer Tooling
│
Tier 2 (depende de port/review de código externo)
├── §15 Cross-OS Compatibility
│
Tier 3 (bloqueado por HW / pós-v2.0.0)
├── §3  GPU Golden HW ───┐
├── §12 GPU Foundations ──┤── AWAITING_HW
├── §5  NPU Full Dispatch ┘
├── §4  Ring3/SFI ───────── Pós-v2.0.0
└── §7  Perci/Bitwork ───── Maintainer OK
```

---

## FASE 0 — Infraestrutura (Semanas 1-2)

Paralelizável em 4 engenheiros.

---

### §1 — VectorStore TF-IDF (RAG in-kernel)

**Fonte:** ADR-0064 · **Conexões:** ADR-0063 (SGDB persistência), ClaudioOS `vectordb.rs` (1.062 LOC referência)
**Prioridade:** 🔴 Alta · **LOC:** ~1.000 · **Dias:** 3-5

| Step | Ação | Arquivo(s) | Verificação |
|------|------|-----------|-------------|
| 1.1 | Criar crate `crates/vector-db` com Cargo.toml no_std+alloc | `vector-db/Cargo.toml`, `vector-db/src/lib.rs` | `cargo check -p vector-db` |
| 1.2 | `tokenize()` — lowercase, split não-alfanumérico, stopwords EN+PT-BR | `vector-db/src/tokenize.rs` | "Rust kernel" → `["rust","kernel"]` |
| 1.3 | `ln_f32()` + `sqrt_f32()` sem libm (IEEE 754 bit tricks) | `vector-db/src/tfidf.rs` | acurácia > 4 dígitos decimais |
| 1.4 | `compute_tfidf()` + `cosine_similarity()` | `vector-db/src/tfidf.rs` | identity=1.0, orthogonal=0.0 |
| 1.5 | `VectorStore` struct — insert, search(top_k), delete, update | `vector-db/src/store.rs` | search devolve top-k relevante |
| 1.6 | Serialização JSON — to_json/from_json (reusa serde_json existente) | `vector-db/src/json.rs` | roundtrip: insert→serialize→load→assert |
| 1.7 | `demo()` self-check assert-based | `vector-db/src/lib.rs` | 3 docs → search "Rust kernel" → top resultado contém "Rust" |
| 1.8 | Thread-safety: `spin::Mutex<VectorStore>` global | `vector-db/src/store.rs` | acesso concorrente seguro (2 agents simultâneos) |
| 1.9 | Integrar Cortex: search() antes de LLM request, insert() pós-resposta | `cortex/src/cortex.rs` ou `cognitive_bridge.rs` | log `[VECTORDB] search top: ...` no serial |
| 1.10 | Integrar Hermes: RAG para skills (kind=Skill query no search) | `hermes/src/cognitive_bridge.rs` | "preciso ler arquivo" → recupera skill file_read |

**Resultado esperado:** RAG on-device, zero dependência de MCP externo. Segunda pergunta de uma conversa recupera contexto da primeira.
**Goal:** Boot QEMU, conversa 2-turn: 1º "qual o clima?" → 2º "e amanhã?" → contexto do 1º preservado.

---

### §4 — Self-Learning OS (IDEA #313)

**Fonte:** IDEA_BANK §1.28 · **Conexões:** EventBus, TrainingAgent, Trinity Hub
**Prioridade:** 🔴 Alta · **LOC:** ~800 · **Dias:** 4

| Step | Ação | Verificação |
|------|------|-------------|
| 4.1 | `DataCollector` — subscreve EventBus, buffer circular com últimos N eventos | `[DATACOLLECTOR] collected N=1000 events` |
| 4.2 | Estruturar eventos como pares (input_text, output_text) para treino | gera arquivo `.jsonl` no FAT |
| 4.3 | Pipeline completo: LogAgent → DataCollector → TrainingAgent → .bitnet → Trinity Hub | modelo .bitnet gerado e registrado |
| 4.4 | Boot loading condicional: carrega modelo self-trained se existir, senão default | `[BOOT] Using self-trained model from boot N-1` |

**Resultado esperado:** Sistema que coleta seus próprios dados de uso, treina um modelo e carrega no boot seguinte — ciclo fechado de auto-aprimoramento.
**Goal:** Boot N+1 carrega modelo treinado exclusivamente com dados gerados no Boot N.

---

### §10 — Success Engine — Feedback Loop (IDEA #149–#152)

**Fonte:** IDEA_BANK §1.20 · **Conexões:** HermesAgent, SleepCycleAgent, k_ai::ternary
**Prioridade:** 🔴 Alta · **LOC:** ~800 · **Dias:** 4

| Step | Ação | Verificação |
|------|------|-------------|
| 10.1 | Adicionar 👍/👎 no card de resposta do Hermes (via CARD_ACTION) | clique do usuário publica `FEEDBACK` no EventBus |
| 10.2 | Buffer de experiência circular — últimas 100 interações com feedback | `[SUCCESS] replay buffer: N=100` |
| 10.3 | SleepCycleAgent REPLAY phase consome buffer — 64 amostras por ciclo | REPLAY executa fine-tuning on-device |
| 10.4 | Ternary weight update on-device — {-1,0,+1} via Straight-Through Estimator | pesos se ajustam sem FPU |
| 10.5 | Export modelo atualizado para .bitnet no FAT32 | `[SUCCESS] model exported: self_v2.bitnet` |

**Resultado esperado:** Cada feedback do usuário (👍/👎) ajusta o modelo. Respostas melhoram com o uso.
**Goal:** 10 feedbacks consecutivos → mudança mensurável nos logits (comparação antes/depois).

---

### §11 — Security Pipeline (IDEA #260–#264)

**Fonte:** IDEA_BANK §1.20 (Tier 3 Security) · **Conexões:** EventBus, SecurityAgent (A-018), TrustCache
**Prioridade:** 🔴 Alta · **LOC:** ~1.080 · **Dias:** 6

| Step | Ação | Verificação |
|------|------|-------------|
| 11.1 | Detector PortScan — monitora conexões TCP a portas sequenciais em < 1s | `[SECURITY] PortScan DETECTED from 10.0.2.2` |
| 11.2 | Detector ArpSpoof — monitora respostas ARP com MAC conflitante | `[SECURITY] ArpSpoof: IP .1 tem 2 MACs` |
| 11.3 | Detector PingFlood — > 100 ICMP/s de mesmo source | `[SECURITY] PingFlood from 10.0.2.2` |
| 11.4 | Detector DhcpStarvation — > 50 DHCP DISCOVER/s mesmo MAC | `[SECURITY] DhcpStarvation` |
| 11.5 | Detector TimerAnomaly — tick drift > 10% indica manipulação de tempo | `[SECURITY] TimerAnomaly: drift +12%` |
| 11.6 | Pipeline Event→Detector→Response — detector stateful → correlação → ação | `[SECURITY] pipeline: alert→correlate→respond` |
| 11.7 | Decision Review + Human Escalation — timeout N ticks, auto-resolve se expirar | baixa confiança → `ESCALATE` com timeout |
| 11.8 | Hash Chain Audit Trail — SHA-256 chain ligando cada evento ao anterior | `verify_chain()` retorna `true` |
| 11.9 | Knowledge Graph — 6 node types (Process, NetEndpoint, File, Skill, Hardware, User) + ~20 relations | busca: "quais processos usaram a rede?" |
| 11.10 | Cross-Layer Correlation — 5 regras: ARP spoof + port scan → Data Exfil alert | alerta multi-estágio com severidade escalada |

**Resultado esperado:** Kernel com detecção de intrusão nativa, audit trail imutável, e correlação cross-layer.
**Goal:** Port scan saindo de VM vizinha → `[SECURITY] PortScan BLOCKED` em menos de 1 segundo.

---

## FASE 1 — Evolução Core (Semanas 3-5)

Paralelizável em 4 engenheiros.

---

### §2 — Dynamic MoE (Birth/Merge/Split)

**Fonte:** ADR-0060 A.3 · **Conexões:** `cortex::moe` (estático), `cortex::trinity`, `k_ai::economy::BudgetManager`
**Prioridade:** 🔴 Alta · **LOC:** ~1.600 · **Dias:** 10

| Step | Ação | Arquivo | Verificação |
|------|------|---------|-------------|
| 2.1 | Estender `MoEConfig` com campos: `max_experts`, `birth_threshold`, `merge_similarity`, `split_call_threshold` | `cortex/src/moe.rs` | defaults backward-compatible |
| 2.2 | `MoE::birth(action: IntentAction)` — clona expert mais próximo, muta pesos aleatoriamente, registra no Trinity | `cortex/src/moe.rs` | novo expert visível no hub |
| 2.3 | `MoE::merge(id_a, id_b)` — cos similarity > threshold, funde pesos (média ponderada por acurácia) | `cortex/src/moe.rs` | merge preserva capacidade total |
| 2.4 | `MoE::split(id)` — expert com >N chamadas/1000 ticks divide em 2, metade dos pesos cada | `cortex/src/moe.rs` | split mantém cobertura do domínio |
| 2.5 | Registrar birth/merge/split no `ExpertLifecycleManager` — log + metadados | `k_ai/src/expert_lifecycle.rs` | `[MOE] birth expert #4 (hw_identify split)` |
| 2.6 | BudgetManager consultado antes de birth — respeita `max_experts` e orçamento de memória | `k_ai/src/economy.rs` | `[MOE] birth denied: budget limit` |
| 2.7 | `demo()` — 3 experts base, força gap de intent, birth dispara | self-test | birth → merge → split → tudo OK |

**Resultado esperado:** Trinity MoE que cresce/encolhe conforme a demanda de intenções. Experts nascem para cobrir gaps, fundem-se quando redundantes, dividem-se quando sobrecarregados.
**Goal:** 3 experts base. Após 100 intenções de um novo domínio → 4 experts. Merge após similaridade > 0.9.

---

### §8 — Self-Optimization / Workflow Learning (IDEA #157–#163)

**Fonte:** IDEA_BANK §1.22 · **Conexões:** HermesAgent, Scheduler, MHI tiers
**Prioridade:** 🔴 Alta · **LOC:** ~1.250 · **Dias:** 8

| Step | Ação | Conexão | Verificação |
|------|------|---------|-------------|
| 8.1 | Usage Pattern Analyzer — LLM recebe últimas N intenções, classifica workflow (dev/escritório/música/...) | HermesAgent + Cortex | `[WORKFLOW] detected: 'desenvolvimento' confidence=0.85` |
| 8.2 | Workflow Predictor — baseado em hora+dia, pré-carrega MHI tiers | MHI ARC suggest_tier | `[PREDICT] 14:30 sex — preload 'dev' tier=Dram` |
| 8.3 | Dynamic Resource Scaling — MHI monitora hit rate e ajusta tiers sem reboot | MHI + EventBus | tier migra Dram↔Nvme por uso real |
| 8.4 | Self-Optimizing Scheduler — agent priority ajustada por workflow detectado | AgentScheduler | agent Render tem prioridade durante workflow "edição" |
| 8.5 | Workflow Profile — exporta/importa perfil para VFS | `profiles/workflow_dev.json` | salva em FAT, carrega no boot |

**Resultado esperado:** Scheduler e memória que se adaptam automaticamente ao padrão de uso do usuário.
**Goal:** Após 1 hora de uso consistente, scheduler automaticamente dá prioridade aos agents corretos no horário correto.

---

### §13 — JARVIS Features (IDEA #315.x)

**Fonte:** IDEA_BANK §1.31 + ADR-0036 · **Conexões:** `hermes::affect`, `jarbas::display`, EventBus
**Prioridade:** 🟡 Média · **LOC:** ~1.450 · **Dias:** 10

| Step | Ação | Arquivo | Verificação |
|------|------|---------|-------------|
| 13.1 | SOUL.md Personality Engine — parser de manifesto YAML-like + tom adaptativo (coach/tutor/tool) | `hermes/src/soul.rs` | SOUL.md carregado no boot, tom muda conforme contexto |
| 13.2 | Notification Gate — 4 urgências (Critical/High/Info/Debug), rate limiting (max 5/s), dedup (hash 30s) | EventBus → `notifications.rs` | notificação duplicada em <30s → descartada |
| 13.3 | Emotion Analysis — BitNet classifier 7 emoções, `adjust_tone()` modula resposta | `hermes/src/affect.rs` | "estou muito triste" → valence=-0.7, tom acolhedor |
| 13.4 | Persona Pipeline — 16 stages (entrada→classify→context→emotion→adapt→generate→output) | `hermes/src/persona.rs` | cada stage logado no serial |
| 13.5 | Proactive Heartbeats — CronAgent dispara JARVIS em idle > 30s | CronAgent | `[JARVIS] Posso ajudar em algo?` |
| 13.6 | Tool-State Save Game — `snapshot()` + `rollback()` de estado de skills | skill-registry | rollback pós-falha restaura skill ao estado anterior |
| 13.7 | Fail-Closed Safety — 4 invariantes (I1 heap ok, I2 agents alive, I3 trust intact, I4 scheduler tick) | SecurityAgent | invariantes checados a cada tick, violação → shutdown |
| 13.8 | Merkle Audit Trail — Ed25519 chain, ring buffer 4096 entradas, `verify_chain()` | `hermes/src/audit.rs` | 10 eventos encadeados → verify PASS |

**Resultado esperado:** JARVIS com personalidade definida por SOUL.md, emoções que modulam tom, proatividade em idle, e auditoria imutável.
**Goal:** Boot → SOUL.md carregado → `[JARVIS] Oi! Como posso ajudar hoje?` (tom: coach).

---

### §14 — Agents Evolution (IDEA A-001–A-020)

**Fonte:** IDEA_BANK §1.28 · **Conexões:** `agent-core`, Scheduler, EventBus
**Prioridade:** 🟡 Média · **LOC:** ~1.800 · **Dias:** 12

| Step | Ação | Arquivo | Verificação |
|------|------|---------|-------------|
| 14.1 | AgentScheduler CFS — vruntime-based fairness, substitui round-robin | `agent-core/src/scheduler.rs` | 2 agents CPU-bound → 50% do tick cada |
| 14.2 | Capability-Based Routing — EventBus filtra receivers por capability declarada no AgentManifest | EventBus + manifest | evento `NET_PACKET` → só NetAgent recebe |
| 14.3 | Agent Budget + Watchdog — tick_budget por ciclo (configurável), se excede → pausa | `agent-core/src/budget.rs` | watchdog pausa agent runaway, log + resume |
| 14.4 | Agent Hooks — PreTick/PostTick hooks, HookRegistry com slots fixos | `agent-core/src/hooks.rs` | hook retorna Allow/Block/Modify |
| 14.5 | Multi-Agent Orchestration — grafo acíclico: sequential → concurrent → handoff → join | `hermes/src/orchestrator.rs` | workflow 3 agents sequential com handoff OK |

**Resultado esperado:** Scheduler justo (CFS), routing por capability (não por nome), agents com budget (anti-runaway), e orquestração por grafo.
**Goal:** Boot com 259 agents, CFS scheduler ativo, zero agents runaway em 10.000 ticks.

---

## FASE 2 — Ecossistema (Semanas 5-8)

---

### §16 — Developer Tooling (IDEA #394, #395, #172, #236)

**Fonte:** IDEA_BANK (múltiplas seções) · **Conexões:** PackageHub, wasmi, Hermes commands
**Prioridade:** 🟡 Média · **LOC:** ~1.700 · **Dias:** 15

| Step | Ação | Verificação |
|------|------|-------------|
| 16.1 | MCP Server — JSON-RPC 2.0 sobre EventBus + SkillRegistry, 4 métodos: list_tools, call_tool, list_resources, read_resource | `[MCP] server listening on event-bus` |
| 16.2 | Plugin Hub / MCP Index — catálogo com AI security scan (verifica imports, fuel budget, CapGate) | scan → `[PLUGIN] VERDICT=SAFE | SUSPICIOUS | BLOCKED` |
| 16.3 | Marketplace Agent — HTTP GET `market.neural.local/search?q=redis` → install → Ed25519 verify → PackageHub register | `market search file_read` → 3 resultados |
| 16.4 | BitNet IDE — editor de texto no Jarbas + "Generate Skill" → Cortex gera WAT → wasmi testa → PackageHub registra | IDE → "ler arquivo" → gera → roda → "Skill criada!" |

**Resultado esperado:** Ecossistema dev completo — MCP, marketplace com verificação, IDE on-device.
**Goal:** BitNet IDE → "crie uma skill que leia arquivos" → gera → compila (wasmi) → executa → mostra resultado no card.

---

### §15 — Cross-OS Compatibility (IDEA #306a-d)

**Fonte:** IDEA_BANK §1.25 + ADR-0062 (ClaudioOS) · **Conexões:** ClaudioOS PE32+/ELF/Win32 code
**⚠️ Due Diligence:** Verificar licenciamento — ClaudioOS é AGPL-3.0. Identificar partes MIT/Apache vs AGPL antes de portar.

**Prioridade:** 🟡 Média · **LOC:** ~3.300 · **Dias:** 30

| Step | Ação | Fonte | Verificação |
|------|------|-------|-------------|
| 15.1 | Auditar código ClaudioOS: inventariar arquivos portáveis por licença | `claudio-os/` repo | planilha de compatibilidade |
| 15.2 | Portar PE32+ loader — MZ header → PE header → sections → imports → relocs | ClaudioOS → nosso | `hello.exe` (MSVC) → roda no AIOS |
| 15.3 | Portar Win32 compat layer — kernel32 (HeapAlloc, CreateFile, etc.) + user32 (MessageBox, CreateWindow) | ClaudioOS → nosso | `MessageBoxA(0, "Hello", "AIOS", 0)` funcional |
| 15.4 | Portar ELF loader + syscall translation — open/read/write/mmap/clone/bRK → agent skills | ClaudioOS → nosso | `/bin/echo hello` via ELF loader |
| 15.5 | Syscall-to-Skill Layer — camada única: syscall (NT ou Linux) → skill request → agent.response | próprio | `open("/etc/passwd")` → DiskAgent.read() |

**Resultado esperado:** Executáveis Windows (.exe) e Linux (ELF) rodando nativamente no AIOS.
**Goal:** `hello.exe` compilado no Windows 11 → copiado para FAT32 → executado no AIOS → "Hello from AIOS!" no terminal.

---

## FASE 3 — Bloqueado por HW (Sem data)

---

### §3 — GPU Golden HW Validation

**Estado:** Código dos 3 vendors implementado. **Nenhum validado em silício real.**
**Bloqueio:** AWAITING_HW — GTX 1050 (NVIDIA Pascal) / AMD gfx1030+ / Intel Gen9 iGPU

| Vendor | Código | Status | Pendente |
|--------|--------|--------|----------|
| NVIDIA Pascal | ACR ✅, D2-D4 ✅, dispatch CUBIN ✅ | ▶️ AWAITING | Boot HW real, log `canary PASS` |
| AMD RDNA | Discovery ✅, PSP ✅, KIQ/MES ✅ | ▶️ AWAITING | Boot HW real, log `canary PASS` |
| Intel Gen9/Arc | Ring alive ✅, GuC boot ✅, COMPUTE_WALKER ✅ | ▶️ AWAITING | Boot HW real, log `canary PASS` |

**Ação quando HW disponível:**
1. Boot physical HW com GPU alvo
2. Verificar log serial: `[GPU] canary_vector_add PASS`
3. Se PASS → `CapToken::GpuReady` concedido → GPU compute desbloqueado
4. Se FAIL → log diagnóstico + fallback CPU (já implementado)

---

### §12 — GPU Foundations (#326–#332)

**Depende de:** §3 GPU golden HW (canário PASS)

| Step | Item | Descrição | LOC | Deps |
|------|------|-----------|-----|------|
| 12.1 | #326 | GPU BAR0/BAR1 mapping UC — mapear PCI BARs como uncacheable para MMIO | ~300 | §3 |
| 12.2 | #327 | GPU doorbell + SPSC job ring — CPU escreve job, doorbell register, GPU executa | ~400 | #326 |
| 12.3 | #328 | VRAM buddy allocator — alocação contígua com coalescing, power-of-2 | ~400 | #327 |
| 12.4 | #329 | Agent.xpu prefill/decode split — CPU faz prefill do prompt, GPU faz decode token | ~400 | #327 |
| 12.5 | #330 | GPU matmul kernel ternário — ADD/SUB via PTX (NVIDIA) / AQL (AMD) / GEN (Intel) | ~300 | HW real |
| 12.6 | #331 | CPU→GPU KV cache DMA — transferir KV cache RAM↔VRAM via DMA engine | ~200 | #330 |
| 12.7 | #332 | XQueue preemptível (XSched) — 3 níveis: pending / in-flight / running | ~600 | #327 |

**Resultado esperado:** BitNet matmul rodando na GPU com 10-25× speedup sobre CPU.
**LOC total:** ~2.600

---

### Ring3/SFI (§4) + NPU (§5) + Perci/Bitwork (§7)

| Item | Status | Gatilho |
|------|--------|---------|
| §4 Ring3/SFI produção | 💤 Pós-v2.0.0 | v2.0.0 gate concluído |
| §5 NPU Full Dispatch | 💤 AWAITING_HW | HW AMD XDNA ou Intel NPU |
| §7 Perci/Bitwork | 💤 Pesquisa | OK do maintainer |

---

## Topologia de Execução (Paralelização Máxima)

```
FASE 0 — Semana 1-2 (4 engenheiros)
  Eng 1: §1 VectorStore TF-IDF (1.000 LOC)
  Eng 2: §4 Self-Learning OS (800 LOC)
  Eng 3: §10 Success Engine (800 LOC)
  Eng 4: §11 Security Pipeline (1.080 LOC)
  Total: ~3.680 LOC

FASE 1 — Semana 3-5 (4 engenheiros)
  Eng 1: §2 Dynamic MoE (1.600 LOC)
  Eng 2: §8 Self-Optimization (1.250 LOC)
  Eng 3: §13 JARVIS Features (1.450 LOC)
  Eng 4: §14 Agents Evolution (1.800 LOC)
  Total: ~6.100 LOC

FASE 2 — Semana 5-8 (3 engenheiros)
  Eng 1: §16 Developer Tooling (1.700 LOC)
  Eng 2-3: §15 Cross-OS Compat (3.300 LOC)
  Total: ~5.000 LOC

TOTAL: ~15.000 LOC core · ~22.000 LOC c/ GPU · 8-12 semanas
```

---

## Gate Checklist (por item)

Cada item só é considerado completo quando:

1. ✅ `cargo check --release` = 0 erros
2. ✅ `demo()` self-test PASS (assert-based, sem framework)
3. ✅ Boot QEMU: 8 fases + `[TIMER] tick=` incrementando
4. ✅ Log de evidência: `[MODULO] ação resultado` no serial
5. ✅ Nenhum `unsafe` novo sem safety comment documentando invariantes
6. ✅ Nenhuma regressão em módulos existentes (comparar log boot antes/depois)
7. ✅ IDEA_BANK.md atualizado com status ✅ e referência cruzada
8. ✅ STATE.md + CHANGELOG.md atualizados
9. ✅ Commit semântico + tag por item completo

---

## Gate v2.0.0

> **Após TODOS os itens Tier 0 + Tier 1 completos, boot QEMU validado com 8 fases + tick, e `cargo check --release` = 0 erros, declaramos v2.0.0.**

O que NÃO bloqueia v2.0.0:
- GPU golden HW (§3) — AWAITING_HW, honesto
- GPU Foundations (§12) — bloqueado por §3
- NPU full dispatch (§5) — AWAITING_HW
- Ring3/SFI (§4) — pós-v2.0.0, item mais complexo
- Perci/Bitwork (§7) — pesquisa, maintainer OK

---

## Referências

- `docs/architecture/pre-v2-residuals.md` — itens não implementados (15 seções)
- `docs/architecture/ideias-v2.md` — catálogo completo com viabilidade/aderência/custo
- `docs/memory/IDEA_BANK.md` — fonte original (336+ ideias)
- `docs/architecture/INDEX.md` — lifecycle das ADRs
- SESSION_220 — auditoria ADR decrescente + IDEA_BANK
