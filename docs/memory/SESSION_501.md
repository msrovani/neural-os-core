# SESSION_501 — A1 claims/drift + NixAIOS bancada (forum HUMAN-0010)

**Sprint:** v1.9.99-s501  
**Data:** 2026-10-22  
**Owner executor:** COMPOSER (forum) · **Validation:** HUMAN  
**Escopo:** docs/ + tools/ + CLAIMS.yaml · **Proibido:** crates/ · **Commit:** não (aguarda pedido)

## Decisões HUMAN (forum)

1. **Q1:** A1 autorizado — escrita em `docs/`, `CLAIMS.yaml`, `tools/`; sem `crates/`; sem commit automático.
2. **Q2:** NixAIOS = **bancada** (não produto) por enquanto. Observação: std é bem mais barato que no_std.
3. **Q3:** ver análise no_std vs std em COMPOSER-0014 (forum) e resumo abaixo.

## Q3 — o que tranca no_std e o que cai no std

| Trava no_std | Cai no std (NixAIOS/Linux)? | Permanece |
|---|---|---|
| Allocator/PMM/#PF/heap ~2030MB (F1) | Sim (alloc do OS) | — |
| Boot/firmware/ESP/ramlog | Sim (systemd/ISO host) | — |
| Drivers xHCI/ATA/NIC/GPU | Sim (kernel Linux) | — |
| SMP x2APIC/SIPI | Sim | — |
| soft-float / AVX2 morto | Sim (host CPU) | — |
| Sem Tokio/fs/net std | Sim | — |
| Prova = imagem bootada | Parcial (ainda precisa smoke) | HOST≠RUNTIME disciplina |
| Tese cognitiva + HITL | Não | R1 |
| CapGate / Trust | Não | R2 |
| Hub honesty / LOO | Não | R3 |
| Persistência que muda boot2 | Não (mais fácil substrato) | R4 |
| Claims n/a≠0 | Não | R5 |

**Estratégia:** usar bancada std para provar R1–R5 barato; no_std só onde a claim exige silício/G3.

## Entregas A1

- [x] Markers `<!-- MEASURED: rs=758 loc=232910 members=12 -->` em AGENTS.md + ROADMAP.md
- [x] `CLAIMS.yaml` (≥10 claims) + `tools/check_claims.py` (+ `--plant-demo`)
- [x] `tools/check_duplication.py` varre nixaios + EXCLUDE_NIXAIOS
- [x] `.gitignore`: `store/`, `volume/`, `models/*.misname_*`
- [x] `docs/memory/SESSION_488.md` recriada
- [x] Este SESSION_501 + headers TODO/SESSION_INDEX/STATE/ROADMAP → s501
- [x] TODO.md: removida cópia mojibake s451

## Gates rodados nesta sessão

| comando | exit |
|---------|------|
| `python tools/measure_repo.py` | 0 (rs=758 loc=232910 members=12) |
| `python tools/check_claims.py` | 0 (12 claims) |
| `python tools/check_claims.py --plant-demo` | 1 (esperado — prova o gate) |
| `python tools/check_duplication.py` | 0 (27 LEGACY_DUP warn; CROSS gguf/hub em EXCLUDE bancada) |

## Residuals

- Commit A1: só com pedido HUMAN
- Working tree nixaios/** (57 M): fora do escopo A1 — bancada, não merge forçado
- P0 funil: F1.5 recall em dev / #PF determinístico — não executados neste A1
