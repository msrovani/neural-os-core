# SESSION_345 — F1–F4 Boot Alive + FAT Safe + Metal Persist + Infer Prefill

**Sprint:** v1.9.99 · **Data:** 2026-09-14  
**Plano aprovado:** F1–F6 (attack front); **código F1–F4**; F5–F6 abertos.

## Diagnóstico (QEMU 8c)

Após `pins=FAT miss file=TLSPINS.BIN`, boot soft-hang em DriverInit:

1. `persist_pins_to_fat()` no hot path (main + HybridVerifier create).
2. `Fat32Writer::write_file` no ramo “já existe” chamava `write_cluster_chain`, que **ignorava** a chain e fazia `find_free_clusters` de novo.
3. `find_free_clusters` varria até **65535** setores FAT via ATA PIO em imagem ~6 GB quase cheia → minutos de silêncio (FB stale PHASE 5).

MSC FAIL em QEMU = ruído esperado; não era a causa.

## F1 — BOOT ALIVE

- DriverInit: só `load_pins_from_fat()`; slog `persist deferred to Runtime`.
- Runtime (pré-scheduler): `persist_pins_to_fat()`.
- `persist_pins_to_fat`: SGDB sempre; FAT **só se** `TLSPINS.BIN` existir (overwrite-only); miss → `warn` DEFER (RAM+SGDB).

## F2 — FAT SAFE

| Item | Mudança |
|------|---------|
| F2a | `mkfat32.py`: pré-aloca `TLSPINS.BIN` 4 KiB (`TLSP`+ver+count=0) |
| F2a+ | Após inject: atualiza FSInfo `FSI_Nxt_Free` (sectors 1 e 7) |
| F2b | Kernel lê FSInfo hint; atualiza após alloc OK |
| F2c | Cap **512** setores + TSC **2 s**; slog `ok`/`warn` com `scanned=`/`us=` |
| Fix raiz | `write_file` existente → `collect_cluster_chain` + `write_data_to_clusters` (zero find_free se couber) |

## Aceite F1+F2 (2026-09-14 QEMU WHPX 8c / 6G)

**Imagem:** `disk_qemu.raw` 6144MB regenerada (`PACK_LLM=falcon3`) com `TLSPINS.BIN` 4 KiB + `FSInfo FSI_Nxt_Free=7386921`.  
**Kernel:** `cargo build --release -p boot` pós-F1/F2.  
**Log:** `logs/boot_whpx_20260914_103113.txt` (~48 KB congelou após saudação).

| Gate | Resultado |
|------|-----------|
| `pins=FAT load` TLSPINS 4096B | ✅ `n=0` |
| `persist deferred to Runtime` | ✅ |
| Passa DriverInit sem soft-hang pins | ✅ `pos-PS2` + `init_after_usb done fat_ready=true` |
| `flush BOOT.LOG ok=true` | ✅ |
| Sem Triple/PANIC | ✅ |
| PHASE 7 / `desktop_ready` / ticks | ❌ stall pós-saudação |

**Veredito F1:** **PASS** — o hang `find_free`/pins sumiu; boot atravessa o ponto que congelava em ~29 KB.  
**Veredito Runtime completo:** **INCOMPLETO** — após `TTS boot greeting`, CPU QEMU alta e log mudo (~8+ min). Próximo no path: load `BGE.BIN` ~135 MB via ATA PIO (`main.rs` pós-`emit_hw_greeting_at_register`) — classe SESSION_340, **não** regressão F1.

**Nota:** 1ª tentativa de mkfat32 falhou porque `if __name__ == "__main__"` foi removido por engano no patch FSInfo; corrigido antes do pack final (`TLSPINS` confirmado no FAT).

## Follow-up — BGE skip sandbox (mesmo dia)

- Gate: `hypervisor().is_sandbox()` → **não** lê BGE via FAT PIO no boot; slog `warn` + defer.
- Metal: path NVMe/AHCI/ATA intacto; USB-MSC continua budget 8 MB.
- Aceite QEMU 8c: **PASS** — PHASE 7 Runtime, `desktop_ready`, ticks, `BOOT SCORE`, `pins=FAT save OK`.
- `bge=ABSENT` no banner QEMU = esperado (sem loader-RAM); embedding pseudo até load deferred/HW.

## F3 — METAL PERSIST (código)

`SysInfoAgent` (`hermes/src/agents/sysinfo_agent.rs`):

- UI viva **sem** MSC → `Pending` (não re-enum xHCI / EnableSlot).
- UI viva **com** MSC → `ensure_persisted()` + remount NSGDB (`remount_after_usb_msc`); slog `ok`/`warn`.
- Sem UI: retry periódico + `clear_msc_port_skips` a cada 64; warn `BOOT.LOG persist pending`.
- BOOT.LOG já gravado mas backend ≠ `file` + MSC → remount FileFlash uma vez.

Overwrite BOOT.LOG continua data-only em `boot_logger` (SESSION_264). Aceite **metal** = operador: linhas reais em `BOOT.LOG` / NSGDB ≠ zeros no stick (QEMU MSC FAIL = não prova F3).

## F4 — INFER UX Prefill layer-yield (código)

`cortex/src/infer_queue.rs` (residual SESSION_328):

| Item | Entrega |
|------|---------|
| Phase `Prefilling` | setup (`embed_for_kv`) + step (`apply_one_layer`) |
| Yield | heavy: 1 layer ativa/slice (`soft_stride=3`); light: 2/slice |
| Telemetria | `telemetry()` → (slices, us_total, last_us, decode_tokens) |
| Budget | slog `prefill_slice slow` se slice >2 s; `prefill_done` com slices+us |
| Barge-in | `cancel_active` já s328 — checado em setup/step/decode |

`poll_slice`: `NeedPrefill` → setup+1º step; `Prefilling` → step; depois Decoding.

Aceite QEMU: modelo muitas vezes `ABSENT` → path só com Falcon3 carregado (loader/HW). Host: `cargo check -p cortex -p hermes`.

## F5–F6 (abertos)

| Frente | Status | Notas |
|--------|--------|-------|
| F5 FS STRATEGY | médio prazo | ESP FAT; dados FAT/exFAT opcional; NeuralFS `0x7F` quando boot ler |
| F6 LAYER S | gated | GPU/NPU W2A8 só com Ready — fora deste entregável |

## Ficheiros

- `crates/neural-kernel/src/main.rs` — defer persist + BGE sandbox skip
- `crates/hermes/src/tls/trust.rs` — overwrite-only FAT
- `crates/hermes/src/agents/sysinfo_agent.rs` — F3 persist/remount
- `crates/k_nano/src/fat32.rs` — FSInfo + cap + write_file overwrite real
- `crates/cortex/src/infer_queue.rs` — F4 Prefilling + telemetry
- `tools/mkfat32.py` — TLSPINS + FSInfo pós-inject

## Check

`cargo check -p k-nano -p hermes -p cortex` → 0 erros (warnings Known).
