# SESSION_145 — Onda 4: USB Trust + UAC-HW fecho

**Data:** 2026-07-18  
**Pista:** Sound/#84 + USB security #6/#12–15  
**Check:** `cargo check --release` = 0 erros  
**PreFlight:** `python tools/preflight_wave.py --wave 4` EXIT=0

## Vereditos

| ID | Status | Nota |
|----|--------|------|
| #84 UAC isoc | ▶️ AWAITING_HW | Parse/probe OK; logs `[UAC-HW] VERDICT=AWAITING_REAL_HW`; iso TRB não inventado |
| #6 usb.tbl | ✅ MVP | `usb_trust.rs` + persist NeuralFS `system/trust/usb.tbl` |
| #12–15 policy | ✅ MVP | `decide` Allow/Deny/Observe; `USB_TRUST_ENFORCE=1`; `disable_port` / `disable_untrusted_ports` |
| soft-float/VITS | ⏳ defer | neural-lite permanece path executável; sem fake HiFi-GAN |
| jarbas cutover | ⏳ defer | truth = `neural-kernel/src/audio` (ADR-0045) |

## Código

| Peça | Path |
|------|------|
| TrustTable | `crates/neural-kernel/src/usb_trust.rs` |
| Gate MSC | `usb_msc::probe` → `usb_trust::decide` |
| Gate UAC | `UsbAudioAgent::tick` |
| PORTSC | `xhci::disable_port` / `disable_untrusted_ports` |
| Persist | `NeuralFsAgent::sync_usb_trust_table` |
| CONFIG | `USB_TRUST_ENFORCE=0` default (`mkexfat.py`) |
| UAC-HW logs | `audio/usb.rs` |

## Policy

- Tabela vazia + MSC boot → **Allow once** + seed `BOOT` (vid=0,pid=0 até EP0).
- `USB_TRUST_ENFORCE=0` → **Observe** (permite + log).
- `USB_TRUST_ENFORCE=1` → **Deny** unknown + tenta limpar PED nas portas CCS.
- Trust-once: `usb_trust::allow(vid,pid,tag)` + re-sync `usb.tbl`.

## Próximo

Onda 5 (GPU / MHI DMA) ou boot QEMU com `USB_TRUST_ENFORCE=1` para evidência serial `[USB-TRUST]`.
