# SESSION 021 — Block 4: MHI + HardwareInventory + SystemArchitecture

**Data:** 2026-06-23  
**Versão:** v0.16.0  
**Sprint:** 21 (Block 4)

## Objetivo

Implementar o Memory Hierarchy Index (MHI), HardwareInventory e SystemArchitecture — o coração do Block 4: detecção automática de hardware e alocação inteligente por tier.

## Progresso

### mhi.rs

- `AllocTier` enum: Dram, Vram, Nvme, Hdd — 4 tiers de memória
- `MemoryTier` struct: kind, capacity_bytes, bandwidth_mbs, latency_ns, name
- `MemoryHierarchy::new()` — cria um tier Dram automaticamente a partir do bitmap frame allocator (via `usable_memory_bytes()`)
- `alloc_by_tier(Dram)` — aloca frames físicos contíguos do `GLOBAL_ALLOCATOR` e retorna `PhysAddr`
- `alloc_by_tier(Vram/Nvme/Hdd)` — retorna `None` com `serial_println!` diagnóstico

### inventory.rs

- `HardwareInventory::collect(pci_devices, acpi_info)` — coleta:
  - CPU count (via lapic_count do ACPI)
  - Total RAM (via `BitmapFrameAllocator::usable_memory_bytes()`)
  - PCI device list + flags (VirtIO-net, VirtIO-GPU, NVMe, XHCI)
- `SystemArchitecture::infer(inv)` — heurísticas rule-based:
  - GPU (PCI class 0x03) → ring1_mode=1
  - RAM > 2 GB → heap 2048 MB, RAM > 0.5 GB → 512 MB, else 64 MB
  - CPU > 4 cores → power_mode=1
  - trust_level=1, tensor_tier=0

### Adaptações

- `memory.rs` — adicionado `pub fn usable_memory_bytes(&self) -> u64` em `BitmapFrameAllocator`
- `main.rs` — boot flow adaptativo:
  1. PCI scan (segunda passagem para inventory)
  2. `HardwareInventory::collect()`
  3. `SystemArchitecture::infer()`
  4. Log em VGA+serial
  5. MHI init com tiers
  6. NeuralExecutor

### Dificuldades e Correções

1. **`usable_frames` é field privado** — `BitmapFrameAllocator::usable_frames` tem visibilidade module-private. Solução: adicionar `pub fn usable_memory_bytes(&self) -> u64` como accessor público.
2. **PCI scan duplicado** — a primeira chamada em `init_pci()` loga dispositivos, a segunda (via `pci::scan_pci()`) alimenta o inventory. PCI scan custa <1ms em QEMU, aceitável.
3. **Unused imports** (limpos) — `alloc::string::String` em inventory.rs, `Ordering` + `PhysFrame`/`Size4KiB` em mhi.rs removidos.
4. **IOAPIC mask bug (crítico)** — `redirect_irq()` em `apic.rs:87` setava `(1u32 << 16)` no redirection entry. Bit 16 = MASK no IOAPIC IOREDTBL. Todos os 24 IRQs ficavam mascarados → PIT timer nunca entregava interrupção → `hlt()` no executor dormia para sempre → apenas 1 ciclo de polling executava. Descoberto via debug: output serial mostrava `IOAPIC redirection[0]: low=0x00010000`. Corrigido removendo o shift. Confirmado: execução contínua com timer ~18.2 Hz, pipeline IPC completo (SYSTEM_READY → EchoSkill).

## Lições Aprendidas

- **IOAPIC redirection entry layout**: bits 0-7 = vector, 8-10 = delivery mode, 11 = dest mode, 12 = delivery status (RO), 13 = polarity, 15 = trigger mode, **16 = MASK** (1 = masked). O código antigo copiou o valor de reset (0x00010000) sem perceber que o bit 16 era o mask.
- **Debug via QEMU log**: `-d int,cpu_reset,guest_errors -D target/qemu-logs/qemu_trace.log` confirmou zero `check_exception` — o sistema não crashava, só dormia. O problema era funcional (interrupções) não estrutural (page fault, double fault).
- **Testes com timer**: sem interrupção de timer, sistemas cooperativos com `hlt()` travam silenciosamente. O indicador era a ausência de `[WATCHDOG]` no log.

## Resultados

- `cargo check --release`: ✅ 0 errors, 16 warnings (todos pre-existentes)
- Saída serial esperada: `[ARCH] System architecture: ring0=0 ring1=0 heap=2048MB trust=1 power=0 tensor=0`
- Saída VGA esperada: `[ARCH] System architecture: ring0=0 ring1=0 heap=2048MB`
- Saída MHI esperada: `[MHI] 1 tier(s). Best: Dram (X bytes avail)`

## Arquivos

| Arquivo | Ação | Linhas |
|---|---|---|
| `src/mhi.rs` | Criado | ~80 |
| `src/inventory.rs` | Criado | ~80 |
| `src/memory.rs` | Modificado | +1 método |
| `src/main.rs` | Modificado | +20 linhas (boot flow adaptativo) |
| `Cargo.toml` | Modificado | v.0.15.0 → v0.16.0 |
| `CHANGELOG.md` | Modificado | +Sprint 21 |
| `docs/memory/STATE.md` | Modificado | +Sprint 21 completo |
| `docs/memory/SESSION_021.md` | Criado | Este arquivo |
