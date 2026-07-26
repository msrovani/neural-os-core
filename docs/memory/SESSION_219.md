# SESSION 219 — Fix compilação WM cosmic-like (ADR-0065) no crate jarbas

**Data:** 2026-07-25
**Branch:** (atual)
**Crate:** `jarbas`
**cargo check --release -p jarbas:** 0 errors (de 74 → 0)

---

## Objetivo

Corrigir 74 erros de compilação no crate `jarbas` decorrentes da integração dos novos módulos WM cosmic-like (ADR-0065): `WindowId`, `notifications`, métodos cosméticos do `JarvisDesktop`, circularidade de imports, `PixelColor` de tuples, e adaptação dos padrões Rust `no_std`.

---

## Erros corrigidos (categorias)

### 1. WindowId ausente + import circular
- `tiling.rs` importava `WindowId` de `window.rs` (não existia lá)
- `window.rs` importava `WindowId` de `tiling.rs`
- **Fix:** `WindowId(pub u64)` definido em `tiling.rs`; removido import circular; `window.rs` importa de `tiling` (agora não-cíclico)

### 2. Módulo `notifications` ausente
- `compositor.rs:40` importava `crate::display::notifications::NotificationQueue`
- **Fix:** criado `notifications.rs` com `NotificationQueue` (push/prune/render via `fill_rect` + `draw_text`); adicionado `pub mod notifications` em `mod.rs`

### 3. `AppId` privado no `compositor` module
- `agent.rs` importava `AppId` de `compositor`, mas `compositor` só tinha `use ... AppId` privado
- **Fix:** `pub use crate::display::window::AppId` em `compositor.rs`

### 4. `AppId::WasmSkill` unit vs tuple variant
- `WasmSkill` era `WasmSkill` (unit) mas usado como `WasmSkill(idx)` (tuple)
- **Fix:** alterado para `WasmSkill(usize)` em `window.rs`

### 5. `(u8,u8,u8)` não implementa `PixelColor`
- `dock.rs` passava `theme.bg_alt` (tupla) para `PrimitiveStyle::with_fill()` — exige `PixelColor`
- **Fix:** substituídas todas as chamadas `embedded-graphics` no dock por `fill_rect` nativo do `DoubleBuffer`, eliminando a dependência de `DrawTarget` + `PixelColor` no contexto do dock

### 6. `draw_text` signature mismatch em `dock.rs`
- Chamadas de 6 args `(fb, text, x, y, color, font)` vs assinatura real de 8 args `(fb, x, y, text, scr_w, r, g, b)`
- **Fix:** reordenado args para bater com a assinatura real

### 7. `Theme::cosmic_*()` → `&'static` reference
- `current_theme()` retornava `&Theme::cosmic_dark()` — referência a temporário (E0515)
- **Fix:** `const COSMIC_DARK/COSMIC_LIGHT/HIGH_CONTRAST` como `Theme::new(...)` estático; `current_theme()` retorna ref a esses

### 8. Dracula theme: `(22)` em vez de `(u8,u8,u8)`
- Erro E0308 — `accent_hover` recebeu `(22)` (inteiro) em vez de tupla
- **Fix:** `(200,100,180)`

### 9. Borrow conflicts no `render()` e `spawn_window()` e `toggle_app()`
- `spawn_window()`: workspace `active_mut()` + acesso a `self.workspaces.active` + `window` moved → extrair `dock_app` antes de push
- `render()`: `ws` (immutable) + `self.render_window(win)` (mutable) → coletar `window_updates` em bloco, depois iterar
- `render_window()` em `&mut self` conflita com `&self.windows[idx]` → função livre `draw_window_fb()`
- `toggle_app()`: `iter_mut().find()` + `iter().filter()` → index + count prévio

### 10. `heapless::String` → `alloc::string::String`
- `spawn_window()` usava `heapless::String::from(title).unwrap_or_default()`
- **Fix:** `alloc::string::String::from(title)`

### 11. `String` import ausente em `workspaces.rs`
- `pub name: Option<String>` sem `use alloc::string::String`
- **Fix:** adicionado import

### 12. `FONT_6X10` re-export ausente
- `dock.rs` referenciou `super::font::FONT_6X10` (vem de `embedded-graphics`)
- **Fix:** `pub use embedded_graphics::mono_font::ascii::FONT_6X10` (e `FONT_9X15_BOLD`) em `font.rs`

### 13. `#[derive(Debug)]` ausente em `UiDeclaration` e `Widget`
- **Fix:** adicionado

### 14. Métodos WM faltantes em `JarvisDesktop`
Adicionados:
- `register_app()`, `ensure_hermes_overlay()`, `render_window()`
- `close_tiled_window(WindowId)`, `close_app_window(AppId)`
- `open_power_dialog()`, `close_power_dialog()`
- `spawn_card()`, `card_click()`, `card_drag_step()`, `card_resize_step()`
- `toggle_app()`, `publish_wasm_skill()`

---

## Arquivos modificados/criados

| Arquivo | Ação |
|---------|------|
| `crates/jarbas/src/display/notifications.rs` | **Criado** — NotificationQueue |
| `crates/jarbas/src/display/mod.rs` | Edit — +notifications |
| `crates/jarbas/src/display/tiling.rs` | Edit — WindowId + rm circular import |
| `crates/jarbas/src/display/window.rs` | Edit — WasmSkill(usize) |
| `crates/jarbas/src/display/workspaces.rs` | Edit — String import + re-export FloatingWindow |
| `crates/jarbas/src/display/card.rs` | Edit — Debug derives |
| `crates/jarbas/src/display/font.rs` | Edit — re-export FONT_6X10 |
| `crates/jarbas/src/display/theme.rs` | Edit — Dracula tuple, cosmic consts, current_theme fix |
| `crates/jarbas/src/display/compositor.rs` | Edit — imports, métodos WM, borrow fixes |
| `crates/jarbas/src/display/dock.rs` | Edit — PixelColor elim, draw_text sig |
| `crates/jarbas/src/display/agent.rs` | Edit — imports, close_app_window |

---

## Lições

- **Free function > method** para render com borrow conflitante: `draw_window_fb(fb, win, theme, scr_w)` separa o borrow de fb (mutable) de win (immutable), evitando E0502.
- **Index antes de mutable borrow** é padrão seguro: `self.apps.iter().position(|a| ...)` resolve conflito `iter_mut` + `iter`.
- **const Theme** com `Theme::new(...)` (const fn) evita o `&temporary` pattern de `current_theme()`.
- `(u8,u8,u8)` não implementa `PixelColor` do embedded-graphics — usar `Rgb888::new(r,g,b)` ou pular eg e usar `fill_rect` nativo.
