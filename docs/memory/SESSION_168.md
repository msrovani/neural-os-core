# SESSION_168 — Display Splash Persistente (Tela Preta pós-claim_graphics)

**Data:** 2026-07-22
**Contexto:** Sprint v1.9.5 TEST — LLM carregando + gerando saudação, tela preta no QEMU
**Tag:** `v1.9.7`

---

## Problema

Após a P4 demo desenhar checker+splash, o `DisplayAgent::tick()` chama `claim_graphics()`, que limpa o framebuffer (`clear_fb_pixels`). O compositor só renderiza no **segundo** tick do DisplayAgent. O LLM leva ~6341 ticks para gerar a saudação — nesse intervalo, a tela fica preta.

Log:
```
[T+1645] DENY MAP_FB held=0x0    ← P4 demo testando Cap::EMPTY (esperado)
[T+1648] SUCCESS MAP_FB+AS+present count=1 (checker cleared+splash)
...
[T+2234] LLM BPE loaded (tokens=0, merges=0)
[T+2425] GEN: LLM forward inicio...
[T+8763] GEN prompt fwd: 6341 ticks
```

## Root Cause

O `claim_graphics()` em `crates/jarbas/src/display/fb.rs:27` chama `clear_fb_pixels()` que zera o FB. O splash da P4 demo é apagado, e nada é redesenhado até o segundo tick do DisplayAgent (`agent.rs:623: desktop.render()`).

Os `DENY MAP_FB/WRITE_FB` vistos no log são do **demo P4** testando `Cap::EMPTY` (`jarbas_fb.rs:267,288`) — **não bloqueiam** o DisplayAgent, que escreve direto no FB via `write_volatile`, bypassando o CapGate.

## Fix

**1 arquivo, 14 linhas:** `crates/jarbas/src/display/fb.rs:34-47`

Após `clear_fb_pixels()`, desenha splash text "Neural OS Core — Inicializando..." centralizado via `splash_draw_text()` (reusa font 8x16 existente). O texto fica visível até o compositor assumir no segundo tick.

```rust
let msg = "Neural OS Core — Inicializando...";
let x0 = (gpu.fb_width as usize).saturating_sub(msg.len() * 8) / 2;
let y0 = gpu.fb_height as usize / 2 - 8;
splash_draw_text(..., x0, y0, msg);
```

## Lições

1. **`claim_graphics()` não é o vilão** — o clear é intencional (remove resíduos K*/TRACE). O problema é o gap entre clear e primeira renderização do compositor.
2. **CapGate não bloqueia DisplayAgent** — o DisplayAgent (Ring 0) escreve direto no FB via endereço virtual do kernel. CapGate só vale para WASM sandbox e syscall `int 0x90`.
3. **`splash_draw_text()` reusa font 8x16 existente** — sem alocação extra, sem dep novo.
4. **Ponytail pattern:** fix de 14 linhas num arquivo, reusa função existente, sem mudança arquitetural.

## Arquivos Alterados

| Arquivo | Mudança |
|---------|---------|
| `crates/jarbas/src/display/fb.rs:34-47` | Splash text após `clear_fb_pixels()` em `claim_graphics()` |
