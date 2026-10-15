# SESSION_476 — Fórum OPCODE/1 retomado: fixes v6/v7 do loader com prova de runtime + gate de heap no sleep_cycle + harness -Force

**Data:** 2026-10-15 · **Estado:** v1.9.99-s476 TEST · **Branch:** `main`
**Fórum:** LOG AGENTES .txt lido integralmente (992 linhas); posts BUFFY-0006..0010.
**Contexto:** retomada após CURAIX-0056 (crítica de leitura de tree) e BUFFY-0001..0005
(agente do mantenedor neste workspace). Consenso vigente: CURAIX-0059 (hw expert v6 no
primeiro scan; LLM v7 só no slot primário; RustCoder inalterado) e CURAIX-0060 (P4 do
FREEBU-0150 já gated no fonte; P3 teto 1024000B; OOM do sleep_cycle sem segunda janela).

---

## 1. Problema P1 — OOM → #PF storm → park core em `CellNetwork::sleep_cycle` (FREEBU-0150)

- **PROBLEMA:** tick T+6018 do mesh 2×6c/6GB: OOM (`TALC sem memoria Tier 1
  size=2359307 agente=infer_worker`) → `#PF storm` → park fail-closed do core.
  Cadeia simbolizada no kernel.elf extraído do uefi.img (FREEBU-0150):
  `<Atomic<bool>>::compare_exchange ← RawVecInner::grow_exact ← CellNetwork::sleep_cycle`.
- **EVIDÊNCIA (medido neste tree):** `crates/cortex/src/cellular.rs` NÃO tinha gate de
  heap; `connect()` tem **1 único caller** (`sleep_cycle`, cellular.rs:286) — o gate no
  topo do caller cobre toda a superfície de crash.
- **CAUSA:** `connect()` → `fan_out/fan_in.push` → Vec grow sob heap esgotado → alloc
  NULL → deref como base atômica → #PF. Mesma classe do flush_merges (SESSION_416).
- **MENOR CORREÇÃO:** gate fail-closed `heap_headroom_critical()` no topo de
  `sleep_cycle`, padrão canônico `moe.rs::flush_merges`, `cfg(target_os = "none")`.
  Pular o ciclo é honesto: `fan_out`/`fan_in` são dedup'd e limitados por construção a
  `cells.len()` índices — o próximo ciclo com headroom reconecta, sem perda estrutural.
- **COMO VALIDAR:** teste host novo `connect_is_dedup_and_pair_survives` 1/1 (dedup +
  par sobrevive a sleep_cycle); `cargo check --release` 0 erros (canônico, 1m24s).
- **COMO FALSIFICAR:** se #PF storm persistir em heap crítico com o gate, o gate está
  no lugar errado.
- **Classificação:** fix = host-VERIFIED; runtime (boot com heap pressionado sem #PF)
  = UNKNOWN — entra na próxima imagem.

## 2. Prova de runtime dos fixes do loader (D2) — 2 boots WHPX ACEITE

Imagem reconstruída do working tree (`cargo build --release -p boot`, 0 erros, 1m25s;
uefi.img 134217728 B). Receita: vars OVMF frescas por boot, `-cpu Haswell` sob WHPX
(política: `max` só TCG), áudio omitido (causa medida de serial 0 bytes), QEMU 11.1.0.

### Boot A — expert v6 empilhado (fecha FREEBU-0148 / aceite CURAIX-0059)
- `FALCON3.BIN @0x100000000` + `hw_expert_v6.bitnet @0x13E000000` (depois do fim do
  LLM 0x13DD9BAB0; sem regions overlap).
- Log `logs/boot_expert_v6_20261015_004149.txt` (134163 B, sha256 `D0B2CFC5…175B1`):
  - `HWEXPERT magic 0xBE11BE11 found @0x13e000000 — tentando parse 1024 KB`
  - `v6 multi-head h=128 L=6 q_dim=32 vocab=64 ff=256 feat=0x03`
  - **`HWEXPERT LOADED v6 (QEMU-loader @0x13e000000) window=1024KB`** ← aceite
- 9 banners PHASE n=0..8 status=ok; runtime vivo (SCHED tick=1 agents=53 polled=63);
  `REJECT header invalido` = 0; panic = 0.
- **Veredito: ACEITE.** O artefato v6 NÃO precisa ser reemitido como v3.

### Boot B — v7 solo no slot primário (fecha SESSION_475 / BUFFY-0003)
- `BITNET2B4T.V7 @0x100000000` sozinho (sem Falcon3, sem expert).
- Log `logs/boot_v7_solo_20261015_005638.txt` (135281 B, sha256 `2F2CA9C2…FD98E`):
  - `v7 LLM h=2560 L=30 q_dim=2560 vocab=128256 act=1 emb=1 feat=0x07 max_seq=4096 tok_len=5390533`
  - `LLM LOADED llama8b.bin (QEMU@4G in-place) size=778810KB` (778810KB = 797501692 B,
    **delta 0** do BUFFY-0003)
  - `REJECT header invalido h=0 L=0` = **0 ocorrências** — o falso h=0 L=0 da
    SESSION_475 não reproduz mais.
- **Veredito: ACEITE.**

### Observação honesta (não bloqueia os aceites)
Ambos os boots terminam com **#PF storm pós-PostRuntime** contido por park fail-closed:
Boot A `ip=0xffffffff8097cc8b cr2=0x000040000007ffe8`, Boot B
`ip=0xffffffff80b33e76 cr2=0x00000000000830d8`, streak=3+. Sites rotativos entre boots
(classe wild-write pós-runtime, lição s438; mesma assinatura em logs de 13/10 com
kernel ANTIGO — BUFFY-0007). **Não é regressão dos fixes.** Classificação: OBSERVED,
causa UNKNOWN, aberto para a lane F1 (watchpoint ORACLE-0026 é o próximo passo).

## 3. Harness

- `run-qemu-whpx.ps1`: switch `-Force` no preflight de RAM (espelha
  `run-mesh4-lab.ps1:31`) — labs com overcommit consciente deixam de abortar
  (`host livre 3.8GB < 5GB exigidos`).
- `tools/run_boot_evidence.ps1` (novo): boot parametrizado por nome/loader/accel,
  vars OVMF frescas por ciclo, `.args.txt` por ciclo (lição SESSION_446: gravar a
  linha de comando antes de bissecar).

## 4. Fórum (posts desta sessão)

- **BUFFY-0006** (evidence): fix do gate do sleep_cycle + teste + check.
- **BUFFY-0007** (evidence): honestidade sobre o #PF storm pós-runtime (classe #630/F1,
  não é regressão; candidatos: MMIO/loader-region read ou PMM double-use).
- **BUFFY-0008** (proposal): correção `-Force` do run-qemu-whpx.ps1.
- **BUFFY-0009** (status): fechamento do round; pergunta ao HUMAN sobre commit.
- **BUFFY-0010** (evidence): prova de runtime dos 2 boots (logs + sha256 + vereditos).

## 5. Lições

1. **Parser de modelo é contrato com o layout do header** — versão nova do formato
   exige dispatch explícito: `load_model` (v1–v5) lê `hidden@14`; o v6 tem `hidden@18`
   → o parser legado via `h=1 L=0` e recusava silenciosamente ("REJECT header
   invalido"). O dispatch v6/v7 (`load_hwexpert_v6`/`load_model_v6`) existia — o que
   faltava era o caller chamar o loader certo por versão/model_type.
2. **Probe de tamanho tem que cobrir o header INTEIRO** — o v7 embute o tokenizer
   BPB1 de 5.390.533 B; um probe de 64 B devolve None e o fallback truncava o arquivo
   (604MB vs 797.501.692 B reais). `probe_llm_file_size` com `Need/Ready` resolve;
   `kv_term=hidden*k_dim` acerta delta=0 em 8 artefatos reais (BUFFY-0004).
3. **Gate de heap no caller único cobre a classe** — antes de gatear, contar os
   callers: `connect()` tem 1 caller → gate no topo do `sleep_cycle` cobre tudo; se
   houvesse N callers, o gate teria de viver dentro do `connect`.
4. **QEMU-loader: empilhar artefatos DEPOIS do fim do modelo primário** — o LLM de
   989MB ocupa `0x100000000..0x13DD9BAB0`; endereço "literalmente certo" da janela do
   expert (`0x129200000`) cai DENTRO do modelo → `regions overlap`. Receita provada:
   primário em `0x100000000`, extras empilhados após o fim real com folga.

## 6. Pendências (registradas, não fechadas)

- **Commit/push** — autorizado pelo HUMAN nesta sessão (CURAIX-0009 satisfeito).
- **Mesh re-run** com o kernel de agora (CURAIX-0060): peers + slog `matmul mesh skip`
  quando o shape não cabe; confirmar que o bin bootado tem o gate do sleep_cycle.
- **F1 1h 8c/8GB** — segue UNKNOWN (FREEBU-0147 provou 6GB/8c estável 2 ciclos).
- **#PF storm pós-PostRuntime** — OBSERVED, causa UNKNOWN (lane F1, watchpoint).
- **USB BOOT.LOG em HW real** — discussão aberta no fórum (próximo ato desta sessão).
