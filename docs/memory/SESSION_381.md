# SESSION_381 — jarbas bughunt AIOS

**Sprint:** v1.9.99-s381 TEST  
**Data:** 2026-09-21  
**Foco:** honesty UI/áudio/persona + slog ADR-0092 + trim deps  
**Canvas:** [jarbas-bughunt-s381.canvas.tsx](/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/jarbas-bughunt-s381.canvas.tsx)

## Premissas

- Jarbas = R3 display/áudio/persona (ADR-0058 cards; compositor único pintor)
- ADR-0092 sev ∈ {ok,warn,fail,trace} — `info`/bind/offer = TRACE mudo
- ADR-0088: silêncio ≠ áudio fake; barge-in fala ≠ cancel InferQueue
- Soft-float: spin 0.12 / x86_64 0.15 = IDEA #597/#602 (não bump cego)

## Aplicado (ordem)

| ID | Fix |
|----|-----|
| H1 | `read_audio_energy` — sem pulso sintético; silêncio = 0 |
| H2 | slog jarbas → ok/warn/fail (agent, voice, usb, uvc, fb, …) |
| H3 | `request_interrupt` — TTS gen + ring; **sem** `cancel_active` |
| M1 | `pcm_buffer` cap 30 s @16 kHz |
| M2 | `virtio_gpu` sucesso = GOP \|\| kick (não Offer Available) |
| M3 | screensaver smoke PASS/FAIL honesto |
| M4 | remove `heapless` (0 callers); pin `embedded-graphics=0.8.2` |
| L1 | EDID/UVC/VGPU sev contratos; IMG fail sev |

## Verificação

- `cargo test -p jarbas --lib` → **100/100**

## Residual

- EnableSlot flaky TCG (validate WHPX) — SESSION_382
- IDEA #597/#602 spin/x86_64 workspace
# (IDEA #542 ✅ s382)
- heapless 0.9.x unused (removido; não bump)
