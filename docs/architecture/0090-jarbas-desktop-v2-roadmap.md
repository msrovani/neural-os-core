# ADR-0090: Jarbas Desktop v2.0 — Plano Completo de Implementação

**Status:** PROPOSTA  
**Autor:** Buffy (AI Agent) + Marcelo Scapin Rovani  
**Data:** 2026-08-22  
**Sprint:** 110–115 (estimado)  
**Dependências:** ADR-0058 (Generative Card), ADR-0065 (WM Cosmic), ADR-0081 (Mesh P2P)

---

## 1. Contexto

O desktop JARVIS (Jarbas) atingiu Fase 1+2 de otimização **no papel** (`fill_circle_glow` integer-only,
dirty tracking por-layer, `fill_rect_fast` bulk, LUT alpha blending). **SESSION_294 mediu o contrário:**
`fill_rect` ainda era `set_pixel`/pixel; `fill_rect_fast` bpp=4 loopava `aw/4` (25% do rect);
`fill_circle_glow` ainda era O(r²) `sqrtf`. Isso foi corrigido (scanline + isqrt + doubling memcpy;
`TARGET_FRAME_TICKS=1`; grid/partículas removidos do hot path; dock 1×). Tier 1 restante:
glyph blit, dirty-region swap. Comparação com Redox OS (Orbital), COSMIC, e macOS revela gaps
em qualidade visual e funcionalidade. Este ADR define o roadmap completo para elevar o desktop a
qualidade "production-grade".

**Estado atual (v1.9.9):** ~7.200 LOC em `jarbas/src/display/`, orb Soul Mirror, WM Cosmic
tiling, cards ADR-0058, mesh P2P visualization, chat window, 5 themes, dirty tracking por-layer.

---

## 2. Decisão

Implementar em 4 Tiers (15 features), priorizados por impacto/custo:

| Tier | Features | Custo Total | Impacto |
|------|----------|-------------|---------|
| **1 — Quick Wins** | 4 features | ~4 dias | Alto (performance 3-5x) |
| **2 — Polish** | 4 features | ~12 dias | Médio-alto (UX) |
| **3 — Desktop Real** | 4 features | ~20 dias | Alto (funcionalidade) |
| **4 — Transformacional** | 3 features | ~30 dias | Revolucionário |

**Total estimado:** ~66 dias de engenharia (13 semanas).

---

## 3. TIER 1 — Quick Wins (Sprint 110, ~4 dias)

### 3.1 Glyph Cache (font.rs) — 1 dia

**Problema:** `draw_text_scaled` itera cada pixel do glifo8x16 via `set_pixel`.
Para "JARBAS" (6 chars) = 6×16×8 = 768 chamadas set_pixel com bounds check.

**Solução:** Pré-rasterizar os 95 glyphs ASCII em `[(u8, [u32; 16]); 95]` — cada glyph
é 16 linhas de 8 pixels = 16×u32 (1 bit/pixel = 32 bytes/glyph, 3KB total).
Blit via `u32::leading_zeros` para packing.

**Arquivos:**
- `crates/jarbas/src/display/font.rs` — adicionar `GLYPH_BLIT: [[u32; 16]; 95]`
- `crates/jarbas/src/display/font.rs` — nova `draw_text_blit()` que usa blit table
- `crates/jarbas/src/display/compositor.rs` — migrar `draw_text()` → `draw_text_blit()`

**Validação:** `cargo check -p jarbas` 0 erros. Benchmark: contagem de `set_pixel` calls
antes vs depois para string "JARBAS 128MB idle NET" (28 chars).

**LOC:** +80 (glyph table) +40 (blit function) = ~120 LOC.

---

### 3.2 Grid Pre-Render (compositor.rs) — 0.5 dia

**Problema:** 260 `fill_rect_fast(2×2)` por frame para grid dots, mesmo sem mudança.

**Solução:** Renderizar grid uma vez no init em buffer estático `[u8; STRIDE * H]`.
A cada frame, `memcpy` do buffer estático para back buffer (parcial, só área visível).

**Arquivos:**
- `crates/jarbas/src/display/compositor.rs` — `static GRID_BUF: Option<Vec<u8>>`
- `crates/jarbas/src/display/compositor.rs` — `init_grid_buffer()` + `blit_grid()`

**Validação:** `cargo check -p jarbas` 0 erros. Comparar ticks/frame antes vs depois.

**LOC:** +40.

---

### 3.3 Lookup Table Seno (fb.rs) — 0.5 dia

**Problema:** 24 chamadas `sinf`/`cosf` por frame nas ambient particles.

**Solução:** Tabela estática `SIN_LUT: [i8; 256]` (index circular `tick % 256`).
Particles usam `SIN_LUT[(tick.wrapping_add(seed) % 256) as usize]` em vez de `sinf`.

**Arquivos:**
- `crates/jarbas/src/display/fb.rs` — `const SIN_LUT: [i8; 256]` (gerada por script)
- `crates/jarbas/src/display/compositor.rs` — `draw_ambient_particles()` usa LUT

**Validação:** `cargo check -p jarbas` 0 erros. Self-test: comparar saída sinf vs LUT.

**LOC:** +30.

---

### 3.4 Dock Rendering (compositor.rs + dock.rs) — 2 dias

**Problema:** `Dock` tem lógica completa (`dock.rs` 123 LOC) mas `compositor.rs` NUNCA
chama `dock.render()`. O dock está "morto" — launchers registrados mas invisíveis.

**Solução:**
1. Integrar `dock.render()` no compositor como CAMADA 2.5 (entre HUD e Windows)
2. Adicionar hover detection no mouse_move (highlight no item sob o cursor)
3. Adicionar click handling (toggle_app ao clicar no launcher)

**Arquivos:**
- `crates/jarbas/src/display/dock.rs` — implementar `render()` real (hover highlight)
- `crates/jarbas/src/display/compositor.rs` — chamar `dock.render()` após HUD
- `crates/jarbas/src/display/agent.rs` — `handle_pointer_click()` detecta dock hit

**Validação:** QEMU boot → dock visível na parte inferior → clique abre/fecha janela.

**LOC:** +120 (dock.render) +30 (compositor integration) +40 (click handling) = ~190 LOC.

---

## 4. TIER 2 — Polish (Sprint 111–112, ~12 dias)

### 4.1 Window Animations (compositor.rs) — 3 dias

**Problema:** Janelas aparecem/desaparecem instantaneamente (snap). Sem feedback visual.

**Solução:** `AnimationState` por janela: `scale: f32` de 0.0→1.0 em 8 frames (ease-out).
Abertura: scale 0→1 + alpha 0→255. Fecho: scale 1→0 + alpha 255→0.

**Arquivos:**
- `crates/jarbas/src/display/window.rs` — adicionar `pub anim: AnimationState`
- `crates/jarbas/src/display/compositor.rs` — `render_app_content()` aplica scale transform
- `crates/jarbas/src/display/compositor.rs` — easing function `ease_out_cubic(t: f32) -> f32`

**Validação:** QEMU → abrir janela via dock → animação de scale suave.

**LOC:** +80 (AnimationState) +60 (render transform) +20 (easing) = ~160 LOC.

---

### 4.2 Chat Scrollback (chat_window.rs) — 2 dias

**Problema:** ChatWindow não tem scroll — mensagens antigas somem quando buffer enche.

**Solução:** `scroll_offset: usize` — mouse wheel ajusta offset. Renderiza
`messages[scroll_offset..scroll_offset+visible_lines]`.

**Arquivos:**
- `crates/jarbas/src/display/chat_window.rs` — adicionar `scroll_offset` + wheel handler
- `crates/jarbas/src/display/agent.rs` — mouse wheel event → `chat.scroll_offset += delta`

**Validação:** QEMU → enviar múltiplas mensagens → scroll com mouse wheel.

**LOC:** +60.

---

### 4.3 Hover States (compositor.rs + agent.rs) — 2 dias

**Problema:** Botões (OFF, dock, cards) não reagem a hover — sem feedback visual.

**Solução:** `hover_target: Option<HitZone>` no compositor. A cada mouse_move,
hit-test contra rects conhecidos. Se hover muda → invalidar camada afetada.

**Arquivos:**
- `crates/jarbas/src/display/compositor.rs` — `enum HitZone { Power, Dock(usize), Card(u32, usize), None }`
- `crates/jarbas/src/display/compositor.rs` — `hit_test_hover(mx, my) -> HitZone`
- `crates/jarbas/src/display/agent.rs` — chamar `hit_test_hover` no mouse_move
- `crates/jarbas/src/display/decorations.rs` — `draw_rounded_rect_hover()` (brighter border)

**Validação:** QEMU → mouse sobre botão OFF → cor muda.

**LOC:** +80.

---

### 4.4 Voice Waveform (compositor.rs + audio) — 5 dias

**Problema:** Orb reage a `fft_energy` (1 float), mas sem visualização de áudio.

**Solução:** 32 barras de waveform no canto inferior. Dados vêm de `audio::read_fft_bins()`
(já existe FFT 16 bins em `avatar.rs`). Expandir para 32 bins via interpolação.

**Arquivos:**
- `crates/jarbas/src/display/compositor.rs` — `draw_waveform()` (32 barras, camada 2.8)
- `crates/jarbas/src/audio/mod.rs` — `pub fn read_fft_bins_32() -> [f32; 32]` (interpolação de 16→32)
- `crates/jarbas/src/display/gauges.rs` — integrar waveform no HUD

**Validação:** QEMU → áudio playing → barras animadas no canto inferior.

**LOC:** +100 (waveform render) +30 (FFT interpolation) = ~130 LOC.

---

## 5. TIER 3 — Desktop Real (Sprint 113–114, ~20 dias)

### 5.1 Per-Window Back Buffers (compositor.rs + window.rs) — 8 dias

**Problema:** Single back buffer — qualquer mudança redesenha tudo. Sem isolamento entre janelas.

**Solução:** Cada `Window` ganha `Option<Vec<u8>>` como buffer próprio. Compositor faz
blit por região clipada (só pixels da janela visíveis). Damage tracking por janela.

**Arquitetura:**
```
Window {
    buffer: Option<Vec<u8>>,    // per-window back buffer
    dirty: bool,                // window-level dirty
    clip_rect: Rect,            // visível na screen
}

Compositor:
  1. fill background (sempre)
  2. orb (se dirty_orb)
  3. para cada window dirty:
     a. window.render_to_buffer()  // desenha no buffer próprio
     b. blit(buffer → back_buffer, clip_rect)  // só pixels visíveis
  4. HUD (se 
