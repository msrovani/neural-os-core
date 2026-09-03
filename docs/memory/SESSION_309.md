# SESSION_309 — Falcon3 GGUF inference: TQ2_0 + BF16 + auto-config + boot wiring

**Data:** 2026-09-03 | **Sprint:** v1.9.99-s308 TEST | **Status:** wired (host tests + QEMU 4c PASS)

---

## Premissa AIOS (ADR-0088)

IA desde o boot: o LLM não é feature, é o modo de operar. O Falcon3-3B-Instruct-
1.58bit entra como modelo conversacional de verdade (BitNet 2B era stepping
stone — não dialoga). Loading **automático** no boot: nenhum modelo é hardcoded
no bin; o kernel *descobre* GGUF/bitnet no QEMU loader scan e constrói o modelo
a partir da metadata do arquivo.

## Problema

```text
GGUF magic 0x46554747 @0x100000000 — parse FAIL "n_dims fora de 1..=4"
cortex::gguf type IDs ≠ spec GGUF padrão → loading de QUALQUER GGUF real quebraria
```

Três bugs de compatibilidade impediam o Falcon3 GGUF de carregar:

| Bug | Sintoma |
|-----|---------|
| **Tensor type IDs errados** (enum Rust) | `type 2 = Q4_0` mas spec = **BF16**; mapa Q4_0/Q5_0/Q6_K deslocado |
| **Metadata value type IDs errados** | `6=uint64` (spec=FLOAT32), `8=float32` (spec=STRING), `9=bool` (spec=ARRAY), `10=string` (spec=FLOAT64). `llama.attention.layer_norm_rms_epsilon` (float32) lia 8 bytes → offset corrompido → "n_dims fora de 1..=4" |
| **Sem TQ2_0 (type 25)** | GGUFs ternários 1.58-bit (Falcon3 1.58bit, BitNet b1.58, PrismML Bonsai) não desquantizavam |

## Fix

| Onda | Mudança | Commit |
|------|---------|--------|
| A | `GgufType::TQ2_0` (type 25) + `dequantize_tq2_0_block` (f16 scale + 2-bit ternary, 24B→32 f32) + `nbytes_for_elements` | `b7d6f20` |
| B | Teste de integração GGUF sintético + `PartialEq` p/ `GgufType` (5 unit + 1 integration = 7 testes) | `84104ba` |
| C | **Tensor type IDs corrigidos p/ spec padrão** (0=F32, 1=F16, 2=BF16, 3=Q4_0 …) + `dequantize_bf16` | `eb8431f` |
| D | `GgufBackedModel` **auto-config** — hidden/layers/heads/kv_heads/intermediate/vocab/rope_theta/RMS norm lidos da metadata GGUF (nada hardcoded) + GQA per-layer | `d74401c` |
| E | Chat prompt format (`<|system|>/<|user|>/<|assistant|>`) + KV-cache 512 (modelos grandes) | `9bb7a7e` |
| F | **Metadata value type IDs corrigidos** (`read_metadata_value` + `_inner`) + `is_gguf()` + `register_bytes` cria `GgufBackedModel` → `CURRENT_MODEL` + scan do boot detecta magic `0x46554747` | `3262057` |

## Descobertas críticas

- **Falcon3 `-1.58bit-q2b0.gguf` NÃO é ternário no formato GGUF:** "1.58bit" é o
  método de *treinamento*, não o layout do arquivo. O GGUF real tem **155 tensors
  BF16 + 45 F16** (type 2). O TQ2_0 ficou para GGUFs ternários genuínos.
- **GGUF v2 (não v3) do Falcon3:** strings u64 idênticas ao v3 — compatível, sem fix.
- **Auto-config validado:** `hidden=3072 layers=22 heads=12/256 kv_heads=4
  rope_theta=1000042 vocab=32678`; `intermediate` veio 6144 (chave metadata
  divergente → fallback) — residual: alinhar nomes de chave metadata GGUF.
- **Sintético regenerado** com type IDs corretos (`tools/gen_test_gguf.py`) — o
  teste antigo usava o mapeamento bugado e passava por engano.

## Aceite

- [x] Host: `cargo test` cortex — 6/6 GGUF (5 unit + 1 integration) PASS
- [x] `cargo check --release` 0 erros
- [x] QEMU `-smp 16` TCG (`logs/boot_16c_inst0_20260903_192920.txt`):
  - **16/16 núcleos** — `online==madt-1 criterion OK (aps=15)`, `CorePools r0=1 r1=8 r2=7 total=16`, roles `sys=1 compute=8 worker=5 memory=2`, `smp_online=16` — fix max_aps 255 valida 16c sem falso warning
  - Saudacao suit-boot @register K44 + **TTS boot greeting 168160 frames** ✅
  - GGUF LOADED → CURRENT_MODEL; NSGDB `ingest ramlog → SGDB L3 boot/0000000 (5935 bytes)`
  - #PF/#UD restantes = demos P6 contenção Ring3 (esperados, non-fatal)
- [x] QEMU `-smp 8` TCG (`logs/boot_8c_inst0_20260903_191814.txt`):
  - **8/8 núcleos** — `CorePools r0=1 r1=4 r2=3 total=8`, roles `sys=1 compute=4 worker=2 memory=1`, `smp_online=8`, 7/7 APs em 64-bit Rust
  - **Fix doutrina falsa:** gate TCG tinha `max_aps=4` hardcoded (SESSION_279 condena "MAX_APS=7/.min(8)") → falso aceite `online=7 != madt_expected=4`. Wake sempre acordou todos os APs do MADT (SIPI dirigido ADR-0057); o cap só mentia no log. `HypervisorKind::Tcg => max_aps 255` — MADT Enabled = observe, não teto.
  - IPI Reschedule ativo em CPU 2/4/5/6; `runqueue: 12 agents → APs`
  - JARBAS greeting + TTS 168160 frames; GGUF LOADED → CURRENT_MODEL
- [x] QEMU `-smp 4` TCG (`logs/qemu4c_inst0_*.txt`):
  - `GGUF magic 0x46554747 @0x100000000 — GGUF LOADED -> CURRENT_MODEL` (~788MB/1109MB)
  - **4/4 cores** com runqueue s307/s308 (`CorePools r0=1 r1=2 r2=1 total=4`,
    roles `sys=1 compute=2 worker=1`) — sem regressão do anti-churn
  - **JARBAS greeting 40s** (TTS 168160 frames) + banner Hermes
  - BOOT SCORE `qemu=true ram_mb=9216 smp_online=4`
- [x] 2ª instância QEMU 4c: JARBAS 40s, sem conflito de lock (NSGDB cross-boot
  recall ainda limitado — TICKV RAM; ver IDEA #547/#539)
- [ ] Metal K23 residual; pipeline GGUF→logits end-to-end com tokenizer real
  (vocab 32678) + KV-cache 32K

## Arquivos

- `crates/cortex/src/gguf.rs` (TQ2_0, BF16, type IDs, auto-config, `is_gguf`, chat)
- `crates/cortex/src/cortex.rs` (KV-cache 512, `generate_speculative`)
- `crates/cortex/src/model_hub.rs` (`register_bytes` GGUF → CURRENT_MODEL)
- `crates/neural-kernel/src/main.rs` (scan QEMU loader: magic `0x46554747`)
- `tools/gen_test_gguf.py`, `crates/cortex/tests/tq2_0_gguf_load.rs`
- `tools/qemu_boot_stdio.py` (descobre `.GGUF` em `target/` + `target/models/`, cap 2GB)