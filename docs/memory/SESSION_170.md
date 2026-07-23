# SESSION_170 — USB-MSC bring-up no stick bootável (ADR-0062 P11)

**Data:** 2026-07-23  
**Foco:** Gravar `BOOT.LOG` / ler FAT no **mesmo pendrive** que fez UEFI boot.

## Problema

`usb_msc::probe` usava `slot=2` + `configure_msc_endpoints` **sem**:
- HCSPARAMS1/DBOFF corretos (init lia Operational em vez de Cap)
- Command Ring / Enable Slot / Address Device
- Configure Endpoint command
- SET_CONFIGURATION

Resultado: `USB-MSC AUSENTE` → `BOOT.LOG` só ramlog (após SESSION_169 sem soft-reboot).

ADR-0062 §2.8/P11/#490: ClaudioOS tem BOT+SCSI completo; neural tinha só “xHCI port scan”.

## Feito

1. **`k_nano/xhci/`** módulo: init corrigido (Cap HCSPARAMS1, DBOFF, RTSOFF, CRCR, ERST/ERDP, DCBAAP@0x30, HCRST).
2. **`xhci/bringup.rs`**: Port CCS → reset → Enable Slot → Address Device → SET_CONFIGURATION → Configure Endpoint (bulk) → `bringup_boot_msc()`.
3. **`usb_msc` (k_nano + bin)**: probe via bringup; TUR + REQUEST SENSE; BOT DMA separado CBW/data/CSW.
4. Doorbell: `DB[slot]=DCI` (antes offset errado).

## Verificação

- `cargo check -p neural-kernel --features fat-boot-log` → 0 erros
- HW: ckpt **K16 USB-MSC OK** + `E:\BOOT.LOG` após boot; se AUSENTE, serial/FB mostra estágio (porta CCS / Enable Slot / Address / SCSI)

## Residual

- Hubs externos / SuperSpeed tuning
- EP descriptor parse (assume EP1 IN/OUT)
- Promover `usb_msc` bin→crate stub quando estável
