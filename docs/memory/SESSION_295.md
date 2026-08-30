# SESSION_295 — HW pendrive regressão: hang pós BOOT.LOG skip + Limine-only reboot

**Sprint:** v1.9.99-s295 TEST  
**Data:** 2026-08-29  
**Objetivo:** Boot HW real no pendrive parava em `LOG: BOOT.LOG skip`; reboots seguintes só Limine (kernel não sobe).  
**ADR:** — (fix pontual boot USB; alinha ADR-0062 P11 / SESSION_292 instalador)

## Sintomas (metal)

1. **1º boot:** splash `"Neural OS Core - Inicializando..."` ou checkpoint K25; última linha visível `LOG: BOOT.LOG skip — USB-MSC/ATA/AHCI AUSENTE`.
2. **Reboots:** só texto Limine — ESP/kernel inconsistente ou stick corrompido.
3. **BOOT.LOG no Windows:** mojibake ("chinês") — UTF-8 sem BOM aberto como ANSI/GBK.

## Causa raiz

| Checkpoint | Problema |
|---|---|
| K25 `init_after_usb` | `BOOT.LOG skip` é **honesto** (sem block device ainda) — não é o hang |
| P24a/P24b | `bringup_hid_keyboard/mouse` no xHCI do pendrive **sem MSC** → travamento longo |
| K27 | `verify_kernel_from_disk()` — PIO ATA interno (minutos em notebook sem disco útil) |
| K71 | `labor_smokes::run_deferred()` — rede/WiFi/xHCI hub antes do Runtime |
| Pipeline build | `cargo build -p boot` exit 0 mas `ESP image creation failed` no Windows → `uefi.img` stale vs `kernel.elf` novo → 2º boot só Limine |
| Display | `DisplayAgent` sem `set_urgency` → rate-limit 80% após ~50 ticks `Pending` → splash congela |

## Fix

| Área | Mudança |
|---|---|
| `main.rs` | `live_usb_no_msc`: skip verify ATA, defer P24a/b HID, `run_deferred_usb_live` quando `hw_real && MSC None` |
| `labor_smokes.rs` | `run_deferred_usb_live()` — ipc + async_io + limine evidence apenas |
| `main.rs` | `registry.set_urgency("display", 220)` |
| `boot_logger.rs` | BOM UTF-8 (`EF BB BF`) em `build_session_bytes()` |
| `boot/build.rs` | `python` antes de `python3` no Windows; log stderr/stdout se mk_esp falhar; não apaga `limine-esp-tree` |
| `build_usb_unified.py` | `sync_uefi_from_limine_esp`, `force_mk_esp_if_stale`, timeout dados 3600s |
| `build_image.py` | `--build-boot` sempre no unified HW |
| `mkfat32.py` | mapa `FW_LNAME_TO_FAT`, skip blobs 0 bytes, aliases firmware |
| `firmware.rs` | aliases FW ampliados |
| `fb.rs` | splash ASCII (sem em dash) |
| Jarbas | caps chat/notifications + trim (anti-OOM); `jarvis.rs` simplificação engine |

## Evidência

```text
cargo clean && cargo build --release -p boot
  → Limine boot image 128 MB (sem ESP image creation failed)

PACK_LLM=all python tools/build_image.py --hw --unified --size 6144
  → target/usb_hw.img 6271 MB
  → uefi.img / limine-esp.img mesmo timestamp
  → FALCON3.V6 ~1.8GB, AGENT.BIN, BGE, firmware, BOOT.LOG, NSGDB.BIN
```

Log: `target/build-usb-hw.log`

## Lições

1. **`BOOT.LOG skip` ≠ fatal** — procurar hang em P24a/b, K27, K71 logo depois.
2. **ESP stale = Limine-only** — sempre sincronizar `uefi.img ← limine-esp.img` antes de `usb_hw.img`; fallback `mk_esp_fat.py` se `build.rs` falhar.
3. **Pendrive boot sem MSC no K25** — path `usb_live` lite; labor completo e verify ATA são para QEMU/disco interno.
4. **BOOT.LOG no Notepad** — exige BOM UTF-8 ou abrir como UTF-8.

## Residual

- MSC pode enumerar tarde no Runtime — `SysInfoAgent` / `ensure_persisted` retry.
- Stick corrompido por boot abortado anterior — reflash DD obrigatório após imagem boa.
- Aceite metal K25+ pós-reflash: `SEC: skip ATA verify (USB live)` → AgentFleet → Runtime.

## Próximo

- Boot HW real com `usb_hw.img` 29/08 21:13; confirmar checkpoints K72+ e compositor.
- Commit `7b0b10b` audit Jarbas — testes unitários pendentes.
