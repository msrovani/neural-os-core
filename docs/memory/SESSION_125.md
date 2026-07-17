# SESSION_125 — Triagem residuals ADR-0040 (deferidos)

**Data:** 2026-07-16  
**Objetivo:** Do que faltou/deferiu no fechamento ADR-0040 — algo ainda viável agora? Se não, marcar `por_fazer`.  
**Nota:** Sem implementação de código; sem git commit.

## Classificacao

| Deferido | Viavel agora? | Motivo |
|----------|:-------------:|--------|
| exFAT/NTFS/EXT **write** | Nao | Risco corromper midia sem validacao HW |
| MHI DMA NVMe↔DRAM / VRAM | Nao | Path DMA storage/GPU nao maduro no tick |
| #421 SysInstaller | Nao | UI/LLM + write HD; pos-MVP grande |
| #423 GPU Direct Storage | Nao | GPU compute + NVMe DMA |
| Cloud mounts plenos (#418) | Nao | Depende Sprint Net |
| NeuralFS disco fisico (#422) | Nao | Risco midia real; multi-level aberto |
| #419 Storage Manager App UI | Nao | CLI `storage_report` ja existe; UI Settings nao e pequeno/seguro sem jarbas |

**Veredito:** zero itens viaveis agora (baixo risco, sem HW real / sem Net / sem GPU DMA).

## Acao tomada

- ADR-0040 permanece lifecycle `completa` (MVP).
- Residuais marcados `por_fazer` em INDEX follow-up, IDEA #417–423, NeuralFS.md, STATE, TODO, TECNOLOGIAS, ADR §0.
- NeuralFS.md: `fazendo` → `por_fazer` (RAM ✅; residual = disco).
- Sem `cargo nk` (sem mudanca de codigo).

## Gate

ADR-0040 MVP intacto. Gate v2.0.0 ainda exige outros `por_fazer` (ADR-0046 + residuals FS + OK humano).
