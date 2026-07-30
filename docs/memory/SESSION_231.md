# SESSION_231: HW Expert v4 + ADR-0082 — HardwareInfo Registry

**Data:** 2026-07-30  
**Sprint:** v1.9.99-s231  
**Bloco:** HW Expert v4 multi-head + HardwareInfo Registry  
**Commit:** `65e95dc fix: path cortex_crate::cortex::load_hwexpert_v5 (shadow resolution)`

## Escopo

Implementação do ADR-0082 (HardwareInfo Registry) + treinamento do HW Expert v4 multi-head com dataset unificado de ~60K amostras.

## O que foi feito

### 1. HardwareInfo MVP
- `HardwareInfo` struct em `platform_probe.rs` — registro público de capacidades de HW
- `hw_info()` function — qualquer crate/agente consulta
- Gate `allow_avx2` não depende mais de `isa.xsave` (WHPX filtra o bit XSAVE)
- Conexão SGDB: `hamming_dispatch.rs` e `art.rs` usam `hw_info().avx2_ready()`

### 2. HW Expert v4 — Modelo ML Multi-Head
- **Dataset unificado:** 59.905 amostras (WDM + SDIO + PCI.IDS + USB.IDS + kernel seed)
- **5 heads:** family (17), fw (8), agent (9), caps (10-bit), next_action (9)
- **Treino:** 100 epochs, hidden=128, 6 layers, ~1M params
- **Modelo:** 260 KB, .bitnet v5 multi-head
- **Acurácia:** FW 97%, Caps 96%, Family 81%, Agent 81%, Next 80%

### 3. Rust v5 Loader + Predição
- `HwExpertV4Model` struct — backbone transformer + 5 heads
- `load_hwexpert_v5()` — carrega .bitnet v5
- `predict_hw_v4()` — forward pass completo (embed → 6 layers → pool → 5 heads)
- `HWEXPERT_V4_MODEL` static + `hwexpert_v4_predict()` API pública
- `HwPrediction` struct em `tensor.rs`

### 4. Integração com boot + SGDB
- `build_card()` tenta HW Expert v4 → tabela → heurística
- Boot carrega `HWEXPRT4.BIN` via QEMU loader ou FAT
- `predict_all_pci()` escreve `/hw/pci/*` no SGDB

### 5. Windows DriverStore
- `tools/extract_wdm_hwids.py` — extrai HWIDs de .inf (protegido, requer admin)
- 478 HWIDs extraídos do DriverStore local

### 6. Correções no ART
- `find_child_byte16_sse` com `#[cfg(target_arch = "x86_64")]` sem `target_feature = "sse2"` causava `art_ok=false` com `art_len==n_art`
- Fix: usar `cfg(target_arch = "x86_64")` + `#[target_feature(enable = "sse2")]` (runtime dispatch)

## Lições Aprendidas

1. **`#[target_feature]` funciona em build soft-float:** LLVM compila kernels AVX2/SSE2 mesmo com `-C target-feature=-sse2`. AIOS adaptativo sem recompilação.
2. **WHPX filtra CPUID xsave:** `allow_avx2()` não pode depender de `isa.xsave` porque o hypervisor não expõe o bit. Remover da gate.
3. **`find_child_byte16_sse` corrompe ART sem cfg sse2:** O sintoma clássico é `art_ok=false` mas `art_len==n_art` — inserts funcionam, get falha. Porque insert_rec usa loop scalar, get usa SSE2 bugado.
4. **171K é raw HWID strings, não devices únicos:** Após colapso SUBSYS/REV → 16K únicos; (vid,did) únicos → ~44K. Usar "44K unique devices".
5. **Windows DriverStore exige admin:** `extract_wdm_hwids.py` precisa `Start-Process -Verb RunAs` ou `takeown`.

## Arquivos modificados

```
crates/k_nano/src/platform_probe.rs     — HardwareInfo, xsave fix
crates/k_ai/src/sgdb/hamming_dispatch.rs — runtime dispatch via hw_info()
crates/k_ai/src/sgdb/art.rs            — runtime SSE2 via hw_info()
crates/k_ai/src/sgdb/mod.rs            — Q-jump per-step logging
crates/k_ai/src/sgdb/bench.rs          — art_len monitoring
crates/k_ai/src/sgdb/store.rs          — predict_all_pci()
crates/cortex/src/tensor.rs            — HwPrediction struct
crates/cortex/src/cortex.rs            — HwExpertV4Model, loader, predictor
crates/k_ai/src/hw_capability.rs       — build_card() ML→table→heuristic
crates/neural-kernel/src/main.rs       — v4 boot loading + SGDB
docs/architecture/0082-hardware-info-registry.md — ADR completa
docs/architecture/INDEX.md             — ADR-0082 entry
AGENTS.md                              — lições corrigidas
tools/train_hw_expert_v4.py           — multi-head training
tools/unify_hwids_v4.py               — dataset unifier
tools/extract_wdm_hwids.py            — Windows DriverStore extractor
models/hw_expert/hw_expert_v4.bitnet   — modelo treinado 260KB
models/hw_expert/v4/dataset.json       — 59.905 amostras
models/WDM/hwids.json                  — extração DriverStore
target/HWEXPRT4.BIN                     — para QEMU loader
```

## Pendências

- Carga automática de `HWEXPRT4.BIN` no boot via bootloader (não só QEMU loader)
- Migrar `cortex::simd_dispatch` para `hw_info()`
- Migrar `hermes::scheduling` para `hw_info()`
- Expandir `/hw/*` no SGDB para GPU, storage, net, audio, wifi
