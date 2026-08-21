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
| GPU command queue | ⚠️ era `Mutex<WorkQueue>` | `k_hal/src/gpu/work_queue.rs` → **lock-free agora** (abaixo) |

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

### PILAR 1: GpuAgent + BARs MMIO + fila MPMC lock-free ✅ (parcial)
- BARs MMIO via HHDM + `map_page_uc`: ✅ já existia (e1000/xHCI padrão, gpu BARs
  no firmware.rs).
- **Fila de comandos lock-free — IMPLEMENTADO agora** (`k_hal/src/gpu/work_queue.rs`):
  o `Mutex<WorkQueue>` foi substituído por `k_nano::sync::mpmc::MpmcQueue` (Vyukov,
  CAS — a MESMA usada pelo `cortex::cellular` em produção; nada de ring caseiro).
  `submit`/`submit_tensor`/`drain`/`stats`/`gate_status` mantêm a API pública
  (backend.rs intacto). Fila cheia → `None` (backoff do caller), nunca bloqueia.
  4 testes host (roundtrip, overflow sem bloqueio, sequência de ids, gate).

### PILAR 2: VirtIO-GPU (QEMU) + skills WASM enviando comandos GPU ✅ (parcial)
- VirtIO-GPU: ✅ real em `jarbas/src/virtio_gpu.rs` (QEMU/dev).
- **`aios_gpu::submit(op, flags)` no wasmi_rt — IMPLEMENTADO agora**: host-import
  novo no sandbox, gated por `CAP_GPU` (bit novo) no CapGate real. Skills WASM
  chamam `aios_gpu::submit` → fila lock-free do GpuAgent → `drain` (HW/CPU).
  Retorna job id; `-1` se fila cheia; trap sem `CAP_GPU`. 3 testes host
  (com cap → id, sem cap → trap, op inválido → MatmulTernary default).
- Venus/Vulkan passthrough: ❌ AWAITING_HW (não há backend Vulkan bare-metal;
  seria um projeto completo).

### PILAR 3: NVIDIA GSP RPC (Turing+) ⚠️ scaffold honesto
- Família GSP detectada (`is_nvidia_gsp_family`) e `try_vector_add_gsp` existe mas
  é **quarantined** ("GSP-RM incompleto"). Firmware atual (Pascal) usa LegacyAcr
  FECS/GPCCS (funciona). GSP-RM real = residual documentado; GPU Turing+ =
  AWAITING_HW.

### PILAR 4: Resizable BAR + DirectStorage NVMe→VRAM ⚠️ AWAITING_HW
- `direct_storage.rs` é stub honesto (`GdsStatus::AwaitingHw`, log único, nunca
  promove Ready — lição "persistência que se finge de pronta"). Resizable BAR =
  configuração PCI ainda não wired. P2P NVMe→VRAM exige HW real.

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
   agente pedir compute (HITL + CapGate, como o `trinity_inject`).
2. **WASM skill real de GPU** — op-IR que emite `aios_gpu::submit` (estende a
   gramática do `wasm_build` com host-calls).
3. **Wiring do ReBAR no boot** — probe por BDF da GPU (`pcie_bypass_report`) no
   PlatformAgent, com `try_enable_resizable_bar` só se suportado + HITL.
4. **GSP-RM** — preencher o scaffold `try_vector_add_gsp` quando HW Turing+
   disponível (AWAITING_HW).
5. **Resizable BAR + P2P DirectStorage** — quando NVMe + dGPU reais (AWAITING_HW).
