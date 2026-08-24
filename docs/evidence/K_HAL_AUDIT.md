# k_hal Deep Audit —硬件抽象层全面审计
**Data:** 2026-08-24 · **LOC:** 14,051 (GPU: 8,674 / Net: 2,681 / Audio: 69 / Core: 2,627)

## 1. Visão Geral

k_hal é o **Ring 1 sensório-motor** (ADR-0041 §9). Deve ser a **única** crate que toca BAR de device, MMIO, e hardware discovery. Hoje:

- **Conectado:** DeviceCap discovery, HalOffer bind/release, CapGate FE, GPU detect/canary/firmware/pcie_bypass, HDA agent, NPU detect, VirtIO init
- **Dead code significativo:** ~2,500 LOC em módulos GPU que ninguém chama
- **Stubs honestos:** nvidia_matmul→None, gpu_ternary→None, NPU→Software (AWAITING_HW)
- **Gaps críticos:** HDA driver minúsculo (46 LOC), VRAM buddy sem integração real, ring/bench/msched/xpu/xqueue/kv_dma/sasos/pipeline_g5/direct_storage/display_coex todos sem caller

## 2. Por Módulo

### 2.1 GPU (8,674 LOC)

| Módulo | LOC | Caller? | Estado |
|--------|-----|---------|--------|
| `detect.rs` | 550 | ✅ main.rs | GPU enumeration real |
| `canary.rs` | 327 | ✅ boot | Vector add validation |
| `backend.rs` | 463 | ✅ compute_dispatch | State machine + gpu_matmul (→None em QEMU) |
| `firmware.rs` | 319 | ✅ boot | Preload + ACR pipeline |
| `nvidia_pascal.rs` | 693 | ✅ firmware | Pascal CE/Falcon boot |
| `nvidia_pascal_ce.rs` | 579 | ✅ firmware | CE command engine |
| `nvidia_pascal_acr.rs` | 556 | ✅ firmware | ACR secure boot |
| `pcie_bypass.rs` | 318 | ✅ main.rs | ReBAR probe |
| `vram.rs` | 246 | ✅ jarbas/gauges | Buddy allocator |
| `work_queue.rs` | 130 | ✅ hermes/tests | Lock-free queue |
| `blit.rs` | 161 | ✅ jarbas/gpu_compositor | 2D blit (GPU) |
| `compute_abi.rs` | 238 | ✅ backend | Tensor types |
| `compute_dispatch.rs` | 44 | ✅ main.rs | Register GPU ternary (→None em QEMU) |
| `intel.rs` | 447 | ⚠️ detect | Intel GPU init (VirtIO fallback) |
| `intel_gen9.rs` | 231 | ⚠️ firmware | Gen9 firmware loading |
| `intel_display.rs` | 355 | ⚠️ display | Intel display pipe |
| `intel_guc.rs` | 200 | ⚠️ firmware | GuC/HuC loading |
| `kernel_pack.rs` | 374 | ⚠️ firmware | KernelPack loader |
| `intel_arc.rs` | 82 | ⚠️ detect | Intel Arc detection |
| `intel_gtt.rs` | 66 | ⚠️ detect | GTT page table |
| `intel_mad.rs` | 59 | ⚠️ detect | Media acceleration |
| `amd.rs` | 149 | ⚠️ detect | AMD GPU detect |
| `amd_discovery.rs` | 195 | ⚠️ detect | AMD IP discovery |
| `amd_kiq.rs` | 136 | ⚠️ detect | AMD Kernel Interface Queue |
| `amd_mad.rs` | 51 | ⚠️ detect | AMD Media Acceleration |
| `amd_mes.rs` | 72 | ⚠️ detect | AMD Micro Engine Scheduler |
| `amd_psp.rs` | 155 | ⚠️ detect | AMD Platform Security Processor |
| `nvidia.rs` | 313 | ⚠️ detect | NVIDIA detection + i2c |
| `nvidia_pascal_qmd.rs` | 136 | ⚠️ firmware | QMD descriptor |
| `nvidia_pascal_sw.rs` | 135 | ⚠️ firmware | Software fallback |
| `ring.rs` | 184 | ❌ **DEAD** | GpuRing — hardware FIFO ring (no caller) |
| `bench.rs` | 42 | ❌ **DEAD** | run_benchmark (no caller) |
| `msched.rs` | 35 | ❌ **DEAD** | MSched Belady evictor (no caller) |
| `direct_storage.rs` | 32 | ❌ **DEAD** | probe_gds (no caller) |
| `display_coex.rs` | 142 | ❌ **DEAD** | GpuAssignment (no caller) |
| `sasos.rs` | 88 | ❌ **DEAD** | sasos_vram_ptr (no caller) |
| `pipeline_g5.rs` | 50 | ❌ **DEAD** | decode_step_cpu (no caller) |
| `xpu.rs` | 74 | ❌ **DEAD** | XpuDispatcher (no caller) |
| `xqueue.rs` | 126 | ❌ **DEAD** | XQueue (no caller) |
| `kv_dma.rs` | 74 | ❌ **DEAD** | KV DMA (no caller) |

**Dead code total GPU:** ~847 LOC (ring+bench+msched+direct_storage+display_coex+sasos+pipeline_g5+xpu+xqueue+kv_dma)

### 2.2 Audio (69 LOC — Mínimo)

| Módulo | LOC | Caller? | Estado |
|--------|-----|---------|--------|
| `hda.rs` | 46 | ✅ main.rs (HdaAudioAgent) | HDA driver — **46 LOC é mínimo** |
| `mod.rs` | 23 | — | Re-exports |

**Problemas do HDA:**
- **46 LOC é mínimo para um driver HDA real** — falta CORB/RIRB negotiation, pin widget config, codec probing
- Endereços físicos hardcoded (0x103000, 0x104000) — não descobertos via BAR
- Sem suporte a múltiplos codecs (HDA permite até 15)
- Sem verb commands (0x200+ codecs exigem verb sending)
- Sem format configuration (sample rate, bits per sample)

### 2.3 Net/WiFi (2,681 LOC)

| Módulo | LOC | Caller? | Estado |
|--------|-----|---------|--------|
| `generic_wifi.rs` | 422 | ✅ hermes (pub use) | Generic WiFi interface |
| `wifi_iwlwifi.rs` | 271 | ✅ hermes (pub use) | Intel iwlwifi driver |
| `wifi_softmac.rs` | 146 | ✅ hermes (pub use) | SoftMAC layer |
| `wifi_compat.rs` | 47 | ✅ hermes (pub use) | Compat layer |
| `wifi_msix.rs` | 137 | ✅ hermes (pub use) | MSI-X interrupt |
| `wifi_crypto.rs` | 40 | ✅ hermes | Crypto primitives |
| `ath10k_ce_bmi.rs` | 359 | ⚠️ ath10k | BMI transport |
| `ath10k_htc_wmi.rs` | 217 | ⚠️ ath10k | HTC/WMI protocol |
| `ath10k_wmi_scan.rs` | 201 | ⚠️ ath10k | WMI scan commands |
| `ath10k_wmi_assoc.rs` | 95 | ⚠️ ath10k | WMI association |
| `ath10k_fw.rs` | 191 | ⚠️ ath10k | Firmware loading |
| `wifi_ath10k.rs` | 384 | ⚠️ ath10k | ath10k main driver |
| `iwl_fw.rs` | 133 | ⚠️ iwlwifi | iwl firmware |
| `mod.rs` | 38 | — | Module declarations |

### 2.4 Core k_hal

| Módulo | LOC | Caller? | Estado |
|--------|-----|---------|--------|
| `cap_gate.rs` | 263 | ✅ hermes/neural-kernel | FE grant/revoke/check |
| `offer.rs` | 573 | ✅ hermes | HalOffer lifecycle |
| `device_cap.rs` | 113 | ✅ hermes | DeviceClass enum |
| `device_recipe.rs` | 284 | ✅ boot | Device recipes |
| `discovery.rs` | 125 | ✅ init_h1 | PCI enumeration |
| `hw_gate.rs` | 141 | ✅ main.rs | Boot smoke markers |
| `unlock_dag.rs` | 132 | ✅ init_h1 | DAG unlock tokens |
| `virtio.rs` | 298 | ✅ main.rs | VirtIO init |
| `pci_bar.rs` | 21 | ✅ discovery | BAR decoding |
| `compute_port.rs` | 65 | ✅ lib.rs | Compute port status |
| `display_port.rs` | 35 | ✅ lib.rs | Display port status |
| `net_port.rs` | 35 | ✅ lib.rs | Net port status |
| `audio_port.rs` | 35 | ✅ lib.rs | Audio port status |
| `video_port.rs` | 47 | ✅ lib.rs | Video port status |
| `npu.rs` | 92 | ✅ main.rs | NPU detect + init |
| `fat_assets.rs` | — | ✅ boot | FAT asset loading |
| `lego_boot.rs` | — | ✅ boot | Lego boot stages |

## 3. Gaps Críticos

### 3.1 HDA Audio — Subdesenvolvido (46 LOC vs necessário ~400+)
- **Falta:** CORB/RIRB verb negotiation, codec probe multi-codec, pin widget config, format negotiation, DMA buffer real (hoje hardcoded phys)
- **Impacto:** Piper TTS funciona via formant synth mas HDA real não funciona em HW

### 3.2 GPU Dead Code (~847 LOC)
- `ring.rs`, `bench.rs`, `msched.rs`, `direct_storage.rs`, `display_coex.rs`, `sasos.rs`, `pipeline_g5.rs`, `xpu.rs`, `xqueue.rs`, `kv_dma.rs` — todos sem caller
- **Ação:** Marcar `#[allow(dead_code)]` explícito OU deletar se sem plano de uso

### 3.3 VRAM Buddy Sem Integração
- `VramBuddy` existe mas não é alimentado por MMIO real (BAR2 não mapeado via HHDM no boot)
- `vram_usage()` retorna dados do buddy mas não do hardware real

### 3.4 GPU Compute Honesto Mas Limitado
- `nvidia_matmul()` → None (sem firmware ACR carregado)
- `gpu_ternary()` → None (sem KernelPack W2A8 assinado)
- **O pipeline inteiro é CPU fallback** — honest mas inútil para HW real

### 3.5 Audio Port Trivial
- `fe_stream()` retorna `PortStatus` mas não faz nada real — só leitura de estado

## 4. O que Funciona (conectado e operacional)

1. **DeviceCap discovery** — PCI scan → device tree → HalOffer
2. **CapGate** — FE grant/revoke/check → hermes/trinity_inject
3. **GPU detect + canary** — enumeração real de GPU, validação vector_add
4. **GPU firmware pipeline** — preload blobs → ACR secure boot → Falcon
5. **PCIe ReBAR bypass** — probe → enable → validação
6. **NPU detect** — PCI scan → verdict honesto (AWAITING_HW)
7. **VirtIO init** — log ring + device setup
8. **HDA agent** — registrado no boot, poll/write/mixer wired
9. **WiFi drivers** — iwlwifi + ath10k (AWAITING_HW para ambos)
10. **Work queue** — lock-free GPU job queue com telemetria

## 5. Prioridades de Correção

| Prioridade | Gap | Esforço | Impacto |
|-----------|-----|---------|---------|
| P0 | HDA: adicionar CORB/RIRB + codec probe real | Alto | Áudio real em HW |
| P1 | Marcar dead code GPU com `#[allow(dead_code)]` | Baixo | Limpeza |
| P2 | VRAM: integrar BAR2 discovery ao buddy allocator | Médio | GPU memory real |
| P3 | Audio Port: implementar fe_stream real (DMA) | Médio | Áudio streaming |
| P4 | XPU dispatcher: integrar prefill/decode ao cortex | Alto | GPU inference |
