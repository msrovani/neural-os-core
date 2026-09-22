# SESSION_391 — Desktop / UI / Orb bughunt (Jarbas compositor)

**Foco:** Premissas ADR-0058/0090/0093/0092/0088 → deep bughunt High→Low → canvas → apply all → pós-tarefas.

## Premissas

- ADR-0058: Cards DrawTarget + UiDeclaration — paint só em `render()`
- ADR-0093: lock-free / dirty-rects / sem alloc hot path
- ADR-0090: Desktop v2 — claim vs reality (dock Tier1.4 stale; Tier4 residual)
- ADR-0092: slog sev `ok|warn|fail|trace`
- ADR-0088: AIOS — UI é agente; política fora do compositor
- S261/294: `render()` único pintor; fill scanline; TARGET wall-clock 60 fps

## Upstream

- `embedded-graphics` **0.8.2** = `max_stable` crates.io (2026-02) — sem bump
- Hot path fill/glow/dock já OK desde SESSION_294

## Achados → fixes (High→Low)

| Sev | ID | Achado | Fix |
|-----|-----|--------|-----|
| HIGH | JD-01 | `/theme` → ACTIVE_THEME; paint via COSMIC_* | `USE_NAMED_THEME` + `current_theme` unificado; COSMIC_DARK navy |
| MED | JD-02 | `snapshot()` clone String no HUD | `core_bar_data()` Copy `[f32;32]+u8` |
| MED | JD-03 | `hit_test_hover` morto | DisplayAgent mouse move → `hit_test_hover` |
| MED | JD-04 | Chat fantasma 35% left | hover só se `AppId::HermesChat` visible |
| MED | JD-05 | BAT/VOL/NET placeholders | omitir; NET só se mesh vivo |
| MED | JD-06 | slog TTS-FB/POWER/HITL Trace | sev `ok`/`warn` ADR-0092 |
| MED | JD-07 | BG hardcoded `(8,12,24)` | `theme.bg` no clear orb/HUD/gauges |
| MED | JD-08 | `client_buffer` fill `aw/4` | loop `0..aw` (SESSION_294 class) |
| MED | JD-09 | mesh `Vec::collect` no orb | stack `[MeshPeerNode;16]` |
| LOW | JD-10 | Tier4 gpu_compositor/edid | honesty residual nos headers |
| LOW | JD-11 | DockItem hover sempre 0 | `dock.hit_test` → idx |
| LOW | JD-14 | `format!` clock no paint | buf `[u8;8]` stack |
| LOW | JD-15 | ADR-0090 §3.4 dock morto | patch Tier1.4 ✅ DONE |

## Gates

- `cargo check -p jarbas --release` → 0 erros
- `cargo check -p neural-kernel --release` → 0 erros
- `cargo test -p jarbas --lib -- theme::` → **5 pass**
- `cargo test -p jarbas --lib -- clock_tests` → pass

## Canvas

[`desktop-ui-orb-bughunt-s391.canvas.tsx`](../../../Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/desktop-ui-orb-bughunt-s391.canvas.tsx)

## Residuais

- Tier4 client buffers / GPU compositing end-to-end em HW (`present_frame` BCS parcial)
- EDID I2C producer AWAITING_HW
- Waveform removido s328 — honesty; não reinventar stub
- Optimizer UI polish / hover highlight visual no dock item
