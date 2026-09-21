# SESSION_382 — IDEA #542 mouse USB tablet (residual s381)

**Sprint:** v1.9.99-s382 TEST  
**Data:** 2026-09-21  
**Foco:** QEMU `usb-tablet` — deferred HID no sandbox + retry pós-UI

## Causa

- `MOUSE_ABS_*` congelado no centro: `poll_mouse` no-op sem `mouse_ready`
- PS/2 `aux=0` é **esperado** com `-device usb-tablet`
- `InputAgent` skipava `try_deferred_hid_bringup` em **todo** hypervisor (`is_sandbox`)
- P24b no DriverInit falha → sem retry no QEMU

## Aplicado

| Fix | Onde |
|-----|------|
| Deferred HID em sandbox (ticks 90/180/360 + retry se !ready) | `hermes::agents::InputAgent` |
| 1× deferred após UI/cursor probe | `jarbas::display::agent` |
| `xhci::mouse_is_ready()` | `k_nano::xhci` |

## Aceite

- Serial: `deferred HID after UI` / `deferred P24b` / `P24b HID mouse OK` ou warn honesto
- `MOUSE_ABS` muda sob movimento do tablet (não só 640×400)

## Residual

- EnableSlot flaky em TCG (AWAITING_HW / WHPX validate)
- IDEA #597/#602 bumps workspace
