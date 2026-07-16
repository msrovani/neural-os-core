# ADR-0042: Adequação Boot OK → Visão K²CHJ (hierarquia de anéis + função)

**Data:** 2026-07-14 · atualizado 2026-07-16  
**Status:** Accepted — plano diretor de adequação (**N1 done**; **N2 ✅ CLOSED**; **N3 ✅ CLOSED**)  
**Depende de:** ADR-0041 (capability PoC P0–P9), Pacotes A/B boot  
**Sprint:** ADR-0042 (pista ativa N4→N5). Sprint 107 Voice ✅ FECHADA; leftovers voz → Sprint Sound.  
**Release:** conclusão de **N1–N5 = versão `v2.0.0`**. Até lá: linha **`1.x`** de adequação.  
**Policy:** `1.5.7` = Cap PoC + boot OK; **`v1.7.0`** (2026-07-15) = marco N1 ✅ + BitNet 2B LOADED (N3 parcial); **`v1.7.4`** (2026-07-16) = N2 ✅; **`v1.7.5`** (2026-07-16) = N3 ✅ CLOSED (cortex cérebro; N3.5 crate link deferred). 1.6.0-dev absorvida (sem tag 1.6.0 vazia).  
**Não declarar `v2.0.0` até N1–N5.**


---

## 1. Contexto

O boot UEFI alcançou **Runtime estável** (AgentFleet → AgentScheduler + timer) no monólito `neural-kernel`. Os PoCs Cap P0–P9 (ADR-0041) provaram mecânicas, **não** a visão de produto. Adequar sem regredir o Runtime.

**Regra de versionamento:** não chamar o tree atual de “v2.0 feito”. `v2.0.0` marca **só** quando a cadeia funcional K²CHJ (N1–N5) estiver adequada à visão (legível → HW-AI → cérebro → orquestra → ego).

### Cadeia canônica (dependência + anel)

```text
k-nano → k-ai → cortex → hermes → jarbas
```

Sem ciclos. Camada de cima orquestra; a de baixo não conhece persona/UI.

### Identidade funcional

| Anel | Função | É | Não é |
|------|--------|---|--------|
| **k-nano** | Sistema **legível** | Tempo, mem, traps, Caps, CR3, drivers brutos, log honesto, scheduler mínimo | Persona, LLM, apps, humor |
| **k-ai** | AI **para hardware** + autonomia de máquina | SelfHeal, Trust, inventário HW, HMI de plataforma, FW/agentes HW | Personalidade; criação de apps |
| **cortex** | O **cérebro** | Tensores, MoE, aprendizado, seleção de experts, busca/retrieval, mmap pesos | Intent de usuário; compositor |
| **hermes** | O **orquestrador** agentic | Intent→skill, ReAct, WASM/SFI, criar conteúdo/apps/skills, aprendizado de fluxo | Ego; pixels finais |
| **jarbas** | O **ego / consciência / persona** | Interface, humor, voz, “sempre +10%”, intelecto situado, frontend | Drivers; matmul; caps de kernel |

---

## 2. Decisão

1. Tratar o boot OK como **N0 baseline verde** — regressão = falha.  
2. Adequar em fases **N1→N5** alinhadas à cadeia (não anéis “paralelos iguais”).  
3. Telemetria **honesta** (LOADED | ABSENT | FAILED) — sem SUCCESS falso.  
4. Caps/IPC sobem a cadeia; jarbas não fala ATA/PCI direto.  
5. PoCs perigosos (Ring3 `iretq`) atrás de flag até estáveis.

### Non-goals imediatos

- Binários separados por crate antes de N1–N2 estáveis  
- Inverter ordem (jarbas antes de hermes; Hermes “LLM 24/7” com cortex ABSENT sem declarar)  
- Desfazer Pacotes A/B ou Runtime  

---

## 3. Fases N0–N5

| Fase | Anel | Missão | Aceite resumido |
|------|------|--------|-----------------|
| **N0** | — | Baseline boot OK | Runtime + timer (já ✅ 2026-07-14) |
| **N1** | k-nano | Sistema legível | Telemetria única; Cap authority; log limpo QEMU; métricas scheduler |
| **N2** | k-ai | HW-AI + SelfHeal + HMI máquina | Heal/noop explícito; HEALTH_ISSUE; inventário gated por VID |
| **N3** | cortex | Cérebro real | Modelo LOADED se no FAT; MoE executa; Cap MAP_WEIGHTS; 1 prompt→texto |
| **N4** | hermes | Orquestra / cria | WASM SFI; skills on demand; intent e2e; IPC→jarbas |
| **N5** | jarbas | Ego / UI / +10% | Compositor vivo; persona; voz como expressão; só via Hermes |

**Gate de release `v2.0.0`:** N1–N5 com gates de qualidade desta ADR (Runtime intacto + telemetria honesta + Caps/IPC por anel).  
**Até `v2.0.0`:** tags/`CHANGELOG` em **`1.x`** (1.5.7 Cap; **1.7.0** = N1 + 2B LOADED; **1.7.5** = N3 CLOSED).

**Ordem fechada:** N0→N1→N2→N3→N4→N5.  
**Paralelo:** drivers VirtIO/DMA em nano+k-ai durante N2 **sem** UI. Proibido jarbas antes de Hermes mínimo.

### Checklist N1 (k-nano legível)

| Item | Aceite | Status |
|------|--------|--------|
| **N1.1** Telemetria honesta | `LoadStatus` + `[STATUS]` coerente com LLM-TEST; zero “2B carregado” falso | ✅ 2026-07-15 |
| **N1.2** Cap + probes limpos | NVIDIA FW só com VID 0x10DE; Cap DENY demos documentados | ✅ |
| **N1.3** Métricas scheduler | Log periódico `[SCHED] tick/agents/polled` pós-Runtime | ✅ código (hook); re-flash uefi se log slim não mostrar |
| **Goal N1** | Log legível; QEMU limpo de FW NVIDIA spam | ✅ |

### Checklist N2 (k-ai HW-AI / SelfHeal)

| Item | Aceite | Status |
|------|--------|--------|
| **N2.1** Heal/noop explícito | Serial `[N2-SELFHEAL] heal\|noop` por VID; sumário `done scanned/noop/heal` | ✅ 2026-07-16 QEMU |
| **N2.2** HEALTH_ISSUE | I3/I4 no EventBus para VID conhecidos; senão log `honest noop (fw_gated=0)` | ✅ |
| **N2.3** Inventário VID-gated | `fw_gated_devices` + subclass fine-gate (Intel net ≠ e1000 02:00) | ✅ |
| **N2.4** Trust (token, agent, skill) | `trust_allow_agent` + Trust antes de SelfHeal; serial `[TRUST] allow (token,agent,skill)=…` | ✅ |
| **N2.5** Link crate `k_ai` no bin | Dep direta neural-kernel→k_ai | ⏳ N2.5 — bloqueado (`#[global_allocator]` k_nano vs monólito); **comportamento via espelho** até então |
| **Goal N2** | Heal honesto + inventário gated + Trust agentic no boot path | ✅ **CLOSED** (critérios funcionais; wire crate = N2.5) |

**Evidência serial:** `logs/boot_n2_20260716_131837.txt` (WHPX short) — Trust allow + inventory + honest noop. Path HEALTH_ISSUE heal também visto em `logs/boot_n2_20260716_131655.txt` (pré fine-gate e1000).  
**Espelho:** `neural-kernel` espelha `k_ai` SelfHeal/Trust/inventory até N2.5; hermes já usa `k_ai` real.

### Checklist N3 (cortex cérebro)

| Item | Aceite | Status |
|------|--------|--------|
| **N3.1** Modelo LOADED | `[STATUS] llm=LOADED` quando BitNet no loader/FAT; telemetria honesta ABSENT/FAILED | ✅ 2026-07-16 QEMU |
| **N3.2** Cap MAP_WEIGHTS | P5 `demo_cortex_mmap` SUCCESS + gate `MAP_WEIGHTS pages>0` | ✅ |
| **N3.3** MoE / Trinity | Experts registrados (≥6, generator OK); HWEXPERT+RustCoder LOADED; router MoE neural = ABSENT→keyword+R3 (honesto) | ✅ |
| **N3.4** prompt→texto | Path `generate_via_model` / weather-e2e HIT **ou** gate `generate=GATED soft-float` + evidência prior | ✅ live GATED + prior `decoded_len=12` |
| **N3.5** Link crate `cortex` no bin | Dep direta neural-kernel→cortex | ⏳ N3.5 — monólito espelha cortex até wire crate (padrão N2.5) |
| **Goal N3** | Cérebro: LOADED + Cap pesos + Trinity wiring + generate path | ✅ **CLOSED** (critérios funcionais; fluency soft-float → Sound; crate = N3.5) |

**Evidência serial N3:** `logs/boot_n3_20260716_132753.txt` (WHPX short) — `[STATUS] llm=LOADED` + `[N3-CORTEX] … criteria=MET`.  
**N3.4 prior HIT:** `logs/boot_whpx_20260716_110041.txt` — `[GEN] decoded_len=12 text='O tempo esta'` (feature `weather-e2e`).  
**Defer:** soft-float latency / chat fluente → Sprint Sound; link crate cortex → N3.5; hermes/jarbas pleno → N4/N5.

---

## 4. Mapa boot OK atual → anel dono

| Trecho do boot | Anel futuro | Ação |
|----------------|-------------|------|
| FB/IDT/heap/STI/FAT bruto | k-nano | N1 |
| PCI sync, probes, FW checks, SelfHeal ready | k-ai | N2 |
| CortexAgent, Trinity, mmap P5–P9 | cortex | N3 |
| WASM, HermesAgent, CapGate | hermes | N4 |
| splash, DisplayAgent, voz/persona | jarbas | N5 |
| Demos Cap no boot | k-nano (provas) | Migrar para testes; limpar boot |

---

## 5. Gates

1. `cargo check --release -p neural-kernel` = 0  
2. QEMU UEFI: Runtime + timer ≥ 5 ticks  
3. Atualizar STATE + checklist desta ADR  
4. Nenhuma linha SUCCESS negada por teste seguinte  

---

## 6. Relação com ADR-0041

ADR-0041 = **PoC mecânico** Cap/AS/Ring3.  
ADR-0042 = **adequação de produto/anel** Boot OK → identidades K²CHJ.  
**N1** ✅ (v1.7.0). **N2** ✅ CLOSED (v1.7.4; N2.5 = link `k_ai`). **N3** ✅ CLOSED (v1.7.5; N3.5 = link `cortex`). Próximo: N4→N5.

---

## 7. IDEAs

Ver IDEA_BANK #433–#438 (N1–N5 + cadeia canônica).
