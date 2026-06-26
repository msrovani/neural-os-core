# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.24.1
#   Sprint 24: smoltcp + e1000 removal + SMP fix
# ═══════════════════════════════════════════════

# Project State — neural-os-core

## Sprint 24 — smoltcp Network Agent + SMP Fix (v0.24.1, 25/06/2026)

### Concluído
- ✅ **smoltcp 0.13.1 integrado** — Device trait para RTL8139, Interface + SocketSet
- ✅ **NetStack lazy** — Criado pelo agente no primeiro tick (não no boot)
- ✅ **HTTP não-bloqueante** — `http_new()` + `http_poll()` (1 estado/tick)
- ✅ **e1000 removido** — Arquivo deletado, init removido, proto.rs limpo
- ✅ **time_utils::datetime()** — Formatação UNIX→data BR, disponível global
- ✅ **SMP huge page fix** — `OffsetPageTable::map_to()` substitui raw PTE write
- ✅ **Page fault APIC eliminado** — Causa raiz: corrupção de tabela via PTE raw
- ✅ **3 APs estáveis** — Boot consistente, sem intermitência

### Resultados QEMU
- ✅ `-nic user,model=rtl8139` — TX funcional (ICMP/UDP/TCP)
- ✅ smoltcp poll por tick — 1 poll/tick, sem bloquear executor
- ✅ 13.200+ ticks sem crashes — 5 tasks persistentes
- ❌ HTTP GET google → timeout — NAT slirp não roteia TCP externo

### Pendências
- DNS resolve via smoltcp UDP socket
- HTTP para host local (10.0.2.2) via `hostfwd`
- Integrar Cortex LLM (Sprint 25)

## Sprints Anteriores (Completos)

| Sprint | v | Foco |
|--------|---|------|
| 1 | 0.1.0 | Toolchain & Boot |
| 2 | 0.2.0 | VGA & Serial |
| 3 | 0.3.0 | IDT & Exceptions |
| 4 | 0.4.0 | Memory & Heap |
| 5 | 0.5.0 | SIMD & Tensor |
| 6 | 0.6.0 | Neural Primitives |
| 7 | 0.7.0 | Intent Router MLP |
| 8 | 0.8.0 | PIC, Watchdog, Page Fault |
| 9 | 0.9.0 | Ternary Inference |
| 10 | 0.10.0 | 2-bit Packing |
| 11 | 0.11.0 | Bitmap Frame Allocator |
| 12 | 0.12.0 | Async Neural Executor |
| 13 | 0.12.0 | Event Bus IPC |
| 14 | 0.12.0 | Skill Registry & MCP |
| 15 | 0.12.0 | IRQ1 → EventBus → Agent |
| 16 | 0.12.0 | Closed Intent Pipeline |
| 17 | 0.12.0 | TicketLock + Refactor |
| 18 | 0.13.0 | PCI + ACPI + APIC |
| 19 | 0.14.1 | SMP + Slab + Heap 4MB |
| 20 | 0.15.0 | Hermes Chat |
| 21 | 0.16.0 | MHI + Inventory |
| 22 | 0.17.0 | Trust Cache + LAPIC Timer |
| 23 | 0.23.3 | RTL8139 + Neural Agent + TCP |
| 24 | 0.24.1 | smoltcp + e1000 removal + SMP fix |
