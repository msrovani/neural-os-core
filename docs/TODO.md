# 📋 TODO/Checklist — neural-os-core v0.79.0

**Data:** 2026-07-04  
**Propósito:** Lista mestra de todas as pendências técnicas do projeto, para qualquer AI DEV (humano ou IA) localizar e contribuir.  
**Total de itens:** 27 (7 🔴 bloqueantes, 8 🟠 alta, 7 🟡 média, 5 🟢 leve)

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
Fase 1 (Sprint 67-70): S67.0+S67.1+B-05+B-28+B-24 ✅ (meta-skill, agency, GPU boot, bugs)
Fase 2 (Sprint 71):   B-31, B-32, B-33, B-34        ✅ (boot bug hunt)
Fase 3 (Rede):        B-01 (RX fix) ──→ B-18 (DHCP fallback)
Fase 4 (WWW):         B-11 (WWW Infra) ──→ B-12 (Browser), B-13 (MCP), B-17, B-27
Fase 5 (GPU Intel):   B-02 (GEN shader) ←── B-07 ✅
Fase 6 (HW real):     B-03 (NVIDIA), B-04 (AMD), B-10 (e1000), B-21 (teste)
Fase 7 (WiFi):        B-30 (Intel WiFi / Atheros / Realtek wireless) ←── B-01
**Fase 8 (FS):        B-25 (FAT32) + B-34 (log) ✅ — FAT12 removido**
**Fase 9 — Sprint 77 (Foundation Quick Wins):  Prompt >, Pre-Flight, TaskSchema, FanOut, /learn, SkillIndex, Contracts ✅**
**Fase 10 — Sprint 78 (Agentic Evolution):     Crew/Flow, Cache, Workflow, GGUF, WASM ✅**
**Fase 11 — Sprint 79 (LLM Infrastructure):     AVX2 BitNet, Trinity MoE, Candle, TrainingAgent**
**Fase 12 — Sprint 80 (JARVIS Persona):         SOUL.md, IPW, Session Compression, Notification Gate**
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

**Status:** ✅ Implementado em Bloco 21a

**Goal:** `IntelRing::gpu_matmul()` compila shader GEN assembly, carrega nos EU (Execution Units), executa matmul, retorna resultado.

**Implementação:** Intel GEN shader assembly implementado para matmul. GPU compute funcional.

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`

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

**Status:** ✅ Implementado em Bloco 21a

**Goal:** `kernel_main()` chama `gpu::detect::detect_all()` e `gpu::backend::init_backend()` durante o boot.

**Implementação:** GPU detection integrada no boot após PCI scan. GPU probe funcional.

**Arquivos:** `crates/neural-kernel/src/main.rs`

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

**Status:** ✅ Implementado em Bloco 21a

**Goal:** Configurar GTT (Graphics Translation Table) para que a GPU Intel enxergue os batch buffers alocados em RAM do sistema e a VRAM mapeada via BAR2.

**Implementação:** GTT setup implementado para Intel GPU. Batch buffers visíveis pela GPU.

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`

---

### B-08: BCS blitter engine — separar blit do RCS ring

**Status:** ✅ Implementado em Bloco 21a

**Goal:** Usar BCS ring (Blitter Command Streamer, offset 0x22000) em vez de RCS (Render, offset 0x120000) para operações de blit (cópia GPU→GPU).

**Implementação:** BCS ring separado do RCS ring implementado. Blit operations no BCS, compute no RCS.

**Arquivos:** `crates/neural-kernel/src/gpu/intel.rs`

---

### B-09: VRAM free list — substituir bump allocator

**Status:** ✅ Implementado em Bloco 21a

**Goal:** `vram_free()` realmente libera memória VRAM para reuso, em vez de ser stub vazio.

**Implementação:** VRAM free list (buddy allocator) implementado. Substitui bump allocator.

**Arquivos:** `crates/neural-kernel/src/gpu/vram.rs`

---

### B-10: e1000/r8169 — NIC real

**Status:** ✅ Implementado em Bloco 21a

**Goal:** e1000 (Intel Pro/1000) ou r8169 (Realtek) funcionando em HW real para acesso à rede em hardware físico.

**Implementação:** Driver e1000/r8169 implementado para NIC real. TX/RX funcional.

**Arquivos:** `crates/neural-kernel/src/e1000.rs`

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

**Status:** ✅ Implementado em Bloco 21e

**Goal:** `WasmSandbox::execute()` realmente interpreta um módulo WASM, não apenas stub.

**Implementação:** WASM runtime (wasmi) integrado. Skills compiladas para WASM rodam no sandbox com memória isolada.

**Arquivos:** `crates/neural-kernel/src/wasm.rs`

---

## 🟡 MÉDIA (completar funcionalidades existentes)

---

### B-15: GGUF model swap — heap >5GB

**Status:** ✅ Implementado em Bloco 21e

**Goal:** Heap do kernel >5GB para carregar modelos GGUF 9B+.

**Implementação:** Adaptive heap implementado. Heap redimensionável via frame allocator. GGUF loader funcional.

**Arquivos:** `crates/neural-kernel/src/allocator.rs`, `memory.rs`, `gguf.rs`

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

**Status:** ✅ Implementado em Sprint 77

**Goal:** Hermes exibe prompt `>` e aguarda input do usuário via teclado.

**Implementação:** `display/console.rs` — `show_prompt` default alterado para `true`. NeuralConsole renderiza `> {input}` na última linha. Input echo via `KEYBOARD_ECHO` topic.

**Arquivos:** `crates/neural-kernel/src/display/console.rs`

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

### B-31: VGA CRTC + UEFI GOP — verificar xuvisco fix em HW Intel 6xx

**Goal:** Confirmar que o VGA buffer clear (v0.79.1) resolve xuvisco em notebook Intel 6xx.

**Por que:** Sprint 71 moveu `probe_uefi_framebuffer()` antes de `vga_buffer::init()` mas nunca limpava 0xB8000. v0.79.1 adiciona `clear_physical_buffer()` que zera VGA text buffer sem tocar CRTC. Resta testar HW real.

**Sub-itens:**
- [ ] Boot em notebook Intel 6xx com imagem v0.79.1
- [ ] Verificar se display não fica garbled

**Arquivos:** `crates/neural-kernel/src/vga_buffer.rs:14-19`, `crates/neural-kernel/src/display/fb.rs:71-75`

**Status:** ✅ Código corrigido (v0.79.1). Teste HW pendente.

---

### B-32: DiagnosticSkill — testes extensivos

**Goal:** DiagnosticSkill executa testes de estresse de alocador, tensor matmul, BitNet MLP e publica resultados.

**Por que:** Substitui os testes inline que estavam no boot. SystemAgent executa na fase Diagnostics.

**Sub-itens:**
- [ ] Verificar se DiagnosticSkill roda corretamente durante boot
- [ ] Verificar se resultados são publicados no EventBus
- [ ] Adicionar mais testes (estresse de heap, page table walk)

**Arquivos:** `crates/neural-kernel/src/agents.rs` (DiagnosticSkill)

**Esforço:** 🟢 ~30 LOC

---

### B-33: Boot phase events — Hermes mostrar progresso

**Goal:** HermesAgent mostra o progresso do boot no display à medida que cada fase `BOOT_PHASE` é publicada.

**Por que:** O usuário vê o sistema acordando fase por fase.

**Sub-itens:**
- [ ] HermesAgent subscrever `TOPIC_BOOT_PHASE`
- [ ] Mostrar fase atual no canto do display
- [ ] BootLogAgent logar cada fase

**Arquivos:** `crates/neural-kernel/src/hermes.rs`, `boot_log_agent.rs`

**Esforço:** 🟢 ~50 LOC

---

### B-34: FAT12 log — validar leitura/escrita

**Goal:** Garantir que `boot_logger::log()` escreve no FAT12 e `BootLogAgent::read_last_boot_log()` consegue ler.

**Por que:** O boot log é o mecanismo de auto-diagnóstico do Cortex.

**Sub-itens:**
- [ ] Boot com patched image → verificar se BOOT.LOG é criado
- [ ] BootLogAgent ler e publicar análise
- [ ] Cortex usar log para auto-correção

**Arquivos:** `crates/neural-kernel/src/boot_logger.rs`, `fat.rs`, `boot_log_agent.rs`

**Esforço:** 🟡 ~100 LOC

---

## 📊 RESUMO GERAL

| Prioridade | Qtd | Esforço total estimado |
|---|---|---|
| 🔴 Bloqueante | 3 | ~2.500 LOC, 6-14 semanas |
| 🟠 Alta | 4 | ~2.100 LOC, 6-14 semanas |
| 🟡 Média | 4 | ~800 LOC, 2-6 semanas |
| 🟢 Leve | 8 | ~670 LOC, 1-3 semanas |
| **Total** | **19** | **~6.070 LOC, 4-10 meses** |

### Itens Completados (Bloco 21a/21b/21e)
- ✅ B-02: Intel GEN shader assembly
- ✅ B-05: GPU no boot
- ✅ B-07: GTT setup
- ✅ B-08: BCS blitter engine
- ✅ B-09: VRAM free list
- ✅ B-10: e1000/r8169 NIC real
- ✅ B-14: WASM sandbox
- ✅ B-15: GGUF model swap

### Ordem sugerida de implementação

```
Bloco 21a (SMP Foundation):            ✅ SPSC ring, IPI, PerCpu
Bloco 21b (Work-Stealing + Matmul):    ✅ Chase-Lev, parallel-for, AgentScheduler multicore
Bloco 21e (Polimento):                  ✅ burn-flex, CFS, GPU+Display co-existência
Bloco 21c (GPU Foundations):           🟡 GPU BAR mapping, ACR secure boot, job ring, VRAM allocator
Bloco 21d (GPU Decode):                🟡 Agent.xpu split, GPU matmul, KV cache DMA, XQueue
Bloco 30 (JARVIS Persona):             🟡 SOUL.md, IPW, Session Compression, Notification Gate
Bloco 31 (JARVIS Security + AHCI):     🟡 Fail-Closed, Merkle, Fluid Persona, SATA
Bloco 32+ (AIOS Evolution):            🔴 B-01 RX fix, WWW Agents, Voice, SKYNET, WiFi
```

---

## 🔗 Como encontrar ajuda

- **GPU:** `docs/architecture/0029-gpu-architecture.md`, `docs/sprint-066-gpu.md`
- **Rede:** `docs/memory/NETWORK_DEBUG_HOME.md`, `docs/sprint-063-www.md`
- **WASM:** `docs/architecture/0010-strategic-roadmap-and-innovations.md` (Phase 5)
- **Memória:** `docs/memory/SESSION_*.md`, `IDEA_BANK.md`
- **Plano diretor:** `docs/memory/STATE.md`
- **Última sessão:** `docs/sprint-078-agentic-evolution.md`

---

*Este arquivo é o ponto de partida para qualquer AI DEV que queira contribuir. Leia este TODO, escolha um item, leia as fontes listadas, e implemente.*
