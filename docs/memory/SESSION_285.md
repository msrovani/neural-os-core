# SESSION_285 — T-044: Per-Core Load Bars no HUD

**Data:** 2026-08-22 21:33
**Commit:** a77bb18
**Objetivo:** Exibir load por-core no HUD do Jarbas (SMP telemetry)

## O que foi feito

1. **gauges.rs**: Adicionado  e  ao 
2. **gauges.rs**: Nova função  via  do runqueue ()
3. **gauges.rs**: Feature-gated  — fallback  quando OFF
4. **gauges.rs**:  getter público para o compositor
5. **compositor.rs**: Barras verticais por-core no HUD (verde < 55%, amarelo < 80%, vermelho >= 80%)

## APIs usadas
-  → 
-  → número de cores online

## Verificação
-  → 0 erros
-  → 0 erros
-  → 116/118 (2 preexistentes)
