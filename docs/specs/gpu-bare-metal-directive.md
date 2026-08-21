# GPU & Aceleração Bare-Metal — Diretiva v2.0 (auditada contra o código real)

> Target: x86_64 / Limine HHDM / Rust `#![no_std]` · Dogma: zero POSIX, zero C-shims,
> zero DRM/Mesa port — controle de HW em Rust puro, GPU gerida por agente + skills
> WASM isoladas por CapGate.
> Regra: NÃO refazer o que já existe (cada item tem estado real verificado).

## 1. BASELINE REAL (não re-implementar — manter + validar)

| Item | Estado | Onde |
|---|---|---|
| Backend detect por vendor (Intel/Nvidia/Amd/VirtIo) + arch + VRAM | ✅ real | `k_hal/src/gpu/detect.rs` (`GpuVendor`/`GpuArch`) |
| GpuDriverAgent A-015 (backend detect, Oneshot) | ✅ real | `hermes/src/native_agents.rs:126` |
| NVIDIA Pascal: PUSH_BUFFER/GPFIFO + doorbell + timeout (HW GTX 1050) | ✅ real | `k_hal/src/gpu/nvidia_pascal*.rs` (acr/ce/qmd/sw) |
| Firmware NVIDIA: WPR 2MB topo VRAM, FECS+GPCCS via BAR2, Falcon boot | ✅ real | `k_hal/src/gpu/firmware.rs` |
| VirtIO-GPU (QEMU) + backend | ✅ real | `jarbas/src/virtio_gpu.rs` + `display/gpu_backend.rs` |
| Compositor Jarbas 1280x800@32bpp (sem Wayland/X11) | ✅ real | `jarbas/src/display/*` (DisplayAgent) |
| AMD KIQ/MES/PSP + Intel gen9/GUC/GTT | ✅ real (scaffolds) | `k_hal/src/gpu/amd_*.rs`, `intel_*.rs` |
| GSP (Turing+): classificação de família | ✅ real | `detect.rs::is_nvidia_gsp_family`, `nvidia.rs::ComputeBackendKind::Gsp` |
| DirectStorage NVMe→VRAM (P2P) | ⚠️ stub HONESTO | `k_hal/src/gpu/direct_storage.rs` — `GdsStatus::AwaitingHw`, nunca promove Ready |
| GPU command queue | ⚠️ ainda `Mutex`/array | `k_hal/src/gpu/work_queue.rs` — **não** é MpmcQueue lock-free no tree wired |

## 2. Correções ao exemplo colado (não compila contra o repo)

- `crate::mem::phys_to_virt` **não existe** em k_hal — o HHDM vive em
  `k_nano::memory::{phys_to_virt, PHYS_MEM_OFFSET}` (lição SESSION_219/ADR-0065).
- `FrameAllocator::alloc_contiguous` **não existe** — usar o bitmap de frames de
  `k_nano::memory` (frame → HHDM via `phys_to_virt`).
- Caminho real: `k_hal/src/gpu/` (não `drivers/gpu/`); mapping MMIO = padrão
  `map_page_uc`/`map_mmio_page` do e1000/xHCI (SESSION_237), não deref físico.
- Mapear BAR não é "init_pci_bar" isolado — a enumeração vive no PlatformAgent
  (PCI) + `k_hal::device_cap`/HalOffer bind.

## 3. PILARES — estado após esta sessão

### PILAR 1: GpuAgent + BARs MMIO + fila de comandos ⚠️ (parcial)
- BARs MMIO via HHDM + `map_page_uc`: ✅ já existia (e1000/xHCI padrão, gpu BARs
  no firmware.rs).
- **Fila de comandos:** `work_queue.rs` no tree wired ainda **não** usa
  `k_nano::sync::mpmc::MpmcQueue` (claim anterior = overclaim). Migrar para MPMC
  Vyukov = residual. `k_nano::mpmc` / `sync::mpmc` já existem para outros paths.

### PILAR 2: VirtIO-GPU (QEMU) + skills WASM enviando comandos GPU ⚠️ (parcial)
- VirtIO-GPU: ✅ real em `jarbas/src/virtio_gpu.rs` (QEMU/dev).
- **`aios_gpu::submit`:** CapGate tem `CAP_GPU`; host-import completo + drain
  lock-free = residual (não declarar IMPLEMENTADO até medido no wasmi_rt).
- Venus/Vulkan passthrough: ❌ AWAITING_HW.

### PILAR 3: NVIDIA GSP RPC (Turing+) ⚠️ scaffold honesto
- Família GSP detectada (`is_nvidia_gsp_family`) e `try_vector_add_gsp` existe mas
  é **quarantined** ("GSP-RM incompleto"). Firmware atual (Pascal) usa LegacyAcr
  FECS/GPCCS (funciona). GSP-RM real = residual documentado; GPU Turing+ =
  AWAITING_HW.

### PILAR 4: Resizable BAR + DirectStorage NVMe→VRAM ⚠️ parcial
- `direct_storage.rs` é stub honesto (`GdsStatus::AwaitingHw`).
- **`pcie_bypass.rs` WIRED (probe-only):** `pcie_bypass_report` no boot por BDF
  da GPU detectada; `try_enable_resizable_bar` / ACS clear = HITL, não automático.
- P2P NVMe→VRAM exige HW real.

### PILAR 5: Jarbas compositor ✅ real
- DisplayAgent + compositor 1280x800@32bpp + mouse/teclado + cards — sem servidor
  de janelas (supersede parcial ADR-0047-HMI). Nada a fazer aqui.

## 4. Protocolo de verificação (por pilar)
- [ ] `cargo clean -p neural-kernel && cargo check --release` → 0 erros
- [ ] `cargo test -p k-hal --lib` (≥11) · `cargo test -p hermes --lib` (≥63)
- [ ] Nenhum stub que se finge de Ready; AWAITING_HW explícito quando não testável
- [ ] Gate v2.0.0 NÃO declarado sem review de ADR + OK do maintainer

## 5. Bypass / desbloqueio de HW (diretiva complementar) — auditado + ReBAR/ACS ✅

O exemplo colado (`drivers/gpu/bypass/nvidia_raw.rs`, `crate::mem::phys_to_virt`,
PMC `0x000200`, FIFO reset `0x002050`, Falcon `0x0010a000`) **não compila contra o
repo** e os offsets NÃO batem com o mapa validado de `nvidia_pascal.rs`
(RUNLIST 0x002270, KICK 0x002634, CHANNEL 0x800000, GPFIFO 0xC06F). Escrita crua
`0xFFFFFFFF` em registrador não-verificado = brick/reboot — regra do módulo novo:
**probe da capability → validação de suporte → RMW com readback**.

- ✅ **`k_hal/src/gpu/pcie_bypass.rs` (novo):** Resizable BAR + ACS para peer DMA,
  com config space abstraído (`PciConfigIo`: real = port I/O `k_nano::pci`
  target-only; fake = host). `find_cap`, `rebar_supported_sizes_mb`,
  `rebar_current_size_mb`, `try_enable_resizable_bar` (potência de 2, na máscara
  suportada, readback verificado), `acs_control`, `try_clear_p2p_redirect`
  (limpa bits 2|3 — P2P Request/Completion Redirect; ⚠️ documentado: enfraquece
  isolamento sem IOMMU, chamada explícita, nunca automática), `pcie_bypass_report`.
  6 testes host (parser de caps, tamanhos, enable com readback, rejeição de
  não-suportado, ACS idempotente, report honesto). `k_nano::pci::write_config_dword`
  tornou-se `pub` (era `pub(crate)`).
- ✅ Bypass de firmware NVIDIA/AMD/Intel **já existia com registradores reais**:
  `firmware.rs` (WPR/FECS/GPCCS/Falcon, Pascal), `amd_kiq` (CP ring/KIQ),
  `intel_gtt`/`intel_gen9` (GTT + engine), `intel_guc` (GuC).
- ⚠️ GSP-RM (Turing+): scaffold quarantined — AWAITING_HW.
- ⚠️ DirectStorage NVMe→VRAM: stub `AwaitingHw` honesto (nunca promove Ready).

## 6. Próximos passos (por valor/risco)
1. **Hook da skill GPU no supervisor** — chamar `aios_gpu`/work_queue quando um
   agente pedir compute (HITL + CapGate). `trinity_inject` = WIP órfão (APIs
   cortex incompletas) — não tratar como IMPLEMENTADO.
2. **WASM skill real de GPU** — op-IR + host-call `aios_gpu::submit` medido.
3. **`try_enable_resizable_bar` sob HITL** — após report no boot (já wired).
4. **GSP-RM** — scaffold `try_vector_add_gsp` quando HW Turing+ (AWAITING_HW).
5. **Resizable BAR + P2P DirectStorage** — NVMe + dGPU reais (AWAITING_HW).
6. **Migrar work_queue → MpmcQueue** — quando medido vs Mutex atual.
