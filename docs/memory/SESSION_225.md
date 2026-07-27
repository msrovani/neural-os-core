# SESSION_225 — Limine Migration + Higher-Half Fixes + Desktop Jarbas na Tela + Soft Power Off (2026-07-27)

## Resumo
Migração do bootloader 0.11 para Limine (higher-half kernel). Correção de múltiplos #PF causados por acesso a endereços físicos sem HHDM offset. Sistema boota completo com desktop Jarbas, rede e soft power off.

## Fixes Aplicados

### 1. Limine Boot + Higher-Half
- Migração: bootloader 0.11 → Limine 6.x (uefi.img)
- Kernel rodando em higher-half (0xffffffff80000000+)
- Framebuffer via Limine HHDM (@0xffff8000c0000000)
- `PHYS_MEM_OFFSET` store no início de `kernel_boot()` (main.rs:1268) ANTES de qualquer driver

### 2. P6 raw_vec capacity overflow
- **Causa**: Subtração entre VA higher-half (0xffff...) e user-space (0x7000...) → delta > isize::MAX
- **Fix**: `TRY_ENTER_RING3 = false` + guarda no topo de `demo_ring3()`

### 3. e1000 RX #PF loop (T+19)
- **Causa**: `PHYS_MEM_OFFSET` atomic = 0 quando o NetAgent fazia o primeiro poll — drivers iniciavam antes do init_memory() setar o offset
- **Fix principal**: `PHYS_MEM_OFFSET.store(pm_offset)` logo após obter `handoff.phys_mem_offset()` (main.rs:1268)
- **Ponytail guard**: `if pmoff == 0 { return None/false; }` em `recv()` e `any_rx_dd()` — previne #PF caso buffer overflow corrompa o static

### 4. BPE vocabulary scan #PF (T+696)
- **Causa**: `cortex/src/bpe.rs:485` — scan de 0x129200000 até 0x200000000 (8GB), mas RAM=6GB (0x180000000)
- **Fix**: Bound alterado de 0x200000000 para 0x180000000

### 5. Cleanup legados
- `tools/limine/mk_esp_fat.py` — mantido (build.rs depende dele)
- `tools/limine/build_esp.ps1` — deletado (build.rs gera ESP automaticamente)
- `tools/limine/limine.cfg` — deletado (só limine.conf é usado)
- `.gitignore` em `tools/limine/esp/`

### 6. NTP silenciado em sandbox
- **Causa**: NTP sync em loop (UDP bloqueado pelo SLIRP)
- **Fix**: `if is_sandbox() { return false }` em `hermes/src/ntp.rs:try_sync()`

### 7. Skill body size aumentado
- `package_hub.rs`: 64KB→512KB
- `self_evolve.rs`: 16KB→256KB

### 8. Power menu (Designer)
- Substituído diálogo simples de confirmação por menu com 3 opções: **Desligar**, **Hibernar**, **Reiniciar**
- Botões coloridos (vermelho/verde/âmbar) com eventos distintos (SYSTEM_SHUTDOWN/HIBERNATE/REBOOT)
- Tela preta com mensagem centralizada antes do ACPI cortar

### 9. WHPX PIT skip
- **Causa**: WHPX emite `Ignoring request for interrupt vector 0` quando o kernel programa o PIT
- **Fix**: Detecção de WHPX via CPUID leaf 0x40000000 em `apic.rs:pit_init()` → skip do PIT (LAPIC only)

### 10. NeuralFS RAM mount (CRC fix)
- **Causa**: `superblock.rs:read_block()` calculava CRC32C sem zerar o campo CRC (diferente de `encode_block()` que zera antes de calcular). CRC nunca batia → mount retornava None.
- **Fix**: `block[12..16].copy_from_slice(&0u32.to_le_bytes())` antes de computar o CRC, em AMBAS as cópias (k_nano e hermes).

## Resultados
- Boot Limine → desktop Jarbas na tela do QEMU (WHPX + janela)
- e1000: ARP, DNS, HTTP funcionam durante boot
- P6-P9 capability demos OK (non-fatal)
- 55 agentes registrados, scheduler rodando
- Soft power off via botão Power → ACPI PM1a_CNT (0xb004) → QEMU desliga
- WHPX sem warning (PIT skip funciona)
- NeuralFS montado: RAM 4MB free_blocks=1010 inodes=2
- Boot rápido: T+107 NeuralFS, T+333 Desktop (WHPX)

## Arquivos Modificados (31+)
- `crates/neural-kernel/src/main.rs` — PHYS_MEM_OFFSET early store, organização boot
- `crates/neural-kernel/src/user_mode.rs` — TRY_ENTER_RING3=false + guard topo
- `crates/neural-kernel/src/elf_loader.rs` — PHYS_MEM_OFFSET guard
- `crates/k_nano/src/e1000.rs` — ponytail guard pmoff==0
- `crates/cortex/src/bpe.rs` — bound 0x200000000→0x180000000
- `crates/boot/build.rs` — simplificado (sem fallback mkfat32)
- `crates/hermes/src/agents.rs`, `lib.rs` — ajustes
- `crates/jarbas/src/display/*` — ajustes compositor
- `tools/limine/build_esp.ps1` — removido
- `tools/limine/limine.cfg` — removido
- `tools/limine/esp/.gitignore` — adicionado

## Pendências
- WHPX: "Ignoring request for interrupt vector 0" — precisa investigar IDT/APIC whpx compatibility
- #PF secundário em 0xffff8001c0000000 (outro subsistema) tratado por SelfHeal LogAndContinue
- Performance: TCG lento (1 CPU), WHPX instability
- Bootloader 0.11 não testado (pode estar quebrado)
