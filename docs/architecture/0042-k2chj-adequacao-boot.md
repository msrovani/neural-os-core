# ADR-0042: Adequação Boot OK → Visão K²CHJ (hierarquia de anéis + função)

**Data:** 2026-07-14 · atualizado 2026-07-15  
**Status:** Accepted — plano diretor de adequação (**N1 done**; N3 em progresso)  
**Depende de:** ADR-0041 (capability PoC P0–P9), Pacotes A/B boot  
**Sprint:** 107+  
**Release:** conclusão de **N1–N5 = versão `v2.0.0`**. Até lá: linha **`1.x`** de adequação.  
**Policy:** `1.5.7` = Cap PoC + boot OK; **`v1.7.0`** (2026-07-15) = marco N1 ✅ + BitNet 2B LOADED (N3 parcial); 1.6.0-dev absorvida (sem tag 1.6.0 vazia).  
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
**Até `v2.0.0`:** tags/`CHANGELOG` em **`1.x`** (1.5.7 Cap; **1.7.0** = N1 + 2B LOADED; N3 generate/TTS ainda parcial).

**Ordem fechada:** N0→N1→N2→N3→N4→N5.  
**Paralelo:** drivers VirtIO/DMA em nano+k-ai durante N2 **sem** UI. Proibido jarbas antes de Hermes mínimo.

### Checklist N1 (k-nano legível)

| Item | Aceite | Status |
|------|--------|--------|
| **N1.1** Telemetria honesta | `LoadStatus` + `[STATUS]` coerente com LLM-TEST; zero “2B carregado” falso | ✅ 2026-07-15 |
| **N1.2** Cap + probes limpos | NVIDIA FW só com VID 0x10DE; Cap DENY demos documentados | ✅ |
| **N1.3** Métricas scheduler | Log periódico `[SCHED] tick/agents/polled` pós-Runtime | ✅ código (hook); re-flash uefi se log slim não mostrar |
| **Goal N1** | Log legível; QEMU limpo de FW NVIDIA spam | ✅ |

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
**N1** ✅ (v1.7.0). **N3 progress:** BitNet 2B LOADED (~590MB, 30 layers, FWD OK); generate/TTS empty = próximo. Depois: N2 / fechar N3–N5.

---

## 7. IDEAs

Ver IDEA_BANK #433–#438 (N1–N5 + cadeia canônica).
