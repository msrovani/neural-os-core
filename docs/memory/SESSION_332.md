# SESSION_332 — Fluidez da UI Jarbas: plano A+B (damage/WC/cursor/BCS) + invalidação + testes — 2026-09-11

**Objetivo:** aumentar a fluidez da tela do Jarbas usando técnicas de outros sistemas (pesquisa externa) e corrigir bugs de UI destraváveis sem HW.

**Fonte:** duas lanes de pesquisa (librarian = survey externo; oracle = fit no nosso código) + implementação em 4 lanes (designer/fixer).

---

## 0. Pesquisa (external → portável)

Survey (LVGL/TouchGFX/emWin, Wayland/X11/Android/DXGI damage, DRM `drmModePageFlip`/fbdev pan, Linux `efifb` WC, DMA2D/PXP→Intel BCS, pixman/embedded-graphics SIMD, DispSync) + assessment no código. **Descoberta-chave:** o renderer não era o gargalo — o limite era o **gate/alvo** (`TARGET_FPS=30` const; bench 112-130 µs/frame vs 33 ms budget).

## 1. Correções destraváveis (sem HW)

- **s332 — Testes host:** `wasmi_rt::run_wasm` resolvia aridade só `[args.len(),0]` → `sandbox_validate_and_run` (0 args) nunca casava `run(a,b)`; corrigido (tenta caller e `0..=4`). `budget_basic` flaky (estado global + testes paralelos) → `GLOBAL_TEST_LOCK`. `session_load_respects_cap` tinha asserção tautológica. `jarbas::soul_*` eram testes stale pós-s319 (hermes::soul canônico). **Suíte de 784/6 → 829/0.**
- **s333 — Invalidação:** `invalidate_windows()` tinha **zero callers** → após o boot, janelas/cards nunca repintavam. Wired em TODOS os mutadores (+ `invalidate_dialog` p/ power). **834/0.**

## 2. Plano de fluidez

### Pacote A (QEMU-testável) — s334
- **#0** `TARGET_FPS` 30→60 (const tunável; gate wall-clock).
- **#1** `DamageList` heapless `[DamageRect;16]` (merge/clip/overflow, zero-alloc) substitui o booleano `need_full` → fim do full swap de 3.6 MB por 1 px; full só na 1ª frame/vcon/power; cada camada empurra rect velho+novo.
- **#4** chrome estático do HUD pré-rasterizado e blitado (~143 KB, invalida por `w/h/bpp/accent`); dinâmico continua desenhado.
- **#5** SSE2 row blitters (`sse2_copy_bytes`/`tint_sse2`) via `#[target_feature]` + gate runtime; fallback escalar; parity exata. **844/0.**

### Pacote B (metal-only) — s335
- **#2** FB **UC → WC** (`init_pat` programa `IA32_PAT` entry 4; `map_page_wc`/`map_page_wc_at`; back buffer fica **WB**) + present com stores NT (`movnti`/`_mm_stream_si64` — `_mm_stream_si128` **não compila** no soft-float) + `sfence`; fallback per-page UC.
- **#3** **Cursor em HW** (Gen9: `CURCNTR` mode `0x27`, `CURBASE`=offset GGTT 32-bit, não PA) — `try_enable_hw_cursor` (gate Intel+engine+BAR0+pin+readback) → `hw_cursor_active()`; quando ativo, UI pula save-under/dirty-rect. **Default OFF**.
- **#6** **BCS blit**: `blit_2d` pinna src/dst na **GGTT** e passa offsets (antes PA/ptr virtual cru); `GLOBAL_NEXT_GTT_INDEX` (pins não colidem); present full-width via BCS **só** com `blit_ready()` (canário), senão CPU/SSE2. `page_flip_hw` **não wired** (unsafe, excluído do plano). **845/0.**

## 3. Estado / pendências

- Suíte host **845 pass / 0 fail**; `cargo nk`/`cargo check --release` = 0 erros; imagem `target/usb_hw.img` regenerada.
- **Validação de metal pendente** para o pacote B (WC/cursor/BCS só medem na 240H; QEMU cai no fallback).
- **#1 USB/mouse no metal** — standby (diagnóstico FB `xHCI cands/stage/ccs` + painel Hub Health prontos p/ ler).
- **#4 licença** AGPL vs MIT — decisão do maintainer (IDEA #555).
- **Stash leftover** `stash@{0}` (branch `cursor/ring3-tcg-accept-s278`, "sse fix stash", arquivos `cortex`/`k_ai`/`k_nano` alheios ao trabalho; quase redundante com HEAD) — deixado no lugar.

## 4. Lições (→ AGENTS.md)

Medir frame cost vs alvo antes de otimizar; damage rects > full swap; FB WC vs UC; `movntdq` não compila no soft-float (usar `movnti`); aceleração HW sempre gated+default-OFF+fallback.
