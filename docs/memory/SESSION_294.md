# SESSION_294 — Desktop Jarbas lento: hot path do compositor

**Sprint:** v1.9.99-s294 TEST  
**Data:** 2026-08-29  
**Objetivo:** Desktop UI no QEMU WHPX estava “muito lenta”; o compositor faminto o scheduler (serial congelava, mouse parecia morto).  
**ADR:** — (fix pontual, ADR-0058 compositor). Roadmap `0090-jarbas-desktop-v2-roadmap.md` afirmava glow integer-only / `fill_rect_fast` bulk — **não era verdade no código**.

## Causa raiz (medida no hot path)

`JarbasDesktop::render()` a cada tick do DisplayAgent:

1. **`DoubleBuffer::fill_rect`** chamava `set_pixel` por pixel. Clear 1280×800 ≈ 1M writes/frame.
2. **`fill_rect_fast` bpp=4** loopava `dx in 0..(aw/4)` → pintava só **25%** do retângulo. Waveform usava o “fast”; o resto da UI usava o `fill_rect` lento.
3. **`fill_circle_glow`:** O(r²) `libm::sqrtf` + `set_pixel`. Soul Mirror ambient `r ≈ 0.15*min(w,h)*2.2 ≈ 264` → ~280k sqrts/frame + até 6 anéis.
4. **`TARGET_FRAME_TICKS = 3`** com comentário “~60 FPS @ 5ms/tick”. PIT é **~18.2 Hz (~55 ms)**. Gate → **~6 FPS**, e cada frame era tão caro que o serial parava de crescer.
5. Grid 2×2 + partículas `sinf`/`cosf` + dock pintado **duas vezes** + disco glow 1.5× extra.
6. `mouse_log_status("display_tick")` a cada tick no serial (I/O extra).

Dirty flags existiam mas **nunca eram limpos** e o orb anima todo frame → early-exit morto. Não é o bug principal; o custo por frame era.

## Fix (jarbas)

| Arquivo | Mudança |
|---|---|
| `display/fb.rs` | `fill_rect` → `fill_rect_fast`; bpp=4 loop `0..aw`; linhas ≥16 *doubling memcpy*; `fill_circle_glow` scanline + `isqrt_u64`; `swap` = `copy_nonoverlapping` |
| `display/compositor.rs` | `TARGET_FRAME_TICKS = 1` (honesto ~18 Hz); sem grid/partículas; dock uma vez (Z-order por cima das janelas) |
| `display/soul_mirror.rs` | ambient 1.35× (era 2.2); max 2 rings; sem glow 1.5×; arco capped 16 passos |
| `display/agent.rs` | `mouse_log_status("display_tick")` só `sn < 2` |

`cargo check --release -p jarbas --target x86_64-unknown-none` 0 erros. ELF empacotado em `target/uefi.img` via `python tools/limine/mk_esp_fat.py` (não `python3`).

## Lições

1. **PIT ≠ 5 ms.** Comentário “60 FPS” com `TARGET_FRAME_TICKS=3` era doutrina falsa. Tick canónico ≈ 55 ms → 1 frame/tick ≈ 18 Hz.
2. **`fill_rect_fast` com `aw/4`** é bug silencioso (UI “funciona” com retângulos listrados / ¼ da área). Loop = pixels, não dwords-de-dwords.
3. **Glow O(r²) sqrtf** no orb central mata o BSP inteiro. Disco = scanline + isqrt; empilhar discos, não blend por pixel.
4. **`cargo build -p boot` exit 0 + `ESP image creation failed`:** Windows Store `python3` “existe” e o `build.rs` falha o `mk_esp_fat`. Usar **`python`**. Guest QEMU já aberto **não** pega ELF novo.
5. **`CARGO_TARGET_DIR` sandbox** (Cursor) ≠ `target/` do repo. `cargo nk` relinka noutro sítio; copiar o ELF à mão para `limine-esp-tree/kernel.elf`.
6. **Mouse `pos=640x400` `aux=0`** após NSGDB ingest é **outro** bug (HID tablet/xhci), não o compositor. Serial A/B 8 min sem crescimento no kernel antigo.

## Residual

- USB tablet QEMU: `MOUSE_ABS_*` não mexe (IDEA #542).
- Dirty-region swap / glyph blit (ADR-0090 Tier 1) ainda abertos.
- Dirty flags do compositor ainda não limpam após frame.

## Evidência

- QEMU WHPX 6G smp 4, duas instâncias: boot 0–8 + Runtime + `desktop_ready`; serial congelava pós-ingest no kernel pré-fix.
- Monitor 8 min: A=B=10046 B, `pos=640x400` constante.
