# SESSION_484 — Passo 0 do hub: validate_bitnet_blob + guard 8.3 (e a premissa que a verificação derrubou)

**Data:** 2026-10-17 · **Estado:** v1.9.99-s484 TEST · **Branch:** `main`
**Continua:** s483 (Passo 2, Reranker + load-on-first-use). Este é o **Passo 0** do plano s481
(validate entre read e load + nomes canônicos 8.3 no build).

---

## 1. A premissa que a verificação derrubou (antes de implementar)

O Passo 0 planejado tinha duas metades: `validate_niche_blob` e "fix dos nomes mortos 8.3"
(`ROUTER.BITNET`/`MICRO.BITNET` truncados pelo writer para `ROUTER.BIT`/`MICRO.BIT`, kernel
procurando o nome longo = **MoE router FAT load DEAD**).

**A segunda metade era falsa.** Antes de renomear 9 sites, li o matcher (`fat32.rs:1138`
`lookup_file_size` → `encode_83`): o kernel trunca ext para 3 chars **exatamente como o writer**
(`encode_83("ROUTER.BITNET")` = `[ROUTER  ][BIT]` = o mesmo dirent). **Nomes longos CASAM** —
truncamento simétrico dos dois lados. Renomear teria sido refactor inútil com diff em 9 sites.

O risco **real** do truncamento simétrico é outro: **colisão** — dois nomes que colapsam para o
mesmo dirent de 11 bytes fazem o 2º sumir do root. Isso é o que o guard verifica.

Corolário (reforço s446/s481): ler o matcher antes de "consertar" nomes. A busca de 13 chars
nunca casa com 11 bytes **como string**, mas casa **como dirent codificado** — a representação
importa, não o comprimento.

## 2. Artefatos de nicho EXISTEM (premissa do s482/s483 estava errada)

O falsificador do footprint (s482) e o Passo 0 (s483) assumiam "precisa de artefato
Vision/Reranker que não existe no repo". Medição direta em `target1/`:

| Artefato | Tamanho | Magic | Versão |
|---|---|---|---|
| `VISION.BIN` | 33.1MB | `0xBE11BE11` | v5 |
| `RERANKER.BIN` | 165MB | `0xBE11BE11` | v5 |
| `AGENT.v6` | 736MB | `0xBE11BE11` | v6 |
| `LEARNER.v6` | 117.9MB | `0xBE11BE11` | v6 |

E `find_file` (mkfat32.py:148) cobre `target1/` **primeiro** — os 4 já são empacotados
(mkfat32.py:423-431) com nomes que o kernel procura (`fat_names_for`). O alinhamento
build↔kernel **já existia**; o que faltava era a validação entre read e load.

## 3. O que foi feito

### 3.1 `validate_bitnet_blob` — `crates/cortex/src/model.rs`

Renomeado de `validate_niche_blob`: a regra é de **formato**, não de slot (sem param `slot` —
YAGNI; o caller loga slot/nome). Vive em `model.rs` porque a fronteira de formato é única lá.

- `< 16 bytes` → `too_short`; magic ≠ `0xBE11BE11` → `bad_magic`.
- **v6/v7**: `data[14] != 0` (model_type expert/router) → `not_llm` — o HwExpert tem path
  dedicado (`load_hwexpert_v6`, main.rs:4612-4826); parsear como LLM é fantasma. Depois
  `probe_llm_file_size`: `Ready(n)` com `n == data.len()` → Ok; senão `size_mismatch` /
  `truncated_header` / `bad_header` (s480: fantasma de tamanho = recusa, nunca registra).
- **v1..=5**: sem tamanho declarado no header → só magic → Ok (o warn de migração já vem do
  `load_model_v6`). VISION/RERANKER v5 passam por aqui.
- **outra versão** → `unsupported_version`.

### 3.2 Wire nos DOIS paths — `crates/neural-kernel/src/main.rs`

Entre `read_file` e `load_model`, com `warn` + `continue` (tenta o próximo nome fallback):
- **FAT** (`try_hub_slot_fat`, ~5026): `hub REJECT file={} slot={} reason={} ({}KB)`.
- **NeuralFS** (`try_hub_slot_neuralfs`, ~4866): mesmo gap read→load, +3 linhas,
  `NeuralFS REJECT path={} slot={} reason={}`.

Detalhe de wiring: o bin usa `cortex_crate::model::` (não `crate::cortex::model::` — o bin tem
facade própria sem o módulo `model`; 1ª tentativa deu E0433, corrigido para o caminho que o
próprio bin já usa em `probe_llm_file_size`, main.rs:284/4467).

### 3.3 Guard 8.3 — `tools/mkfat32.py`

`warn_83_collisions(files)` chamado antes do write (cobre TODOS os nomes: modelos, firmware,
NKP, CONFIG/BOOT.LOG/NSGDB/TLSPINS/HINT):
- **Colisão**: dois nomes → mesmo dirent 11 bytes → `[!] COLISAO 8.3: X == Y — o 2º some do root`.
- **Truncamento**: base > 8 ou ext > 3 → `[~] 8.3 truncado: NOME -> dirent ...` (informativo —
  o kernel casa igual, mas o operador vê o nome real vs dirent).

Demo com nomes reais da lista: 5 truncamentos avisados (ROUTER.BITNET, MICRO.BITNET,
MICROPY.WASM, BITNET850.BIN, HW_EXPERT.BITNET), colisão sintética detectada, `None` ignorado,
**zero falso-positivo** entre os nomes reais (FALCON3.V6 vs FALCON3.BIN diferem no ext).

## 4. Prova

- `cargo test -p cortex --lib model` — **11/11** (novo `validate_bitnet_blob_gates`: v6 válido
  Ok, truncado `size_mismatch`, magic ruim, curto, v5 Ok, v8, model_type≠0 `not_llm`; blob v6
  construído via `synth_header` + `parse_model_header` — auto-consistente, sem include_bytes).
- `cargo check --release` — **0 erros** (1m13s).
- `mkfat32.py` — sintaxe OK + demo do guard acima.
- Crash `0xc0000096 STATUS_PRIVILEGED_INSTRUCTION` em `cargo test -p cortex --lib` (suite
  completa): **pré-existente, provado pela 3ª vez** pelo método do repo — `git stash` → rodar →
  crasha igual (mesmo binário `cortex-9efca80e55e91f2b.exe`) → `stash pop` limpo (SESSION_418/483).

## 5. Runtime = UNKNOWN (mesmo bloqueio do s483)

Sob QEMU, `qemu_dev_skip_models` pula o bloco do hub inteiro — o validador não dispara em
runtime. Aceite exige HW real ou gate de sandbox liberado pelo dono. `IMPLEMENTED`, não
`VERIFIED` (ADR-0113).

## 6. Registrado, não fixado

- **MICROPY.WASM**: empacotado com **zero leitores** em todas as crates — peso morto confirmado
  (grep em k_nano/k_hal/k_ai/cortex/hermes/jarbas/neural-kernel). Candidato ao Passo 5-6 do
  plano s481 (podar), não fixado aqui.
- **Nomes longos funcionam** via truncamento simétrico — nenhum rename feito; o guard de colisão
  é a proteção da classe.

## 7. Pendências

- Aceite de runtime do validador (HW real ou gate liberado).
- Passo 3 do plano @oracle: evict explícito de 1 slot (Active ↔ Pro|Agent) com
  `heap_headroom_low()`.
- NeuralFS ainda eager (fora do 2b por ser outro transporte).
- #PF storm sob falcon3 (pre-existente, IP migrante — watchpoint, não whack-a-mole).
- Passo 5-6 do plano s481: podar `fat_names_for`/apagar `slot_from_bitnet_bytes` +
  `load_models_multi` (+ MICROPY.WASM).
