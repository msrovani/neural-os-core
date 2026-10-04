# SESSION_451 — Fórum multi-AI (AION) + F1.5 PROVADO + v4 checkpoint + §3/§6/§15

**Data:** 2026-10-04
**Versao:** v1.9.99-s451 TEST
**Branch:** `main` · **Base:** `166305be` (s449) · **Anterior:** SESSION_450
**Alcance:** `k_nano` (tickv/boot_ramlog/flash/virtio_*), `k_ai` (self_heal), `cortex` (infer_queue), `jarbas` (jarvis), `hermes` (agents/lib/vfs), `neural-kernel` (main), `tools/*`, `docs/archive/dead-modules/`.
**Papel:** AION (participante do fórum OPCODE/1, sucessor do codename OPKIMI).

---

## 1. F1.5 — o experimento central **PASSOU** (runtime, QEMU TCG)

O falsificador da tese (ORACLE-0022): *boot 1 gera/persiste uma skill → power cycle → boot 2 recorda e executa diferente*. Antes: **FALSIFIED** (`reuse not_found`). Agora: **PASS**.

| Ato | Evidência (log) |
|---|---|
| Boot 1 | `act=gen name=oracle_rt_expr_v1 prov=model-born hash=0x458653425da3b4a5` + `run(6,7)=52` + `tickv flush … (ckpt)` |
| Power cycle | QEMU morto/relançado, mesmo disco |
| Boot 2 | `reuse name=oracle_rt_expr_v1 has_skill=1 hash=0x458653425da3b4a5` (MESMO) + `act=reuse a=9 b=2 result=82`; **sem** `act=gen` |
| Ablação | disco pristine → `md_keys=0` → `escalate=reuse reason=not_found`, sem `result=82` → reverte |

Prova `state(t1)!=state(t2)` **e** `behavior(t2)=f(recalled_state)`; a ablação descarta cache/template/hardcode.

### 1.1 Cadeia de bloqueios destravada (cada um com evidência)
1. **OVMF NVRAM corrompido** — `target/ovmf_vars.fd` fazia o OVMF não achar o ESP → serial de 0 bytes. A/B: vars virgem = 98 927 B em 60 s. Fix: `run-f15.ps1` usa template de vars virgem + vars regenerado.
2. **Static duplicado de virtio-blk** — o boot usa `virtio_modern::MODERN_BLK` (zero leitores) e o FileFlash lia `virtio_blk::VIRTIO_BLK_DEV` (vazio) → `backend=RAM` silencioso. Fix: legacy-first + `BlockDevice` para o moderno.
3. **Scan de mount** — `scan_range`: `append_off = degraded ? size : off` (fail-closed, não sobrescreve a cauda); `flush_opportunistic` grava ckpt para `file`/`nvme`; replay da cauda pós-ckpt.
4. **A causa final** — `sys/checkpoint` = **2 097 242 B** (`vlen`), **90 B acima** de `MAX_VLEN`; o scan rejeitava o record e avançava 512-a-512 pelo corpo, fazendo `break` no primeiro trecho zerado. Fix: `advance_oversized()` — um record shaped oversized é **pulado pelo total** (bounded), não 512-a-512.

## 2. v4 do checkpoint — removeu um footgun de 2 MB

`Checkpoint::serialize()` persistia o **bitmap PMM inteiro (2 MiB)** e `restore_checkpoint` o devolvia ao allocator — incoerente (heap/PT/drivers não são restaurados) ⇒ **footgun** (libera frames em uso → double-alloc). v4 = magic `SHV4` + `bitmap_hash` FNV + escalares (~102 B); `restore_checkpoint` = **diagnostics-only**. Log do Tickv: **4 365 312 → 171 008 B** (~26×). Host: 2 testes v4 + legado v3.

## 3. F1 — instrumentação de FASE (missão §5)

O `stage` do `[SILENCE]` era binário (1=no slice / 0=fora). Estendi `note_infer_stage` para as fases: **1**=prefill, **2**=decode, **3**=coarse (em `poll_slice` antes de cada passo), **4**=drain `LLM_RESPONSE` (jarbas). O dump agora localiza *em qual fase* o core parou.
**Runtime:** corrida 4c/4 GB por invocação direta = **ESTÁVEL ~54 min** (T+58787, 0 `[SILENCE]`/`[OOM-HALT]`/stall) — mas o LLM está **GATED** (soft-float) ⇒ o contexto do stall (`a2_proof`) não executou ⇒ stall = **INCONCLUSIVO**.

## 4. §3 / §15 — auditorias + arquivamento (missão §3/§15)

- **§3 (@explorer):** `LogAnalystAgent` morto (0 callers); `AutoLearnAgent` **probe de lab fabricava** 3 eventos `security` → **removido**; `forum_repair_ids.py` destrutivo; `forum_read.py` redundante; `BOOT_PHASE_RX` no-op; `ConsoleAgent` eco duplicado.
- **§15 (@oracle):** ~30 arquivos órfãos (o bin tem `#![allow(dead_code)]` mascarando); 3 módulos 0-callers; duplicatas reais; `check_duplication.py` com ~20/33 **falsos positivos** → **corrigido** (ignora `#![...]`/`use`).
- **Arquivamento (`git mv`, não delete):** **36 módulos** → `docs/archive/dead-modules/<crate>/` + índice `README.md` (função de cada um) + **SINAL** (neural-sgdb `mom/constraint` + linha na NAVEGAÇÃO do AGENTS.md) para **não re-planejar** o que já existe. `hermes/vfs/path.rs` facadado → `pub use k_nano::vfs::path::*`.
- **§6 (Recovery):** extraído `recover_budget_exhausted(count)` (pura) + teste (falha persistente ≠ reboot infinito).

## 5. Launcher canônico (investigação)

`run-qemu-whpx.ps1` dá **0 byte de serial** neste host (QEMU vivo), mas o **comando QEMU capturado via WMI, replicado direto, boota** (51–112 KB). Bisseção: disco IDE, loaders @4 GB, netmode, hostfwd, e1000, hda, xhci, virtio-gpu — cada um boota isolado. Suspeito: **dsound** (uma corrida com áudio OFF = 112 KB) + flakiness. Fix aplicado: `-AudioBridge` default `$false` (o comentário já dizia "default none"; o código estava `$true`). **Prático:** invocação **direta** do QEMU é confiável (foi o que rodou o F1).

## 6. Reparos de build (autorizados; edição concorrente da outra sessão em `hermes`)

`matrix_learn.rs` deletado com `pub mod` órfão; `log_analyst_agent` sem `mod`; chave `}` extra do `else` removido (`agents.rs:2392`).

## 7. Evidência e classificação

- `cargo build --release -p boot` = **0 erros**.
- Testes isolados (`--test-threads=1`): **k_nano 282**, **cortex 126**, **jarbas 135**, **hermes 311 + 1 pré-existente** (`permission_gate::test_risk_level_classify` — lista ORACLE-0024). `sgdb::bench::d_series_100k` aborta isolado = pré-existente.
- Fórum: **AION-0001…0025** (978/979 linhas NDJSON, 0 bad).

| Propriedade | Estado |
|---|---|
| F1.5 (2 boots + ablação) | **VERIFIED** (runtime) |
| v4 checkpoint (102 B, diagnostics-only) | **VERIFIED** (runtime + host) |
| Tickv scan oversized | **VERIFIED** (runtime) |
| virtio-blk / OVMF | **VERIFIED** (runtime) |
| F1 fase (instrumentação) | host-VERIFIED; stall runtime = **INCONCLUSIVO** (LLM gated) |
| §6 cap durável | **VERIFIED** (host) |
| §3/§15 auditorias + archive | **VERIFIED** (estático + build) |
| launcher canônico | **FLAKY/UNKNOWN** (invocação direta é o caminho confiável) |

## 8. Residual

- **F1 stall** precisa do LLM carregado (modelo **não** está no `disk_qemu.raw`; exige 8 GB).
- Launcher canônico flaky (dsound suspeito) — usar invocação direta.
- `fs/ata_agent.rs` **não** é duplicata simples (trait `FilesystemAgent` ring-local) — manter.
- `CHAT_UI_ENABLED=false` é flag **temporária**; `labor_smokes` são stubs ADR-0062 — manter.
- Nada commitado nesta árvore até o commit pós-task.
