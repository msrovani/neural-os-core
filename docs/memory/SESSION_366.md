# SESSION_366 — ADR-0106 D4 map + HUD decide + M4 OUT

## Feito

### D4 mapping
- Ring `note_labeled_utterance` (weak_auto em intent claro + HITL `/decisions correct`)
- `export_labels_jsonl` emite `type=example`
- `train_router.load_decision_labels` + merge HITL×3 no train set
- `tools/export_decision_labels.py --write-demo`

### M3 HUD
- `HUB_ROWS=16`, row `decide` com `hub_posture_sev` / `hub_posture_line`
- Participa do pill do header (como fault/infer)

### M4
- `docs/architecture/0106-m4-excluded-sites.md`

## Lições (aprenda)
1. **Deny imune a confiança** — θ/Noul nunca abrandam Deny; só Auto/Escalate.
2. **Parsing ≠ juízo** — CLI/FAT/GGUF/scancode ficam OUT (M4); migrar degradaria determinismo.
3. **HITL×3 no host** — peso no trainer Python, não f32 no kernel; weak_auto ×1.
4. **Dual Master mesh** — `set_role` deve sincronizar `is_master` (Worker com flag presa = ROLE confuso).

## Residual opcional
- `r3::update_with_replay` contínuo a partir dos labels
- ECE report pós-treino com JSONL

## Testes
- intent_decide 4/4 · decision 5/5 · site_policy 5/5 · hub_tests 3/3
- `python tools/export_decision_labels.py --write-demo` + `load_decision_labels` OK

## Docs
- ADR-0106 lifecycle `feito`; IDEA #588 ✅; STATE s366; INDEX SESSION_364–366
