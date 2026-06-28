# Sessão 056 — Sprint 56-58: HARDWARE REAL + USB KEYBOARD + FAT12 + ATA

**Data:** 28/06/2026  
**Versão:** v0.58.0  
**Marco:** 🏆 **Primeiro boot do Neural OS Hermes em hardware real (notebook físico via SDHC USB)**

---

## O que entrou

### 🏆 MARCO: Boot em Hardware Real
Pela primeira vez, o kernel bootou em um notebook físico (x86-64) via cartão SDHC em leitor USB. A imagem de 2.7 MB (kernel 606KB + partição FAT12 de 2MB) foi gravada com Rufus 4.5 (Imagem DD, MBR, CSM habilitado).

**Resultado:** VGA text mode ligou, mensagens de boot apareceram, Hermes Cognitive começou a rodar. Zero panics após correção do OOM.

### xHCI USB HID Keyboard Driver (completo)
- `init_xhci()` — inicialização única do controlador xHCI no boot (DCBAA, Event Ring, slots)
- `poll_keyboard()` — lê Event Ring, parseia HID report de 8 bytes (modifiers + keycodes)
- `hid_to_scancode()` — tabela de 68 teclas (A-Z, 0-9, símbolos, ENTER, BACKSPACE, DELETE, etc.)
- CAD detection: LCtrl(bit0 do modifier) + LAlt(bit2) + usage 0x4C (Delete) → scancode DEL
- InputAgent::poll_usb_keyboard() → chama xhci::poll_keyboard() a cada 5 ticks
- Driver PERSISTENTE: XhciState global (não recria a cada poll)

### MBR + FAT12 Partition Recognition (PERMANENTE)
- `fat.rs::read_mbr()` — lê tabela de partições do setor 0 via ATA PIO, retorna Vec<Partition>
- `FatBpb::read()` — parseia BIOS Parameter Block de FAT12/16/32
- `Fat12Writer::new()` + `append_log()` — escreve no arquivo BOOT.LOG via ATA read/write
- Reconhecimento de partições é feature permanente do kernel

### FAT12 Boot Log Partition (temporário)
- `tools/patch_image.py` — script Python que concatena partição FAT12 de 2MB ao final da bootimage
- BOOT.LOG vazio (cluster 2, size 0) — preenchido em runtime pelo kernel
- Visível no Windows Explorer após boot

### ATA PIO Driver
- `AtaDriver::probe()` — scan PCI class 0x01 (IDE/SATA), detecta por status register
- `read_sectors()` — LBA28, wait_bsy + wait_drq, word por word via `in ax, dx`
- `write_sectors()` — mesmo protocolo com `out dx, ax`, cache flush via 0xE7
- Fallback silencioso sem ATA
- `read_io()` / `write_io()` — funções públicas para acesso raw

### OOM Fix em Hardware Real
- `HEAP_SIZE`: 4MB → 16MB (4096 páginas)
- `serial_println!` sem alloc: `SERIAL.lock().write_fmt(args)` direto, sem `alloc::format!`
- Panic handler: safe path sem alocação (`write!` para VGA/serial), tentative path com `try_alloc_check()`
- `#[alloc_error_handler]`: diagnostico OOM sem alocar (escreve direto no VGA+serial)
- `LogBuf`: implementação própria de `fmt::Write` em buffer stack de 256 bytes

### VGA Scrolling Fix
- `row` tracking no Writer (não mais escreve sempre na última linha)
- `new_line()` incrementa row até BUFFER_HEIGHT-1, depois scrolla
- Cursor visual correto

### Ctrl+Alt+Del com Log Dump
- Detecção: PS/2 (IRQ1, scancode 0x1D+0x38+0x53) + USB HID (LCtrl+LAlt+0x4C)
- Ação: `handle_cad()` → tenta ATA write do log na FAT12 → PS/2 8042 reset → hlt
- Boot log: 64KB circular, prefixo `[T+SSS.mmm]` sem alocação de heap

### Ecosystem Analysis (16 repos)
- Alta aderência: OpenMontage (pipeline), OpenHuman (Memory Tree), codebase-memory-mcp (KG)
- Média aderência: Rinne (DAG), daily_stock (Dashboard), ComPilot (closed-loop)
- Baixa aderência: design.md (tokens), Penpot (design), Voicebox (MCP)

### Consolidation Sprint
- Plugin Hub (#236): install/remove/scan_risk/discover
- x2APIC ativado: CPUID via `core::arch::x86_64::__cpuid()`
- Ed25519 real: `ed25519-compact` crate (2.3.1, no_std puro)
- SMP per-AP stacks: 64KB dedicado por AP
- VirtIO-GPU: `sti;hlt` no poll para VM exit no TCG
- WHPX: Windows Hypervisor Platform ativado

## Dificuldades e Aprendizados

### 1. OOM em Hardware Real (CRÍTICO)
**Problema:** O heap de 4MB que funciona perfeitamente no QEMU não é suficiente em hardware real. O bootloader aloca mais memória no HW real, deixando menos frames disponíveis. Além disso, `alloc::format!` dentro de `serial_println!` causava alocação de String para cada linha de log, fragmentando o heap rapidamente.

**Correção:** Heap 16MB + serial sem alloc. Lição: todo caminho de log/erro deve ser NO_ALLOC.

### 2. PS/2 vs USB Keyboard
**Problema:** Notebooks modernos não têm controlador PS/2 (8042). O IRQ1 nunca dispara. Teclado USB só funciona via xHCI.

**Correção:** Driver xHCI HID Boot Protocol completo (~200 LOC). Lição: assuma USB keyboard, PS/2 é fallback.

### 3. HW Real vs QEMU
**Diferenças observadas:**
- QEMU tolera MBR sem signature 55AA; HW real requer
- QEMU aloca menos memória que HW real (bootloader UEFI/BIOS differences)
- QEMU TCG sem atomicidade cross-core (WHPX necessário para SMP)
- VirtIO-GPU não existe em HW real (precisa de UEFI GOP framebuffer)

### 4. FAT12 sem alocador de blocos
**Problema:** Escrever no SDHC requer driver de bloco. ATA PIO funciona para leitores internos; USB mass storage precisaria de driver separado.

**Solução:** ATA PIO via PCI class 0x01. Funciona em HW com controladora SATA em modo legado/IDE.

### 5. VGA text mode vs UEFI GOP
**Problema:** Bootloader 0.9.34 não expõe framebuffer UEFI. VGA text mode (0xB8000) funciona apenas com CSM ativado.

**Pendência:** Upgrade para bootloader 0.11+ para suporte GOP.

## Arquivos Criados/Modificados

| Arquivo | Ação | Linhas |
|---|---|---|
| `xhci.rs` | Reescreito | 160 |
| `fat.rs` | NOVO | 110 |
| `ata.rs` | Reescreito | 110 |
| `serial.rs` | Modificado | +50 |
| `allocator.rs` | Modificado | +15 |
| `main.rs` | Modificado | +10 |
| `agents.rs` | Modificado | +20 |
| `vga_buffer.rs` | Reescreito | 150 |
| `patch_image.py` | NOVO | 150 |
| `plugin_hub.rs` | NOVO | 63 |
| `identity.rs` | Reescreito | 45 |
| `apic.rs` | Modificado | +20 |
| `smp/mod.rs` | Modificado | +15 |
| `virtio_gpu.rs` | Modificado | +10 |
| `display/fb.rs` | Modificado | +25 |
| `event-bus/*` | 7 módulos | +450 |
| `agent-core/*` | 3 módulos | +220 |
| `docs/root` | 3 docs | +100 |

**Total:** ~1600 LOC novos, 0 erros `cargo check --release`

## Próximos Passos
1. **Prompt interativo** `>` para chat com Hermes (~50 LOC)
2. **Completar call de funções de agentes** (Hermes executar skills via teclado)
3. **Testar teclado USB no notebook** (driver já implementado)
4. **Upgrade bootloader 0.11+** para framebuffer UEFI GOP
5. **Bootloader nativo** (substituir `tools/patch_image.py`)
