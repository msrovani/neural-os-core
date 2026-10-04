# SESSION_450 — Reconciliação dos `.md` da raiz (doc-drift pós-s449)

**Data:** 2026-10-04
**Versao:** v1.9.99-s450 TEST
**Branch:** `main` · **Base:** `166305be` (s449)
**Alcance:** os 14 `.md` da raiz + `tools/measure_repo.py`. **Zero `.rs` de produto.**
**Retomada:** [RESUME.md](RESUME.md)

---

## 1. O que foi revisado

Os 14 `.md` da raiz: `README`, `AGENTS`, `CHANGELOG`, `TODO`, `ROADMAP`, `SUMMARY`,
`TECNOLOGIAS`, `CONTEXT`, `HOWTO`, `COMMERCIAL`, `ATTRIBUTIONS`, `CONTRIBUTING`,
`codemap`, `DEAD_WEIGHT_AUDIT`. Revisão por 2 lanes de explorer (identidade/técnica +
processo/legal).

**Limpos (sem mudança):** `CONTEXT.md` (totalmente atual), `COMMERCIAL.md`,
`CONTRIBUTING.md` (comandos e topologia de ring conferem).

## 2. Drift encontrado e corrigido

O drift era **universal** — os docs estavam parados em `v1.9.99-s412` / `2026-09-26`
(README, SUMMARY, ROADMAP, TECNOLOGIAS, codemap) e as métricas `~169K LOC / ~672 .rs`
não batiam com o repo.

| arquivo | correção |
|---|---|
| `README.md` | versão `s412`→`s449`; `784/6` testes → 3 falhas pré-existentes; `~100`→`~110` ADRs; link `0042-*.md` (glob literal) → `0042-k2chj-adequacao-boot.md`; HW Expert v3→v4; "twelve crates plus a boot binary" → 12 members |
| `SUMMARY.md` | versão/data; métricas; pista ativa s449; gating B/C `ADR-0041`→`ADR-0077/0102` |
| `ROADMAP.md` | data/versão/métricas; pista ativa; **ADR-0058 `Proposed`→`✅ S1–S4`**; `5 crates`→`6`; **workspace members reais (12)**; **rings corretos** (k_hal=R1, k_ai=R2, hermes/jarbas=R3) |
| `TECNOLOGIAS.md` | métricas/versão; **licença `MIT`→`AGPL-3.0`** (header + tabela — o repo é AGPL); Limine "UEFI only"; SDIO `2.794`→`95.812`; blobs `92`→`90`/`~13.7 MB`; tags `s315/s328`→`s440/s449`; footer; **2.10g retargetado de ADR-0082 (Hardware Info Registry) para ADR-0077/0102 (Ring3 Isolation)** — o corpo era de Ring3, o número estava errado |
| `codemap.md` | métricas/versão; FS `ext2/NTFS/Btrfs` (não existem) → `FAT32/exFAT/NeuralFS/VFS`; `512MB heap`→`512MB floor + auto-grow` |
| `HOWTO.md` | footer `s412`→`s449`; nota sobre `fat-boot-log` (já vem ON por `boot`→`neural-kernel features=["fat-boot-log"]`; o alias `cargo nk` só repete) |
| `ATTRIBUTIONS.md` | URL placeholder `https://github.com/` → sem-URL (não inventar repo) |
| `DEAD_WEIGHT_AUDIT.md` | **status anotado**: #1/#2/#3/#5/#6 `✅ IMPLEMENTED (s449)`; #7 `❌ INVERTED` (E3 criou o consumidor); footer "read-only, no code changes" → "pre-fix snapshot" |
| `AGENTS.md` | header `~242K/~771`→`~214K/~714`; marcador `MEASURED` |

## 3. Correção estrutural: o gate de métricas media um repo irmão

`tools/measure_repo.py` conta **todos** os `.rs` sob a raiz (excluindo só `target/`/`.git`).
`crates/neural-sgdb/` é um **repo irmão clonado localmente** (path-dep do `k_ai`),
gitignored (`.gitignore` L95) — seus **64 `.rs`** entravam na conta → o marcador dizia
`rs=777 loc=252696` quando o repo de fato tem **`rs=714 loc=213629`**.

**Menor correção:** `SKIP_PARTS = ("target", ".git", "neural-sgdb")`. Marcadores em
`AGENTS.md`/`ROADMAP.md` atualizados para `rs=714 loc=213629 members=12`.
Lição: métrica que varre o diretório mede o vizinho, não o repo.

## 4. Verificação

| check | resultado |
|---|---|
| `python tools/measure_repo.py` | **exit 0** — `AGENTS.md: OK`, `ROADMAP.md: OK` (rs=714 loc=213629 members=12) |
| `LICENSE` | confirmado **AGPL-3.0** (a tabela TECNOLOGIAS dizia MIT) |
| ADRs | 110 arquivos em `docs/architecture/` |
| FS ext2/NTFS/Btrfs | 0 módulos (codemap afirmava existir) |
| firmware | 90 blobs / ~13.7 MB |
| Sem build | mudanças são só docs + 1 gate Python; nada em `.rs` de produto |

## 5. UNKNOWN (não afirmar o contrário)

- **Contagens de prosa não re-medidas:** HW Expert v3 (61.453) vs v4 (~60K/44K),
  estrelas de repo de terceiros, SLA comercial — fora do escopo do drift apontado.
- **TECNOLOGIAS 2.10g:** retargetado para `ADR-0077/0102` porque o **corpo** é Ring3
  isolation; se o slot deveria mesmo ser o **Hardware Info Registry (ADR-0082)**, a
  decisão é do maintainer (o número ADR original estava errado para o corpo).
- `s412` ainda aparece em texto de **histórico de lições** (AGENTS/ROADMAP) — referência
  legítima, não marcador de versão.

## 6. Lições

1. **Doc-drift é universal, não pontual.** Em ~5 meses de sessões (s412→s449) *cinco*
   arquivos de topo ficaram presos na mesma versão. Um único "passe de reconciliação"
   (s329) envelhece; a métrica deve ser medida a cada release, não copiada.
2. **Métrica que varre o diretório mede o vizinho.** `measure_repo.py` contava o repo
   irmão gitignored. O gate só vale se medir o que diz medir.
3. **Título e corpo de uma linha de catálogo são um contrato:** a linha 2.10g tinha
   número ADR (0082) e corpo (Ring3) de tecnologias diferentes — um deles estava errado.
