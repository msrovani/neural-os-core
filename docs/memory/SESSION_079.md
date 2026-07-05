# SESSION_079 — Sprint 79: LLM Infrastructure + Display Xuvisco Fix

**Data:** 2026-07-04 | **Sprint:** 79 — Bloco 23 | **v0.79.0–v0.79.1**

## Objective
Integrate Microsoft BitNet-b1.58-2B-4T (real: 850M params) as primary Cortex LLM — RMSNorm vetorial, BPE tokenizer, AVX2 ternary matmul, MoE Trinity Router stub, QEMU loader boot pipeline.

## New Files
- **`bitnet_avx2.rs`** — AVX2 ternary matmul kernel (`ternary_matmul()`) with scalar fallback. Unpack i8 → `_mm256_cvtepi8_epi32` → `_mm256_cvtepi32_ps` → FMA.
- **`trinity.rs`** — `TrinityRouter` MoE stub: `register_expert()` + `classify_intent()` rule-based (5 classes).
- **`bpe.rs`** — `BpeTokenizer` with JSON parser for HuggingFace `tokenizer.json`. `encode()`/`decode()`/`init_from_json()` globals.

## Modified Files
- **`cortex.rs`** (+22/−7): `vocab_size` u16→u32. BPE init inside `load_model()`. Dynamic `TransformerModel` fields (`hidden`, `num_layers`, `max_seq`, `vocab_size: u32`). `LayerWeights.rms_attn/rms_ffn` como `Vec<f32>`.
- **`gguf.rs`** (+6/−3): `vocab_size()` returns `u32`.
- **`main.rs`** (+24/−1): `mod bitnet_avx2`, `mod trinity`. Ramdisk loading + QEMU loader at phys `0x100000000` (4GB).
- **`tensor.rs`** (+0/−22): `matmul_hybrid()` → `bitnet_avx2::ternary_matmul()`.
- **`download_bitnet.py`** (+15/−4): Header `.bitnet` v2 fixed: u32 vocab, ffn_dim, BPE extract.
- **`build_image.py`** (+0/−31): BIOS-only mode to avoid UEFI build error.

## Model
- **Downloaded:** `microsoft/BitNet-b1.58-2B-4T` (real: 850M params, 849,787,090)
- **Architecture:** hidden=2560, layers=30, heads=20 (GQA=5 KV), vocab=128256, intermediate=6912, tie_word_embeddings=true
- **.bitnet:** 1,464 MB (2-bit packed ternary), magic `0xBE11BE11` v2 header
- **Blocker:** BitFFN grouped down_proj (640→6912) + GQA — forward pass incorreto até Sprint 80
- **micro.bitnet:** 71KB synthetic model mantido como fallback

## Boot Pipeline
- **Ramdisk via bootloader:** FALHA — FAT partition autosized ~64MB insuficiente para 1.46GB
- **QEMU loader (`-device loader`):** Funciona com `-m 6G` + WHPX + addr=0x100000000 (4GB)
- **WHPX + 6G + loader:** Boot OK ~30s (lento devido overhead 1.5GB)
- **2G + loader:** FAIL — modelo em 512MB conflita com alocador de frames do bootloader

## Key Decisions
- `.bitnet` v2: `vocab_size` como u32 (128K > 65535), `ffn_dim` adicionado, struct.pack corrigido
- BPE init automático dentro de `load_model()` — sem chamada externa
- AVX2: i8→f32 por peso (correto, lento). Otimização futura com bit masks.
- Trinity Router: stub rule-based. ML router adiado.
- Model loading: QEMU loader em 4GB evita conflito com alocação precoce.

## Blockers
1. GQA + BitFFN grouped projections — forward pass quebrado (Sprint 80)
2. Build_image.py: UEFI bootloader compile error (serde build panic) — BIOS-only com `default-features=false` precisa verificação offline
3. QEMU loader ~30s overhead — aceitável para dev

## Next Steps
1. Forward pass fix: GQA support + BitFFN grouped projections
2. Verificar cargo build + boot image
3. QEMU boot test: WHPX + 6G + loader at 4GB, provar pipeline
4. Sprint 80: JARVIS Persona (SOUL.md, IPW, Compression, Notification Gate)

---

## v0.79.1 — Display Xuvisco Fix (2026-07-05)

### Problem
Between bootloader `Jumping to kernel entry` and the first `println!` (RTL8139 detect), the display shows garbage artifacts and the line `[BOOT] FB ativo — VGA text mode desligado` was deceptive — no actual VGA disable occurred.

### Root Cause Analysis
Two display layers were active and dirty during transition:

1. **VGA text buffer (0xB8000)** never cleared — bootloader left its last messages there. On QEMU `-vga std`, the VGA text overlays the GOP framebuffer causing "texto pula pro topo". On Intel 6xx, the VGA plane still active corrupts the display pipe (xuvisco).

2. **UEFI GOP framebuffer** never cleared — bootloader artifacts persisted until the first framebuffer write (`println!` at RTL8139 detection). 27+ serial-only log lines scrolled with no display update.

3. **`hide_cursor()` and `clear_vga_buffer()`** were defined in `vga_buffer.rs` but **NEVER called** anywhere in the codebase — both orphaned since Sprint 71 when CRTC access was banned.

4. **Previous fix (Sprint 71):** Moved `probe_uefi_framebuffer()` before `vga_buffer::init()` to avoid CRTC writes on Intel 6xx. However, it went too far — never touched VGA CRTC at all, even to clear the text buffer.

### Fix
Three changes, ~10 LOC total:

**`fb.rs:71-75`** — Clear framebuffer to black immediately after GOP probe:
```rust
let fb_size = gpu.fb_height as usize * gpu.fb_stride as usize;
if fb_size > 0 {
    unsafe { core::ptr::write_bytes(gpu.fb_addr as *mut u8, 0x00, fb_size); }
}
```

**`vga_buffer.rs:14-19`** — New `clear_physical_buffer()`:
```rust
pub fn clear_physical_buffer(phys_offset: u64) {
    let vga = (0xB8000 + phys_offset) as *mut u8;
    unsafe { core::ptr::write_bytes(vga, 0x00, 4000); }
}
```
Writes 4000 zeros directly to VGA text memory — **no CRTC I/O**. Safe for Intel 6xx.

**`main.rs:448`** — Call in framebuffer branch:
```rust
vga_buffer::clear_physical_buffer(pm_offset);
```

### What was NOT changed
- `hide_cursor()` remains unused (writes to CRTC 0x3D4/0x3D5 — unsafe on Intel 6xx)
- No CRTC registers touched — zero xuvisco risk
- Boot order preserved: framebuffer probe first, VGA init only in `else` branch

### Files Changed
| File | Change |
|---|---|
| `crates/neural-kernel/src/display/fb.rs` | +7 LOC — framebuffer clear after probe |
| `crates/neural-kernel/src/vga_buffer.rs` | +7 LOC — new `clear_physical_buffer()` |
| `crates/neural-kernel/src/main.rs` | +1 LOC — call `clear_physical_buffer()` |

### Verification
`cargo check --release`: **0 errors.** Expected warnings: 73 (dead code per policy).

### Key Lesson
"When printing 'X is disabled', actually disable X." The root cause was not a complicated hardware quirk — it was a misleading log message that created a false sense of correctness for 4 sprints.

---

## Appendix: BitNet-b1.58 Real Architecture
| Field | Value |
|---|---|
| model_type | bitnet_b1_58 |
| hidden_size | 2560 |
| num_layers | 30 |
| num_heads | 20 |
| num_kv_heads | 5 (GQA) |
| vocab_size | 128256 |
| max_seq | 2048 |
| intermediate_size | 6912 |
| ffn_dim | 1728 |
| tie_emb | true |
| total_params | 849,787,090 |
| .bitnet size | 1,535,378,211 bytes (1,464 MB) |
