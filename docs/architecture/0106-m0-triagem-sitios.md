# ADR-0106 M0 — Triagem canônica de sítios de decisão

**Data:** 2026-09-19  
**Status:** inventário + M1–M3 wired (s364–s365)  
**Regra:** IN = significado de texto livre; OUT = sintaxe/contrato (não migrar).

## Contagem canônica (primeira passagem)

| Classe | Anel | Exemplos | Ação |
|--------|------|----------|------|
| **IN** | cortex | `think`/`decide_intent`, `difficulty_gate` tier | D1/D3 ✅ |
| **IN** | hermes | skill risk, wasm, emotion, skill_creation, plugin scan | D2/M1/M2 ✅ |
| **IN** | hermes | marketplace install risk merge | M2 ✅ |
| **OUT** | hermes | `parse_command` slash, JSON-RPC method names | código |
| **OUT** | k_nano | extensão arquivo, GPT type GUID, protocolo USB | código |
| **OUT** | cortex | GGUF type IDs, magic numbers | código |
| **OUT** | jarbas | scancode tables, theme enum match | código |

## Sítios IN priorizados

1. `intent.think` — **DONE**
2. `skill.risk` — **DONE**
3. `compute.tier` — **DONE**
4. `wasm.host` — **DONE** (PermissionGate → classify)
5. `emotion.hint` — **DONE** (`typed_sites`)
6. `skill.creation` — **DONE** (Noul)
7. `plugin.scan` — **DONE**
8. marketplace risk — **DONE**
9. HUD pill dedicado — residual (posture slog HubHealth ~30s)

## Excluídos (OUT)

- `parse_command` / `Command::*` — contrato CLI
- FAT 8.3 / Limine paths — layout
- PCI BAR / VID:DID — hardware contract
- GGUF type IDs — wire format
- `RiskLevel` allowlist exata `aios::log` — host ABI

## Cauda

- D4: mapear JSONL → exemplos no `train_router.py`
- M4: expandir OUT no INDEX se novos sítios aparecerem
