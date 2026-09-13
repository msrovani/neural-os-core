# SESSION_333 — Fix orb azul-escuro (intrinsics SSE2 no soft-float) + verificação QEMU — 2026-09-11

**Objetivo:** fechar a verificação QEMU da fluidez (o orb não aparecia) e corrigir a causa-raiz.

## 1. Achado (verificação QEMU)

Screenshots do QEMU (TCG, `-vga std` + screendump) mostravam o desktop com fundo navy + HUD, mas **sem o orb ciano** — e a tela estava **estática** (3 screenshots pixel-idênticos). O log mostrava o desktop vivo (`Desktop iniciado`, `orb + HUD`, `policy cost_us=159095` — 159 ms/frame, TTS/metrics ativos).

## 2. Causa-raiz

O orb **renderizava azul-escuro (canal G zerado)**: `sse2_copy_bytes` (`_mm_loadu_si128`/`_mm_storeu_si128` sob `#[target_feature(enable="sse2")]`) é **mal-compilado no target soft-float** (`-C target-feature=-sse2`): LLVM baixa para scalar que **zera o byte 1 de cada u32** (o G no BGRX). Mesma classe do `find_child_byte16_sse` (SESSION_249b). Host tests não pegavam (host tem SSE2 nativo). O `movnti` (GP) do dock estava correto — só o caminho XMM (orb/HUD/chrome cache) corrompia.

## 3. Fix (`crates/jarbas/src/display/fb.rs`)

- `copy_bytes` → `core::ptr::copy_nonoverlapping` (memcpy do compilador, correto).
- `fill_rect_darken_tint` → `tint_swar_scalar` (referência de paridade).
- Mantido `sse2_stream_copy_bytes` (movnti, operando GP — correto).
- `sse2_copy_bytes`/`tint_sse2` ficam só p/ testes host (nativos).

## 4. Verificação QEMU (pós-fix)

- Frame cost **159 → 18 ms**.
- Orb **ciano-azulado `(0,96,176)` com G intacto** (estado Dreaming do sleep-cycle — correto).
- **Animação viva** (screenshots diferem).
- Sem OOM/panic (diskless). jarbas **89 pass / 0 fail**; `cargo nk`/`cargo check --release` = 0 erros.
- 3 testes host novos (render pinta orb ciano; swap_rect aligned/unaligned preserva G).

## 5. Estado

- Commit `d61f1e32` (fix + CHANGELOG s336 + lição AGENTS).
- **Pendências:** #1 USB/mouse (metal), #4 licença, OOM heap-wrap no QEMU **com** disco (pré-existente), stash leftover `stash@{0}` (alheio).