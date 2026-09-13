# SESSION_341 — Jarbas Grand Bughunt: alloc fixes no render path

**Data:** 2026-09-13
**Sprint:** s341 (pós-s340)
**Status:** ✅ Implementado + testado

## Escopo

Auditoria profunda do crate jarbas (63 files, 18K LOC): render loop, damage
rects, framebuffer, font rendering, orb/soul_mirror, agent tick, gauges,
compositor, cards, audio.

## Bugs corrigidos

### 1. RENDER_OVERLAYS clone no hot path (compositor.rs:1565)
- **Bug:** `RENDER_OVERLAYS.lock().clone()` allocava um `Vec<RenderOverlay>` a
  cada paint call (60Hz). Cada overlay tem `name: String` + `data: Vec<u8>` —
  2 allocs por clone.
- **Fix:** Itera sob lock direto (`for ov in overlays.iter()`). Zero alloc.
- **Impacto:** elimina ~120 allocs/segundo no render path quando overlays existem.

### 2. HUD_CACHE_STR clone no paint (compositor.rs:1044-1047)
- **Bug:** `HUD_CACHE_STR.lock().clone()` clonava uma `String` a cada paint
  mesmo quando `mem_mb` não mudava. Com 60 paints/segundo = 60 allocs/segundo
  desnecessários.
- **Fix:** Static buffer `[u8; 128]` + `AtomicUsize` para tamanho. String
  gravada uma vez; subsequentes leem o buffer sem alloc.
- **Impacto:** zero allocs no HUD cache path.

## Análise (não fixada, documentada)

- `dirty_orb = true` a cada paint: justificado (orb anima todo frame via TSC)
- `SNAPSHOT.lock().clone()` em gauges: chamado 1×/s (aceitável)
- `MESH_GRAPH` lock durante draw: rápido (<10µs, max 8 peers)
- `prev_win_rects` cap 8: aceitável para uso típico (3-5 janelas)
- `EMBED_MARKS` lock durante draw: aceitável (max 32 marks, draw rápido)

## Validação
- `cargo check --release` — 0 erros
- `cargo test --workspace` — todos passam (jarbas 89/89, hermes 188/188)
