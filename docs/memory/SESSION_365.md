# SESSION_365 — ADR-0106 M1–M3 + D4 seed

**Continua:** SESSION_364 (D0–D3)

## Feito

### M1 — `hermes::typed_sites`
- `decide_emotion` (Choice 6) + wrapper `emotion_hint`
- `noul_skill_creation` + `is_skill_creation_request`
- Paridade com testes antigos do cognitive_bridge (25/25)

### M2
- `plugin_hub::scan` → Score tipado (`decide_plugin_risk`)
- marketplace `install_local` merge com `ApprovalGate::classify`

### M3
- HubHealth slog postura Decide (rate-limit 30s)
- `/decisions export` → JSONL

### D4 seed
- `export_labels_jsonl` + `tools/export_decision_labels.py`
- `train_router.py` detecta `data/decision_labels.jsonl` (mapping residual)

## Testes
- typed_sites 7/7 · cognitive_bridge 25/25 · contract_check OK

## Residual
- Mapear labels runtime → corpus de treino
- HUD pill dedicado
- M4 lista OUT expandida
