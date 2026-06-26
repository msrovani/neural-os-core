# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.28.0
#   Sprint 28: HW-Aware Cortex LLM
# ═══════════════════════════════════════════════

# Project State

## Sprint 28 — HW-Aware Cortex LLM (v0.28.0)
- ✅ PCI ID database (23.858 entradas) → dataset (31.436 pares)
- ✅ Modelo treinado (loss 3.3→1.39) → .bitnet export
- ✅ HwIdentifySkill: `/hw` → PCI scan → LLM identification
- ✅ Intent::HardwareIdentify no Cortex

## Sprints Completos
| Sprint | v | Foco |
|--------|---|------|
| 1-25 | 0.1–0.25 | MVP → VGA → IDT → heap → tensor → SMP → RTL8139 → smoltcp → Cortex |
| 26 | 0.26.0 | Transformer Engine (Attention, 4 layers, generate) |
| 27 | 0.27.0 | Cortex LLM Daemon (8 tasks, LLM_REQUEST/LLM_RESPONSE) |
| 28 | 0.28.0 | HW-Aware Cortex LLM + PCI ID training + HwIdentifySkill |

## 8 Tasks no Executor
1. system_daemon → 2. monitor → 3. hw_bridge → 4. network_agent →
5. input_daemon → 6. cortex_llm → 7. intent_router → 8. hermes_console
