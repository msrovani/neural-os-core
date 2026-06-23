# ADR-0014: Hardware Abstraction — AI-Driven System Architecture, USB, SMP, Periféricos e Áudio/Video

**Status:** Draft  
**Date:** 2026-06-23  
**Driver:** Definir como o Neural OS lida com todo o hardware via IA — inventário → MLP → configuração dinâmica da arquitetura de rings, memória, storage e dispositivos.

## Context

O roadmap atual (ADR-0010) cobre NVMe (Phase 4) e WASM (Phase 5) mas não aborda:
- USB e dispositivos conectáveis a quente
- SMP (multicore/multithreading)
- Periféricos genéricos (PCI, GPIO)
- Entrada/Saída de áudio e vídeo

O Neural OS **não é um SO de propósito geral**. Todo suporte a hardware deve ser justificado por uma necessidade direta de orquestração de inferência neural.

## Princípios Gerais

1. **Mínimo viável:** Só implementar driver se for requisito para uma skill WASM ou para o boot flow.
2. **VirtIO first:** Em QEMU, usar dispositivos VirtIO (net, blk, gpu, sound) que têm spec simples e estável. Hardware real só após protótipo QEMU validado.
3. **Polling > Interrupção:** Preferir polling em dispositivos de baixa taxa para evitar complexidade de IRQ sharing + MSI-X. Interrupções só quando latência crítica (ex: NVMe).
4. **Sem HAL genérica:** Cada driver é um módulo autocontido sem trait `Device` universal. A inicialização é explícita no `kernel_main`.

---

## 1. USB

### Decisão: **USB gerenciado pelo Neural Cortex, não por driver stack.**

**Justificativa:** USB é o exemplo perfeito de onde um kernel clássico incha (~50k LOC para Linux USB stack) enquanto um AIOS pode fazer o mínimo hardware + decisão por inferência.

### Arquitetura: AI-driven USB

O kernel clássico implementa:

```
USB device conecta
  → xHCI detecta (HW)
  → Driver xHCI lê descritores (port, device, config, interface, endpoint)
  → USB core matcha contra database de drivers
  → Class driver assume (HID, Mass Storage, UVC, Audio...)
```

O Neural OS faz:

```
USB device conecta
  → xHCI detecta (HW) — MÍNIMO POSSÍVEL
  → Kernel lê só o essencial: VID + PID + class code (3 registros)
  → Neural Cortex (Ring 0) infere:
       "VID:046D, PID:C077, Class:03 (HID) → device provável: Logitech mouse.
        Ação: permitir, rotear para handler HID mínimo."
    OU
       "VID:0781, PID:5583, Class:08 (Mass Storage) → device: USB flash.
        Ação: negar. Storage é NVMe (Phase 4), USB storage não suportado."
    OU
       "VID:XXXX, PID:XXXX, Class:FF (Vendor-specific) → desconhecido.
        Ação: negar. Logar tentativa para auditoria."
  → Kernel executa a ação decidida pelo Cortex
```

### O kernel xHCI mínimo (HW layer)

Só o necessário para trazer o barramento USB à vida e ler identificação do dispositivo:

```rust
// src/usb_xhci.rs — < 500 linhas, sem abstração de classes

pub struct XhciController {
    cap_regs: &'static CapRegs,   // BAR0 mapeado
    op_regs: &'static OpRegs,
    dcbaa: PhysAddr,               // Device Context Base Address Array
}

impl XhciController {
    /// Init mínimo: reset HC, configuração de portas, habilita slots
    pub fn try_init(pci_bar: MmioRegion) -> Option<Self> { ... }

    /// Detecta dispositivos conectados nas portas Root Hub
    pub fn poll_ports(&self) -> impl Iterator<Item = PortStatus> {
        self.op_regs.portsc.iter().enumerate().filter_map(|(i, p)| {
            if p.ccs() {  // Current Connect Status
                Some(PortStatus { port: i, speed: p.ps(), changed: true })
            } else { None }
        })
    }

    /// Lê VID, PID e class code do device connectado
    /// Usa Address Device + Get Descriptor via transferência de controle
    pub fn identify_device(&mut self, port: u8) -> Option<DeviceId> {
        // 1. Disable slot
        // 2. Address device
        // 3. Get Device Descriptor (só 8 bytes: VID + PID)
        // 4. Get Config Descriptor (só 1 byte: class code)
        Some(DeviceId { vid, pid, class, subclass, protocol })
    }
}
```

### A inferência do Neural Cortex — 3 níveis

O tratamento USB no Neural OS opera em 3 níveis de abstração, todos decididos pelo Cortex:

**Nível 1 — HW Detection (kernel mínimo, sem IA)**
```rust
// xHCI lê 3 campos do dispositivo conectado:
struct DeviceId {
    vid: u16,      // Vendor ID (ex: 0x046D = Logitech)
    pid: u16,      // Product ID (ex: 0xC077 = Mouse M705)
    class: u8,     // USB Class (ex: 0x03 = HID)
    subclass: u8,  // (ex: 0x01 = Boot Interface)
    protocol: u8,  // (ex: 0x02 = Mouse)
}
```

Aqui **não há IA**. É hardware puro — xHCI lê registros padrão do USB Device Descriptor (primeiros 8 bytes). ~500 linhas de Rust, ponto.

**Nível 2 — Device Classification (MLP treinado offline)**

O Cortex classifica o que é o dispositivo e o que fazer com ele. Mas em vez de allow/deny simples, ele produz 3 saídas:

```
Entrada: [vid_hi, vid_lo, pid_hi, pid_lo, class, subclass, protocol]
    │
    ▼  MLP (7→5, SiLU, argmax)
    │
    ├── 0 = "Sei o que é, tenho handler: <skill_id>"
    ├── 1 = "Sei o que é, não tenho handler: ignorar"
    ├── 2 = "Sei o que é, mas preciso de skill sobre demanda: baixar/criar"
    ├── 3 = "Não sei o que é, usuário precisa inferir intenção"
    └── 4 = "Parece suspeito: negar e logar"
```

Nota: o Neural OS **não faz o papel de um SO clássico**. A saída `3` significa literalmente "não sei o que o usuário quer fazer com isso — perguntar/aguardar intent". O Cortex não assume automaticamente que um mouse deve ser usado como apontador — ele espera o usuário inferir.

**Nível 3 — Dynamic Interface Creation (WASM skill dispatch)**

Aqui está a inovação: o handler não é um driver C compilado no kernel. É uma **skill WASM carregada sob demanda** que sabe falar o protocolo do dispositivo:

```
Cortex decide: "mouse HID, skill_id = 'hid_mouse' "
    │
    ▼  procura skill na SFS (Phase 4) ou baixa de rede (Phase 5+)
    │
    ▼ instancia em Ring 2 (WASM):
    skill.hid_mouse.wasm
      ├── exports: parse_report(raw_hid: &[u8]) → PointerEvent
      └── imports: kernel::usb::interrupt_in(endpoint, buf)
    │
    ▼ loop do Ring 0:
      poll xHCI → raw HID report → skill.parse_report → PointerEvent
      │
      ▼ intent_router(classify(PointerEvent) + UserIntent)
        ├── "quero clicar no botão X" → action = navigation
        └── "estou só movendo" → action = idle, só atualiza cursor
```

**O usuário precisa inferir.** O Neural OS não assume comportamento default para nenhum dispositivo. Se um mouse é conectado:
- **Sem intent do usuário:** dispositivo detectado, Cortex loga, mas nada acontece. Nenhum cursor aparece. Nenhum driver é carregado.
- **Usuário infere "quero usar como apontador":** intent router recebe "mouse control" → carrega skill `hid_mouse` → cursor aparece.
- **Usuário conecta uma câmera USB e infere "quero fazer videoconferência":** intent router carrega skill `uvc_capture` + skill `audio_capture` + skill `wasm_video_chat`.
- **Usuário conecta um pendrive e não infere nada:** O dispositivo fica detectado mas inacessível. Nenhum filesystem é montado. Só quando o usuário inferir "quero ver arquivos" ou "quero fazer backup" o Cortex age.

Isso é **radicalmente diferente** de qualquer SO existente. Nenhum dispositivo tem autoridade implícita. Tudo precisa de uma intenção do usuário (explícita ou inferida por contexto).

### Exemplos completos

#### Mouse conectado — fluxo com intent do usuário

```
[USB] Port 3: device connected (VID=046D, PID=C077, Class=03 HID)
[CORTEX] Dispositivo HID reconhecido. Usuário não infriu intenção ainda.
[CORTEX] Aguardando intent...

...usuário mexe o mouse (interrupção HID raw)...
[CORTEX] Movimento detectado. Intenção inferida: "apontar/clicar"
[CORTEX] Carregando skill hid_mouse.wasm em Ring 2...
           └── Skill abstrai relatório HID → PointerEvent { dx, dy, buttons }
[CORTEX] Cursor criado. Eventos roteados para skills visuais em foco.
```

Sem a skill, o movimento do mouse seria ignorado. O Cortex decide carregar a skill **depois** de inferir a intenção do usuário (pelo próprio movimento do mouse — o usuário "vota com o hardware").

#### Câmera USB conectada — fluxo com intent

```
[USB] Port 5: device connected (VID=046D, PID=0866, Class=0E UVC)
[CORTEX] Dispositivo de vídeo. Aguardando intent...

...usuário abre skill de visão ou diz "quero camera"...
[CORTEX] Intent: "captura de vídeo"
[CORTEX] Carregando skill uvc_capture.wasm em Ring 2...
           └── Skill: isochronous stream → tensor { width, height, yuv }
[CORTEX] Stream de vídeo disponível para skills consumidoras.
```

#### Pendrive conectado — fluxo com intent

```
[USB] Port 2: device connected (VID=0781, PID=5583, Class=08 Mass Storage)
[CORTEX] Mass Storage detectado. Aguardando intent...
[CORTEX] Análise de segurança: device não confiável (desconhecido).
[CORTEX] Nenhuma ação tomada. Storage não montado.

...usuário infere "quero copiar fotos"...
[CORTEX] Intent: "acesso a arquivos de dispositivo externo"
[CORTEX] Risco aceito pelo usuário. Carregando skill scsi_bulk.wasm...
           └── Skill: SCSI Inquiry → LBA 0 → MBR → FAT32 → dir /DCIM
[CORTEX] Conteúdo: 42 arquivos JPEG. Exibindo thumbs para o usuário.
```

#### Dispositivo desconhecido (hotplug sem matching)

```
[USB] Port 4: device connected (VID=FFFF, PID=0001, Class=FF)
[CORTEX] Dispositivo desconhecido. Sem handler, sem confiança.
[CORTEX] Porta desabilitada. Evento logado para auditoria.
[CORTEX] Se usuário confiar manualmente, pode inferir "usar mesmo assim".
```

### Arquitetura: USB Device → WASM Skill Pipeline

```
Hardware                      Kernel (Ring 0)                 Ring 2 (WASM)
─────────                     ───────────────                 ──────────────

xHCI Controller               xHCI Driver (< 500 LOC)
  │                             │
  ├── Port connect ───────────► │ poll_ports()
  │                             │ identify_device() → DeviceId
  │                             │
  │                             ▼
  │                           Neural Cortex
  │                             │ intent_router(classify(DeviceId))
  │                             │
  │                             ├── "reconhecido + tem intent" ──► carrega skill
  │                             │                                     │
  │                             │                               skill.wasm
  │                             │                                 ├── parse()
  │                             │                                 ├── handle()
  │                             │                                 └── output()
  │                             │                                     │
  │◄─── interrupt_in() ────────┤◄─ kernel::usb::interrupt_in() ◄─────┤ (skill pede dados)
  │                             │                                     │
  │                             ├── "desconhecido" ──► log + deny     │
  │                             │                                     ▼
  │                             └── "sem intent" ────► aguarda     Tensor/Event
  │                                                                   │
  │                                                              próxima skill
  │                                                            (ex: render cursor)
```

### Trust Cache — "Já reconhecido, já autorizado"

Um dispositivo reconhecido, autorizado pelo usuário e usado com sucesso **não pode pedir permissão de novo toda vez que conectar**. Isso destruiria a usabilidade.

**Modelo trust-once-use-always (similar a SSH known_hosts):**

```
1ª conexão:  USB connectado → Cortex: "desconhecido"
             Usuário infere intent → autoriza → usa
             ↓
             Kernel salva em TrustEntry:
               { vid: 0x046D, pid: 0xC077, serial: "ABCD1234",
                 skill: "hid_mouse.wasm", trusted: true, first_seen: 123456 }
             Armazenado no SFS (Phase 4): /system/trust/usb.tbl

2ª conexão:  USB connectado → vid/pid/serial batem com TrustEntry
             ↓
             Cortex: "confiável, skill conhecida, sem necessidade de intent"
             ↓
             Skill carregada automaticamente, dispositivo ON imediato

3ª conexão:  Mesmo modelo, serial diferente → TrustEntry parcial
             ↓
             Cortex: "VID/PID conhecidos, serial novo. Risco baixo, autorizar?"
             ↓
             Se usuário confirmou uma vez para este modelo, auto-allow.

Revogação:  Usuário infere "não confio mais neste dispositivo"
             ↓
             TrustEntry removido. Próxima conexão = primeira vez.
```

**Estrutura do TrustEntry:**

```rust
#[derive(FromBytes, AsBytes)]  // zerocopy, sem serialização
#[repr(C)]
struct TrustEntry {
    vid: u16,
    pid: u16,
    serial: [u8; 16],        // Número de série USB (ou hash do device descriptor)
    skill_name: [u8; 32],    // "hid_mouse" etc.
    trusted: bool,
    first_seen: u64,         // Timestamp monotônico
    last_seen: u64,          // Para LRU cache
}

struct TrustTable {
    entries: [TrustEntry; 64],  // Slot fixo, sem alocação dinâmica
    len: usize,
}
```

**Regras do cache:**

| Situação | Ação |
|---|---|
| VID+PID+serial batem com entry `trusted=true` | **Auto-ON.** Skill carregada. Zero intervenção. |
| VID+PID conhecidos, serial novo, entry `trusted=true` | **Auto-ON.** Mesmo modelo já autorizado. |
| VID+PID conhecidos, nenhum entry | Cortext pergunta; se autorizar, cria entry. |
| VID+PID conhecidos, entry `trusted=false` | **Negado.** Dispositivo já foi rejeitado antes. |
| Desconhecido total | Negado. Logado. Sem entry criado. |

**Persistência:** A `TrustTable` é armazenada no SFS (`/system/trust/usb.tbl`) como um bloco raw de bytes (zerocopy). Rebooting não limpa o cache. Formatando o SFS, limpa.

### Vantagens da abordagem Neural profunda

1. **Nenhum driver clássico.** O kernel não contém HID driver, UVC driver, Mass Storage driver. Cada protocolo é uma skill WASM carregada sob demanda só quando o usuário infere intenção.
2. **Nenhum dispositivo tem autoridade implícita.** Sem autorun, sem montagem automática, sem drivers carregados automaticamente. O padrão é negar tudo até entrar no trust cache.
3. **Trust-once-use-always.** Depois de autorizado, reconectou = ON. Usabilidade preservada.
4. **O usuário precisa inferir (só na primeira vez).** O kernel não adivinha. Se você conecta um pendrive e não diz o que quer, o pendrive fica lá, detectado mas inacessível. Zero superfície de ataque por dispositivo desconhecido. **Na segunda vez** que conectar o mesmo pendrive, se já foi autorizado, o cache permite automaticamente.
5. **A IA "cria" a interface.** Para o mouse, a skill `hid_mouse.wasm` abstrai relatórios HID crus em `PointerEvent { dx, dy, buttons }`. Para a câmera, `uvc_capture.wasm` abstrai streams isócronos em `Tensor { w, h, format }`. O kernel não sabe o que é um "mouse" ou uma "câmera" — só vê skills e tensores.

### Trade-offs

| Prós | Contras |
|---|---|
| Kernel enxuto (xHCI ~500 LOC, sem class drivers) | Latência adicional: detectar → classificar → carregar skill WASM → só então usar |
| Zero autorun, zero superfície de ataque USB | Usuário precisa inferir intenção — experiência diferente de SO tradicional |
| Protocolos novos = novas skills WASM, não novo kernel | Skill WASM para SCSI/FAT32 é complexa (~2k LOC de WASM) |
| Hotplug seguro: dispositivo desconhecido = porta desligada | Hotplug rápido (ex: mouse que o usuário sempre usou) pode exigir cache de decisões |

---

## 2. SMP — Multicore e Multithreading

### Contexto Extra: AMD APU (Hardware-Alvo do Neural OS)

O Neural OS tem como alvo final a **AMD APU com memória unificada** (mentionado no AGENTS.md). As APUs modernas da AMD são literalmente a materialização física dos 3 rings:

| Ring | Neural OS | Hardware AMD APU |
|---|---|---|
| Ring 0 (NPU) | Neural Microkernel | **XDNA 2** (Ryzen AI 300) / XDNA 1 (Ryzen 7040/8040) |
| Ring 1 (GPU) | Tensor execution | **RDNA 3.5** iGPU (Strix Point) / RDNA 3 (Phoenix) |
| Ring 2 (CPU) | WASM skills | **Zen 5** (Strix Point) / Zen 4 (Phoenix) |

**Por que isso importa para SMP:**
- APUs têm **memória unificada** (CPU + GPU + NPU compartilham o mesmo espaço de endereçamento físico). Sem PCIe copies. A Zero-Copy SFS (Phase 4) é *nativamente* viável.
- NPU XDNA é uma **CGRA** (Coarse-Grained Reconfigurable Array), não um core x86. Ele não executa código Rust — recebe *overlays* compilados externamente. O Ring 0 do Neural OS conversa com o XDNA via MMIO + AXI transactions.
- A iGPU RDNA é acessível via comandos indiretos (não é um core x86). Ring 1 enfileira comandos no GPU queue.
- **Os "cores" detectados via MADT/CPUID são só os Zen (Ring 2).** O NPU e GPU aparecem como dispositivos PCIe, não como LAPICs.

**Implicações para o design SMP:**
- Os pools Ring 0, 1, 2 em *cores x86* são na verdade apenas Ring 2 (WASM) + um core reservado para gerenciar filas do NPU/GPU.
- Ring 0 (NPU) e Ring 1 (GPU) não são threads x86 — são aceleradores PCIe com suas próprias filas de comando.
- O `CorePools` precisa ser renomeado para `ComputePools` para incluir aceleradores não-x86.

```
ComputePools {
    ring0: Accelerator::NPU(XDNA),      // Não é um core x86
    ring1: Accelerator::GPU(RDNA),      // iGPU via fila de comandos
    ring2: Cores {                       // Cores Zen (P + E)
        wasm_pool: [...],                // Para skills WASM
        supervisor: CoreId(0),           // 1 core dedicado a gerenciar NPU/GPU
    }
}
```

### Como NPUs Realmente Funcionam — Arquitetura e Programação

#### A Natureza de uma CGRA

NPUs não são CPUs. Eles não executam código Rust, C, ou qualquer ISA sequencial. São **CGRAs** (Coarse-Grained Reconfigurable Arrays) — matrizes de tiles de computação interconectados por uma rede on-chip, onde o "programa" é um mapeamento estático de dados e operações no espaço físico:

```
   ┌─────┐   ┌─────┐   ┌─────┐   ┌─────┐
   │Tile0│───│Tile1│───│Tile2│───│Tile3│  ← AI Engine tiles (cada um: MAC array + memória local)
   └──┬──┘   └──┬──┘   └──┬──┘   └──┬──┘
      │         │         │         │
   ┌──▼──┐   ┌──▼──┐   ┌──▼──┐   ┌──▼──┐
   │Tile4│───│Tile5│───│Tile6│───│Tile7│
   └─────┘   └─────┘   └─────┘   └─────┘
        \   Roteamento de dados em pipeline /
         \     (dataflow espacial)        /
```

Cada tile tem sua própria memória local (SRAM) e uma unidade MAC/vectorial. O compilador do NPU decide:
- Qual tile executa qual camada da rede
- Como os dados fluem entre tiles (via DMA on-chip)
- Quando cada tile começa a processar

Não há "programa" no sentido de von Neumann — é um grafo de fluxo de dados **mapeado estaticamente** no hardware.

#### Cadeia de Programação (antes do boot)

```
Modelo (PyTorch/TF/ONNX)
       │
       ▼
 Vendor Compiler (AMD Vitis AI / Intel OpenVINO / Qualcomm QNN)
       │
       ├── Análise do grafo computacional
       ├── Particionamento: o que vai no NPU, o que fica na CPU
       ├── Mapeamento espacial: qual tile executa qual op
       ├── Roteamento de dados entre tiles
       ├── Agendamento estático (tick preciso)
       └── Geração do overlay binário + descritores de execução
              │
              ▼
       Overlay (.bin) + Metadados (entradas/saídas, endereços)
```

O **overlay** é um binário vendor-specific que contém:
- Configuração dos tiles (registradores, pesos, modos de operação)
- Roteamento da rede on-chip
- Tabela de endereços de I/O (onde ler entrada, onde escrever saída)

#### Interface de Execução (em runtime, no kernel)

No hardware real, o kernel conversa com o NPU via **interface de comandos** sobre PCIe ou MMIO, não executando código no NPU:

```
CPU (Ring 0 supervisor)
  │
  │  1. Prepara tensores de entrada em região de memória compartilhada
  │     (unified memory na AMD APU — sem cópia)
  │
  │  2. Escreve descritor de comando na fila do NPU:
  │     { overlay_addr, input_addr, output_addr, input_shape, ... }
  │
  │  3. Notifica NPU via MMIO write (doorbell register)
  │     ├── Escreve 1 no offset 0x20 da BAR do PCIe NPU
  │     └── NPU lê a fila, busca overlay da DRAM, executa
  │
  │  4. Poll ou interrupção:
  │     ├── Poll: lê status register até completion bit = 1
  │     └── IRQ: MSI-X → handler → acorda fila de skills
  │
  ▼
NPU executa overlay nos AI Engine tiles
  │
  ├── Tile 0-1: Embedding (lookup table em SRAM local)
  ├── Tile 2-5: Self-attention (MAC em pipeline)
  ├── Tile 6-7: FFN (SiLU + matmul ternário)
  └── DMA escrita → resultado na DRAM compartilhada
      (output tensor no mesmo espaço de endereçamento)
```

**O Ring 0 do Neural OS (Intent Router MLP) NÃO precisa do NPU.** O modelo é minúsculo (3→2, ~20 pesos). Um core Zen 5 processa em nanossegundos. O NPU seria usado para modelos maiores invocados por skills WASM (Ring 2).

#### Estratégia para o Neural OS — 3 Cenários

| Cenário | O que acontece | Implementação |
|---|---|---|
| **QEMU (dev)** | Sem NPU real. Rings 0/1/2 simulados como cores x86. O Intent Router roda no core 0 como software. | `ring0::Accelerator::Software` — struct que executa o MLP inline no core supervisor |
| **AMD APU real (com NPU desconhecido)** | NPU detectado via PCI (VID 1002, DID específico) mas sem driver. Kernel usa `ring0::Accelerator::Software` transparentemente. | Fallback automático no `init_npu()`: se falha ao carregar overlay → usa software |
| **AMD APU real (com firmware NPU disponível)** | Kernel carrega overlay do Intent Router no NPU via MMIO. Ring 0 se torna verdadeiramente acelerado por hardware. | `ring0::Accelerator::XDNA { bar: MmioAddr, queue: CommandQueue }` — 50 TOPS disponíveis |

#### Código conceitual: interface NPU no kernel

```rust
// src/npu.rs — Driver do NPU como acelerador Ring 0

pub enum NpuCommand {
    RunOverlay {
        overlay_addr: PhysAddr,   // overlay pré-carregado na DRAM
        input_addr: VirtAddr,     // tensor de entrada (memória compartilhada)
        output_addr: VirtAddr,    // tensor de saída
        input_len: usize,
        output_len: usize,
    },
}

pub struct Npu {
    bar: MmioRegion,              // BAR0 do PCIe (mapeado após init_memory)
    queue: CommandQueue,          // fila circular de comandos (DRAM ou SRAM)
    irq: Option<MsixVector>,
    state: NpuState,
}

impl Npu {
    /// Tenta inicializar. Se não achar hardware ou firmware, retorna None.
    pub fn try_init() -> Option<Self> {
        let pci_device = pci::scan(0x1002, 0x150F)?;  // AMD XDNA VID/DID
        let bar = memory::mmio_map(pci_device.bar(0))?;
        let firmware = firmware::load("xdnp0.bin")?;   // blob do fabricante
        
        // Sequência de init:
        // 1. Mapear BAR
        // 2. Carregar firmware de boot do NPU via MMIO
        // 3. Aguardar READY (poll status register)
        // 4. Alocar command queue na DRAM compartilhada
        // 5. Configurar base address register da queue
        // 6. Registrar MSI-X se disponível
        // 7. Carregar overlay base do Ring 0
        
        Some(Npu { bar, queue, irq, state: NpuState::Ready })
    }

    /// Submete overlay para execução no NPU.
    pub fn submit(&mut self, cmd: &NpuCommand) -> Result<(), NpuError> {
        self.queue.enqueue(cmd)?;              // Escreve na fila circular
        self.bar.write(u32::MAX, DOORBELL_OFFSET); // Notifica NPU
        while !self.bar.read::<u32>(STATUS_OFFSET) & COMPLETION_BIT != 0 {
            core::hint::spin_loop();           // Poll (ou esperar IRQ)
        }
        Ok(())
    }
}

// Se Npu::try_init() retornar None, Ring 0 usa fallback:
pub enum Accelerator {
    XDNA(Npu),           // Hardware real
    Software,            // Fallback: executa MLP inline no core supervisor
}
```

#### Lições dos Fornecedores

| Vendor | Driver Linux | FW Blob? | Registers documentados? | SDK de compile |
|---|---|---|---|---|
| **AMD XDNA** | `amdxdna.ko` | ✅ Sim | ❌ Não | Ryzen AI Software (ONNX RT EP) |
| **Intel NPU 4** | `ivpu.ko` | ✅ Sim | ❌ Não | OpenVINO |
| **Qualcomm Hexagon** | `qcom_q6v5_mss` | ✅ Sim | ❌ Não | Qualcomm AI Engine Direct |
| **Apple Neural Engine** | Fechado | ✅ Sim | ❌ Não | Core ML |

**Conclusão:** Nenhum fabricante documenta o nível de registro/MMIO do NPU. Todos exigem firmware blob + SDK proprietário. A abordagem prática para o Neural OS:

1. **QEMU:** `Accelerator::Software` — MLP roda inline no core supervisor. Mesmo comportamento funcional.
2. **APU real, fase 1:** Detectar PCI, tentar init, fallback silencioso para Software se falhar.
3. **APU real, fase 2:** Adaptar `amdxdna.ko` para `no_std` — possível pois o driver é GPL e open source (~5k LOC de kernel, ~2k de HW interface). O firmware continua sendo blob fechado.
4. **APU real, fase 3:** Compilar o overlay do Intent Router via Ryzen AI Software offline (no PC host) e embutir no boot image. O kernel só carrega o overlay já compilado na DRAM e escreve o descritor.

**Resumo:** O Neural OS trata o NPU como um acelerador **opcional**. O Ring 0 funciona em software com performance mais que suficiente (<1 TOPS necessário, 1 core Zen 5 entrega ~100 TOPS INT8 via AVX-512). O NPU real é um upgrade de eficiência energética, não de funcionalidade.

### Decisão: **Implementar APIC + SMP no Sprint 12 (Phase 4).**

**Justificativa:**
- O PIC 8259A atual (Sprint 8) é single-core. Para escalar inferência, precisamos de múltiplos núcleos.
- A arquitetura de 3 rings mapeia naturalmente para agrupamentos de cores dedicados.

### Problema: N+1 cores e heterogeneidade P-core / E-core

CPUs modernas variam drasticamente em contagem e tipo de núcleo:

| CPU | P-cores | E-cores | Total (com HT) |
|---|---|---|---|
| Intel i3-N305 (NUC) | 0 | 8 | 8 |
| Intel i5-13600K | 6 | 8 | 20 |
| Intel i9-14900K | 8 | 16 | 32 |
| AMD Ryzen 9 7950X | 16 (todos iguais) | 0 | 32 (SMT) |
| QEMU `-smp 2` | 2 (virt) | 0 | 2 |

**P-cores (Performance):** maior clock, hiperthreading, instruções pesadas (AVX-512, AMX). Ideais para Ring 0 (roteamento de baixa latência) e Ring 1 (matmul tensorial pesado).

**E-cores (Efficiency):** menor clock, sem HT, menor cache. Ideais para Ring 2 (WASM skills — I/O bound, isolamento por sandbox).

**Detecção — Intel Hybrid:** CPUID leaf `0x1A` (native model ID + core type) + ACPI PPTT (`Processor Properties Topology Table`). `leaf.0x1A.ECX[1:0]`: `00` = P-core, `01` = E-core.

**RISC observado:** QEMU não expõe heterogeneidade. Em AMD (todos P-cores iguais), o kernel trata todos como P-cores.

### Solução: Pool dinâmico de cores por Ring

Em vez de pinagem fixa (Core 0 = Ring 0, Core 1 = Ring 1...), usar **pools de afinidade** construídos em tempo de boot:

```
Cores descobertos (N)
  │
  ├─ Classificar por tipo (P-core / E-core)
  │
  ├─ Ring 0 Pool:    1 P-core (o BSP, latência mais baixa)
  ├─ Ring 1 Pool:    P-cores restantes (matmul pesado)
  └─ Ring 2 Pool:    E-cores + P-cores ociosos (WASM skills)
```

**Algoritmo de atribuição:**

```rust
fn assign_cores(cpus: &[CpuInfo]) -> CorePools {
    let bsp = cpus.iter().find(|c| c.is_bsp).unwrap();
    let p_cores: Vec<_> = cpus.iter().filter(|c| c.core_type == CoreType::Performance).collect();
    let e_cores: Vec<_> = cpus.iter().filter(|c| c.core_type == CoreType::Efficiency).collect();

    CorePools {
        ring0: vec![bsp],                          // 1 P-core fixo (baixa latência)
        ring1: p_cores.iter().filter(|c| c.id != bsp.id).copied().collect(), // P-cores restantes
        ring2: e_cores.iter().copied().collect(),  // E-cores primeiro
    }
    // Se não há E-cores, ring2 pega P-cores excedentes:
    // if ring2.is_empty() { ring2 = ring1.split_off(ring1.len() / 2); }
}
```

### Regras de escalonamento:

| Trabalho | Pool alvo | Motivo |
|---|---|---|
| Intent Router forward pass | Ring 0 | Latência crítica, 1 core basta |
| `Tensor::matmul()` grande | Ring 1 | P-cores + AVX/AMX |
| `Tensor::matmul()` pequeno | Ring 0 ou Ring 1 | Decisão do roteador por carga |
| Skill WASM (I/O bound) | Ring 2 | E-cores são suficientes |
| Skill WASM (compute-heavy) | Ring 1 (sobressalente) | Migração se Ring 2 saturado |

### Implementação:

1. **Detecção de topologia:**
   - **ACPI MADT:** lista de LAPICs (quantos processadores lógicos existem).
   - **CPUID leaf `0x0B` (Extended Topology Enumeration):** hierarquia de níveis (thread, core, package).
   - **CPUID leaf `0x1A` (Hybrid enumeration — Intel):** `ECX[7:0]` = native model ID, `ECX[1:0]` = core type (`00` Intel Atom / E-core, `01` Intel Core / P-core).
   - **Fallback:** se leaf `0x1A` não existir, todos os cores são tratados como P-core.
2. **APIC → x2APIC:** Substituir PIC por LAPIC (Local APIC) + IOAPIC. x2APIC mode (MSR-based, sem MMIO) para simplificar.
3. **Per-CPU data (`struct PerCpu`):**
   ```rust
   #[repr(C)]
   struct PerCpu {
       core_id: u64,
       lapic_id: u32,
       core_type: CoreType,       // Performance ou Efficiency
       ring: Ring,                // 0, 1, ou 2
       kernel_stack: *mut u8,
       current_work: Option<WorkId>,
       r0_queue: mpsc::Receiver<Intent>,   // fila do Ring 0
       r1_queue: mpsc::Receiver<TensorOp>, // fila do Ring 1
       r2_queue: mpsc::Receiver<WasmSkill>,// fila do Ring 2
   }
   ```
   Acesso via `FS.base` ou `GS.base` (segment register per-core) com macro `core_local!()`.

4. **Startup IPI:** Core 0 (BSP) envia INIT-SIPI-SIPI para cada AP. Cada AP executa stub de assembly que configura stack e salta para `rust_ap_entry(core_id)`. Cada AP então consulta sua `struct PerCpu` para saber qual fila pollar.

5. **Sincronização:**
   - `spin::Mutex` → substituir por `ticket_lock` (fair, SMP-safe) ou `MCS lock` (cache-friendly).
   - Contador do timer → `AtomicU64` (já feito).
   - IPC entre cores → `lock-free mpsc queue` (ring buffer atômico) baseado em `crossbeam` portado para `no_std` ou implementação própria com `AtomicPtr` + CAS.

### Diagrama de Boot SMP:

```rust
kernel_main(BootInfo)               // BSP, detectado como P-core
  ├─ init_memory(), init_heap(), enable_simd()
  ├─ init_apic()                    // LAPIC + IOAPIC
  ├─ init_smp()                     // MADT + CPUID 0x1A → CorePools
  │    ├─ detect_topology()         // Quem somos? Quantos? P ou E?
  │    ├─ assign_cores()            // Ring 0/1/2 pools
  │    ├─ build_percpu_tables()     // GDT/TSS/IDT/stack por core
  │    └─ wake_aps()                // INIT-SIPI-SIPI para APs
  │         ├─ AP (P-core)  → rust_ap_entry → poll r1_queue (Ring 1)
  │         ├─ AP (E-core)  → rust_ap_entry → poll r2_queue (Ring 2)
  │         ├─ AP (P-core)  → rust_ap_entry → poll r1_queue (Ring 1)
  │         └─ ... (N núcleos)
  ├─ init_timer()                   // LAPIC timer (mais preciso que PIT)
  ├─ enable_interrupts()
  └─ loop { intent_router() }       // Ring 0: classifica intent → enfileira em r1 ou r2
```

### Casos de borda:

| Cenário | Comportamento |
|---|---|
| 1 core apenas (QEMU `-smp 1`) | Ring 0, Ring 1 e Ring 2 rodam todos no mesmo core. Sem concorrência. |
| 2 cores (QEMU `-smp 2`) | Core 0 = Ring 0, Core 1 = Ring 1+2 (compartilhado). |
| Só E-cores (Intel N100 / N305) | Ring 0 no E-core 0. Tudo roda em E-cores (mais lentos, mas funcionais). |
| P-core + HT (2 threads mesmo core) | HT threads compartilham cache L1. Atribuir apenas 1 thread por core físico a Ring 0/1; HT restante vai para Ring 2. Detectado via CPUID `0x0B` nível "core" vs "thread". |
| Hotplug / CPU offline | Ignorado. Neural OS não faz hotplug. Cores não detectados no boot ficam offline para sempre. |

### Dependências:

| Crate | Versão | Para quê |
|---|---|---|
| `x86_64` | 0.14.11+ | Já temos. APIC MSR, seg registers, CPUID |
| `acpi` | (futuro) | Parser MADT (descoberta de cores) + PPTT (topologia híbrida) |
| `raw-cpuid` | (futuro) | Detecção de features (x2APIC, SMP, hybrid leaf 0x1A) |

---

## 3. AI-Driven Hardware Detection — A IA Arquitetou o Sistema

Toda a discussão anterior (SMP pools, USB trust cache, NPU fallback) converge para um ponto: **nenhuma decisão de hardware é hardcoded.** O Neural Cortex recebe um inventário completo do hardware no boot e produz a configuração ótima por inferência.

### O Boot Inventário

Antes de qualquer configuração, o kernel faz um inventário bruto de tudo que existe:

```rust
struct HardwareInventory {
    // CPU
    cpu_brand: [u8; 48],           // "Intel(R) Core(TM) i5-7200U"
    cpu_cores: Vec<CoreInfo>,       // Cada core: P ou E, frequência, cache
    cpu_features: CpuFeatureSet,    // AVX-512? AMX? VP2INTERSECT? x2APIC?

    // Memória
    total_ram: u64,                 // Bytes totais (e.g., 16GB = 17179869184)
    memory_map: &'static MemoryMap, // Regiões do bootloader

    // Aceleradores
    accelerators: Vec<AccelInfo>,   // NPUs, GPUs, DSPs via PCI scan
    // { vid, did, class, bars, num_queues, tops_estimate }

    // Armazenamento
    storage: Vec<StorageInfo>,      // NVMe, VirtIO-blk, SATA (via PCI)
    // { pci_addr, capacity, is_nvme, is_rotational }

    // Barramento
    pci_devices: Vec<PciDevice>,    // Tudo que achou no PCI scan
}
```

### A Inferência de Arquitetura

O Cortex recebe o inventário e produz uma `SystemArchitecture`:

```rust
struct SystemArchitecture {
    ring0: Ring0Config,   // Onde roda o Intent Router
    ring1: Ring1Config,   // Onde roda tensor matmul
    ring2: Ring2Config,   // Onde rodam skills WASM
    sfs: SfsConfig,       // Zero-copy storage mapping
    heap: HeapConfig,     // Tamanho e localização do heap
    trust: TrustPolicy,   // Política default para novos dispositivos
}
```

**O MLP de arquitetura** é um modelo separado do Intent Router (maior, pois entrada é mais complexa):

```
Entrada: ~512 features normalizadas
  ├── brand_embedding: [f32; 32]     (embedding do nome do processador)
  ├── core_counts: [p_count, e_count, ht_count, total_threads]
  ├── core_freqs: [base_freq, boost_freq]
  ├── cache: [l1_per_core, l2_per_core, l3_shared]
  ├── features: [avx512, amx, vnni, smp, x2apic, hybrid, ...] (one-hot)
  ├── ram_total_gb: f32
  ├── ram_speed_mhz: f32
  ├── ram_ecc: bool
  ├── accelerators: [npu_tops, gpu_flops, num_gpu_cores, ...]
  ├── storage: [fastest_nvme_gbps, has_hdd, has_sata, ...]
  └── pci_devices_count: u32

      │
      ▼  MLP (512 → 256 → 64 → 9, SiLU, argmax)
      │    Treinado offline com milhares de configurações de hardware
      │
Saída: 9 decisões categóricas (argmax por grupo)
  ├── ring0_target:  { 0=software, 1=NPU_if_available, 2=dedicated_P_core }
  ├── ring1_target:  { 0=P_cores, 1=GPU, 2=P+GPU_hybrid }
  ├── ring2_target:  { 0=E_cores, 1=P_cores_idle, 2=E+P_mixed }
  ├── p_cores_for_ring1: u8        (quantos P-cores dedicar ao tensor)
  ├── sfs_policy:    { 0=NVMe_only, 1=NVMe+HDD_tiered, 2=all_ram }
  ├── heap_size_mb:  u16           (em MB, ex: 64 para 16GB, 1024 para 1TB)
  ├── trust_default: { 0=deny_all, 1=allow_known, 2=allow_all }
  └── power_policy:  { 0=performance, 1=balanced, 2=low_power }
```

### Memory Hierarchy Index — Alocação por Equipamento e Velocidade

Cada dispositivo de memória/armazenamento no sistema é indexado pelo Cortex em uma **tabela hierárquica ordenada por velocidade**. A IA usa este índice para decidir *onde* alocar cada tensor, cache ou página.

```rust
struct MemoryTier {
    device: MemoryDevice,     // Qual hardware
    kind: MemoryKind,         // VRAM, RAM, NVMe, HDD
    capacity: u64,            // Bytes totais
    bandwidth_gbps: f32,      // Largura de banda estimada
    latency_ns: u64,          // Latência aproximada
    is_unified: bool,         // Compartilhado com CPU? (AMD APU)
    numa_node: u8,            // Nó NUMA (0 se single-socket)
}

struct MemoryHierarchy {
    tiers: Vec<MemoryTier>,   // Ordenado do mais rápido para o mais lento
    // Exemplo notebook antigo:
    // [0] VRAM GTX 1050:    4GB @ 112 GB/s   (GDDR5)
    // [1] RAM DDR4-2400:   16GB @ 19.2 GB/s  (2 canais)
    // [2] NVMe:           256GB @ 3.5 GB/s
    // [3] HDD 5400rpm:    1TB  @ 0.1 GB/s
    //
    // Exemplo Xeon 6900:
    // [0] RAM DDR5-8800:  1024GB @ 140+ GB/s (múltiplos canais)
    // [1] NVMe RAID:       varios TB @ ~14 GB/s
    //
    // Exemplo AMD APU (memória unificada):
    // [0] RAM LPDDR5X-7500: 32GB @ 120 GB/s (CPU+GPU+NPU compartilham!)
    // [1] NVMe:             1TB  @ 5 GB/s
}
```

#### Como a IA usa o Memory Hierarchy Index

O MLP de arquitetura produz **políticas de alocação por camada**, não só um tamanho de heap:

```
Saída do MLP (expandido):
  ├── ringX: ... (já definido)
  ├── heap_tier:  u8   (0=VRAM, 1=RAM, 2=NVMe, 3=HDD - qual camada usar para heap)
  ├── sfs_active_tier: u8  (qual camada para SFS quente: 0=RAM, 1=NVMe)
  ├── sfs_cold_tier: u8   (qual camada para SFS frio: 1=NVMe, 2=HDD)
  ├── tensor_active_tier: u8  (onde alocar tensores de inferência ativa)
  ├── tensor_swap_tier: u8    (onde swapar tensores ociosos)
  ├── kv_cache_tier: u8       (onde alocar KV-cache do modelo)
  └── skill_heap_tier: u8     (onde alocar heaps de skills WASM)
```

**Cada tier é uma decisão de trade-off entre velocidade e capacidade:**

| Tier | Latência | Capacidade | Ideal para |
|---|---|---|---|
| VRAM (GPU) | ~10ns | Pequena (4-24GB) | Tensores ativos de inferência |
| RAM | ~100ns | Média (16-1024GB) | Heap, SFS cache, KV-cache |
| NVMe | ~10µs | Grande (256GB-8TB) | SFS, tensor swap, memória episódica |
| HDD | ~10ms | Massiva (1-20TB) | Cold storage, logs, arquivos |

#### Exemplo: notebook antigo (i5 + GTX 1050 + 16GB + NVMe + HDD)

```
MemoryHierarchy:
  tier[0] = VRAM GTX 1050:  4GB @ 112 GB/s   ← tensor_active (matmul na GPU)
  tier[1] = RAM DDR4:        16GB @ 19.2 GB/s  ← heap, kv_cache, skill_heap, sfs_active
  tier[2] = NVMe 256GB:      256GB @ 3.5 GB/s  ← sfs_cold, tensor_swap
  tier[3] = HDD 1TB:         1TB @ 0.1 GB/s    ← episodic_memory, logs

Decisão do Cortex:
  tensor_active_tier  → 0 (VRAM)   — matmul na GPU, dados na VRAM
  heap_tier           → 1 (RAM)    — heap clássico na RAM
  kv_cache_tier       → 1 (RAM)    — KV-cache na RAM (só 16GB, mas headroom)
  sfs_active_tier     → 1 (RAM)    — SFS quente mapeado na RAM (NVMe como backing)
  sfs_cold_tier       → 2 (NVMe)   — SFS frio no NVMe
  tensor_swap_tier    → 2 (NVMe)   — tensores ociosos swapados pro NVMe
  episodic_memory     → 3 (HDD)    — memória episódica de longo prazo no HDD
```

#### Exemplo: Xeon 6900 (1TB RAM, NVMe RAID)

```
MemoryHierarchy:
  tier[0] = RAM DDR5-8800: 1024GB @ 140+ GB/s  ← TUDO na RAM
  tier[1] = NVMe RAID:      varios TB @ 14 GB/s ← só se a RAM acabar

Decisão do Cortex:
  tensor_active_tier  → 0 (RAM)    — 1TB de RAM, cabe tudo
  heap_tier           → 0 (RAM)    — heap de 1GB é insignificante
  kv_cache_tier       → 0 (RAM)    — KV-cache de modelos grandes na RAM
  sfs_active_tier     → 0 (RAM)    — SFS virtual inteiro mapeado na RAM
  sfs_cold_tier       → 1 (NVMe)   — NVMe só para persistência entre reboots
  tensor_swap_tier    → 0 (RAM)    — swap desnecessário
```

#### Exemplo: AMD APU Strix Point (memória unificada)

```
MemoryHierarchy:
  tier[0] = RAM LPDDR5X-7500: 32GB @ 120 GB/s  ← unified (CPU+GPU+NPU)
  tier[1] = NVMe 1TB:         1TB @ 5 GB/s

Decisão do Cortex:
  tensor_active_tier  → 0 (RAM unified) — GPU e NPU acessam mesma RAM, zero cópia
  heap_tier           → 0 (RAM)         — heap na memória unificada
  kv_cache_tier       → 0 (RAM)         — mesma RAM, acessível pelo NPU
  sfs_active_tier     → 0 (RAM)         — SFS cache na unified memory
  sfs_cold_tier       → 1 (NVMe)        — persistência
  DESTAQUE:           → zero cópias entre CPU/GPU/NPU. Tudo no mesmo barramento.
```

#### Alocação Física

A `MemoryTier` não é apenas consultiva — ela **guia o frame allocator**:

```rust
// src/memory.rs
pub enum AllocTier {
    Vram,       // Alocar na VRAM da GPU (via BAR ou GTT)
    Dram,       // Alocar na DRAM do sistema (heap normal)
    Nvme,       // Alocar páginas no NVMe (mapeado via SFS)
    Hdd,        // Alocar no HDD (apenas para cold storage)
}

pub fn alloc_by_tier(tier: AllocTier, size: usize) -> Option<PhysAddr> {
    match tier {
        AllocTier::Vram  => gpu::alloc_vram(size),  // BAR da GPU
        AllocTier::Dram  => frame_allocator.alloc(size * PAGE_SIZE),
        AllocTier::Nvme  => sfs::mmap(size, SfsTier::Active),
        AllocTier::Hdd   => sfs::mmap(size, SfsTier::Cold),
    }
}
```

### Exemplos reais (dados do usuário)

#### Caso 1: Notebook antigo — i5-7200U, GTX 1050, 16GB DDR4, NVMe 256GB + HDD 1TB

```
INVENTÁRIO:
  CPU:  Intel i5-7200U (Kaby Lake, 2016)
        2 P-cores + 0 E-cores + 2 HT = 4 threads
        Sem NPU, sem AVX-512, sem AMX
        Base 2.5GHz, Boost 3.1GHz
  RAM:  16GB DDR4-2400, sem ECC
  GPU:  GTX 1050 (Pascal, 4GB) via PCIe — detectada como acelerador
  NPU:  NÃO detectado
  SFS:  NVMe 256GB (primário) + HDD 1TB 5400rpm (secundário)
  PCI:  12 dispositivos (GPU, xHCI, SATA, etc.)

INFERÊNCIA DO CORTEX:
  ring0_target      → software (sem NPU, roda no core 0)
  ring1_target      → GPU (GTX 1050 é melhor que P-cores sem AVX-512)
  ring2_target      → P-cores_idle (só 2 P-cores, ring1 usa GPU, ring2 pega CPU)
  p_cores_for_ring1 → 0 (GPU faz tudo)
  sfs_policy        → NVMe+HDD_tiered (NVMe para SFS ativo, HDD para memória episódica fria)
  heap_size_mb      → 64 (16GB RAM é pouco, heap modesto)
  trust_default     → allow_known (máquina pessoal, usa trust cache)
  power_policy      → balanced (laptop, não queimar bateria)
```

**Resultado final:**

```
Ring 0  → Core 0 (Intent Router software)
Ring 1  → GTX 1050 (matmul pesado na GPU)
Ring 2  → Cores 1,2,3 (WASM skills nos 3 threads restantes)
Heap    → 64 MB em 0x4444_4444_0000
SFS     → NVMe mapeado direto, HDD para cold storage via DMA
```

#### Caso 2: Servidor Xeon 6900 — 288 cores, DDR5 MRDIMM 8800MT/s, TBs de RAM, AI coprocessors

```
INVENTÁRIO:
  CPU:  Intel Xeon 6900 (Granite Rapids, 2025)
        144 P-cores + 144 E-cores + HT = 576 threads
        AI coprocessor integrado (NPU! Detectado via PCI)
        AMX, AVX-512, VNNI, x2APIC
        Base ~2.0GHz (E), ~3.0GHz (P)
  RAM:  1TB DDR5 MRDIMM @8800MT/s, ECC
  GPU:  Nenhuma dedicada detectada (servidor headless)
  NPU:  Intel AI coprocessor (VID 8086, DID específico) — ~50 TOPS estimado
  SFS:  NVMe RAID (vários, via PCI) — sem HDD
  PCI:  40+ dispositivos

INFERÊNCIA DO CORTEX:
  ring0_target      → NPU (AI coprocessor disponível! 50 TOPS)
  ring1_target      → P_cores (144 P-cores com AMX/AVX-512) + NPU (overflow)
  ring2_target      → E_cores (144 E-cores, mais que suficientes para WASM)
  p_cores_for_ring1 → 72 (metade dos P-cores para tensor)
  sfs_policy        → all_ram (1TB RAM + NVMe RAID é essencialmente RAM drive)
  heap_size_mb      → 1024 (terabytes de RAM, heap generoso)
  trust_default     → deny_all (servidor, segurança máxima)
  power_policy      → performance (servidor, sem economia)
```

**Resultado final:**

```
Ring 0  → NPU (Intel AI coprocessor, 50 TOPS — roteamento em hardware)
Ring 1  → 72 P-cores (matmul com AMX/AVX-512, throughput massivo)
Ring 2  → 144 E-cores + 72 P-cores restantes (WASM skills)
          + 288 HT threads (background I/O)
Heap    → 1 GB em 0x4444_4444_0000
SFS     → NVMe RAID mapeado como RAM (zerocopy, terabytes)
```

#### Caso 3: AMD APU — Ryzen AI 9 HX 370 (Strix Point)

```
INVENTÁRIO:
  CPU:  AMD Ryzen AI 9 HX 370 (Strix Point, 2024)
        4 Zen 5 P-cores + 8 Zen 5c E-cores + SMT = 24 threads
        AVX-512, VNNI
  RAM:  32GB LPDDR5X-7500 (unified! CPU+GPU+NPU compartilham)
  GPU:  Radeon 890M (RDNA 3.5, 16 CUs) integrada
  NPU:  XDNA 2 (50 TOPS) — detectado via PCI (VID 1002)
  SFS:  NVMe 1TB (único)
  PCI:  15 dispositivos

INFERÊNCIA DO CORTEX:
  ring0_target      → NPU (XDNA 2, 50 TOPS — hardware-alvo)
  ring1_target      → GPU (RDNA 3.5, memória unificada — zero cópia)
  ring2_target      → E-cores (Zen 5c, 8 cores para WASM)
  p_cores_for_ring1 → 4 (P-cores auxiliam GPU em tarefas sequenciais)
  sfs_policy        → NVMe_only + ram_cache (unified memory permite cache agressivo)
  heap_size_mb      → 128 (32GB RAM confortável)
  trust_default     → allow_known (laptop pessoal)
  power_policy      → balanced (bateria)
```

**Resultado final:**

```
Ring 0  → XDNA 2 NPU (roteamento neural em hardware, consumo <15W)
Ring 1  → RDNA 3.5 iGPU + 4 P-cores (matmul na GPU, controle nos P-cores)
Ring 2  → 8 Zen 5c E-cores (WASM skills eficientes)
Heap    → 128 MB (memória unificada reduz necessidade de heap separado)
SFS     → NVMe com cache em RAM unificada
```

### Como o MLP é treinado

O modelo de arquitetura (512→256→64→9) é **treinado offline** com milhares de configurações de hardware:

1. **Database de treinamento:** Gera-se uma matriz de ~10k hardware profiles (variando CPU, RAM, GPU, NPU, storage) e a configuração ótima para cada um (calculada por simulação ou benchmark).
2. **Target variable:** Para cada profile, calcula-se a configuração que maximiza throughput de inferência e minimiza latência do intent router.
3. **Formato dos pesos:** O MLP treinado é quantizado para ternário (`PackedTernaryTensor`) e embutido no kernel como constante — mesmo formato do Sprint 10.
4. **Atualização:** O modelo pode ser substituído por uma skill WASM que baixa pesos novos (Phase 5+).

### Por que isso é inovador

| Abordagem | Como configura o sistema |
|---|---|
| **Linux** | kernel + initramfs + device tree + módulos + sysadmin |
| **seL4** | static configuration em tempo de compilação |
| **Folkering OS** | hardcoded para x86-64 SMP |
| **Neural OS (esta ADR)** | **Inventário → MLP → Config dinâmica. A IA decide.** |

Nenhum SO existente faz isso. O Neural OS literalmente *pensa* sobre o hardware que encontrou e *decide* a melhor arquitetura na hora.

### Implementação no boot flow

```rust
kernel_main(BootInfo) {
    // 1. Inventário (hardcoded, sem IA)
    let inventory = HardwareInventory::collect(boot_info);

    // 2. Inferência de arquitetura (MLP)
    let arch = cortex::infer_architecture(&inventory);

    // 3. Configuração dinâmica (usando a decisão do MLP)
    init_memory(arch.heap_size_mb);
    init_smp(arch.ring0_target, arch.ring1_target, arch.ring2_target,
             arch.p_cores_for_ring1);
    init_npu(arch.ring0_target);         // se NPU, init comando queue
    init_gpu(arch.ring1_target);         // se GPU, init compute queue
    init_sfs(arch.sfs_policy);
    init_trust(arch.trust_default);
    init_power(arch.power_policy);

    // 4. Boot completo
    serial_println!("[ARCH] Ring 0: {:?}", arch.ring0_target);
    serial_println!("[ARCH] Ring 1: {:?}", arch.ring1_target);
    serial_println!("[ARCH] Ring 2: {:?}", arch.ring2_target);
    serial_println!("[ARCH] Heap: {} MB at 0x4444_4444_0000", arch.heap_size_mb);

    enable_interrupts();
    loop { intent_router() }
}
```

### Considerações finais

1. **O modelo de arquitetura é pequeno.** 512→256→64→9 = ~150k pesos ternários ≈ 150k * 2 bits = ~37 KB. Cabe no kernel sem drama.
2. **A inferência é rápida.** Forward pass de 4 camadas MLP ternário em 1 core = microssegundos. Não atrasa o boot.
3. **Fallback sempre existe.** Se o MLP produzir algo absurdo (ex: heap 0 MB), o kernel usa valores seguros predefinidos.
4. **Extensível.** Novos tipos de hardware (ex: FPGA, TPU, CXL memory) só precisam estender o inventário e re-treinar o modelo.

---

## 4. Periféricos

### Decisão: Enumeração PCI mínima + VirtIO. Sem driver model genérico.

**Justificativa:** Neural OS conhece exatamente quais dispositivos espera encontrar:

| Dispositivo | Purpose | Driver |
|---|---|---|
| NVMe (PCI Class 01.08) | Storage SFS | `src/nvme.rs` (Phase 4) |
| VirtIO-blk (PCI 1AF4:1001) | Storage QEMU | VirtIO (alternativa ao NVMe em dev) |
| VirtIO-net (PCI 1AF4:1041) | Rede | VirtIO (para skills de rede) |
| VirtIO-gpu (PCI 1AF4:1050) | Vídeo | VirtIO (Phase 5 skills visuais) |
| Intel HDA (PCI 04.03) | Áudio | Futuro |

### Enumeração PCI:

```rust
// Mínimo: ler Vendor ID, Device ID, BARs, configurando só o necessário
for (bus, dev, func) in pci_scan() {
    match (vendor_id, device_id) {
        (0x1AF4, 0x1041) => init_virtio_net(bars),
        (0x1AF4, 0x1050) => init_virtio_gpu(bars),
        (0x8086, 0x2922) => init_nvme(bars),
        _ => skip,  // dispositivo desconhecido = ignorar
    }
}
```

- Sem kernel thread de hotplug.
- Sem sysfs genérico.
- Sem device tree.

---

## 4. Entrada/Saída — Áudio e Vídeo

### 4.1 Vídeo

**Estágio atual:** VGA text mode 80×25 (Sprint 2).

**Evolução:**

| Estágio | Sprint | Descrição |
|---|---|---|
| VGA Text | Sprint 2 ✅ | 80×25, 16 cores, scrolling |
| UEFI Framebuffer | Sprint 12 (Phase 4) | Resolução nativa via `BootInfo::framebuffer` |
| VirtIO-GPU | Sprint 16+ (Phase 5) | 2D/3D acelerado para visualizações neurais |
| Tensor Visualization | Phase 5+ | Renderizar ativações, attention maps diretamente no framebuffer |

**UEFI Framebuffer:**
- `bootloader` crate com `map_physical_memory` já mapeia o framebuffer.
- Implementar `FramebufferWriter { base: *mut u32, width: usize, height: usize, stride: usize }`.
- Pixel format: BGRA32 (padrão UEFI).
- Font rendering bitmaps para texto em alta resolução.
- Manter VGA text mode como fallback se framebuffer não disponível.

### 4.2 Áudio

**Decisão:** Adiar para Phase 5+ (quando uma skill WASM de voz/fala for requisito).

**Justificativa:**
- Nenhuma skill atual precisa de áudio.
- Intel HDA é complexo (CORB/RIRB, DMA engines, verb tables).
- USB Audio Class é ainda mais complexo.
- VirtIO-sound não está maduro no QEMU.

**Plano futuro (se requisitado):**
- Driver Intel HDA mínimo: só playback PCM (sem captura, sem mixer).
- Buffer circular DMA → codec → saída analógica.
- Skill WASM envia tensor f32 → kernel converte para PCM → DMA.

---

## Resumo: Matriz Hardware × Sprint

| Hardware | Sprint | Prioridade | Depende de |
|---|---|---|---|
| PS/2 keyboard (legacy UEFI) | Já ✅ | — | — |
| **APIC + x2APIC** | **Sprint 12** | 🔥 Alta | IDT (Sprint 3) |
| **SMP (N+1 cores, P/E detect)** | **Sprint 12** | 🔥 Alta | APIC, ACPI MADT, CPUID 0x1A |
| **CorePools (Ring 0/1/2 pools)** | **Sprint 12** | 🔥 Alta | SMP, detecção de tipo de core |
| **SMP ticket lock** | **Sprint 12** | 🔥 Alta | — |
| **Per-CPU structs (FS/GS.base)** | **Sprint 12** | 🔥 Alta | GDT, seg registers |
| PCI enumeração | Sprint 13 | Média | — |
| VirtIO-blk | Sprint 13 | Média | PCI |
| VirtIO-net | Sprint 14 | Média | PCI, smoltcp |
| UEFI framebuffer | Sprint 14 | Média | BootInfo |
| HDA áudio | Phase 5+ | Baixa | PCI |
| **USB xHCI mínimo (< 500 LOC)** | **Sprint 14** | 🔥 Alta | PCI enumeração |
| **USB Neural Classifier (MLP)** | **Sprint 15** | 🔥 Alta | Intent Router (Sprint 7), xHCI mínimo |
| **USB HID handler (ps2 bridge)** | **Sprint 15** | Média | Neural USB classifier, PS/2 input |
| **USB UVC handler (câmera)** | Phase 5+ | Baixa | Neural USB classifier |
| VirtIO-GPU | Phase 5 | Baixa | PCI, framebuffer |
| **NPU PCI detection + fallback** | **Sprint 14** | 🔥 Alta | PCI enumeração |
| **NPU command queue + overlay** | **Phase 5** | Média | NPU detect, firmware blob |
| **NPU software fallback** | **Sprint 14** | 🔥 Alta | `Accelerator::Software` (default) |
| **Hardware Inventory (coleta)** | **Sprint 13** | 🔥 Alta | PCI enum, CPUID, MemoryMap |
| **Architecture MLP (512→256→64→9)** | **Sprint 15** | 🔥 Alta | PackedTernaryTensor, Intent Router |
| **Dynamic system config (ring pool, heap, sfs, trust)** | **Sprint 15** | 🔥 Alta | Architecture MLP |

## Consequências

**Positivo:**
- Kernel mantém-se enxuto (~5-8k LOC mesmo com SMP e PCI).
- Modelo "VirtIO first" permite testar tudo em QEMU antes de hardware real.
- **Inovação central:** A IA configura o sistema dinamicamente. Nenhum SO existente faz inventário → MLP → arquitetura em tempo de boot.
- USB com trust-once-use-always + classificação neural elimina 50k LOC de stack USB clássica.
- Nenhum dispositivo tem autoridade implícita — segurança por default.

**Negativo:**
- O Architecture MLP (512→256→64→9) precisa de uma base de treinamento com milhares de configurações de hardware. Sem isso, as decisões serão subótimas.
- Hardware muito exótico (ex: FPGA, CXL) pode não estar representado no treinamento → fallback para defaults seguros.
- USB confiável só funciona se o trust cache for persistente (SFS precisa estar funcional).

**Riscos:**
- SMP adiciona concorrência real ao kernel (deadlocks, data races). `spin::Mutex` atual é single-core; substituir por ticket lock requer revisão de todas as seções críticas.
- ACPI MADT parser pode ser frágil em firmware não conformante. Ter fallback para número fixo de cores (1 core) em caso de falha.
- MLP de arquitetura pode tomar decisão catastrófica (ex: alocar heap de 0 bytes). **Solução:** validar saída contra constraints mínimas antes de aplicar.

## Referências

- ADR-0003: IDT, Breakpoint, Double Fault (IST base para SMP stacks)
- ADR-0009: PIC Watchdog (a ser substituído por APIC timer)
- ADR-0010: Roadmap (Phase 4 como ponto de entrada para SMP)
- Intel x2APIC Spec (Vol 3A, Ch 10)
- ACPI 6.4 Spec: MADT Table (Section 5.2.12)
- VirtIO 1.2 Spec: https://docs.oasis-open.org/virtio/virtio/v1.2/
- Folkering OS: modelo de pinagem de cores (inspiração)
- AMD XDNA Architecture (IEEE Micro, Vol 44, Issue 6, 2024): https://doi.org/10.1109/MM.2024.3423692
- AMD Ryzen AI Software: https://www.amd.com/en/developer/resources/ryzen-ai-software.html
- Linux amdxdna driver: https://github.com/torvalds/linux/tree/master/drivers/accel/amdxdna
- Intel NPU 4 Architecture: https://cdrdv2-public.intel.com/824436/2024_Intel_Tech_Tour_TW_Lunar_Lake_AI_Hardware_Accelerators.pdf
- Qualcomm Hexagon NPU SDK: https://www.qualcomm.com/developer/software/qualcomm-ai-engine-direct
- BitNet b1.58 (ADR-0011, ADR-0012) — base para overlay ternário no NPU
