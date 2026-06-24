# ADR-0018: Sprint 24 Plan — Remaining Bugfix Sprint

**Status:** Draft (2026-06-24)  
**Context:** Após Sprint 23 (Code Review + 10 Critical fixes), restam 12 HIGH, 16+ MEDIUM, 12+ LOW bugs. Este ADR define a priorização e execução do Sprint 24.

## Strategy

1. **HIGH bugs first** — causam falha funcional ou crash em cenários reais
2. **MEDIUM bugs second** — prejudicam funcionalidade ou robustez
3. **LOW bugs last** — problemas cosméticos ou de eficiência

## 🔴 HIGH (12)

| # | File | Bug | Fix |
|---|---|---|---|
| H1 | `e1000.rs:234` | DMA buffer mapping PageFault — `send()` acessa TX buffer físico sem offset correto | Allocate TX buffers in identity-mapped region or fix PHYS_MEM_OFFSET handling |
| H2 | `proto.rs:203` | DHCP ACK single-shot poll (falso negativo) — após REQUEST, verifica RX ring **exatamente 1x** | Loop polling like OFFER (igual linhas 190-217) |
| H3 | `apic.rs:52` | LAPIC Spurious Vector não inicializado — se `svr=0`, vetor = 0 (Divide Error) | `self.write(LAPIC_SVR, (svr & 0xFFFFFF00) \| 0xFF \| 0x100)` |
| H4 | `interrupts.rs:158` | CPU exceptions 0-31 sem handlers dedicados — #DE, #UD, #NM, #MC caem em `unhandled_interrupt_handler` que faz EOI+retorna → loop infinito | Add handlers: `divide_error`, `invalid_opcode`, `device_not_available`, `machine_check` |
| H5 | `interrupts.rs:106` | `send_eoi()` PIC fallback só envia EOI ao master | Add slave EOI (`out 0xA0, al`) quando IRQ ≥ 8 |
| H6 | `memory.rs:159` | `allocate_below_1mb()` sem proteção para dupla alocação SMP | Atomic lock ou bitmap reservado para trampoline |
| H7 | `pci.rs` | PCI bridge scan fixo para `device` = bridge device (ignora outros device numbers no secondary bus) | Scan todos devices 0..31 no secondary bus |
| H8 | `main.rs:308` | `net::init_network()` nunca chama `resolve_gateway()` — gateway_mac = [0;6], ping/DNS falham | Chamar `resolve_gateway()` após DHCP |
| H9 | `net.rs:215` | `run_network_diagnostics()` chama `ping()` sem gateway_mac resolvido | Resolver gateway antes de ping |
| H10 | `dns_lookup()` usa MAC do gateway para DNS server (pode ser host diferente) | ARP resolve DNS server MAC separadamente |
| H11 | PCI multi-function: header type bit 7 ignorado para devices não-bridge | Verificar `header_type & 0x80` para todo device |
| H12 | IOAPIC unused IRQs não mascarados — spurious interrupts possíveis | Mask IRQs 2-23 no IOAPIC (exceto 0 timer, 1 kbd) |

## 🟡 MEDIUM (16+)

| # | File | Bug | Fix |
|---|---|---|---|
| M1 | `e1000.rs:268` | RX descriptor length sem upper bound — DoS por length malicioso | `min(len, 4096)` |
| M2 | `pci.rs:88` | Bridge scan assume `device` = bridge device number | Scan 0..31 no secondary bus |
| M3 | `apic.rs:95` | ISO polarity/trigger ignorados no IOAPIC redirect | Aplicar flags MADT no redirection entry |
| M4 | `apic.rs:275` | INIT de-assert usa edge trigger em vez de level trigger | `icr_val = (5u8)<<8 | (1<<14) | (3<<18)` |
| M5 | `proto.rs:312` | `parse_dhcp_ack` não trata PAD (option 0) | Adicionar `if opt == 0 { off += 1; continue }` |
| M6 | `proto.rs:123` | `parse_icmp_reply` ignora source IP (parâmetro `_expected_src` prefixado com `_`) | Validar source IP |
| M7 | `main.rs:116` | `HardwareInfoSkill` indexa `m.tiers[0]` sem verificar se array é vazio | Usar `m.tiers.first()` |
| M8 | `e1000.rs:128` | EEPROM timeout retorna MAC = [0;6] sem validação | Validar MAC não-zero; fallback para MAC via RAL/RAH |
| M9 | `proto.rs:381` | DNS `parse_dns_response` não lida com múltiplas answers | Iterar `ancount`, pular CNAME |
| M10 | `net.rs:26` | `online` flag nunca setada como `true` | Setar após ping/DNS sucesso |
| M11 | `e1000.rs:244` | TX buffer física sem map_page_uc — DMA pode cachear dados | Mapear TX buffer páginas como uncacheable |
| M12 | `smp/mod.rs:65` | Identity-mapping do trampoline usa `write_volatile` direto sem mapper | Usar OffsetPageTable existente |
| M13 | `acpi.rs` | ISO override não aplicado — loga mas não usa | Aplicar flags MADT nos redirection entries IOAPIC |
| M14 | `apic.rs` | LAPIC timer sem calibração — count fixo 8,388,608 | Calibrar via PIT ou TSC |
| M15 | `smp/percpu.rs` | PerCpu struct sem padding explícito — compiler pode adicionar | Usar `repr(C, packed)` |
| M16 | `trampoline.rs` | `asm!("nop")` barrier pode não serializar | Usar `lfence` ou `cpuid` |

## 🟢 LOW (12+)

| # | File | Bug | Fix |
|---|---|---|---|
| L1 | `proto.rs:139` | DHCP descobre adquire lock E1000 duas vezes por função | Expor `driver.mac()` sem lock |
| L2 | `main.rs:264` vs `285` | PCI scan executado duas vezes no boot | Reusar resultado de `init_pci()` |
| L3 | `hermes.rs:51` | `/trust allow 42` sem skill aceita vazio | Validar argumento não-vazio |
| L4 | `memory.rs:242` | `PHYS_MEM_OFFSET` store(Release) / load(Relaxed) sem sync SMP | Load(Relaxed) é suficiente pois BSP sempre seta antes de acordar APs |
| L5 | `e1000.rs:13` | `REG_ICR` definido mas nunca usado | Remover ou usar para diagnostico |
| L6 | `acpi.rs:24` | `ioapic_count`, `has_x2apic` campos nunca lidos | Remover ou expor via API |
| L7 | `apic.rs:47` | `Lapic::eoi()` definido mas nunca usado (usa `send_eoi()` em vez) | Remover ou alinhar |
| L8 | `inventory.rs:10` | Campos `lapic_count`, `has_virtio_*` nunca lidos | Remover ou conectar |
| L9 | `pci.rs:16` | `bar1..bar5` e `prog_if` nunca lidos | Remover ou expor |
| L10 | `slab.rs:64` | `SlabBucket::alloc/dealloc/contains` nunca usados | Remover (ou manter para futuro uso) |
| L11 | `smp/percpu.rs:30` | `AP_ONLINE` estático nunca lido | Conectar ao scheduler |
| L12 | `net.rs:11` | `subnet_mask` nunca lido | Usar em validação de IP |

## Sprint 24 Execution Order

```
Week 1 (HIGH):
  ├─ H1: Fix e1000 DMA buffer mapping (pre-req for all networking)
  ├─ H2: Fix DHCP ACK polling loop
  ├─ H3: Fix LAPIC spurious vector
  ├─ H4: Add exception handlers 0-31
  ├─ H7: Fix PCI bridge scan
  └─ H11: Fix PCI multi-function detection
  └─ cargo check --release + QEMU boot

Week 2 (HIGH + MEDIUM):
  ├─ H5: Fix PIC slave EOI
  ├─ H6: Fix allocate_below_1mb SMP safety
  ├─ H8-H10: Fix gateway resolution pipeline
  ├─ H12: Mask unused IOAPIC IRQs
  ├─ M1-M16: Medium bugs
  └─ cargo check --release + QEMU boot (DHCP funcional)

Week 3 (LOW + cleanup):
  ├─ L1-L12: Low bugs
  ├─ cargo check --release + QEMU boot
  ├─ Documentação: ADR-0018, STATE.md, SESSION_024.md
  └─ Commit + push (v0.18.0)
```

## Dependencies

- **H1 (DMA mapping)** é pré-requisito para testar H2, H8-H10 (rede funcional)
- **H8-H10 (gateway)** dependem de H1 + H2 (rede funcional + DHCP OK)
- **H7 + H11 (PCI)** são independentes — podem ser feitos em paralelo
- **H4 (exceptions)** + **H5 (PIC EOI)** são ortogonais
- **M1 + M4 (INIT de-assert)** dependem de testes SMP > 1 core
