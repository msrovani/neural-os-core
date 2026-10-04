# Dead branches — arquivo local (Sessão AION · s451)

> **SINAL:** branches legados/merged arquivados aqui. Antes de recriar um branch
> de feature, cheque se já foi feito/mergeado (`git tag -l 'legacy/*'`).
> Refs locais preservadas como tags `legacy/<nome>`; os branches foram removidos
> do `git branch` local.

**Por que:** nenhum tem trabalho pendente útil. Seis estão **totalmente merged**
no `main` (tip = ancestral de main, `0` commits à frente); um é **WIP abandonado**
(`silicon-gpu-directives-wip`, 1 commit à frente, prefixo `deprecated`). Os 2
branches que pareciam "ativos" (`falcon3-3b-cognitive-lab`, `hda-bringup-s346`)
são **antigos e já em main** (266/140 commits atrás) — merge não traz nada.

| Branch | Tip | Data | à frente/atrás de main | origin |
|---|---|---|---|---|
| `cursor/falcon3-3b-cognitive-lab` | 20ecae8b | 2026-08-31 | 0 / 266 | sim |
| `fix/hda-bringup-s346` | 166e8b74 | 2026-09-16 | 0 / 140 | sim |
| `deprecated/cursor/ring3-tcg-accept-s278` | 6425c3f9 | 2026-08-24 | 0 / 332 | sim |
| `deprecated/cursor/silicon-gpu-directives-wip` | 1528b21d | 2026-08-21 | **1** / 413 | sim |
| `deprecated/cursor/silicon-wire-tsc-pad-rebar` | ebd262fc | 2026-08-21 | 0 / 407 | não |
| `deprecated/cursor/cherry-net-apic-models` | 29d80daf | 2026-08-21 | 0 / 409 | não |
| `deprecated/cursor/jarbas-honest-s276` | 55a776b8 | 2026-08-21 | 0 / 408 | não |

**Recuperar um branch:** `git branch <nome> legacy/<nome>`.

**Conteúdo (assunto do tip):**
- `falcon3-3b-cognitive-lab` — Falcon3-3B como lab cognitivo primário (ADR-0101).
- `hda-bringup-s346` — HDA bring-up em QEMU (CRST, kick do ICW, verbo `[19:8]`, enumeração em 2 níveis).
- `ring3-tcg-accept-s278` — ADR-0092 dmesg Neural (slog sev, PHASE, BOOT SCORE).
- `silicon-gpu-directives-wip` — WIP diretivas silício/GPU + módulos órfãos (AMX/TSC/PCIe).
- `silicon-wire-tsc-pad-rebar` — wire TSC+CachePadded+ReBAR; Fat32Io/format_bps; specs honestas.
- `cherry-net-apic-models` — `D:\modelos` + BGE_M3/hw_expert_v6 no mkfat32.
- `jarbas-honest-s276` — compositor honesto + HDA único + `infer_in_flight` (SESSION_276).
