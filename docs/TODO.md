# 📋 TODO/Checklist — neural-os-core v0.66.0

**Data:** 2026-07-01  
**Propósito:** Lista mestra de todas as pendências técnicas do projeto, para qualquer AI DEV (humano ou IA) localizar e contribuir.  
**Total de itens:** 28 (6 🔴 bloqueantes, 8 🟠 alta, 6 🟡 média, 8 🟢 leve)

---

## Como usar este arquivo

Cada item segue o formato:

```
[B-#] Prioridade | Título
├── Goal:          O que queremos alcançar
├── Por que:       Por que é necessário
├── Bloqueia:      Quais itens dependem deste
├── Sub-itens:     Passos concretos (checkboxes)
├── Dificuldades:  O que torna este item difícil
├── Travas:        O que impede o item de começar
├── Arquivos:      Onde mexer
├── Fontes:        Onde aprender mais
└── Esforço:       Estimativa de LOC/tempo
```

---

## 🧩 Mapa de Dependências (DAG)

O gráfico abaixo mostra QUEM BLOQUEIA QUEM. Só implemente um item se TODOS os seus pais (↑) estiverem resolvidos.

```
LAN / DHCP ──┬──→ B-11 (WWW Infra) ──┬──→ B-12 (Browser Agent)
             │                        ├──→ B-13 (MCP TCP)
             │                        ├──→ B-17 (WWW restantes: Email, RSS, Download, WS)
             │                        └──→ B-27 (Plugin Hub)
             ├──→ B-18 (DHCP refactor)
             └──→ (silenciosamente bloqueia browser_agent.rs real)

GTT ───────────→ B-02 (Intel GEN shader)

B-11 (WWW Infra) ──→ B-12, B-13, B-17, B-27

GPU probe (existe) ──→ B-05 (integrar no boot)
                      └──→ B-02, B-03, B-04 (shaders)

HW real ────────→ B-03 (NVIDIA), B-04 (AMD), B-21 (teste HW)
                └──→ B-10 (e1000/r8169)
```

### Legenda
```
A ──→ B    = "A bloqueia B" (B não pode começar sem A)
(A)        = item já existe / está implementado
```

### Ordem Topológica (qual fazer primeiro)

```
Fase 0 (já existe):  GPU probe, RTL8139 TX, PCI scan, display
Fase 1 (Sprint 67):  S67.0+S67.1+B-05+B-28      ✅ (meta-skill, agency, GPU boot)
Fase 2 (GPU fix):    B-24, B-09, B-08, B-22, B-07, B-14, B-25  ✅
Fase 3 (Rede):       B-01 (RX fix) ──→ B-18 (DHCP fallback)
Fase 4 (WWW):        B-11 (WWW Infra) ──→ B-12 (Browser), B-13 (MCP), B-17, B-27
Fase 5 (GPU Intel):  B-02 (GEN shader) ←── B-07 ✅
Fase 6 (HW real):    B-03 (NVIDIA), B-04 (AMD), B-10 (e1000), B-21 (teste)
Fase 7 (WiFi):       B-30 (Intel WiFi / Atheros / Realtek wireless) ←── B-01
Backlog:              B-06, B-15, B-16, B-19, B-20
```

---

## 🔴 BLOQUEANTES (impedem features core)

---

### B-01: DHCP/DNS/HTTP — Rede funcional

**Prioridade:** 🔴 Crítica  
**Goal:** smoltcp DHCP obtém IP, DNS resolve nomes, HTTP faz GET/POST. Sem isso, todos os WWW Agents (Browser, Email, Search, RSS, Download, WebSocket) estão bloqueados.

**Por que:** A stack de rede existe (RTL8139 TX confirmado funcionando, smoltcp 0.13 integrado) mas DHCP falha. QEMU SLiRP com `-nic user` espera DHCP request do guest, mas smoltcp não completa o handshake. Sem IP, não há rota, não há DNS, não há HTTP.

**Bloqueia:** B-11 (WWW Infra), B-12 (Browser Agent), B-13 (MCP TCP), B-17 (Email/Search/RSS/Download/WS), B-18 (DHCP refactor), B-27 (Plugin Hub) — **6 itens bloqueados** (toda a cadeia WWW)

**Sub-itens:**
- [ ] Debug smoltcp DHCP: descobrir por que `dhcp_poll()` nunca retorna `Configured`
- [ ] Verificar se RTL8139 RX está recebendo pacotes DHCP offer do QEMU
- [ ] Testar com `-nic user,model=rtl8139` (atual) vs `-nic tap,model=rtl8139`
- [ ] Implementar fallback: static IP `10.0.2.15/24` com gateway `10.0.2.2`
- [ ] Se RX não funciona: debug RTL8139 RX path (CAPR, RBSTART, interrupção)
- [ ] Se RX funciona mas DHCP não: debug smoltcp socket state machine
- [ ] Testar `ping 10.0.2.2` via ICMP (smoltcp ICMP socket)
- [ ] Testar `ncat 10.0.2.2 80` (smoltcp TCP socket)

**Dificuldades:**
- RTL8139 RX é interrupção-driven (IRQ11), precisa de IOAPIC roteando corretamente
- smoltcp poll é non-blocking — precisa de timer correto (ms, não ticks)
- QEMU SLiRP não é um roteador real — pode ter bugs com DHCP
- Depuração de rede em bare-metal é cega (serial + prints lentos)

**Travas:**
- Nenhuma — HW necessário: QEMU com `-nic user,model=rtl8139 -serial stdio`

**Arquivos:** `crates/neural-kernel/src/rtl8139.rs`, `netstack.rs`, `net.rs`, `agents.rs`

**Fontes:** `docs/memory/NETWORK_DEBUG_HOME.md`, `docs/sprint-063-www.md`, smoltcp docs

**Esforço:** 🔴 3-7 dias (incerto — depende do diagnóstico)

---

### B-02: Intel GEN shader assembly para gpu_matmul

**Prioridade:** 🔴 Pesado  
**Goal:** `IntelRing::gpu_matmul()` compila shader GEN assembly, carrega nos EU (Execution Units), executa matmul, retorna resultado. Em vez de stub `None`, retorna `Some(Tensor)`.

**Por que:** A GPU sprint (66) implementou o ring buffer e a submissão de comandos, mas o kernel real do compute é o shader. Sem shader, é CPU fallback. O ganho esperado é 3-35× sobre CPU.

**Bloqueia:** Nenhum — é folha na DAG (só depende de B-07)

**Sub-itens:**
- [ ] Pesquisar formato de shader GEN (EU assembly, GRF registers, send messages)
- [ ] Implementar matmul como shader GEN: load A e B da VRAM, MAC, store em VRAM
- [ ] Compilar shader para GEN binary (ou usar assembler externo e embutir como &[u8])
- [ ] Carregar shader nos EU via MEDIA_OBJECT + pipe_control
- [ ] Submeter shader via ring buffer (MI_BATCH_BUFFER_START apontando para batch com shader)
- [ ] Ler resultado da VRAM de volta para RAM do sistema
- [ ] Benchmark: CPU matmul vs GPU matmul para tensor 128×128, 512×512, 1024×1024

**Dificuldades:**
- Documentação GEN assembly é NDA/própria da Intel — precisa de engenharia reversa
- i915 Linux driver é referência, mas GPL — não podemos copiar
- Formato de shader GEN varia entre gerações (Gen9 vs Gen12 vs Xe)
- Precisa de GTT (B-07) para batch buffers em RAM

**Travas:**
- B-07 (GTT setup) — sem GTT, GPU não enxerga a RAM do sistema
- Documentação insuficiente — pode precisar de experimentação em HW real

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`, `gpu/backend.rs`

**Fontes:** `docs/architecture/0029-gpu-architecture.md`, i915 driver (referência), OpenSource Intel GPU docs

**Esforço:** 🔴 ~800 LOC, 2-4 semanas

---

### B-03: NVIDIA PFIFO PUSH_BUFFER + FALCON firmware

**Prioridade:** 🔴 Pesado  
**Goal:** `NvidiaGpu::submit_kernel()` escreve PUSH_BUFFER no PFIFO ring, FALCON microcontrolador executa shader CUDA-style na VRAM.

**Por que:** NVIDIA é a GPU mais comum em desktops. Sem suporte NVIDIA, perdemos 70%+ dos hardwares. P8 mode (405MHz) é o mínimo — com firmware extraído, a GPU opera em P0 (full clock, >1.8GHz).

**Bloqueia:** Nenhum (folha na DAG — depende de HW real)

**Sub-itens:**
- [ ] Extrair firmware FALCON do driver NVIDIA (nv-kernel.o ou nvidia.ko)
- [ ] Implementar PFIFO ring buffer: PUSH_BUFFER, METHOD_COUNT, chanel ID
- [ ] Boot FALCON: carregar firmware no VRAM via BAR2, acordar FALCON via registers
- [ ] Submeter compute shader: método 0x0xxx (compute class) via PUSH_BUFFER
- [ ] Testar: VRAM → VRAM copy via PFIFO, benchmark

**Dificuldades:**
- Extração de firmware NVIDIA é legalmente complexa (reversing do driver)
- PFIFO register layout muda entre Pascal, Turing, Ampere, Ada, Blackwell
- FALCON é um microcontrolador proprietário (SPARC-like), ISA não documentada
- Sem firmware: P8 mode só (405MHz, ~500 GFLOPS) — não é suficiente para LLM 9B

**Travas:**
- HW real com NVIDIA GPU (QEMU não emula NVIDIA)
- Documentação NVIDIA é NDA total — reverse engineering do driver Linux
- Ferramentas: mmiotrace, nvgpu driver open source (referência parcial)

**Arquivos:** `crates/neural-kernel/src/gpu/nvidia.rs`, `gpu/backend.rs`

**Fontes:** `docs/architecture/0029-gpu-architecture.md`, nouveau driver (reverse), nvgpu (NVIDIA open kernel)

**Esforço:** 🔴 ~1500 LOC, 3-6 semanas

---

### B-04: AMD PM4 ring buffer real

**Prioridade:** 🔴 Pesado  
**Goal:** `AmdGpu::submit_pm4()` escreve pacotes PM4 reais no ring buffer AMD, GPU executa compute shader.

**Por que:** AMD RDNA é a 2ª GPU mais comum. Sem suporte AMD, perdemos mercado de GPU. AMD tem firmware sob licença MIT (mais fácil que NVIDIA).

**Sub-itens:**
- [ ] Implementar ring buffer AMD: PM4 packets `PKT3_WRITE_DATA`, `PKT3_DMA_DATA`
- [ ] Inicializar PSP (Platform Security Processor) para carregar firmware AMD
- [ ] Submeter AQL (Architected Queuing Language) packets para compute
- [ ] Testar: VRAM write/read via PM4, benchmark vs CPU

**Dificuldades:**
- PM4 packet formato varia entre RDNA1/2/3/4
- PSP init requer firmware binary incluso (licença MIT, ok)
- Documentação AMD é parcialmente aberta (GPUOpen) mas incompleta

**Travas:**
- HW real com AMD GPU (QEMU não emula AMD)
- Firmware AMD precisa ser extraído de linux-firmware e embutido no kernel

**Arquivos:** `crates/neural-kernel/src/gpu/amd.rs`, `gpu/backend.rs`

**Fontes:** `docs/architecture/0029-gpu-architecture.md`, AMD GPUOpen docs, amdgpu Linux driver

**Esforço:** 🔴 ~500 LOC, 2-4 semanas

---

### B-05: GPU não integrada no boot

**Prioridade:** 🔴 Crítica  
**Goal:** `kernel_main()` chama `gpu::detect::detect_all()` e `gpu::backend::init_backend()` durante o boot.

**Por que:** O módulo GPU compila mas nunca é executado. A linha `mod gpu;` foi adicionada no `main.rs`, mas a inicialização (`detect_all()`, `init_backend()`) não é chamada em lugar nenhum. GPU detection e ring buffer nunca rodam.

**Sub-itens:**
- [ ] Adicionar em `kernel_main()`, após PCI scan:
```rust
let gpus = unsafe { gpu::detect::detect_all() };
if !gpus.is_empty() {
    let compute = gpu::detect::best_compute_gpu(&gpus);
    if let Some(g) = &compute {
        gpu::vram::init_vram_tier(g);
    }
    gpu::backend::init_backend(&gpus);
}
```
- [ ] Printar status GPU no boot (quantas, quais, VRAM total)
- [ ] Testar: `serial_println!` confirma detecção no boot log

**Dificuldades:** Nenhuma — 1 linha de chamada + ~15 LOC de glue

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/main.rs`

**Fontes:** Próprio código — `gpu/detect.rs`, `gpu/backend.rs`

**Esforço:** 🟢 ~15 LOC, 30 minutos

---

### B-06: USB-MSC BOT — bulk endpoints não implementados

**Prioridade:** 🔴 Pesado  
**Goal:** `UsbMassStorage::read_sector()` e `write_sector()` funcionam — enviam CBW via bulk OUT, data phase, recebem CSW via bulk IN.

**Por que:** O driver USB-MSC foi escrito para detectar e enumerar dispositivos de massa, mas as funções de I/O (`send_scsi`, `bulk_write`, `bulk_read`) são stubs que não programam TRBs no transfer ring do xHCI. Sem isso, não há acesso a pendrives USB, SDHC cards USB, etc.

**Sub-itens:**
- [ ] Implementar `bulk_write()`: programar TRB Normal no transfer ring do bulk OUT endpoint
- [ ] Implementar `bulk_read()`: programar TRB Normal no transfer ring do bulk IN endpoint
- [ ] Implementar `send_scsi()`: CBW → bulk_write → data → bulk_read → CSW validation
- [ ] Testar: enumerar pendrive USB, ler setor 0, validar MBR signature 0xAA55

**Dificuldades:**
- xHCI transfer ring TRB programação é complexa (TRB tipos, ciclo bit, evento completion)
- Bulk endpoints precisam de eventos de conclusão — xHCI event ring + ERST
- USB 3.0 (xHCI) é diferente de USB 2.0 (EHCI) — código assume xHCI
- Debug é cega — sem USB analyzer, só serial prints

**Travas:**
- xHCI event ring pode não estar funcionando (interrupção não chega)
- HW real: pendrive USB 2.0/3.0 para testar

**Arquivos:** `crates/neural-kernel/src/usb_msc.rs`, `xhci.rs`

**Fontes:** xHCI spec 1.2 (cap 4 — TRBs, cap 6 — bulk), Linux usb-storage driver (referência)

**Esforço:** 🔴 ~300 LOC, 1-2 semanas

---

## 🟠 ALTA (features importantes incompletas)

---

### B-07: GTT setup — Intel GPU precisa de Graphics Translation Table

**Goal:** Configurar GTT (Graphics Translation Table) para que a GPU Intel enxergue os batch buffers alocados em RAM do sistema e a VRAM mapeada via BAR2.

**Por que:** Intel GPU não acessa RAM do sistema diretamente — ela usa GTT, uma tabela de páginas interna da GPU. Sem GTT, a GPU não consegue ler os batch buffers que escrevemos no ring buffer. Atualmente o `gpu_blit()` e `exec_batch()` escrevem comandos na RAM, mas a GPU não consegue executá-los em HW real (em QEMU sem GPU real, isso nunca é testado).

**Bloqueia:** B-02 (Intel GEN shader) — shader precisa de batch buffer na RAM visível pela GPU

**Sub-itens:**
- [ ] Localizar registers GTT na GPU Intel: `GFX_FLSH_CNTL` (0x101008), `PPGTT` registers
- [ ] Alocar página para GTT (Global GTT = 2MB, suporta 512 entradas de 4KB cada)
- [ ] Escrever entradas GTT: PA → GTT address, para cada página do ring buffer e batch buffers
- [ ] Configurar `GFX_MODE` para habilitar PPGTT (Per-Process Graphics Translation Table) ou Global GTT
- [ ] Testar: submeter batch buffer via ring → GPU executa → HEAD avança = GTT funcionando

**Dificuldades:**
- GTT register layout varia entre Gen9 (legacy) e Gen12+ (GT1/GT2)
- PPGTT (per-process) é mais complexa que Global GTT
- Documentação Intel é parcial (i915 Linux driver GPL)

**Travas:** Nenhuma — pode ser implementado e testado em QEMU (sem GPU real, só confirma que registers escrevem sem crash)

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`

**Fontes:** i915 driver (i915_gem_gtt.c), Intel PRM (Programmer Reference Manual) Gen9/Gen12

**Esforço:** 🔴 ~400 LOC, 1-2 semanas

---

### B-08: BCS blitter engine — separar blit do RCS ring

**Goal:** Usar BCS ring (Blitter Command Streamer, offset 0x22000) em vez de RCS (Render, offset 0x120000) para operações de blit (cópia GPU→GPU).

**Por que:** RCS é para render 3D e compute. Misturar blit com render pode causar contenção de pipeline. BCS é o engine dedicado para blit — mais rápido e não bloqueia o pipeline de render.

**Sub-itens:**
- [ ] Identificar BCS ring registers: `BCS_RING_BASE` (0x22000), `BCS_RING_HEAD` (0x22034), `BCS_RING_TAIL` (0x22038), `BCS_RING_CTL` (0x2203C)
- [ ] Implementar `BcsRing` struct similar a `IntelRing` mas com registers BCS
- [ ] Mover `gpu_blit()` para BCS ring
- [ ] Manter RCS ring para `gpu_matmul()`

**Dificuldades:** Register layout BCS é idêntico ao RCS, só muda a base offset — implementação é direta

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`

**Fontes:** Intel PRM (BCS engine), i915 driver

**Esforço:** 🟡 ~150 LOC, 3-5 dias

---

### B-09: VRAM free list — substituir bump allocator

**Goal:** `vram_free()` realmente libera memória VRAM para reuso, em vez de ser stub vazio.

**Por que:** O bump allocator atual (`vram_alloc()` com `next_offset`) aloca sequencialmente e nunca libera. Depois de N alocações, a VRAM acaba. Precisamos de uma free list (bitmap ou lista ligada) para reuso de blocos.

**Sub-itens:**
- [ ] Implementar `VramFreeList`: `BTreeMap<u64, u64>` mapeando `base → size` de blocos livres
- [ ] `vram_alloc(size)`: busca bloco livre com best-fit ou first-fit
- [ ] `vram_free(addr, size)`: insere bloco na free list, coalesce adjacentes
- [ ] Opcional: bitmap allocator para VRAM (256 bytes para 1GB VRAM com 4KB páginas)

**Dificuldades:** Best-fit vs first-fit tradeoff (fragmentação). Coalescing de blocos adjacentes.

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/gpu/vram.rs`

**Fontes:** Algoritmos clássicos de allocação de memória (buddy system, slab)

**Esforço:** 🟡 ~150 LOC, 2-4 dias

---

### B-10: e1000/r8169 — NIC real

**Goal:** e1000 (Intel Pro/1000) ou r8169 (Realtek) funcionando em HW real para acesso à rede em hardware físico.

**Por que:** RTL8139 é emulado por QEMU mas não existe em HW real moderno. Para boot em notebook/desktop físicos, precisamos de driver para NIC real.

**Sub-itens:**
- [ ] e1000: verificar se TX/RX fluem em HW real (já temos init + registers)
- [ ] r8169: implementar driver básico (PCI class 0x02, BAR0 MMIO)
- [ ] Testar em HW real com cabo Ethernet

**Dificuldades:** HW real pode ter variações de chipset. Sem HW para testar, é cego.

**Travas:** HW real com e1000 ou r8169

**Arquivos:** `crates/neural-kernel/src/e1000.rs` (já existe), `rtl8139.rs`

**Fontes:** Intel e1000 datasheet, Realtek r8169 datasheet

**Esforço:** 🟡 ~300 LOC, 1-2 semanas

---

### B-11: Network Infrastructure (WWW 63.1)

**Goal:** ConnectionPool + HttpClient + URL parser — base para todos os WWW Agents.

**Por que:** Sem essa camada, BrowserAgent (B-12) e os outros 5 WWW Agents não podem ser construídos. Conexão TCP, DNS, HTTP GET/POST são blocos fundamentais.

**Bloqueia:** B-12 (Browser Agent), B-13 (MCP TCP), B-17 (Email/Search/RSS/Download/WS), B-27 (Plugin Hub) — **4 itens**

**Sub-itens:**
- [ ] `ConnectionPool`: gerenciar até 16 sockets TCP concorrentes
- [ ] `HttpClient`: GET/POST com headers, parsing de resposta
- [ ] `Url`: parser de scheme, host, port, path, query
- [ ] Testar: `HttpGet("http://example.com")` retorna HTML
- [ ] Depende de B-01 (rede funcional) — sem DHCP, não testa

**Dificuldades:** smoltcp API sutil — sockets precisam de poll frequente, timers em ms

**Travas:** **B-01 (rede funcional)** — sem rede, não testa

**Arquivos:** `crates/neural-kernel/src/net/connection_pool.rs`, `http_client.rs`, `url.rs` (novos)

**Fontes:** `docs/sprint-063-www.md`

**Esforço:** 🔴 ~400 LOC, 1-2 semanas

---

### B-12: Browser Agent (WWW 63.2)

**Goal:** `fetch_page(url)` baixa página web, extrai texto, exibe no Hermes ou PageViewerApp.

**Por que:** BrowserAgent é o WWW Agent mais importante — permite ao Hermes ler a web, buscar informação, responder perguntas com conteúdo atual.

**Sub-itens:**
- [ ] HTML parser mínimo: tags, texto, atributos, links, headings
- [ ] Extrator de texto: markdown-like output
- [ ] PageViewerApp no Compositor (janela com scroll)
- [ ] Comando `/browse <url>` no shell
- [ ] Depende de B-11 (HttpClient)

**Dificuldades:** HTML real é malformado — parser precisa ser robusto

**Travas:** B-01 (rede) → B-11 (HttpClient)

**Arquivos:** `crates/neural-kernel/src/browser_agent.rs` (já existe stub), `net/html_parser.rs` (novo)

**Fontes:** `docs/sprint-063-www.md`

**Esforço:** 🔴 ~500 LOC, 1-2 semanas

---

### B-13: MCP Agent — TCP listener

**Goal:** `McpAgent::tick()` aceita conexão TCP, processa requisição MCP, responde. Atualmente tem um `// TODO: TCP listener`.

**Por que:** MCP (Model Context Protocol) é como o Hermes expõe skills para o mundo exterior. Sem listener TCP, MCP não serve para nada — só processa requisições internas.

**Sub-itens:**
- [ ] Na tick do McpAgent: `smoltcp.listen(port)` → accept → read request → process → write response
- [ ] Formato: JSON-RPC sobre TCP (padrão MCP)
- [ ] Comando `/mcp listen 8080` para iniciar servidor

**Dificuldades:** smoltcp listener API, multi-conexão

**Travas:** B-01 (rede)

**Arquivos:** `crates/neural-kernel/src/mcp.rs`

**Fontes:** Model Context Protocol spec

**Esforço:** 🟡 ~200 LOC, 3-5 dias

---

### B-14: WASM sandbox — interpretar bytecode

**Goal:** `WasmSandbox::execute()` realmente interpreta um módulo WASM, não apenas stub.

**Por que:** WASM é o mecanismo de skills do futuro — skills compiladas para WASM rodam no sandbox com memória isolada. Sem WASM, skills são código Rust arbitrário (inseguro).

**Sub-itens:**
- [ ] Integrar crate `wasmi` (ou `wasmtime`) para interpretar bytecode WASM
- [ ] Criar host functions: `nn:silu`, `tensor:matmul`, `vfs:read` como imports
- [ ] Sandbox de memória: linear memory pre-alocada (256KB)
- [ ] Comando `/run skill.wasm` para carregar e executar

**Dificuldades:** `wasmi` é no_std? Pode precisar de patches. Cadeia de ferramentas WASM (assemblar wat → wasm).

**Travas:** Nenhuma — pode integrar `wasmi` como dependência

**Arquivos:** `crates/neural-kernel/src/wasm.rs`

**Fontes:** wasmi crate docs, WASM spec

**Esforço:** 🔴 ~500 LOC, 1-3 semanas

---

## 🟡 MÉDIA (completar funcionalidades existentes)

---

### B-15: GGUF model swap — heap >5GB

**Goal:** Heap do kernel >5GB para carregar modelos GGUF 9B+.

**Por que:** Qwythos 9B em GGUF Q4_K_M ocupa ~5GB. O heap atual é 16MB (allocator.rs). A diferença é 2 ordens de magnitude (16MB vs 5GB). Precisamos de Huge Pages (1GB) e mapeamento de toda a RAM disponível.

**Sub-itens:**
- [ ] Configurar Huge Pages (1GB) no kernel para mapear >4GB de RAM
- [ ] Aumentar `HEAP_SIZE` em `allocator.rs` para >5GB
- [ ] Verificar bootloader mapping de memória física
- [ ] Testar: `alloc::vec![0u8; 5 * 1024 * 1024 * 1024]` (5GB allocation)

**Dificuldades:** 
- Huge Pages 1GB precisam de suporte do bootloader (bootloader 0.11+ mapeia?)
- Page table level 4 (PML4) suporta até 512GB — suficiente
- Alocação de 5GB contiguous pode falhar se memória estiver fragmentada

**Travas:** Bootloader mapping (depende de `physical_memory` config)

**Arquivos:** `crates/neural-kernel/src/allocator.rs`, `memory.rs`

**Fontes:** x86_64 paging (1GB pages), bootloader-api docs

**Esforço:** 🟡 ~200 LOC, 1 semana

---

### B-16: Mempalace MCP — cache corrompido

**Goal:** Mempalace MCP server conecta e mantém estado entre sessões.

**Por que:** Mempalace é a memória de longo prazo do Hermes. Sem MCP, as memórias das sessões anteriores são perdidas após reboot.

**Sub-itens:**
- [ ] Limpar cache MCP em `%LOCALAPPDATA%\opencode`
- [ ] Verificar path do MCP server em `opencode.json`
- [ ] Debug: MCP handshake, verificar se servidor está rodando

**Dificuldades:** MCP é externo ao projeto (servidor Node.js) — debug depende do ambiente

**Travas:** Ambiente de desenvolvimento (WSL, Node.js, servidor MCP)

**Fontes:** `~/.config/opencode/opencode.json`

**Esforço:** 🟢 ~1 hora

---

### B-17: WWW Agents restantes (63.3-63.7)

**Goal:** Email Agent (SMTP/POP3/IMAP), Search Agent (DuckDuckGo), RSS/Feed Agent, Download Agent, WebSocket Agent.

**Por que:** 5 dos 7 WWW Agents do Sprint 63 não foram iniciados. Cada um adiciona uma capacidade de internet ao Hermes.

**Sub-itens:**
- [ ] 63.3 — Email Agent: SMTP send + POP3/IMAP read
- [ ] 63.4 — Search Agent: DuckDuckGo Lite HTML parse
- [ ] 63.5 — RSS/Feed Agent: RSS 2.0 + Atom parser
- [ ] 63.6 — Download Agent: HTTP download com Range
- [ ] 63.7 — WebSocket Agent: handshake + frame parser

**Dificuldades:** Cada agente requer protocolo de rede diferente, parsing especializado

**Travas:** B-01 (rede) → B-11 (HttpClient)

**Fontes:** `docs/sprint-063-www.md`

**Esforço:** 🔴 ~1700 LOC total, 4-8 semanas

---

### B-18: DHCP/ARP refactor

**Goal:** Refatorar DHCP e ARP para serem mais robustos, com fallback a IP estático.

**Por que:** DHCP atual nunca completa — precisamos de fallback a IP estático `10.0.2.15/24`.

**Sub-itens:**
- [ ] Implementar static IP config no boot
- [ ] DHCP com timeout: se não configurar em 5s, fallback para static
- [ ] ARP: debug se resolução funciona

**Dificuldades:** smoltcp API para IP estático

**Travas:** B-01 (rede)

**Arquivos:** `crates/neural-kernel/src/net.rs`, `netstack.rs`

**Fontes:** `IDEA_BANK #250`

**Esforço:** 🟡 ~100 LOC, 2-3 dias

---

### B-19: VirtIO-GPU GET_DISPLAY_INFO

**Goal:** VirtIO-GPU `GET_DISPLAY_INFO` retorna resolução correta do monitor.

**Por que:** QEMU TCG pode ter bug com VirtIO-GPU display info. Isso afeta quem usa VirtIO-GPU em vez de framebuffer UEFI.

**Sub-itens:**
- [ ] Debug: enviar `GET_DISPLAY_INFO` control message no VirtIO control queue
- [ ] Verificar resposta: resolução, pitch, formato
- [ ] Se TCG bug: reportar upstream ou contornar

**Dificuldades:** VirtIO control queue implementação pode ter race condition

**Travas:** QEMU TCG específico

**Arquivos:** `crates/neural-kernel/src/virtio_gpu.rs`

**Esforço:** 🟢 ~50 LOC, 1-2 dias

---

### B-20: SMP sem WHPX — TCG atomicidade

**Goal:** `-smp 2` funciona sem WHPX (TCG mode) para debugging em máquinas sem virtualização.

**Por que:** WHPX (Windows Hypervisor Platform) é específico do Windows. Em Linux ou macOS sem KVM, TCG é a única opção. SMP com TCG é instável.

**Sub-itens:**
- [ ] Investigar se o problema é lock-free atomics vs TCG
- [ ] Se TCG não suporta `SeqCst` corretamente, substituir por locks
- [ ] Testar `-accel tcg -smp 2` de forma estável

**Dificuldades:** TCG não garante atomicidade de instruções como HW real

**Travas:** Nenhuma

**Esforço:** 🟡 ~100 LOC, 1 semana

---

## 🟢 LEVE (melhorias, stubs, limpeza)

---

### B-21: Testar GPU em hardware real

**Goal:** Verificar se GPU detection + ring buffer + VRAM mapping funcionam em hardware real (não QEMU).

**Por que:** QEMU não emula Intel Gen9+, NVIDIA, AMD. Só hardware real valida o módulo GPU. 

**Sub-itens:**
- [ ] Boot em notebook com Intel iGPU (HD 620, Iris Xe, UHD)
- [ ] Boot em desktop com NVIDIA (RTX 3060+, P8 mode)
- [ ] Boot em desktop com AMD (RX 6000+)
- [ ] Verificar `serial_println` GPU log

**Dificuldades:** Risco de crash/page fault se registers GPU não responderem como esperado

**Travas:** Hardware real disponível

**Esforço:** 🟢 Teste, ~1 dia

---

### B-22: VRAM window full — mapear GPU inteira

**Goal:** Mapear toda a VRAM da GPU (8GB+), não apenas 1MB (256 páginas).

**Por que:** Atualmente mapeamos 256 páginas (1MB) da BAR2. GPU com 8GB VRAM tem 2M páginas — mapear 1 por 1 é proibitivo. Precisamos de mapeamento em bloco.

**Sub-itens:**
- [ ] Usar Huge Pages (2MB ou 1GB) no page table para mapear BAR2
- [ ] Ou implementar janela sliding: mapear 256MB por vez, trocar on demand
- [ ] Verificar: `map_page_uc()` com 2MB pages

**Dificuldades:** Page table manipulation para mapear grandes regiões de MMIO

**Travas:** Bootloader physical memory mapping

**Arquivos:** `crates/neural-kernel/src/apic.rs` (map_page_uc), `gpu/nvidia.rs`, `gpu/amd.rs`

**Esforço:** 🟡 ~100 LOC, 3-5 dias

---

### B-23: ATA IDENTIFY — QEMU sem IDE

**Goal:** ATA `total_sectors()` funciona em QEMU sem legacy IDE emulação.

**Por que:** QEMU moderno não emula controller IDE legacy por padrão. `ata.rs` usa `in al, dx` que só funciona se QEMU tiver `-device ide-hd`.

**Sub-itens:**
- [ ] Adicionar QEMU argumento `-device ide-hd,drive=hd` ou `-drive if=ide`
- [ ] Ou implementar AHCI (SATA) para compatibilidade com HW real

**Dificuldades:** ATA PIO vs AHCI são protocolos diferentes

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/ata.rs`

**Esforço:** 🟢 ~20 LOC (config QEMU)

---

### B-24: 514 warnings — cleanup

**Goal:** `cargo check --release` com 0 warnings.

**Por que:** 514 warnings poluem output e podem esconder warnings reais de novos bugs.

**Sub-itens:**
- [ ] `cargo fix` para aplicar sugestões automáticas
- [ ] Revisar unused imports, dead code, unnecessary unsafe blocks
- [ ] Adicionar `#[allow(dead_code)]` para stubs intencionais

**Dificuldades:** Alguns dead code são stubs propositais — precisam de `#[allow]` em vez de remoção

**Travas:** Nenhuma

**Arquivos:** Todo o projeto

**Esforço:** 🟡 ~30 minutos com `cargo fix`

---

### B-25: FAT32 suporte

**Goal:** `fat.rs` lê e escreve partições FAT32 (não apenas FAT12).

**Por que:** SDHC cards >2GB usam FAT32, não FAT12. O leitor atual só suporta FAT12.

**Sub-itens:**
- [ ] Implementar BPB FAT32 parsing (BPB diferente de FAT12/16)
- [ ] FSInfo sector, cluster chain (FAT32 usa 28-bit clusters)
- [ ] Leitura/escrita de arquivos em FAT32

**Dificuldades:** FAT32 cluster chain é mais complexa que FAT12 (28-bit, não 12-bit)

**Travas:** SDHC card FAT32 para testar

**Arquivos:** `crates/neural-kernel/src/fat.rs`

**Esforço:** 🟡 ~300 LOC, 1 semana

---

### B-26: Prompt interativo `>`

**Goal:** Hermes exibe prompt `>` e aguarda input do usuário via teclado.

**Por que:** Atualmente o Hermes inicializa e imprime mensagens, mas não há cursor piscando ou prompt esperando input. O usuário não sabe que pode digitar.

**Sub-itens:**
- [ ] Adicionar `>` no final do output do DisplayAgent
- [ ] Cursor piscando (alternar a cada 500ms)
- [ ] Input echo: teclas digitadas aparecem no prompt

**Dificuldades:** Nenhuma — mudança cosmética

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/display/agent.rs`

**Esforço:** 🟢 ~30 LOC

---

### B-27: Plugin Hub MCP Index

**Goal:** Plugin Hub indexa skills MCP disponíveis, permitindo `skill install <name>`.

**Por que:** Plugin Hub é o mecanismo de descoberta de skills. Sem ele, o usuário precisa escrever skills manualmente.

**Sub-itens:**
- [ ] Index de skills MCP em registry local
- [ ] Comando `/skill search`, `/skill install`, `/skill list`
- [ ] Download de skill de repositório remoto (futuro)

**Dificuldades:** Index remoto requer rede (B-01)

**Travas:** B-01 (rede)

**Arquivos:** `crates/neural-kernel/src/plugin_hub.rs` (já existe stub)

**Esforço:** 🟡 ~400 LOC, 1 semana

---

### B-29: WiFi — Intel Wireless / Atheros / Realtek

**Goal:** Conectar a redes WiFi 802.11, WPA2/WPA3, scan de redes, DHCP sobre WiFi.

**Por que:** Sem WiFi, o Hermes só funciona com cabo Ethernet. Para ser um SO mobile/desktop completo, WiFi é essencial.

**Sub-itens:**
- [ ] Pesquisar chipsets WiFi suportados em bare-metal (Intel, Atheros, Realtek)
- [ ] Implementar PCI detection de wireless cards
- [ ] 802.11 scan + association (management frames)
- [ ] WPA2/WPA3 handshake (PSK, EAP)
- [ ] Bridge entre WiFi e smoltcp (NetPhy WiFi)

**Dificuldades:**
- Firmware loading (Intel iwlwifi, Atheros ath9k)
- 802.11 frame format é diferente de Ethernet
- WPA2 cryptography (CCMP/AES) requer crypto em no_std
- Firmware licensing pode ser problemática (Intel é NDA)

**Travas:** B-01 (rede funcional) — sem IP stack testada, WiFi não tem onde se apoiar

**Arquivos:** `crates/neural-kernel/src/wifi/` (novo módulo)

**Esforço:** 🔴 ~2000 LOC, 4-8 semanas

---

### B-28: Auto-skill generation — integrado ao ciclo

**Goal:** `maybe_auto_skill()` é chamado automaticamente quando um padrão de tarefa repete 3+ vezes.

**Por que:** `skill_gen.rs` implementa o sistema de auto-skill (TaskPattern registry + detecção de repetição), mas nunca é chamado no ciclo principal.

**Sub-itens:**
- [ ] Chamar `maybe_auto_skill(name)` no OptimizerAgent ou CronAgent a cada N ticks
- [ ] Quando detecta repetição: gerar skill, registrar no SkillRegistry, notificar Hermes

**Dificuldades:** Nenhuma — glue code simples

**Travas:** Nenhuma

**Arquivos:** `crates/neural-kernel/src/skill_gen.rs`, `optimizer.rs` ou `cron.rs`

**Esforço:** 🟢 ~30 LOC

---

## 📊 RESUMO GERAL

| Prioridade | Qtd | Esforço total estimado |
|---|---|---|
| 🔴 Bloqueante | 6 | ~3.100 LOC, 8-18 semanas |
| 🟠 Alta | 8 | ~3.550 LOC, 8-20 semanas |
| 🟡 Média | 6 | ~800 LOC, 4-8 semanas |
| 🟢 Leve | 8 | ~700 LOC, 1-3 semanas |
| **Total** | **28** | **~8.150 LOC, 4-12 meses** |

### Ordem sugerida de implementação

```
Fase 1 (Sprint 67):  B-05 (GPU boot), B-26 (prompt >), B-28 (auto-skill)
Fase 2 (Sprint 68):  B-01 (rede), B-18 (DHCP fallback)
Fase 3 (Sprint 69):  B-11 (WWW infra), B-12 (Browser Agent)
Fase 4 (Sprint 70):  B-07 (GTT), B-09 (VRAM free list), B-08 (BCS)
Fase 5 (Sprint 71+):  B-02/B-03/B-04 (GPU compute), B-06 (USB-MSC)
Fase 6 (Sprint 72+):  B-14 (WASM), B-17 (WWW restantes)
Backlog:              B-15 a B-27
```

---

## 🔗 Como encontrar ajuda

- **GPU:** `docs/architecture/0029-gpu-architecture.md`, `docs/sprint-066-gpu.md`
- **Rede:** `docs/memory/NETWORK_DEBUG_HOME.md`, `docs/sprint-063-www.md`
- **WASM:** `docs/architecture/0010-strategic-roadmap-and-innovations.md` (Phase 5)
- **Memória:** `docs/memory/SESSION_*.md`, `IDEA_BANK.md`
- **Plano diretor:** `docs/memory/STATE.md`
- **Última sessão:** `docs/memory/SESSION_066.md`

---

*Este arquivo é o ponto de partida para qualquer AI DEV que queira contribuir. Leia este TODO, escolha um item, leia as fontes listadas, e implemente.*
