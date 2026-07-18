# ADR-0041: K³CHJ Capability-Based Rings + SFI

**Data:** 2026-07-14 · **revisão anéis:** 2026-07-18 · **rebrand K³CHJ:** 2026-07-18  
**Status:** Accepted — P0–P9 PoC complete; **emenda R0–R3 + k-HAL** H1–H5 + HalOffer + H4+/H5+/AS ✅ código (v1.8.6)  
**Lifecycle:** `fazendo` (aceite QEMU maintainer + ≠ isolamento produção)  
**Sprint:** 107+ → **v1.8.6 TEST** (SESSION_140)  
**Ideia:** #424–#432 (PoC); **#459** (k-HAL); **#460** (marco 1.8.6); **#461** (rebrand K³CHJ)  
**Propósito:** Formalizar anéis por capability, o estado real (monólito Ring 0) e o **mapa-alvo de privilégio + ownership de HW** sem desfazer Pacotes A/B nem N1–N5 (ADR-0042).

**Nome de produto:** **K³CHJ** = `k_nano` + `k_hal` + `k_ai` + Cortex + Hermes + Jarbas (ver ADR-0042 §0). Histórico **K²CHJ** = sem `k_hal` na marca.

---

## 1. Contexto

A visão-alvo é um **capability microkernel** onde crates K³CHJ são fronteiras de **função** (ADR-0042) e anéis R0–R3 são fronteiras de **privilégio / MMIO**.

### 1.1 Mapa legado (PoC 2026-07-14) — ainda válido como histórico

| Anel lógico | Privilege | Contrato |
|-------------|-----------|----------|
| **K-Nano** | Ring 0 único | CR3/GDT/IDT exclusivos; slab/lock-free no scheduling; sem heap dinâmico no path crítico |
| **K-IA** | Ring 3 + MMIO mapeado | PCI, VirtIO rings, zero-copy DMA pinning |
| **Cortex** | Ring 3 | mmap de pesos, AVX/AMX, MoE |
| **Hermes** | Ring 3 (WASM SFI) | Host functions gated por capability |
| **JARBAS** | Ring 3 + FB MMIO | Double-buffer / VSync |
| **IPC** | Cross-AS | Só ring buffers lock-free (sem sockets internos) |

**Problema observado (2026-07-18):** o mapa legado colocava **MMIO/VirtIO em K-IA** e **FB MMIO em JARBAS**. Na prática o monólito vazou ainda mais: GPU BAR em `jarbas/gpu`, WiFi/net MMIO em `hermes`, inventário limpo em `k_ai`, cérebro limpo em `cortex`. Trocar HW exige tocar 3+ crates — o oposto do sonho VirtIO-portátil.

**Realidade atual:** boot = monólito `neural-kernel` em **CPU Ring 0 único**. Crates existem; **não há isolamento de privilégio de produção**. Pacotes A+B e Runtime **permanecem**.

---

## 2. Decisão (base PoC — inalterada)

1. Tratar K³CHJ crates como **fronteiras lógicas** até haver address spaces + IPC real.
2. Evoluir o monólito com provas de conceito **não-fatais** no boot (falha → warn, continua).
3. Adotar **capability tokens de operação** (bitflags) além do `CapabilityToken` do EventBus (legado/Ed25519).
4. IPC interno futuro = **SPSC/MPMC ring buffers** em páginas compartilhadas mapeadas nos address spaces envolvidos.
5. Syscall mínimo = trap software (`int 0x90`; 0x80–0x82 reservados para IPI SMP). Ring 3 completo é fase seguinte.

### Non-goals desta sprint / MVP C (histórico P0–P9)

- Separar binários por crate K³CHJ
- Hermes WASM SFI completo; VirtIO ring DMA real (QUEUE_NOTIFY); streaming GGUF >RAM
- Reescrever Agency / drivers / Pacotes A+B

*(Aceites P0–P9: ver §5–§7 abaixo — PoC ✅.)*

---

## 9. Emenda 2026-07-18 — Anéis R0–R3 + k-HAL (direcionamento canônico)

Esta seção **supersede** a atribuição de MMIO da tabela §1.1 para o **roadmap futuro**. Não invalida PoCs P0–P9; redefine **quem pode tocar silício**.

### 9.1 Princípio

```text
Privilégio CPU (x86 rings / AS futuros)  ≠  Função de produto (ADR-0042)
```

- **ADR-0042** responde: *o que a caixa faz* (legível / HW-AI / cérebro / orquestra / ego).
- **Esta emenda** responde: *quem pode fazer MMIO, IRQ de device, DMA pin de device*.

### 9.2 Mapa-alvo de privilégio

```text
CPU / AS Ring 0 ── k-nano          subir máquina (tempo, mem, traps, PCI cfg mínimo)
CPU / AS Ring 1 ── k-HAL           descoberta HW + backends nativos + fachulação VirtIO-xxx
CPU / AS Ring 2 ── cortex + k-ai   cérebro + autonomia (SEM MMIO de device)
CPU / AS Ring 3 ── hermes + jarbas orquestra + persona (SEM MMIO; só Caps / vring FE)
```

| Anel | Componente | É | Não é | MMIO device? |
|------|------------|---|--------|--------------|
| **R0** | **k-nano** | Boot, CR3/GDT/IDT, heap/slab, timer, IRQ *roteamento*, PCI *config space* (scan), Cap authority, DMA *pin genérico*, serial | Persona, LLM, “entender” GPU | Só fundação (não BAR de GPU/NIC/HDA) |
| **R1** | **k-HAL** (novo / extraído) | Enumerar BARs, bind driver nativo **ou** VirtIO backend, publicar `DeviceCap` + filas, AI HW Expert *binding*, quarantine local de device | Intent de usuário; matmul de política; humor | **Sim — único dono** |
| **R2** | **cortex** | LLM, Trinity MoE, tensores, mmap pesos, experts de *cálculo* | Orquestrar skills; tocar BAR | **Não** — só `ComputeJob` / Cap MAP_WEIGHTS |
| **R2** | **k-ai** | SelfHeal, Trust, inventário *lógico*, HEALTH_ISSUE, Agency de máquina | Trinity/LLM; compositor | **Não** — consome telemetria HAL |
| **R3** | **hermes** | Intent→skill, ReAct, WASM/SFI, PackageHub, criar artefatos | Drivers; FB direto | **Não** — CapGate + hostcalls |
| **R3** | **jarbas** | Persona, compositor, voz como expressão, +10% | BAR GPU; HDA MMIO; ATA | **Não** — `DisplayCap` / `AudioCap` FE |

### 9.3 Onde fica o Cortex? (resposta explícita)

**Cortex não é HAL e não é k-ai.**

| | **cortex** | **k-ai** |
|--|------------|----------|
| Metáfora ADR-0042 | O **cérebro** | AI **para a máquina** / autonomia |
| Conteúdo | BitNet, MoE/Trinity, tokenizer, tensores, speculative decode | SelfHeal, Trust, inventário, HW agents *de política* |
| Entrada | tokens / tensores / KernelPack *já validado* | EventBus `HEALTH_*`, DeviceTree lógico |
| Saída | logits / embeddings / LatentBus | heal/noop, Trust allow/deny |
| HW Expert *modelo* (pesos) | pode **inferir** (R2) | — |
| HW Expert *vinculação* (qual BAR/blob) | — | observa; **bind** é R1 k-HAL |

Colocar “LLM/Trinity em k-ai” seria **inverter** ADR-0042 e o Cargo atual (`cortex` abaixo de `k_ai`). **Rejeitado.**

### 9.4 k-HAL, HalOffer e VirtIO — portátil sem misturar nomes

**Separação canônica (1.8.x):**

| Nome | Papel | Quem chama |
|------|-------|------------|
| **HalOffer** (`k_hal::offer`) | API de alto nível R3→R1: *tem X? conecte o agente neste port/tópico* | hermes / jarbas / cortex / k_ai (**sem MMIO**) |
| **DevicePort** | Canal tipado pós-bind (`video_port`, `display_port`, `audio_port`, …) | FE após `bind` |
| **VirtIO** | Transporte OASIS/QEMU (vring, QUEUE_NOTIFY, VID `1AF4`) | só **backend** dentro de k-hal |

Exemplo vertical câmera:

```text
Jarbas (Vision/UVC FE) → Hermes → HalOffer::query/bind(Video)
  → Available/Bound + topic CAMERA_FRAME
  → BE nativo (xHCI/UVC) ou VirtIO-input no R1 — R3 nunca faz pci::scan
```

Referências externas (comportamento, não cópia de código):

| Sistema | Lição para Neural OS |
|---------|----------------------|
| **VirtIO 1.x** (OASIS) | FE↔BE via virtqueue — **só o transporte BE**; a API de produto é HalOffer |
| **QEMU/ACRN VirtIO** | FE no guest, BE no hypervisor; data plane = vring |
| **seL4 sDDF** | Drivers isolados; clients não falam MMIO |
| **Fuchsia Zircon DDK** | Device tree + protocols; isolamento por política |

**Modelo Neural OS:**

```text
                    ┌─────────────────────────────────────┐
   R3 hermes/jarbas │  HalOffer client + DevicePort FE    │  query/bind/topic
                    └──────────────┬──────────────────────┘
                                   │ Cap + EventBus (sem BAR)
                    ┌──────────────▼──────────────────────┐
   R1 k-HAL         │  HalOffer server + DeviceTree       │
                    │  VirtIO BE  ←→  Native BE           │  MMIO/IRQ/DMA
                    └──────────────┬──────────────────────┘
                                   │ pin / map / IRQ route
                    ┌──────────────▼──────────────────────┐
   R0 k-nano        │  machine services (sem política AI) │
                    └─────────────────────────────────────┘
```

Classes HalOffer (DeviceClass), com backend nativo **ou** VirtIO atrás do **mesmo** port:

| Classe lógica | Frontend (R2/R3) | Backend R1 (nativo ou VirtIO) |
|---------------|------------------|-------------------------------|
| `gpu` / compute | jarbas DisplayCap; cortex ComputeJob | NVIDIA/AMD/Intel **ou** VirtIO-GPU |
| `net` / `wifi` | hermes NetAgent (só FE) | RTL/e1000/iwlwifi **ou** VirtIO-net |
| `block` | NeuralFS / PackageHub I/O | NVMe/AHCI/ATA **ou** VirtIO-blk |
| `input` | InputAgent FE | HID **ou** VirtIO-input |
| `snd` | jarbas voz FE | HDA/UAC **ou** VirtIO-snd |
| `video` | Vision/UVC FE | xHCI/UVC **ou** VirtIO-input video |

**AI HW Expert** vive em duas metades:

1. **Binding (R1):** dado VID/DID → escolher backend, FW blob, quarantine; publicar HalOffer.  
2. **Inferência (R2 cortex):** classificar device / sugerir skill — **sem** escrever BAR.

### 9.5 Matriz Cap (quem pode o quê)

| Cap / recurso | R0 nano | R1 HAL | R2 cortex | R2 k-ai | R3 hermes | R3 jarbas |
|---------------|---------|--------|-----------|---------|-----------|-----------|
| CR3 / IDT / IRQ route | ✅ | pedida | — | — | — | — |
| PCI config scan | ✅ mínimo | ✅ full | — | leitura lógica | — | — |
| BAR MMIO GPU/NIC/HDA | — | ✅ | — | — | — | — |
| DMA pin device | auxilia | ✅ | — | — | — | — |
| MAP_WEIGHTS / DEMAND_PAGE | grant | — | ✅ use | — | — | — |
| ComputeJob submit | — | executa BE | ✅ pede | observa | pede via skill | — |
| SEND_TCP / net FE | — | BE | — | heal path | ✅ | — |
| MAP_FB / PRESENT | grant | BE scanout | — | — | — | ✅ FE |
| Trust / SelfHeal | — | reporta fault | — | ✅ | escalate | — |
| WASM hostcalls | — | — | — | — | ✅ CapGate | — |

### 9.6 Divisão de tarefas (infraestrutura)

#### Boot (fases × anel dono)

| Fase boot | Dono privilégio | Notas |
|-----------|-----------------|-------|
| SafeHarbor → MemoryCore → SystemBringup | **R0 nano** | serial, IDT, heap, SMP mínimo |
| Diagnostics | R0 + telemetria | sem BAR GPU |
| HardwareDiscovery | **R1 HAL** (PCI/ACPI enumerate) | nano só entrega cfg space / map UC |
| DriverInit | **R1 HAL** | bind VirtIO ou nativo; **não** jarbas/hermes |
| AgentFleet | R2/R3 registro | agents sem MMIO |
| Runtime | scheduler R0; ticks R2/R3 | compute via HAL BE |

#### Cargo / workspace (alvo)

```text
k-nano          (R0)
   ↑
k-hal           (R1)  ← NOVO: extrair jarbas/gpu + hermes wifi/net drivers + HDA
   ↑
cortex          (R2)  ← só k-nano + k-hal ABI (ComputePort), sem MMIO
k-ai            (R2)  ← k-nano + k-hal DeviceTree + cortex (opcional)
   ↑
hermes          (R3)  ← Caps; Net FE; sem iwlwifi MMIO
jarbas          (R3)  ← Display/Audio FE; sem BAR
neural-kernel   bin integração
```

Cadeia ADR-0042 `k-nano → k-ai → cortex → …` permanece como **identidade de produto**; a cadeia de **privilégio** passa a exigir `k-hal` entre nano e o resto. Ajustar Cargo quando a extração começar (sem ciclo).

#### Conciliação com IDEA #88 (“sem HAL genérica”)

Não reintroduzir “HAL Linux” (open/read/write genéricos).  
**k-HAL = catálogo de classes VirtIO-shaped + backends tipados** (GPU/net/block/snd). Cada backend é explícito; o FE é estável.

### 9.7 Realidade × alvo (gap 2026-07-18)

| Claim | Hoje | Alvo |
|-------|------|------|
| Único MMIO GPU | `jarbas/gpu/*` | `k-hal` backends ADR-0048–50 |
| Único MMIO WiFi/net live | `hermes` wifi_* / net init | `k-hal` + hermes só FE |
| HDA MMIO | `jarbas/audio` (+ residual bin) | `k-hal` snd BE |
| Inventário | `k_ai` (bom) | permanece; fonte = HAL events |
| Trinity/LLM | `cortex` (bom) | permanece R2 |
| VirtIO vring | PoC P8 layout | BE real + QUEUE_NOTIFY em HAL |
| Isolamento AS | PoC shallow L4 | R1/R2/R3 AS quando estável |

### 9.8 Roadmap de migração (após PoC P0–P9)

| Degrau | Entrega | Aceite |
|--------|---------|--------|
| **H0** | Esta emenda + IDEA #459 | Doc ✅ |
| **H1** | Crate `k-hal` + `DeviceCap` + ports + DeviceTree PCI | ✅ `k_hal::init`; serial `[K-HAL] H1 DeviceTree`; jarbas/k_ai/neural-kernel wire |
| **H2** | Extrair `gpu/*` → `k-hal`; jarbas FE (`pub use` + cube) | ✅ MMIO GPU em `crates/k_hal/src/gpu`; jarbas sem BAR |
| **H3** | Extrair net/wifi + HDA → `k-hal`; hermes/jarbas FE | ✅ `k_hal::net` + `k_hal::audio::hda`; inject callback |
| **H4** | VirtIO FE/BE + QUEUE_NOTIFY + SCL log | ✅ **H4+** `map_bars_uc` VirtIO-PCI + `try_pci_queue_notify` → **NotifySent** (QEMU); NotifySkipped honesto se BAR inválido |
| **H5** | AS R1 BAR + Cap deny R3 MMIO | ✅ **H5+** `check_map_bar`/`check_fe` nos ports + HalOffer grant Cap; `demo_as_r1_r3_shallow` CR3 (PoC ≠ produção) |

**Non-goals H0–H3:** segundo binário kernel; reescrever Agency; declarar v2.0.0; PRIME/P2P.

### 9.9 Log estruturado (localização)

Formato canônico (tick prefixado por `serial::_print`):

```text
[T+n] [Rn] [k-xxx] [Item] [subitem] - texto e dados
```

Macros em `k_nano::slog` / `slog_nano!` / `slog_hal!` / `slog_kai!` / `slog_cortex!` / `slog_hermes!` / `slog_jarbas!` / `slog_bin!`.

Exemplo (k-hal H1–H5):

```text
[T+0] [R1] [k-hal] [DeviceTree] [populate] - devices=6
[T+0] [R1] [k-hal] [DeviceCap] [ready] - devices=6 compute=NotBound ...
[T+0] [R1] [k-hal] [VirtIO] [select] - BE net=Native gpu=VirtioPci ...
[T+0] [R1] [k-hal] [SCL] [map] - control=k_ai cognition=cortex action=hermes ...
[T+0] [R1] [k-hal] [Cap] [MAP_BAR] - DENY ring=3
```

Grepar por anel (`[R1]`), crate (`[k-hal]`) ou item (`[VirtIO]`) isola falhas visualmente.

### 9.10 Riscos aceitos

- Extrair GPU de jarbas é grande (ADR-0048–50 ainda `fazendo`) — migrar **depois** de canários honestos ou em paralelo com facade.
- VirtIO-net “puro” no lab real não substitui iwlwifi — backends nativos obrigatórios atrás do mesmo FE.
- Monólito Ring0 permanece até H5; **fronteira de código primeiro**, isolamento CPU depois (lição P0–P9).

### 9.11 Decisões fechadas nesta emenda

1. **Sim**, há camada entre nano e k-ai: **k-HAL (R1)** — não um segundo kernel binário.  
2. **Cortex = R2 cérebro** (Trinity/LLM); **k-ai = R2 autonomia/HW-policy** — sem MMIO.  
3. **Hermes/Jarbas = R3** — orquestra/persona; clientes VirtIO/Cap apenas.  
4. Portabilidade HW = **trocar backend R1**, não reescrever R2/R3.  
5. ADR-0042 identidades de produto **permanecem**; privilégio MMIO **move** para esta emenda.

### 9.12 Glossário — hierarquia de consciência de *máquina* (sem psique humana)

Copia-se só a **estrutura hierárquica** (deliberativo / automático / sensório-motor / substrato).  
**Proibido** importar desejo, repressão, infância ou metáforas clínicas.

| Nível | Nome canônico | Componente | Papel operacional |
|-------|---------------|------------|-------------------|
| L4 | **Consciência de ação / expressão** | hermes + jarbas (R3) | Orquestra intent→skill; persona/UI/voz. Sem BAR. |
| L3 | **Consciência deliberativa** | cortex (R2) | Raciocínio “agora”: LLM, Trinity MoE, tensores. Capacidade limitada (contexto/RAM). |
| L2 | **Consciência automática de máquina** | k-ai (R2) | Reflexos de sobrevivência: Trust, SelfHeal, quarantine, inventário lógico. Rápido, simbólico. |
| L1 | **Sensório-motor** | k-HAL (R1) | BAR, IRQ, VirtIO BE/nativo. Sem narrativa. |
| L0 | **Substrato** | k-nano (R0) | Tempo, mem, traps, Cap authority, PCI cfg mínimo. |

```text
        ╱╲          L4  hermes/jarbas — ação + expressão
       ╱  ╲
  ────╱────╲────    linha d'água = Cap / EventBus (o que sobe a L3/L4)
     ╱ L3   ╲       cortex — deliberativo
    ╱────────╲
   ╱   L2     ╲     k-ai — automático de máquina
  ╱────────────╲
 ╱     L1       ╲   k-HAL — sensório-motor
╱_______L0_______╲  k-nano — substrato
```

**Invariantes do glossário**

1. L2 **não** é “LLM escondido”; L3 **não** é Trust/SelfHeal.  
2. Surpresa de silício sobe L1→L2 (HEALTH_*); deliberação sobe L2→L3 só se Cap/Hermes pedir.  
3. L4 nunca fala MMIO; L1 nunca decide persona.

---

## 10. Pesquisa de ponta aderente (2026-07-18) — o que usar e custo

Fontes: arXiv, GitHub (OS/AI-native), seL4/sDDF, VirtIO OASIS, Theseus OSDI, folkering/coconut/RVM/eo9.  
Custo em **esforço relativo** (S=dias–1 sem; M=2–6 sem; G=mês+; X=trimestre+) e **risco boot** (Baixo/Médio/Alto). Não é orçamento financeiro.

### 10.1 Cognitivo / governança (L2–L4)

| Ideia | Fonte | Aderência ADR-0041 | Usar? | Como no Neural OS | Custo |
|-------|-------|--------------------|-------|-------------------|-------|
| **Structured Cognitive Loop (R-CCAM)** + Soft Symbolic Control | arXiv:2511.17673, 2510.05107; github.com/enkiluv/scl-core-experiment | Alta: separa Cognition vs Control | **Sim (padrão)** | Control = **k-ai** + CapGate; Cognition = **cortex**; Action = **hermes**; Memory = EventBus/NeuralFS/SleepCycle | S–M (mapear fases; sem portar Python) |
| **Talker–Reasoner** (fast/slow) | arXiv:2410.08328 (Google) | Média–alta; dual process | **Sim (parcial)** | Talker≈**jarbas**+Hermes resposta rápida; Reasoner≈**cortex**+Hermes ReAct. **Não** colocar Trust no Talker | S (política de latência) |
| Dual System 1/2 visual (FaST) | arXiv:2408.08862 | Baixa no bare-metal | Referência só | Switch adapter → Cap “budget deliberativo” | — |
| Hierarchical skill/tool execution | arXiv:2504.16563 | Alta p/ Hermes | **Sim** | Já alinhado a skills/PackageHub ADR-0052; planner global → skill → tool Cap | S–M |
| Active Inference / free energy | arXiv:2401.12917, 2311.10215 | Média (teoria) | **Parcial / defer** | L2: minimizar “surpresa” = HEALTH mismatch vs DeviceTree esperado; **não** POMDP pleno no boot | M teoria; X se POMDP |
| Society of Mind / PEACE meta-arch | arXiv:2507.16184 | Baixa implementação | Glossário | Já coberto por L0–L4 | — |

### 10.2 HAL / VirtIO / isolamento (L0–L1)

| Ideia | Fonte | Aderência | Usar? | Como | Custo |
|-------|-------|-----------|-------|------|-------|
| **VirtIO FE/BE + vring** | OASIS VirtIO 1.3; ACRN HLD | Canônica §9.4 | **Sim** | FE em R3; BE em k-HAL; P8→H4 QUEUE_NOTIFY | M (net/gpu classes); G full |
| **seL4 sDDF** (async zero-copy, driver isolado) | Heiser et al.; github.com/sel4-cap/sDDF | Alta p/ contrato L1↔L2 | **Sim (padrão de filas)** | Copiar *ideia* de filas Rx/Tx/Rq + notify; **não** portar seL4 | M desenho; X se Microkit |
| VirtIO+QEMU on seL4 | TrustCom 2023 / Summit | Alta p/ lab | **Sim (H4)** | QEMU BE = um backend; silício = outro | M com P8 existente |
| **Theseus** cells / intralingual | OSDI’20; theseus-os/Theseus | Média: SAS+SPL ≈ monólito atual | **Parcial** | Fronteira de *crate/célula* antes de AS (H1–H3); não abandonar Cap HW | S (disciplina); X se SAS-only forever |
| coconutOS GPU shards + IOMMU Cap | github.com/coconut-os/coconutOS | Alta p/ GPU Cap | **Ideias** | IOMMU/VRAM Cap quando lab tiver IOMMU; pós H2 | G–X |
| folkering-os AI-native | github.com/merknu/folkering-os | Média (já IDEA #341) | Referência | Dream cycle ≈ SleepCycle; WASM apps ≈ Hermes | — |
| RVM / eo9 / Oreulius | ruvnet/rvm; wyager/eo9 | Baixa–média | Cherry-pick | Witness/boot gating; WASM component Caps (#426) | M se só Cap; X se Cranelift-on-metal |
| Fuchsia Zircon DDK | fuchsia.dev | Média | Referência | Device tree + protocols ≈ DeviceCap | — |

### 10.3 Já no tree (reforçar, não reinventar)

| Peça Neural OS | Papel na hierarquia | Gap vs pesquisa |
|----------------|---------------------|-----------------|
| CapGate + `int 0x90` (P2–P3) | Soft Symbolic Control mínimo | Falta Control *sempre* antes de Action (SCL) |
| EventBus + LatentBus | Memory / linha d’água | Indexação episódio vs long-term (SCL Memory) |
| Trust + SelfHeal (k_ai) | L2 automático | Explicitar “zero action sem precondition” |
| Hermes ReAct / skills | L4 Action + planner | Hierarchical execution (skill→tool→params) |
| compute_abi / KernelPack | L3 pede → L1 executa | FE estável; BE ainda em jarbas |
| virtio_vring PoC (P8) | embrião L1 | Sem QUEUE_NOTIFY / sem classes net-gpu unificadas |
| SleepCycle / Evolve | consolidação “offline” | Parecido a memory update + genesis; manter honesto |

### 10.4 Prioridade de adoção (recomendado)

| Pri | Adotar | Degrau | Custo | Não adotar agora |
|-----|--------|--------|-------|------------------|
| 1 | Glossário L0–L4 + SCL Control=k-ai | H0–H1 | S | Freudian labels |
| 2 | VirtIO FE/BE classes + extrair k-hal | H1–H3 | G | Portar seL4/Microkit |
| 3 | Talker/Reasoner latência (jarbas vs cortex) | paralelo N5 | S | Dois LLMs sempre |
| 4 | QUEUE_NOTIFY + BE QEMU==nativo | H4 | M–G | PRIME/P2P |
| 5 | IOMMU/GPU shard Caps (coconut-like) | pós H2+HW | G–X | Theseus abandonar rings |
| 6 | Active Inference lite (surpresa HEALTH) | pós H1 | M | POMDP/free-energy pleno |

### 10.5 Custo consolidado H1–H5 (ordem de grandeza)

| Degrau | Esforço | Risco boot | Dependência |
|--------|---------|------------|-------------|
| H1 skeleton `k-hal` + DeviceCap | S–M | Baixo | nenhuma |
| H2 GPU → k-hal | G | Médio | ADR-0048–50 estáveis o bastante p/ facade |
| H3 net/wifi/HDA → k-hal | G | Médio | lab WiFi/HDA |
| H4 VirtIO unificado | M–G | Médio | P8 + QEMU |
| H5 AS R1/R3 | G–X | Alto | Ring3 QEMU estável (P6) |

**Teto consciente:** não embutir runtimes host (ROCm/L0/Python SCL) no alvo `no_std`. Pesquisa vira **contrato e fases**, não dependência crate.

---

## 3. Gap analysis (visão × realidade) — PoC mecânico

| Claim visão | Status | Evidência | Esforço | Risco boot |
|-------------|--------|-----------|---------|------------|
| K-Nano Ring 0 exclusivo CR3/GDT/IDT | **Parcial** | `memory.rs` CR3 único; `interrupts.rs` GDT/IDT globais | G | Alto se CR3 errar |
| Slab / lock-free scheduling, sem heap no path crítico | **Parcial** | `slab.rs`, `agent-core` RR; heap ainda no boot path | M | Médio |
| K-IA Ring 3 + MMIO / VirtIO / DMA pin | **Parcial** (P5+P8 PoC) → **reorientado §9** (MMIO → k-HAL) | `k_ia_dma` + `virtio_vring` | G | Médio |
| Cortex Ring 3 + mmap pesos | **Parcial** (P5+P7+P9) | `cortex_mmap` + `demand_page` + `gguf_mmap` | G | Baixo |
| Hermes WASM SFI + host caps | **Parcial** | `wasm*.rs`, CapGate — sem AS separado | M | Baixo |
| JARBAS Ring 3 + FB MMIO + VSync | **Parcial** (P4) → **FE only §9** | `jarbas_fb.rs` | G | Médio |
| IPC só ring lock-free entre AS | **MVP C parcial** | EventBus in-process; SPSC shared pages | M | Baixo se isolado |
| Capability autoritativa por operação | **Parcial** | Cap bitflags + syscall | P | Baixo |
| Dois address spaces + CR3 switch | **MVP C** | `address_space.rs` | M | Médio (non-fatal) |
| Ring3 CPL=3 real (`iretq`) | **P6 PoC** | `user_mode.rs` | G | Médio |
| **k-HAL R1 único dono MMIO** | **Fictício → H1+** | hoje espalhado jarbas/hermes | G | Médio (migração) |

---

## 4. Prioridades P0 → P9

| Pri | Item | Status pós-MVP C |
|-----|------|------------------|
| **P0** | Gap documentado (esta ADR) | ✅ |
| **P1** | ADR curto + non-goals | ✅ |
| **P2** | **MVP C:** 2 AS + CR3 switch + ring SPSC shared + Cap + trap `int 0x90` + demo boot non-fatal | ✅ PoC |
| **P3** | Hermes WASM host-functions por Cap (sem AS full) | ✅ CapGate + SEND_TCP/WRITE_RING |
| **P4** | JARBAS FB MMIO capability + double-buffer contract | ✅ PoC |
| **P5** | K-IA DMA pin + Cortex mmap pesos (AS dedicado) | ✅ PoC |
| **P6** | Ring3 user-mode real (`iretq` + stub USER + Cap::ENTER_USER + return) | ✅ PoC |
| **P7** | Demand-paging via #PF (lazy Cortex weights) | ✅ PoC |
| **P8** | VirtIO vring wiring sobre DMA pin | ✅ PoC |
| **P9** | GGUF/FAT file-backed mmap sobre demand-paging | ✅ PoC |

Roadmap mecânico P0–P9 ✅ PoC.  
**Próximo eixo de produto:** aceite QEMU H4+/AS (§11.3) + SFI Hermes (#426). H1→H5 **implementados** (código); isolamento CPU produção **não**.


---

## 5. MVP C — aceite

- Dois `AddressSpace` (L4 próprio, shallow-copy do kernel + mapas privados).
- `Cr3::write` A → B → kernel, com interrupções mascaradas na janela crítica.
- Página compartilhada com `SpscRing`; escrita num AS, leitura no outro.
- `Cap::{PING, WRITE_RING, READ_RING}` + `syscall::dispatch` via `int 0x90` (ABI staging via atomics).
- Demo após DriverInit; erro → serial WARN, boot segue.
- **P6 Ring3:** ver aceite abaixo (`user_mode.rs`).

### P3 — aceite (parcial → done mínimo)

- `capability_gate.rs`: `check` / `host_send_tcp` / `host_write_ring` + demo boot non-fatal.
- `Cap::SEND_TCP` + `SYS_SEND_TCP`; `aios_send_tcp` / `aios_write_ring` em `aios_api.rs`.
- Hermes `execute_skill`: skills net/http/tcp passam por CapGate; `wasm_rt::host_call_gated` para imports.
- Ainda sem AS separado para WASM (SFI pleno = #426).

### P4 — aceite (JARBAS FB MMIO + double-buffer)

- `jarbas_fb.rs`: `FbContract` (virt/phys/stride/w/h/bpp) a partir do FB bootloader (`GpuDevice`).
- `Cap::{MAP_FB,WRITE_FB}` + `SYS_MAP_FB` / `SYS_PRESENT_FB`; deny sem Cap + log serial.
- AS JARBAS (`AddressSpace::clone_current`) mapeia `DEMO_MAP_PAGES` do FB em `JARBAS_FB_VA`.
- `JarbasDoubleBuffer` (backheap) + `present` (cópia + stub vsync via `TIMER_TICKS`/`sfence`).
- Demo boot non-fatal após P3; sem FB → Cap-only SUCCESS; falha → WARN, boot segue.
- Path primário = UEFI/bootloader FB (VirtIO-GPU BAR = evolução). Ring3 jump = bônus futuro.
- **Emenda §9:** FB raw MMIO migra para k-HAL BE; jarbas mantém apenas FE/present Cap.

### P5 — aceite (K-IA DMA pin + Cortex weight mmap)

- `k_ia_dma.rs`: `pin_frames` / `map_pinned` / `unpin` opcional; Cap `PIN_DMA`/`MAP_DMA`; AS K-IA em `K_IA_DMA_VA`.
- Stub VirtIO (pré-P8): phys addr logado como “buffer pinned ready”; ring/vring wiring = **P8**.
- `cortex_mmap.rs`: aloca N páginas peso simuladas, mapeia em `CORTEX_WEIGHT_VA` (eager); Cap `MAP_WEIGHTS`.
- Demand-paging (#PF first touch) = **P7**; mmap GGUF/FAT = TODO; PoC = memória simulada.
- Demo boot non-fatal pós-P4: deny → pin+map DMA → mmap pesos + touch → restore CR3; falha frame alloc → Cap-only SUCCESS / WARN.
- `SYS_PIN_DMA` / `SYS_MAP_DMA` / `SYS_MAP_WEIGHTS` em `syscall.rs`.
- **Emenda §9:** pin de *device* → k-HAL; k-ai deixa de ser dono de MMIO (só política).

### P6 — aceite (Ring3 user-mode real)

- GDT: `kernel_data` + `user_code` + `user_data` (DPL=3); TSS `privilege_stack_table[0]` (RSP0).
- IDT `int 0x90` com DPL=3; `Cap::ENTER_USER` + `SYS_EXIT_USER`.
- `address_space::map_user_page` propaga `USER_ACCESSIBLE` em toda a cadeia PT.
- `user_mode.rs`: stub (marker + `int 0x90`) em páginas USER dedicadas; `enter_user_mode` via `iretq` (IF=0); return salva RIP/RSP e `jmp` kernel; deny sem Cap.
- Demo boot non-fatal pós-P5; #GP/#PF durante demo → WARN + restore (não halt).
- Flag `TRY_ENTER_RING3` para disable se WHPX/QEMU instável (🟡 parcial).
- Limitação: PoC single-threaded; sem ELF loader / preemptive usermode; shallow L4 ainda compartilha PTs do kernel.

### P7 — aceite (demand-paging via #PF)

- `demand_page.rs`: registry global (IrqSafeLock) de VAs lazy; frames **pré-alocados** no register (path #PF sem `GLOBAL_ALLOCATOR`).
- `AddressSpace::reserve_page`: caminho PT CoW + leaf NOT PRESENT; `install_present_leaf_current` só instala leaf no CR3 atual.
- `cortex_mmap::mmap_weights_lazy` + Cap `MAP_WEIGHTS|DEMAND_PAGE` / `SYS_DEMAND_PAGE`; deny sem Cap.
- `#PF` handler: se CR2 em range → map PRESENT + return (retry); senão comportamento anterior (count/warn/hlt).
- Demo boot non-fatal pós-P6: lazy 4 pages → switch CR3 → first-touch R/W → verify magic → restore; falha → WARN.
- Limitação: PoC simulado (não GGUF/FAT); cure usa try_lock (se falhar, não cura); USER leaf opcional no registry.

### P8 — aceite (VirtIO vring + DMA pin)

- `virtio_vring.rs`: Virtqueue layout-compatible (`Desc`+`AvailRing`+`UsedRing` espelhando `virtio_net`); Cap `VRING_SETUP` / `SYS_VRING_SETUP`.
- Backing: `k_ia_dma::pin_frames` (4 pages: desc|avail|used|payload); `Desc.addr` = phys pinnado (zero-copy claim).
- Path paralelo: se `VIRTIO_DEV` presente, loga `rx/tx_queue_phys` **sem mutar** filas live (NIC intacto).
- Sem device VirtIO: PoC layout-only ainda = SUCCESS documentado.
- Demo boot non-fatal pós-P7: deny Cap → pin+setup SUCCESS → log phys/indices; falha frame → Cap-only / WARN.
- **Emenda §9:** QUEUE_NOTIFY + BE nativo = degrau H4 em k-HAL.

### P9 — aceite (GGUF/FAT file-backed mmap)

- `gguf_mmap.rs`: localiza blob no FAT (`BITNET.BIN`/`HWEXPRT.BIN`/…); pré-lê 1–4 páginas via `read_file_range` em frames alocados **antes** do #PF.
- Cap `MAP_FILE` (+ `MAP_WEIGHTS|DEMAND_PAGE`) / `SYS_MAP_FILE`; deny sem Cap; CapGate `aios_map_file`.
- Integra `demand_page::register_lazy` em `FILE_WEIGHT_VA` — leaf NOT PRESENT; first-touch só instala PRESENT (sem I/O no fault path).
- Fallback documentado se arquivo ausente: stub magic `NFIL` + WARN (non-fatal) ou Cap-only SUCCESS.
- Demo boot non-fatal pós-P8: deny → mmap → touch → verify magic GGUF/`0xBE11BE11`/fallback → restore CR3.
- Limitação: PoC = prefixo do arquivo (não streaming 8GB); parser GGUF completo permanece em `gguf.rs`.

---

## 6. Consequências

- Positivo: prova hardware-level de isolamento de página + IPC shared-memory sem reinventar drivers (P0–P9).
- Positivo (emenda): caminho claro para **portabilidade HW** via VirtIO FE/BE e um único dono MMIO (k-HAL).
- Negativo: shallow-copy L4 compartilha PageTables inferiores do kernel — AS ainda não é isolamento forte contra o kernel (intencional no PoC).
- Negativo (emenda): migração H2–H3 é grande (GPU/net hoje em jarbas/hermes).
- EventBus continua pub/sub in-process até migração gradual para rings cross-AS.

---

## 7. Real vs stub (checklist operacional)

| Peça | Real | Stub / limite |
|------|------|----------------|
| Pacotes A+B boot | ✅ | — |
| P0–P2 AS/CR3/SPSC/Cap/int 0x90 | ✅ | Shallow L4 |
| P3 CapGate | ✅ | SFI/AS WASM pleno = #426 |
| P4 JARBAS FB | ✅ | VSync stub; bootloader FB |
| P5 DMA + mmap | ✅ | Pesos eager simulados |
| P6 Ring3 iretq | ✅ código | Untested QEMU estável; sem ELF/preempt |
| P7 demand-page #PF | ✅ | Sem I/O no fault |
| P8 VirtIO vring | ✅ layout+pin | H4+ acrescenta QUEUE_NOTIFY em `k_hal::virtio` |
| P9 GGUF/FAT mmap | ✅ pré-fill | Prefixo 1–4 pág.; sem streaming |
| **k-HAL R1** | ✅ H1–H5 + H4+/H5+ (v1.8.6) | PoC monólito; ≠ isolamento CPU produção; aceite QEMU maintainer pendente |

**Checklist P0–P9:** todos ✅ PoC.  
**Checklist H0–H5:** ✅ H1–H3 BE + **H4+ QUEUE_NOTIFY real** + **H5+ Cap enforce + AS shallow** (2026-07-18 / SESSION_140 / tag **v1.8.6**). Lifecycle permanece `fazendo` até aceite QEMU maintainer; **≠** ADR `completa` / v2.0.0 / isolamento Ring0 produção.

### Aceite por fase (2026-07-18, permanece **1.8.x**)

| Fase | Entrega | Aceite |
|------|---------|--------|
| 1 H4+ | `k_hal::virtio` map UC + `try_pci_queue_notify` | boot: `NotifySent` ≥1× (virtio-gpu/net); sem #GP/#PF fatal |
| 2 MMIO residual | hermes/jarbas FE; VGACNTRL em `k_hal`; HalOffer | zero device-BAR MMIO live em hermes/jarbas tick (FB GOP OK) |
| 3 H5+ Cap | `check_fe_bound` ports + grant em `offer::bind` | R3_no_cap=Deny; Bound=Allow; FE sem bind=Deny |
| 4 AS shallow | `address_space::demo_as_r1_r3_shallow` | CR3 switch + touch BAR + restore; R3 MAP_BAR Deny |

## 8. Próximos (pós-implementação H1–H5)

**Adequação Boot OK → visão K³CHJ (função):** **ADR-0042** (N1–N5) ✅ v1.8.0.  
**Privilégio / HAL:** planos H1–H5 + HalOffer + H4+/H5+/AS ✅ código (v1.8.6) — ver §11.

1. **Aceite QEMU** — confirmar slog `NotifySent` + Cap/AS non-fatal no boot UEFI.  
2. SFI WASM + Cap contract (#426) — fase N4 contínua.  
3. Validar Ring3 em QEMU UEFI (`TRY_ENTER_RING3`) sob nano estável.  
4. **Não** declarar esta ADR `completa` / v2.0.0 sem OK maintainer + isolamento real.

### Fontes (pesquisa)

- VirtIO 1.3: <https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html>  
- ACRN VirtIO HLD (FE/BE/vring): Project ACRN docs  
- seL4 sDDF (drivers isolados, zero-copy): Trustworthy Systems / Heiser et al.  
- Fuchsia Zircon device model (devhost + protocols)  
- ADR-0042 identidades; ADR-0048–50 compute backends (viram BE de k-HAL)

---

## 11. Planos Cursor implementados (registro canônico)

Três planos encadeados fecharam a emenda §9–§10 em **v1.8.x** (tag **v1.8.6**, SESSION_140). Gate **v2.0.0** intacto.

### 11.1 Plano `k-HAL H1-H5` (`k-hal_h1-h5`)

| Degrau | Entrega | Status | Artefatos |
|--------|---------|--------|-----------|
| **H1** | Crate `k_hal` + DeviceCap + ports + DeviceTree PCI + wire facade | ✅ | `crates/k_hal/`, workspace member |
| **H2** | GPU MMIO → k-hal; jarbas FE (`pub use` + cube) | ✅ | `k_hal/gpu/*`; `jarbas/gpu/mod.rs` |
| **H3** | net/wifi + HDA → k-hal; hermes/jarbas FE + inject | ✅ | `k_hal/net/*`, `k_hal/audio/hda` |
| **H4** | VirtIO FE/BE Degrau + SCL log | ✅ → supersedido por H4+ (§11.3) | `k_hal/virtio.rs` |
| **H5** | Cap demo + `bind_hal_as` flag | ✅ → supersedido por H5+/AS (§11.3) | `k_hal/cap_gate.rs` |

Cadeia Cargo: `k_nano ← k_hal ← {cortex,k_ai,hermes←jarbas}`; `neural-kernel` integra.

### 11.2 Plano `HalOffer API 1.8.x` (`haloffer_api_1.8.x`)

| Item | Entrega | Status |
|------|---------|--------|
| API | `k_hal::offer` query/bind/release/list/request | ✅ |
| Classes | `DeviceClass` incl. **Video**; ports tipados | ✅ |
| Hermes | `hermes::hal_offer` + EventBus `HW_OFFER`/`HW_BOUND`/`CAMERA_BOUND` | ✅ |
| Jarbas | UVC/Vision FE sem `pci::scan` no path câmera | ✅ |
| Docs | §9.4 VirtIO=transporte; HalOffer=API R3; **ficar 1.8.x** (não 1.9.0) | ✅ |

**Não-meta do plano (cumprido):** sem QUEUE_NOTIFY produção na leva HalOffer (feito depois em H4+); sem AS R1/R3 real na leva HalOffer (feito depois em AS shallow).

### 11.3 Plano `ADR41 H4 H5 full` (`adr41_h4_h5_full`)

| Fase | Entrega | Status | Aceite |
|------|---------|--------|--------|
| **1 H4+** | Map UC VirtIO-PCI + `try_pci_queue_notify` → `NotifySent`/`NotifySkipped` | ✅ código | QEMU slog pendente maintainer |
| **2 MMIO residual** | hermes/jarbas zero BAR live; VGACNTRL em k-hal; virtio_gpu/link_watcher FE | ✅ | FB GOP permitido |
| **3 H5+ Cap** | `grant_fe` no bind; `check_fe_bound` nos ports; CapDenied→Quarantined | ✅ | demo boot Deny/Allow |
| **4 AS shallow** | `demo_as_r1_r3_shallow` CR3 + touch BAR + restore | ✅ PoC | ≠ isolamento produção |
| **Docs** | ADR/STATE/TECNOLOGIAS; tag v1.8.6 | ✅ | lifecycle `fazendo` |

**Não-meta (cumprido):** sem segundo binário kernel; sem SFI Hermes pleno; sem “CPU Ring 3 de produção”; sem declarar ADR `completa` / v2.0.0.

### 11.4 Log estruturado (acompanha H1–H5)

Formato canônico `[T+n] [Rn] [k-xxx] [Item] [subitem] - …` via `k_nano::slog_*!` (§9.9). Migração massiva SESSION_140 era.

### 11.5 Residuals honestos

- Boot QEMU: evidência serial `NotifySent` ainda sob aceite maintainer.  
- `neural-kernel/virtio_net.rs`: data-plane legado + bridge HalOffer/notify k-hal (não move completo).  
- Orfãos disco `jarbas/gpu/*.rs` / cópias hermes wifi (não `mod`) — higiene, não path live.  
- Isolamento AS/IOMMU/ELF multi-agent = fora de escopo.