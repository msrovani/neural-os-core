# ═══════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v0.25.0
#   Sprint 25: Neural Cortex in Hermes
# ═══════════════════════════════════════════════

# Project State — neural-os-core

## Sprint 25 — Neural Cortex (v0.25.0, 25/06/2026)

### Concluído
- ✅ **Cortex::think()** — classificador neural com 12 intenções (SystemStatus, Echo, HardwareInfo, TrustAllow/Deny, Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat)
- ✅ **Intent routing neural** — intent_router_daemon substitui INTENT_MLP por Cortex, dispatch automático para skills
- ✅ **Pipeline completo** — teclado → input_daemon → USER_INTENT → intent_router_daemon → Cortex → SkillRegistry → VGA
- ✅ **MemPalace 3.5.0** instalado e init no projeto (1271 drawers)

### Pipeline Neural
```
Usuário digita → scancode IRQ → hw_bridge → EventBus →
  input_daemon (buffer ASCII) → ENTER → USER_INTENT →
  intent_router_daemon → CORTEX.think("texto") → Intent (12) →
  SkillRegistry::execute_skill() → resultado → VGA
```

### Resultados QEMU
- ✅ Boot completo: 3 APs, RTL8139, smoltcp, DNS
- ✅ 7 tasks assíncronas rodando
- ✅ 6100+ ticks sem panics
- ✅ Cortex responde a qualquer texto

## Sprints Anteriores

| Sprint | v | Foco |
|--------|---|------|
| 1-22 | 0.1–0.17 | MVP básico (toolchain, VGA, IDT, heap, SIMD, tensor, NN, IPC, skills, SMP, APIC) |
| 23 | 0.23.3 | RTL8139 + Neural Network Agent + TCP handshake |
| 24 | 0.24.1 | smoltcp + e1000 removal + SMP huge page fix |
| 25 | 0.25.0 | Neural Cortex in Hermes |
