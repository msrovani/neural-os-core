# SESSION_131 — HW PnP: HwCapabilityCard + Expert v4 schema

**Data:** 2026-07-16  
**Versão:** v1.8.5 TEST (pós)  
**ADR:** — (fix + feature pontual; GOVERNANCE Regra A)  
**IDEA:** AIOS plug-and-play (identificar a quente → usar)

## Objetivo

Visão AIOS: HWID → contrato de uso (firmware, regmap, agent, next_action) → Hermes decide agenticamente → skill efêmera → com rotina vira WASM.

## Entregas

| Item | Status |
|------|--------|
| `k_ai::hw_capability` (`HwCapabilityCard`, topics) | ✅ |
| `HwDetectAgent` → cards + publish (sem free-text Expert) | ✅ |
| `dispatch_pnp_action` (NEED_FW / WIFI / …) | ✅ hooks honestos |
| `hermes::hw_pnp::hermes_decide_card` | ✅ observe → SkillOpt → escalate |
| Hermes `HW_CAPABILITY` → USER_INTENT (wifi/GPU) | ✅ |
| SkillOpt → `evolve::promote_ephemeral_to_wasm` | ✅ cola ≥3/70% |
| Remoção dump device_tree → `LLM_REQUEST` no detect | ✅ |
| `tools/train_hw_expert_v4.py` seed | ✅ (treino multi-head TODO) |

## Fluxo agentico

```
PCI → build_card → HW_CAPABILITY
                 → HW_PNP_ACTION (hint)
                 → dispatch (HEALTH / NET_IFACE / …)
Hermes cap_receiver → hermes_decide_card:
  observe_intent (S108)
  skill_opt.record_python_run (efêmera)
  maybe_promote → evolve WASM runtime
  maybe_auto_skill → SKILL.md
  escalate bind_wifi_scan|bind_gpu_compute → USER_INTENT (Cortex decide)
```

Hint `next_action` = sugestão do detect, **não** match hardcoded de ordem.

## Limites

- WASM gerado por `create_skill` ainda é bytecode template por keywords (não Python real).
- MicroPython→WASM SkillOpt path ainda sem `record_python_run` fora do PnP.
- Expert v4 sem `.bitnet` multi-head.
- Não inventa FW/scan/UVC.

## Evidência

- `cargo check --release` após wire agentico
