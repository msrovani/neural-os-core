# ADR-0093: Jarbas Crate Optimization — Lock-Free Render, Dirty-Rects, PT-BR TTS

**Status:** Accepted  
**Date:** 2026-08-29  
**Sprint:** v1.9.9 TEST (Jarbas Optimization)  
**Deciders:** Marcelo Scapin Rovani (Lead Architect)

---

## Context

The Jarbas crate (13,288 LOC, 65 files) is the UI/voice layer of Neural OS. A deep audit (docs/evidence/JARBAS_AUDIT.md) revealed:

- **0 host tests** across 13K LOC
- **129 IrqSafeLock acquisitions/frame** at 60Hz (MOUSE_X/Y, POWER_STATE, theme)
- **1M+ pixel background fill** every frame even when only the cursor moves
- **String allocation per frame** for HUD status line
- **No memory caps** on NotificationQueue or ChatWindow (OOM risk)
- **TTS formant synthesis** missing PT-BR phonemes (lh, nh, nasal vowels)

## Decision

### P0-1: 45 Host Tests for jarvis.rs

Added comprehensive test coverage for all engine subsystems:
- EmotionAnalysis (8 tests: empty, joy, sadness, anger, fear, surprise, bounds, dominant)
- SoulProfile (6 tests: default, fluid_update anger/joy/urgency, mode coach/tool, describe)
- SessionHistory (4 tests: push, compress drop_lowest/summarize/merge_similar)
- NotificationGate (5 tests: push/pop, dedup, rate_limit, priority_sort, status)
- DreamEngine, EgoLayer, Heartbeat, ToolState, BabelIndex, ConsentGate, SkillDiscovery, SemanticCache, JarbasEngine (18 tests)

**Evidence:** `cargo test -p jarbas --lib` → 45 PASS

### P0-3: Lock-Free Mouse State

Converted `MOUSE_X: IrqSafeLock<usize>`, `MOUSE_Y: IrqSafeLock<usize>`, `MOUSE_BUTTONS: IrqSafeLock<u8>` to `AtomicUsize`/`AtomicU8`.

**Impact:** ~10 lock acquisitions/frame eliminated (60Hz = 600 locks/sec saved)

### P0-4: Memory Caps

- `NotificationQueue::MAX_QUEUE = 32` — hard cap prevents OOM
- `ChatWindow::MAX_MESSAGES = 100, MAX_TIMELINE = 64` — with `trim_caps()` enforcement

### P1-1: HUD String Cache

`HUD_CACHE_MEM` + `HUD_CACHE_STR`: `hud_line()` recomputes only when `mem_mb` changes, eliminating per-frame `alloc::format!()`.

### P1-2: Dirty-Rect Gating

Background fill + orb + mesh now gated by dirty flags:
```rust
let need_bg = self.dirty_orb || self.dirty_mesh || self.dirty_windows || self.dirty_hud;
if need_bg {
    self.fb.fill_rect(0, 0, w, h, theme.bg.0, theme.bg.1, theme.bg.2);
}
```

**Impact:** When only cursor moves, skips 1,024,000 pixel writes/frame.

### P1-3: mesh_health_json Safety

Verified parser has bounds checks on all loops (`i < len`) and `parse().unwrap_or(0)` on all conversions. No panic path on malformed JSON.

### P1-4: PT-BR TTS Phonemes

8 new phonemes added to formant synthesis:
| Phoneme | IPA | Example |
|---------|-----|---------|
| `lh` | /ʎ/ | dinhe**lh**o |
| `nh` | /ɲ/ | **nh**oque |
| `ao` | /ãw̃/ | m**ão** |
| `rx` | /ɾ/ | ca**r**o |
| `rj` | /ʁ/ | po**r**ta |
| `gn` | /ɲ/ | conta**g**em |
| `sx` | /s/ | e**x**ame |
| `zx` | /z/ | e**x**emplo |

`text_to_phonemes()` updated to detect nasal context (vowel+m/n → nasal vowel) and preserve accented characters.

### P1-5: Lock-Free Theme Engine

`THEME_MODE: Mutex<ThemeMode>` → `THEME_MODE_ATOMIC: AtomicU8`. `current_theme()` now inline, lock-free, ~0ns.

**Impact:** ~30 lock acquisitions/frame eliminated.

## Consequences

### Positive
- 45 host tests catch regressions early
- ~40 lock acquisitions/frame eliminated
- ~1M pixel writes/frame saved when only cursor moves
- OOM prevention via hard caps
- PT-BR TTS more natural with nasal vowels and palatal sounds

### Negative
- Theme mode changes are now eventual (AtomicU8, not Mutex) — acceptable since theme changes are rare and user-initiated
- HUD cache only invalidates on `mem_mb` change — if `net` status changes frequently, cache may be stale (mitigated: net changes are rare)

### Risks
- Dirty-rect gating may cause visual artifacts if a layer is invalidated but not redrawn — mitigated by `invalidate_all()` on boot/theme change
- PT-BR phoneme mapping is simplified (context-free) — full PT-BR TTS requires Piper/VITS (already wired)

## References

- docs/evidence/JARBAS_AUDIT.md — full audit
- docs/evidence/K_HAL_AUDIT.md — k_hal audit (parallel)
- docs/evidence/K_NANO_AUDIT.md — k_nano audit (parallel)
- AGENTS.md — project conventions
- ADR-0058 — Generative Card Desktop
- ADR-0065 — Cosmic Window Manager
