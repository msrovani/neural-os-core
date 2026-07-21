# SESSION_163 — Emagrecer neural-kernel (cutover seguro, zero perda) + ADR-0057/0058

**Data:** 2026-07-19 (Emagrecer) / 2026-07-21 (ADR-0057/0058)  
**Release:** **v1.9.5 TEST** (não v2.0.0)  

---

## Parte 1: Emagrecer neural-kernel (Ondas 0–6)

**Escopo:** Cutover cirúrgico bin→crates — inventário + stubs + promotes.

### Onda 0
- Script: `tools/diff_bin_crate.py` (`--markdown`, `--onda N`, `--strict`)
- Tabela: `docs/memory/BIN_CRATE_DIFF.md`
- Regra: `bin_ahead` → promover antes de stub

### Onda 1 (k_nano baixo risco)
Stubs: `sync`, `hw_rng`, `tpm`, `slip`, `dma`, `slab`, `io_scheduler`.

### Onda 2 (k_ai thin)
Stubs: `conversation`, `chunker`, `usage`, `profile`, `cognitive`, `training_agent`.  
Split: `shutdown` (HW no bin). Adiados: `boot_log_agent`, `memory_agent` (VRAM), `gguf`.

### Onda 3 (pci / USB)
- `pci` stub + `read_config_word` → `pub` em k_nano  
- Stubs: `simd`, `xhci`, `rtl8139`, `ahci`, `hw_agents`  

### Onda 4 (disco)
- Promovidos bin→k_nano: `fat32`, `ata` (probe exFAT), `e1000` (`prove_rx`, `read32` pub)  
- Um `ATA_DRIVER`: `pub use k_nano::ATA_DRIVER` (mirror removido)

### Onda 5 (plataforma)
- `acpi` stub; `TIMER_TICKS` + `MOUSE_ABS_*` canônicos k_nano; `apic` stub

### Onda 6 (residuals)
- `global_arena` promovido → cortex; stub no bin  
- Mantidos no bin (honesto): `cortex.rs`, `bpe`, `model_hub`, `agents`, `net*`, `audio/*`

**Gate:** `cargo nk` = 0 erros.

---

## Parte 2: ADR-0057 (Compute Dispatch) + ADR-0058 (Card Desktop)

### 1. ADR-0057 — Compute Dispatch SMP+GPU+NPU

**Causa-raiz do não-wake:** SIPI broadcast + stack compartilhada → ≥2 APs corrompem stack. Fix: IPI direcionado + wake sequencial + stack/PerCpu por-AP + retry 3x.

- **WS-A ✅:** `-smp 4` → APs=3, CorePools r0=1 r1=2 r2=1
- **WS-B/C ✅ wired:** `cortex::compute::dispatch_ternary` — choke point NPU→GPU→CPU-SMP→AVX2→scalar; gated `ap_pollable`
- **WS-D/E ✅ honesto:** GPU canário; NPU VERDICT=SOFTWARE
- **WS-G #412 ✅:** `cortex::decode` structured decoding, self-test PASS

### 2. ADR-0058 — Generative Card Desktop (UI/Jarbas) S1–S4 ✅

- `embedded-graphics` DrawTarget sobre DoubleBuffer
- `UiDeclaration`/`UiRenderer`: Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel
- Cards: close, mover, redimensionar, botão→CARD_ACTION
- 3 cards demo (Sistema/Clima/Chamada de Vídeo); orb+HUD preservados

### 3. Lições
- SMP wake: sequencial + recursos por-AP + retry
- APs sem IDT: `parallel_*` gated por `ap_pollable` (evita deadlock)
- embedded-graphics compila bare-metal soft-float
- GPU/NPU/vídeo: gated honestamente, nunca fingir Ready
