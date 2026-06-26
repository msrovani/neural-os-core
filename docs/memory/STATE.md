# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.34.0
#   Self-Healing Kernel + Failure Taxonomy
# ═══════════════════════════════════════════════

## Últimos Sprints
| Sprint | v | Foco |
|--------|---|------|
| 31 | 0.31.0 | Hardware Capabilities (o que cada HW faz) |
| 32 | 0.32.0 | Self-Healing Kernel (panic → LLM → recovery) |
| 33 | 0.33.0 | Feedback loop (Hermes aprende com erros) |
| 34 | 0.34.0 | Failure Taxonomy + KernelError EventLog |

## 8 Tasks
1. system → 2. monitor → 3. hw_bridge → 4. network_agent →
5. input → 6. cortex_llm → 7. intent_router → 8. hermes_console

## Modelo
- 66.780 pares de treino (PCI + USB + SMBIOS + kernel + git + capabilities + errors)
- GTX 1050, 12 épocas, 12K exemplos, loss 1.156
- 68 KB, ~177K params ternários
