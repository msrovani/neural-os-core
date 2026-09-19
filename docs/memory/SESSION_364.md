# SESSION_364 — ADR-0106 Decisões calibradas wired no AIOS

**Sprint:** s364  
**Foco:** Integrar forma System One (Choice/Score/Noul) **sem** TypeSafe — contrato no ecossistema K³CHJ.

## Feito

### D0 — `cortex::decision`
- `Decision<T>`, `Confidence` Q8, `Q8Dist`, `AbstainReason`, `DecisionSource`
- `note_outcome` + reliableza in-memory + `persist_reliability` / `hydrate_reliability` (Tickv `ai/decisions/reliability`)
- `pin_theta` + contadores + slog `Decide ok`

### D1 — `cortex::intent_decide`
- Score por braço (não first-match); `Intent::Unknown` escape hatch
- Anti-sombreamento "volume do disco" ≠ AudioVolume
- `Cortex::think` = wrapper; `Cortex::decide` = decisão tipada
- Hermes: abstenção → sem skill estruturada → path LLM (não HITL)

### D2 — `hermes::site_policy`
- Classe Conversational vs Action; `Deny` imune a confiança
- `ApprovalGate::classify` **religado** → Noul destrutividade
- PermissionGate unifica com classify
- HITL `resolve` → `note_outcome` (seed D4)

### D3
- `difficulty_gate::decide_tier` → `Decision<ComputeTier,3>`
- Noul em `site_policy::noul_destructive`

### M0 + tooling
- `docs/architecture/0106-m0-triagem-sitios.md`
- `tools/decision_contract_check.py` (OK)

### UX
- `/decisions status|theta|persist|hydrate`
- `init_decision_layer` no `HermesAgent::new`
- status_line cognitiva inclui Decide

### P0 SSE2
- Rebuild shape `(m,n)` também no path host SSE2 (parity bare-metal)

## Residual (não neste entregável)
- D4 treino `train_router.py` com labels honestos + ECE
- M1 emotion_hint / skill_creation
- M2 marketplace juízos
- M3 HUD postura dedicada
- θ do diagrama de reliableza (ainda conservador estático)

## Testes
- `cargo test -p cortex --lib intent_decide` → 4/4
- `python tools/decision_contract_check.py` → OK
- site_policy / decision — ver log desta sessão

## Honesty
- Fonte ainda `Heuristic` até head neural próprio
- Não há dependência TypeSafe/Jev cloud
