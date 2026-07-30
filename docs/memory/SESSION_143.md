# SESSION_143 — Auditoria ideias antigas (viabilidade)

**Data:** 2026-07-18  
**Escopo:** docs only (IDEA_BANK + TODO + STATE). Zero LOC kernel.

## Objetivo

Reclassificar ideias antigas 🟡/⏳ frente a v1.8.6 / ondas 0–7 / ADR-0052 / `depends_on: lan`. Marcar destino de cada ID tocado (feito / descartado / adiado / AWAITING_HW / lan).

## Fecho agregado

### Feito / supersedido (✅ / 🔄)

| IDs | Destino |
|-----|---------|
| #1, #2, #9 | ✅ xHCI / probe |
| #3–5, #7, #10 | ✅/🔄 Trust+Cortex |
| #74 | 🔄 → #73b |
| #79, #80 | ✅ UEFI FB / font |
| #96, #103, #104, #309a, #309c | ✅ scheduler / WASM SFI próprio |
| #105, #107, #126–133, #138–140, #142–148 | ✅ cortex/cognitive (scaffold onde notado) |
| #151 parcial, #153–155, #157–159, #161, #163 parcial | ✅ tools / SelfOpt / self_evolve |
| #165, #166, #176 | ✅ Crompressor núcleo |
| #278a | ✅ AirLLM MVP; residuals Onda 6 |
| #279c | 🔄 → #422 NeuralFS |
| #280a–k | ✅ Batch 3 |
| #306a–b parcial, #308c parcial | ✅ loaders / SelfHeal |
| #310a | 🔄 → #315 |
| #310b | ✅ N5 / ADR-0036 |
| #315.21–22, #453 | ❌ já superseded (voz antiga / H3 ISA) |

### Adiado fora do gate (⏳ defer)

| IDs | Nota |
|-----|------|
| #8, #11 | WASM-USB pesquisa |
| #68–69, #72, #81, #92–93, #97–98, #100–101, #106, #108, #136, #149–150, #152, #156, #160, #162 | polish / pesquisa |
| #278b, #279a–b, #279e, #280l, #283a, #306c–d, #307, #309b | UX/compat/macro fora do gate |
| #315.26–27 | sync/SKYNET + `depends_on: lan` |
📋 Transferido para ADR-0081 (Malha Cognitiva Distribuída) — Fase C pós-gate v2.0.0

### Em onda / AWAITING / lan (🟡 / ▶️)

| IDs | Destino |
|-----|---------|
| #6, #12–15 | 🟡 Onda 4/security |
| #67, #71, #420, #423, #454–456 | 🟡 Onda 5 / ▶️ AWAITING_HW |
| #73, #73b, #117–124, #251–252, #308a–b, #418, #134 | 🟡 Onda 7; HTTP/TLS/cloud/fetch = **`depends_on: lan`** |
| #84 | ▶️ AWAITING_HW Onda 4 `[UAC-HW]` |
| #277c | 🟡 ADR-0041; net intent → lan |
| #279d, #282e–h, #283b | 🟡 Onda 3/4 HMI/FS |
| #417, #419 | 🟡 Onda 3 |
| AirLLM DMA/K-quant/9B (#278 residuals) | 🟡 Onda 6 |

### Sponsor (💰)

| IDs | Nota |
|-----|------|
| #43–52, #115–116 | NPU XDNA / ARM — sem mudança; permanece 💰 |

## Checklist fecho sessão

- [x] IDEA_BANK: legenda AWAITING_HW + linhas A–E
- [x] TODO.md: pista auditoria + ponteiro SESSION_143
- [x] STATE.md: fato operacional auditoria
- [x] SESSION_143 + SESSION_INDEX
- [x] PreFlight: docs-only; `python tools/preflight_wave.py --wave 0` recomendado pós-leitura

## Checklist plano E (conflitos)

- [x] #278a → ✅ MVP + Onda 6 residuals
- [x] #79 FB → ✅ UEFI
- [x] #1 xHCI → ✅; trust/UAC residuals
- [x] #103 → SFI próprio (não wasmi crate)
- [x] Legenda ▶️ AWAITING_HW
- [x] #279/#282/#283 sem target Sprint 91/92 fantasma

## Próximo

Continuar ondas de implementação (3 write / 4 Sound / …) sem reabrir IDs defer/sponsor; LAN só Onda 7.
