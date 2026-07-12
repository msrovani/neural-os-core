# Sprint Plan v1.1.x — A Era do Silício (Continuação)
# GPU · WiFi · Rede Nativa · JARVIS v2

**Data:** 2026-07-12  
**Versão alvo:** v1.1.5  
**Lema:** *"O hardware real não perdoa. O silício obedece."*

---

## ✓ v1.1.1 — GPU Compute (~1.200 LOC) ✅

**Foco:** Pipeline DMA GPU + Firmware ACR loading + HW Expert v3

| Item | LOC | Status | Descrição |
|------|:---:|:------:|-----------|
| NVIDIA PFIFO PUSH_BUFFER channel | ~300 | ✅ | GPFIFO doorbell, cmdbuf @0x200000, timeout |
| DMA CPU↔VRAM via BAR2 | ~100 | ✅ | `cpu_to_vram()` + `vram_to_cpu()` |
| Fallback automático CPU | ~50 | ✅ | NVIDIA→Intel→CPU com AVX2 |
| Benchmark TFLOPS | ~100 | ✅ | matmul 32/64/128 |
| **Firmware ACR loading** | ~300 | ✅ | **Pipeline WPR: blobs baixados, firmware.rs 231 LOC** |
| HW Expert v3 | ~300 | ✅ | 61.453 VID/DID, 1M params |
| **Matmul shader ternário** | ~200 | 🔴 NDA | NVIDIA ISA não pública |
| **Intel Gen9+ EU shader** | ~300 | 🔴 NDA | Intel GEN ISA não pública |

---

## ✓ v1.1.2 — SelfHealing + Firmware Pipeline (~800 LOC) ✅

**Foco:** Auto-detect + firmware download + hot-load

| Item | LOC | Status |
|------|:---:|:------:|
| SelfHeal I3/I4: firmware + skill ausente | ~200 | ✅ HEALTH_ISSUE → Hermes → LLM |
| firmware.rs: hot_load_firmware(vid,did,class) | ~100 | ✅ Universal |
| HermesAgent: assina HEALTH_ISSUE | ~50 | ✅ |
| pci.ids + usb.ids + kernel PCI tables | ~200 | ✅ 48.346 registros |
| WHENCE + AMD ucode + firmware metadata | ~200 | ✅ 1.207 records |
| regulatory.db | ~50 | ✅ 174 países |
| **Total** | **~800** | **✅** |

---

## ✓ v1.1.3 — 3 Camadas Visuais + Audio + Rede (~600 LOC) ✅

**Foco:** Orb + Hermes CLI + Window Manager + HDA playback

| Item | LOC | Status |
|------|:---:|:------:|
| Z-order real (Layer enum) | ~80 | ✅ OrbBackground < HermesOverlay < AppWindows < DockBar |
| FPS control 60Hz | ~40 | ✅ LAST_FRAME_TICK |
| Hermes CLI overlay semi-transparente | ~80 | ✅ Console sempre visível no canto |
| FFT áudio → animação do Orbe | ~120 | ✅ process_audio_fft() + 16 bins |
| Mouse PS/2 integrado | ~150 | ✅ MOUSE_MOVED/CLICK → compositor |
| HDA playback (SD1) | ~80 | ✅ write_hda_playback() → auto-falante |
| BrowserAgent real | ~50 | ✅ HTTP GET via smoltcp |
| **Total** | **~600** | **✅** |

---

## ✓ v1.1.4 — WiFi Intel AX200 (~300 LOC) ✅

**Foco:** iwlwifi firmware loading + command protocol

| Item | LOC | Status |
|------|:---:|:------:|
| CSR/HBUS/SRAM register defs | ~60 | ✅ iwlwifi real offsets |
| ucode loading pipeline | ~100 | ✅ wake → SRAM → seções → alive |
| Command/response protocol | ~50 | ✅ HBUS + doorbell NMI |
| Scan command (0x34) | ~50 | ✅ SRAM cmd + RX poll |
| Firmware blobs AX200/210 | 5 blobs | ✅ cc-a0, Qu, so-a0, ty-a0, ~7.5MB |
| **Total** | **~260** | **✅** |

---

## v1.1.5 — Integração + Testes HW Real (~500 LOC)

**Foco:** Preparar para boot em HW real (i5-6400 + GTX 1050)

| Item | LOC | Descrição | Status |
|------|:---:|-----------|--------|
| Script imagem bootável | ~100 | `tools/build_image.py` → pendrive.img | ⏳ |
| Bateria de testes automatizados | ~200 | Testes MemoryDisk para NeuralFS, PCI, FAT32 | ⏳ |
| Documentação de integração | ~100 | HOWTO: boot em HW real, pinagem, requisitos | ⏳ |
| CHANGELOG + release notes | ~50 | Histórico completo v1.1.0 → v1.1.5 | ⏳ |
| Tag v1.1.5 | — | Release | ⏳ |
| **Total** | **~500** | | |

---

## Resumo

| Versão | Foco | LOC | Status |
|:------:|------|:---:|:------:|
| v1.1.0 | FS-v2 base | ~3.884 | ✅ |
| v1.1.1 | GPU + Firmware + HW Expert | ~1.200 | ✅ |
| v1.1.2 | SelfHealing + HWID datasets | ~800 | ✅ |
| v1.1.3 | Visual 3-camadas + Audio + Browser | ~600 | ✅ |
| v1.1.4 | WiFi Intel AX200 | ~260 | ✅ |
| **v1.1.5** | **Integração HW + Documentação** | **~500** | **🔄 Ativo** |
| **Total** | | **~7.244** | |
