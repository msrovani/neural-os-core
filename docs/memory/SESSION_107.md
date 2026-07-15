# SESSION 107 — Boot A/B → Cap ladder → Adequação ADR-0042

**Data:** 2026-07-14  
**Sprint:** 107  
**Status:** A/B + ADR-0041 P0–P9 ✅ PoC · QEMU Runtime OK · **ADR-0042** documentado  
**Continua em:** `SESSION_108.md` (N1 ✅ · 2B LOADED · **v1.7.0**)

---

## Fluxo da sessão

1. **Boot audit** → STI/PIC, stack, fases → Pacotes **A/B**.
2. **ADR-0041** P0–P9 PoC Cap (CR3, CapGate, FB, DMA, Ring3, #PF, vring, GGUF).
3. **Scripts QEMU** — path `target\disk_*.raw`, logs em `logs\`, size 1 GB.
4. **Loop boot** — hang FAT (`find_free_clusters`) · deadlock ATA+boot_logger · #PF P6 → Runtime OK.
5. **Análise visão × log** — Boot OK ≠ K²CHJ completo.
6. **Plano adequação** — cadeia + identidades → **ADR-0042** (N0–N5).

## Cadeia canônica (memorizar)

```text
k-nano → k-ai → cortex → hermes → jarbas
```

| Anel | Função |
|------|--------|
| k-nano | Sistema **legível** |
| k-ai | AI **para hardware** + SelfHeal + HMI máquina |
| cortex | **Cérebro** (MoE, learn, busca) |
| hermes | **Orquestrador** agentic / cria |
| jarbas | **Ego / persona / +10%** / frontend |

## Lições críticas

- Demos Cap no boot ≠ política Cap global.
- `TRY_ENTER_RING3=false` no path estável; P6 PoC ≠ usermode pleno.
- Telemetria de modelo pode mentir — alvo N1 (fechado em SESSION_108 / v1.7.0).
- FAT `find_free_clusters` por-entrada = hang em disco grande (fix: scan por setor).
- Checker 64×64 no FB = demo P4 residual (splash AIOS depois).

## Próximo (histórico → ver SESSION_108)

- N1 fechado; BitNet 2B LOADED; próximo = fix generate/TTS → fechar e2e clima.
- Gate de produto: **`v2.0.0` = N1–N5 done**; marco atual = **v1.7.0**.
