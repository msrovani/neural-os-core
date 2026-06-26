# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.36.0
#   Self-Healing Kernel (Bloco Único)
# ═══════════════════════════════════════════════

## Blocos Completos
| Bloco | Sprints | v | Foco |
|---|---|---|---|
| Chassi | 1-17 | 0.1–0.12 | VGA, heap, EventBus, IPC, SMP, APIC |
| Discovery | 18-22 | 0.13–0.17 | PCI, ACPI, MHI, Trust, LAPIC |
| Rede | 23-24 | 0.23–0.24 | RTL8139, smoltcp, NIC |
| Transformer | 26-27 | 0.26–0.27 | Attention, 4 layers, generate |
| HW-Aware LLM | 28-30 | 0.28–0.30 | PCI+USB+SMBIOS+xHCI+model |
| Capabilities | 31 | 0.31 | O que cada HW faz + skills + MHI |
| **Self-Healing** | **32-36** | **0.32–0.36** | **Panic→LLM→recovery, taxonomy, respawn, corrective** |

## 8 Tasks
1. system → 2. monitor → 3. hw_bridge → 4. network_agent →
5. input → 6. cortex_llm → 7. intent_router → 8. hermes_console

## Modelo
- 66.780 pares, GTX 1050, loss 1.156, 68 KB
