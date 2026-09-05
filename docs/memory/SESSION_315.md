# SESSION 315 — Destravar Jarbas UI (orb / mouse / relógio / voz)

**Data:** 2026-09-05  
**Escopo:** Desktop metal congelado (orb, mouse, relógio 00:00, voz/mic).  
**Não abre:** Ring3, 0103 S2+, Falcon3 sprint.

## Sintoma

Alienware chega ao desktop (SESSION_313/314) mas UI morta: relógio zerado,
mouse parado, orb sem animação. Interface canônica = orb + som + mic.

## Causas

1. **`hlt` eterno** se IRQ timer não avança (LAPIC one-shot / PIC) → scheduler
   para após 1 round → Display não pinta de novo.
2. **HID deferido exigia `USB_MSC`** e marcava done no skip → live USB sem MSC
   **nunca** fazia P24a/b (mouse/teclado USB mortos).
3. **PIC fallback mascarava IRQ12** (`0xFF` no slave) no path `init_pic_fallback_and_sti`.
4. Voz/mic Continuous sem `set_urgency` → rate-limit sob pressão.

## Fixes

- `k_nano::interrupts::scheduler_idle_halt` + `SOFT_TIMER_TICKS` / `wall_ticks()`
- Dock + gauges usam `wall_ticks()`
- `try_deferred_hid_bringup`: sem exigir MSC; só após `ui_is_live`; retry T+120
- PIC slave mask `0xEF` (IRQ12)
- Urgency: `jarvis_voice`/`wakeword`/`audio_pipeline`/`audio_mixer`/`JARBAS`

## Aceite

Rebuild `usb_hw.img` + flash: orb anima, relógio >00:00, mouse move, mic/TTS
não congelam o scheduler.

## SESSION_315b — Tela preta pós-Limine (Alienware)

**Sintoma:** Limine OK → tela preta (sem desktop).

**Hipótese:** early hub→MSC EP0/`wait_transfer_event`/`host_reset_port` sem teto
TSC = hang longo com `boot_ckpt` só em ramlog (FB mudo).

**Fix:** budget 3s no MSC bringup; EP0/reset com teto TSC 50–100ms; `boot_progress_line`
visível no FB em cada passo early USB. Imagem regenerada pós-fix.
