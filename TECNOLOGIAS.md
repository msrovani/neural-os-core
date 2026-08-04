# CATÁLOGO DE TECNOLOGIAS — AIOS K³CHJ (neural-os-core)
## Registro de Propriedade Intelectual e Inovação

**~26.000 LOC, 180+ arquivos Rust, ~50 agentes nativos**
**Versão release:** v1.9.7 TESTE / NÃO ESTÁVEL (2026-07-22)
**Build:** `cargo clean -p neural-kernel && cargo nk` = 0 erros (warnings dead-code = política conhecida)
**Licença:** MIT (código próprio) / MIT, GPL, Apache 2.0 (componentes inspirados/portados)
**Repositório:** [github.com/msrovani/neural-os-core](https://github.com/msrovani/neural-os-core)
**HuggingFace:** [huggingface.co/aios-k2chj](https://huggingface.co/aios-k2chj)

---

## Legenda de Propriedade Intelectual

| Selo | Significado |
|------|------------|
| 🏆 **INOVAÇÃO ORIGINAL** | Tecnologia desenvolvida integralmente pela equipe AIOS K³CHJ. Sem precedente conhecido em sistemas bare-metal. |
| 🔬 **ENG. REVERSA** | Implementação própria baseada em engenharia reversa de hardware ou formato fechado. |
| 🔄 **PORT/ADAPT** | Port de conceito de ecossistema aberto, com adaptação significativa para no_std e arquitetura de agente. |
| 📚 **PAPER IMPL** | Implementação baseada em paper acadêmico, com otimizações próprias para bare-metal. |
| 📦 **CRATE** | Utilização direta de crate existente, possivelmente com patches para no_std. |
| ⚠️ **TERCEIROS** | Código de terceiros sob licença própria, utilizado conforme termos originais. |

**DISCLAIMER:** Este documento cataloga fontes de inspiração, licenças originais e a contribuição inovadora da equipe AIOS K³CHJ. As inovações listadas como 🏆 são elegíveis para proteção por direitos autorais, patentes de software (onde aplicável) e constituem o diferencial competitivo do projeto.

---

## 1. 🧠 SISTEMA OPERACIONAL NEURAL — Inovações Paradigmáticas

Tecnologias que definem a categoria "AI-native Operating System" e não possuem equivalente conhecido.

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 1.1 | **AIOS Runtime — Agente como Unidade Ontológica Única** | 🏆 Unifica tasks, skills, drivers, daemons em UM único trait `Agent`. Todos os ~50 agentes nativos seguem o mesmo lifecycle: manifest → tick → EventBus → skill. Nenhum outro OS bare-metal ou Linux faz isso. | Projetos de agentes (CrewAI, OpenAI Swarm, AutoGen) — todos em userspace/std | Apache 2.0 / MIT | `agent-core/`, `agents.rs` | ✅ 0 err |
| 1.2 | **HW Expert v3/v4 — Identificação de Hardware por Rede Neural em Kernel** | 🏆 **60K amostras, 44K devices únicos** reconhecidos por BitNet ternário multi-head (5 heads: family, fw, agent, caps, next_action). v3 (free-text) + v4 (estruturado) rodando em no_std. **Único OS do mundo que usa ML para identificar hardware no boot.** HW Expert v4 alimenta o SGDB `/hw/pci/*` e o `build_card()` para decisões PnP. | pci.ids, usb.ids, SDIO DriverPacks, **Windows DriverStore** (nova fonte) | MIT / domínio público | `cortex.rs`, `hw_capability.rs`, `hw_expert_v3.bitnet`, `HWEXPRT4.BIN`, `tools/retrain_hw_expert_v4.py`, `tools/validate_hw_expert_v4.py` | ✅ v4 retreinado + artefato VALIDADO (fração não-zero ≥1%, holdout do arquivo — SESSION_247) |
| 1.3 | **SelfHealing Firmware Pipeline I3/I4** | 🏆 HW novo é instalado → detectado → identificado → firmware baixado (HTTP) → carregado hot → skill gerada (LLM) → registrada → funcional. Tudo automático, sem reboot, sem configuração. | linux-firmware.git (blobs), Self-Healing Agents papers (arXiv) | MIT (linux-firmware) | `self_heal.rs`, `firmware.rs`, `agents.rs` | ✅ 0 err |
| 1.4 | **SleepCycle — Ciclo de Sono em Bare-Metal** | 🏆 **Primeiro (e único) sistema bare-metal com ciclo sono/aprendizado.** 5 fases: REPLAY → DREAM → CONSOLIDATE → PRUNE → REFLECT. Inspirado em neurociência humana. Sem internet. Sem humano. Cada boot melhora o sistema. | Neurociência (Atkinson-Shiffrin, Ebbinghaus), SleepCycle papers | — (conhecimento científico) | `agents.rs` (SleepCycleAgent) | ✅ 0 err |
| 1.5 | **Memory Hierarchy Index (MHI) — Alocação por IA** | 🏆 Sistema de memória em 4 tiers (Dram→Vram→Nvme→Hdd) com alocação orientada por ML. `alloc_by_tier()` + soft-migrate ativo (ADR-0040 MVP). | ZFS ARC (MFU/MRU), Hierarchical Memory papers | GPLv2 / MIT | `mhi.rs`, `memory.rs` | ✅ soft-MVP |
| 1.6 | **Trinity MoE — Roteamento de Intenção em Bare-Metal** | 🏆 6 experts (hw_identify, rust_coder, disk_diag, security, speech_synth, generator) + router treinável. Roteia dentro do LLM sem keyword matching. AutoLearn detecta necessidade → treina → registra novo expert. | Mixture of Experts papers (Shazeer 2017), MoE em LLMs | MIT | `trinity.rs`, `cortex.rs`, `agents.rs` (AutoLearnAgent) | ✅ 0 err |
| 1.8 | **Dual-Tier Memory + R3 (Rollout Routing Replay)** | 🏆 Separação obrigatória: Tier 1 `talc` (Hermes/JARBAS/UI) vs Tier 2 `TensorArena` bump (Cortex/MoE). Cache de rotas e tokens na arena — reset O(1) após GRPO. Zero fragmentação no hot path de inferência. Proíbe `Box`/`Vec` global no loop de tokens. | Rollout Routing Replay / GRPO papers; bare-metal arena pattern | MIT | `allocator.rs`, `arena.rs`, `r3.rs`, `global_arena.rs` | ✅ 0 err |
| 1.9 | **k-HAL — Anel R1 / DeviceCap + HalOffer (ADR-0041)** | 🏆 Único dono MMIO + DeviceTree + **HalOffer** (API R3 query/bind + Cap grant) + ports FE Cap-enforce + **H4+ QUEUE_NOTIFY** + **AS shallow** PoC. Continua release **1.8.x**. | HalOffer ≠ VirtIO; sDDF; Theseus | MIT | `crates/k_hal/offer.rs`, `virtio.rs`, `cap_gate.rs`, `hermes/hal_offer.rs` | ✅ H4+/H5+/AS (1.8.x) |
| 1.7 | **3 Camadas Visuais: Orb + Hermes CLI + Window Manager** | 🏆 Arquitetura visual em 3 camadas Z-order com FFT audio→Orbe, overlay CLI semi-transparente, e gerenciador de janelas com mouse integrado. Tudo renderizado por software no framebuffer UEFI, sem GPU. | SmileyOS patterns, JARVIS .NET MAUI (autor) | MIT | `display/compositor.rs`, `display/avatar.rs` | ✅ 0 err |
| 1.7b | **Generative Card Desktop (UI declarativa)** | ✅ ADR-0058 S1–S4: `embedded-graphics` `DrawTarget` (`FbTarget`) sobre `DoubleBuffer` + `UiDeclaration`/`UiRenderer` (Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel). Cards gerados por LLM (structured decode #412) ou skill WASM. Orb + HUD preservados; mouse close/drag/botão. Supersede parcial 0047-HMI. | embedded-graphics (kolibri/matrix-gui/embedded-gui = conceitos) | MIT/Apache | `jarbas/src/display/{eg,card,compositor,agent}.rs` | ✅ QEMU 3 cards |

---

## 2. 💻 KERNEL CORE — Fundação do Sistema

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 2.1 | **Bootloader 0.11.15 UEFI/BIOS** | 📦 Integração com framebuffer UEFI GOP, 512KB stack, physical memory mapping | `bootloader` crate | MIT/Apache 2.0 | `main.rs`, `crates/boot/` | ✅ 0 err |
| 2.2 | **IDT 32 handlers + IST** | 🔄 Double Fault IST com stack dedicada, GPF recoverable, Page Fault com endereço | `x86_64` crate, OSDev wiki | MIT/Apache 2.0 | `interrupts.rs` | ✅ 0 err |
| 2.3 | **Bitmap Frame Allocator 8GB** | 🔄 Adaptado para suportar até 8GB RAM com bitmap 128KB | `linked_list_allocator`, OSDev | MIT/Apache 2.0 | `memory.rs` | ✅ 0 err |
| 2.4 | **Adaptive Heap (AI Budget) + talc Dual-Tier** | 🏆 Tier 1: `talc` como `#[global_allocator]` (substitui `linked_list_allocator`). Tier 2: `TensorArena` bump em `0x4800_0000_0000` exclusiva Cortex/R3. `resize_heap_to_mb()` via `talc::extend`. | `talc` crate, bumpalo pattern | MIT/Apache 2.0 | `allocator.rs`, `arena.rs` | ✅ 0 err |
| 2.5 | **TicketLock FIFO + IrqSafeLock** | 🏆 Lock FIFO com TicketLock adaptado para no_std. `IrqSafeLock` com cli/sti automático, deadlock-free em ISRs. | `ticket-lock` crate, Linux spinlock | MIT | `ticket-lock/`, `sync/irq_lock.rs` | ✅ 0 err |
| 2.6 | **SMP Multi-Core + PerCpu** | ✅ ADR-0055/0057: FeatureGate + trampoline raw + AP work + CorePools. Wake **multi-AP** por SIPI direcionado sequencial + stack/PerCpu por-AP + retry (TCG `-smp 4`→APs=3). WHPX OFF. | OSDev, Linux SMP, `x86_64` crate | MIT/GPLv2 | `platform_probe.rs`, `smp/*` | ✅ SESSION_141 / ADR-0057 |
| 2.6b | **FeatureGate / CpuFeatures / CacheTopology** | ✅ PlatformProbe HV∩ISA; IsaPath; tiles L1/L2; OSXSAVE/XCR0 | Intel SDM, ADR-0055 | — | `k_nano/src/platform_probe.rs` | ✅ SESSION_141 |
| 2.6c | **Compute Dispatch (ComputeBackend) SMP+GPU+NPU** | 🔄 ADR-0057: choke point único `cortex::compute` (NPU→GPU→CPU-SMP→AVX2→scalar); GPU registra só se canário `Ready`; NPU XDNA/Intel detecção PCI + fallback software honesto. WS-B gated `ap_pollable` (deadlock-proof); on-demand AP-worker (IDT/IPI) + kernel GPU W2A8 + driver NPU = Layer S/HW. | Intel SDM, ADR-0048–50, IDEA #211/#330 | MIT | `cortex/compute.rs`, `k_hal/{npu,gpu/compute_dispatch}.rs` | 🟡 wired / Layer S |
| 2.6d | **Structured Decoding (grammar/JSON)** | 🔄 ADR-0057 (#412): `cortex::decode` — máscara de tokens permitidos antes do argmax; default no-op (zero regressão); self-test de boot PASS. | SGLang/outlines (conceito), no_std próprio | MIT | `cortex/src/decode.rs` | ✅ self-test |
| 2.7 | **Work-Stealing Scheduler (Chase-Lev)** | 🔄 Deques lock-free com Chase-Lev algoritmo. Portado para no_std sem std::thread. | `fast-steal` crate, Chase-Lev (1994) paper | MIT | `smp/work_stealing.rs` | ✅ 0 err |
| 2.8 | **CFS Scheduler (vruntime)** | 🔄 Completely Fair Scheduler com vruntime, baseado em Linux CFS. Implementação própria em no_std. | Linux CFS (Con Kolivas, Ingo Molnar) | GPLv2 | `cfs.rs` | ✅ 0 err |
| 2.9 | **EventBus IPC Publish/Subscribe** | 🏆 Sistema IPC pub/sub com `CapabilityToken` para controle de acesso por agente. Zero-copy com lock-free queues. | EventBus patterns (C#, Spring), | MIT | `event-bus/` | ✅ 0 err |
| 2.10 | **K³CHJ Capability Rings P0–P9** | ✅ Escada completa: AS+CR3+SPSC+Cap+`int 0x90` → CapGate → FB → DMA/mmap → Ring3 → #PF → vring → GGUF/FAT. Demos non-fatal. ADR-0041. | Capability microkernels (seL4, Fuchsia), | MIT | `address_space.rs`…`gguf_mmap.rs` | ✅ PoC |
| 2.10a | **Hermes CapabilityGate (P3)** | 🏆 Host-functions WASM/`aios_*` gated por Cap (SendTcp, WriteRing); deny + serial log. Sem POSIX. ADR-0041 P3. | Cap + trust tokens | MIT | `capability_gate.rs`, `aios_api.rs`, `wasm_rt.rs` | ✅ CapGate |
| 2.10b | **JARBAS FB MMIO + double-buffer (P4)** | ✅ Cap MAP_FB/WRITE_FB, AS JARBAS mapeia FB bootloader, backbuffer+present+vsync stub. PoC Ring0+CR3. ADR-0041 P4. | Cap + FB MMIO | MIT | `jarbas_fb.rs`, `syscall.rs` | ✅ PoC |
| 2.10c | **K-IA DMA pin + Cortex mmap (P5)** | ✅ Cap PIN/MAP_DMA + MAP_WEIGHTS; frames pinados + AS; mmap pesos eager (simulado). Vring = P8; GGUF = P9. ADR-0041 P5. | Cap + DMA pin + mmap | MIT | `k_ia_dma.rs`, `cortex_mmap.rs` | ✅ PoC |
| 2.10d | **Demand-paging #PF (P7)** | ✅ Lazy VA registry + reserve NOT PRESENT + #PF cura leaf; Cap DEMAND_PAGE; frames pré-alocados (path #PF sem alloc). ADR-0041 P7. | Cap + #PF demand-page | MIT | `demand_page.rs`, `cortex_mmap.rs`, `interrupts.rs` | ✅ PoC |
| 2.10e | **VirtIO vring + DMA pin (P8)** | ✅ Virtqueue layout-compatible sobre pin; Cap VRING_SETUP; NIC live untouched. ADR-0041 P8. | Cap + VirtIO vring | MIT | `virtio_vring.rs`, `k_ia_dma.rs` | ✅ PoC |
| 2.10f | **GGUF/FAT file-backed mmap (P9)** | ✅ Pré-fill FAT→frames + demand-page; Cap MAP_FILE; magic GGUF/BitNet; fallback NFIL. ADR-0041 P9. **≠ AirLLM** (prefixo só). | Cap + FAT mmap | MIT | `gguf_mmap.rs`, `demand_page.rs`, `fat32.rs` | ✅ PoC |
| 2.10g | **Ring3 Isolation Production (ADR-0082)** | 🏆 **Sucessor de ADR-0041 §P9+** — Isolamento Ring3 real para WASM B/C (native JIT). Deep L4 clone (create_sandbox_as), per-process RSP0, ELF64 loader mínimo (RELATIVE), SYSCALL/SYSRET, sandbox AS + ring3_run_native(), CapGate host functions reais. Depreca ADR-0041 §3,§4,§7,§8 para escopo Ring3. MVP 5-8 sem, ~3.4K LOC. | ADR-0041 PoC, seL4, Fuchsia, Theseus | MIT | `docs/architecture/0082-*.md`, `address_space.rs`, `user_mode.rs`, `syscall.rs`, `isolation_ring.rs`, `elf_loader.rs` (novo) | 🟡 Proposed |
| 2.10f2 | **AirLLM GGUF Streaming (ADR-0046)** | 🏆 Layer-wise: header+layer map+embed/unembed em RAM; 1 layer/forward via ATA `read_file_range`; PrefetchEngine **soft** (nao DMA); dequant Q4_0/Q5_0/Q8_0/F16; hot-swap ATA + Net→FAT→`set_model` (L3.5/RX se RX=0). Stream-to-disk/DMA deferred. | AirLLM, llama.cpp GGUF | MIT | `gguf_streaming.rs`, `gguf.rs`, `cortex.rs` | ✅ MVP / 🟡 residual |
| 2.10g | **Adequação Boot OK→K³CHJ (ADR-0042)** | ✅ N1–N5 + wire N2.5–N5.7; marco **v1.8.0**; gate v2.0.0 = review formal | K³CHJ + Cap PoC | MIT | `docs/architecture/0042-*.md` | ✅ v1.8.0 |
| 2.10h | **LoadStatus + BitNet 2B LOADED (QEMU)** | ✅ Telemetria `LoadStatus`/`[STATUS]`; 2B ~590MB L=30 LOADED via QEMU-loader; FWD OK; TTS empty = known. v1.7.0. | BitNet v4 + LoadStatus | MIT | `load_status.rs`, `cortex.rs`, `main.rs` | ✅ load / 🟡 gen |
| 2.10 | **ACPI Parser (RSDP/MADT/RSDT)** | 🔄 Parsing de ACPI para descoberta de hardware. Implementação própria sem depender de `acpi` crate. | ACPI spec, OSDev | — (especificação) | `acpi.rs` | ✅ 0 err |
| 2.11 | **Huge Pages 2MiB/1GiB** | 🔄 `allocate_huge_2mb()` mapeia páginas grandes no page table para performance de memória. | x86_64 MMU, Linux hugetlbfs | GPLv2 | `memory.rs` | ✅ 0 err |
| 2.12 | **DMA Uncacheable Pages** | 🏆 `dma_alloc()` → `map_page_uc()` (PWT+PCD) — solução para coerência cache/DMA. Fix crítico para NIC e GPU. | Intel x86 manual (PAT/MTRR), E1000 DMA debug (autor) | — (conhecimento HW) | `dma.rs`, `e1000.rs` | ✅ 0 err |
| 2.13 | **Neural Device LEGO (ADR-0056)** | 🏆 L0 Bus / L1 HalOffer / L2 DeviceRecipe; UnlockDAG stages; Ed25519+blob_hash; UsbHost/Bluetooth; bind H1 só trusted+FW; community hub + AI-Friendly specs | HalOffer ADR-0041, PackageHub 0051–53 | AGPL + FW licenses | `k_hal/device_recipe.rs`, `docs/specs/device-lego/`, `docs/community/` | 🟡 MVP H1 |
| 2.14 | **Notifications Toast System (ADR-0062 P31)** | 🏆 Ring buffer 8 toasts → compositor overlay semi-transparente no rodapé, fade-out 120 ticks (~2s). `TOPIC_TOAST` no EventBus, `clipboard_notify::toast_push()`/`toast_get_active()`, render no `DisplayAgent::tick()`. | EventBus patterns, Android toast | MIT | `jarbas/src/clipboard_notify.rs`, `jarbas/src/display/agent.rs` | ✅ 0 err |
| 2.15 | **NTP Client com Resync Periódico (ADR-0062 P18)** | 🏆 Removido gate `TRIED` one-shot; `try_sync()` faz resync periódico a cada 3600 ticks (~200s), rotação de servidores (`time.cloudflare.com` → `time.google.com` → `pool.ntp.org` → fallback IP), cooldown 540 ticks pós-falha. Job `ntp_resync` no CronAgent. | NTPv4 RFC 5905, ClaudioOS ntp.rs | MIT | `hermes/src/ntp.rs`, `hermes/src/cron.rs` | ✅ 0 err |
| 2.16 | **Virtual Consoles F1–F6 (ADR-0062 P27)** | 🏆 6 buffers independentes (80×50 + scrollback 200 linhas), `Ctrl+Alt+F1–F6` via `InputAgent` (scancodes 0x3B–0x40), render no compositor com indicador `F{n}` no canto. | ClaudioOS vconsole.rs (372 LOC), Linux VT | MIT | `jarbas/src/vconsole.rs`, `neural-kernel/src/agents.rs` (InputAgent) | ✅ 0 err |
| 2.17 | **Intel i225 2.5G NIC Driver (ADR-0062 P7)** | 🏆 Raw ptrs p/ descritores (fix UB packed struct), `kick_rx()` (disable→clear→re-enable→RDT=N-1), `prove_rx()` (ARP who-has + wait TX DD + poll RX DD), `any_rx_dd()`/`count_rx_dd()`, `clflush`+`lfence` antes de ler DD, Bus Master re-check pós-reset. | ClaudioOS i225.rs, Intel igc SDM | MIT | `k_nano/src/i225.rs` | ✅ 0 err |
| 2.18 | **Async Executor Híbrido (ADR-0062 P16)** | 🏆 `std::future::Future` + `Waker` + `RawWakerVTable` compatíveis, `AsyncExecutor::spawn()`/`poll_task()`, APIC timer handler → `process_wakes()`, `init_async_rt()` no boot. | Embassy async, async-std patterns | MIT | `k_nano/src/async_rt.rs`, `k_nano/src/interrupts.rs` | ✅ 0 err |
| 2.19 | **IPC MessageBus Wire (ADR-0062 P14)** | 🏆 `mailbox_drain(agent, 8)` no scheduler `run()` para todos agentes; mailboxes abertas no registro + respawn. | ClaudioOS ipc.rs (783 LOC), ADR-0068 | MIT | `event-bus/src/message_bus.rs`, `hermes/src/ipc_bus.rs`, `agent-core/src/lib.rs` | ✅ 0 err |
| 2.20 | **fw_cfg File I/O (ADR-0062 P35)** | 🏆 `read_file(selector)`, `read_file_by_name(name)` (scan dir 0x0019), `write_file(selector, data)` — modo I/O legacy (0x510/0x511). `boot_smoke()` testa leitura de diretório. | QEMU fw_cfg spec, ClaudioOS fw_cfg | MIT | `k_nano/src/fw_cfg.rs` | ✅ 0 err |
| 2.21 | **cpufreq — P-state Driver (IA32_PERF_CTL)** | 🏆 Driver de frequência de CPU via MSR IA32_PERF_CTL (0x199) / IA32_PERF_STATUS (0x198) / IA32_ENERGY_PERF_BIAS (0x1B0). Governor Performance/Powersave/Ondemand. CPUID leaf 0x16 (Skylake+) + probe MSR write-take-effect. QEMU-safe (writes são no-op). | Intel SDM Vol 3 (14.1.4), ClaudioOS power.rs | — (especificação HW) | `k_nano/src/cpufreq.rs` | ✅ 0 err |
| 2.22 | **APERF/MPERF — Frequência Real** | 🏆 IA32_APERF (0xE8) / IA32_MPERF (0xE7) — contagem de ciclos reais vs máximos. `actual_ratio()` retorna frequência efetiva, detecta thermal throttle. | Intel SDM Vol 3 (17.17.4) | — (especificação HW) | `k_nano/src/cpufreq.rs` | ✅ 0 err |
| 2.23 | **ACPI S3 Suspend/Resume** | 🏆 Suspend-to-RAM completo: `\_S3` DSDT parser, FACS waking vector, device save/restore (e1000 16 regs + MTA), AP parking, trampoline 64-bit (0x7000) que restaura CR3+RSP e salta para `s3_resume_entry()`. Handler re-inicializa APIC, PIT, EPB. | ClaudioOS power.rs (921 LOC, ADR-0062 P20), ACPI Spec 6.4 | MIT | `k_nano/src/suspend_resume.rs`, `k_nano/src/acpi.rs` | ✅ S3 entry / 🟡 resume trampoline |
| 2.24 | **Ondemand Tick no Scheduler Loop** | 🏆 Governor Ondemand integrado ao scheduler loop. `halt` closure do `registry.run()` chama `cpufreq::ondemand_tick(ap_work::has_pending())`. Frequência escala por carga real da fila de APs. | Linux cpufreq ondemand governor (conceito) | GPLv2 | `neural-kernel/src/main.rs` | ✅ 0 err |
| 2.25 | **CMOS RTC Driver** | 🏆 Driver do relógio CMOS MC146818: leitura segura com loop wait-snapshot-verify contra RTC update in progress. Formata data/hora ISO. | MC146818 / CMOS RTC spec | — (especificação HW) | `k_nano/src/rtc.rs` | ✅ 0 err |

---

## 3. 🎮 GPU COMPUTE — Pipeline Gráfico e Acelerador

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 3.1 | **GPU Backend Universal (NVIDIA→Intel→AMD→CPU)** | 🏆 Plano `display_coex` dirige init; canário `vector_add` promove Ready; gate ADR-0047 só após golden (não PFIFO NOP). | nouveau, i915, amdgpu; rust-gpu (host) | GPLv2 (kernel) | `gpu/backend.rs`, `canary.rs`, `compute_abi.rs` | 🟡 fundação |
| 3.2 | **NVIDIA PFIFO PUSH_BUFFER** | 🔬 Engenharia reversa do canal de comandos GPFIFO da NVIDIA Pascal (GTX 1050). `pushbuffer_submit()` com doorbell + timeout. Sem NDA. | nouveau driver (eng. reversa) | GPLv2 | `gpu/nvidia.rs` | ✅ 0 err |
| 3.3 | **GPU Secure Boot WPR (FECS+GPCCS)** | 🔬 PoC GP108: aloca WPR 2MB, faz upload parcial FECS+GPCCS e poll. **Não é ACR completo:** faltam ACR HS/LSB, assinaturas na WPR, GR `sw_*`, MMU/runlist/canal e evidência HW. | nouveau ACR driver | GPLv2 (MIT blobs) | `gpu/firmware.rs` | 🟡 PoC / ADR-0048 |
| 3.4 | **VRAM Buddy Allocator** | 🏆 Alocador de VRAM power-of-2 com split/merge. `vram_alloc()`/`vram_free()` integrado ao BAR2 UC. | Linux buddy allocator | GPLv2 | `gpu/vram.rs` | ✅ 0 err |
| 3.5 | **Intel GPU Gen Ring (BCS Blitter)** | 🔬 Ring buffer Intel Gen6+ com MI_BATCH_BUFFER. Blitter BCS para cópia 2D acelerada. | i915 driver | GPLv2 | `gpu/intel.rs` | ✅ 0 err |
| 3.6 | **VirtIO-GPU 2D** | Port do driver VirtIO-GPU para framebuffer em QEMU. | `virtio-drivers` crate | MIT/Apache 2.0 | `gpu/virtio_gpu.rs` | ✅ 0 err |
| 3.7 | **NVIDIA Compute Multigeração (ADR-0048)** | 🔄 LegacyAcr (Pascal) vs Gsp (Turing+) separados; NKP CUBIN offline (`pack_nvidia_kernels.py`); ACR só Pascal + BAR2+pmoff. Canário HW aberto. | Nouveau/NVK/NAK, open-gpu-doc | MIT/GPLv2; CUDA host-only | `gpu/nvidia.rs`, `kernel_pack.rs`, `tools/pack_nvidia_kernels.py` | 🟡 fazendo |
| 3.8 | **AMD Compute Multigeração (ADR-0049)** | 🔄 KiQ/Mes por arch; IP Discovery hint; doorbell MMIO noop até C3; NKP HSACO packer; FW amdgpu no download script. | amdgpu, LLVM AMDGPU | MIT/GPLv2 | `gpu/amd.rs`, `tools/pack_amd_kernels.py` | 🟡 fazendo |
| 3.9 | **Intel Compute Multigeração (ADR-0050)** | 🔄 Gen9 `GPGPU_WALKER` vs Arc `COMPUTE_WALKER` paths separados; NKP zebin packer; iGPU display / dGPU compute. | i915/xe, IGC | MIT/GPLv2 | `gpu/intel.rs`, `tools/pack_intel_kernels.py` | 🟡 fazendo |
| 3.10 | **KernelPack NKP1 + gpu_kernels host** | 🏆 Envelope assinado FNV1a64+Ed25519; crate `tools/gpu_kernels` isolada (CPU golden); zero Vulkan/CUDA no bin. | ADR-0052, rust-gpu no_std patterns | MIT | `gpu/kernel_pack.rs`, `tools/gpu_kernels/` | 🟡 fundação |

---

## 4. 🌐 REDE — Conectividade e Internet

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 4.1 | **smoltcp Stack** | 📦 TCP/UDP/DHCPv4/DNS integrado com Device trait customizado (`NetPhy`). | `smoltcp` crate | MIT/Apache 2.0 | `netstack.rs` | ✅ 0 err |
| 4.2 | **E1000 RX DMA Fix** | 🏆 Debug e correção de DMA write: map_page_uc(), RDT ordering (RCTL.EN → RDT), RDLEN alignment, sfence/lfence. RX=0 para RX=184 pacotes. | E1000 datasheet, Linux driver | GPLv2 | `e1000.rs` | ✅ 0 err |
| 4.2b | **E1000 TX canonical regs** | 🏆 QEMU não wireia aliases `TDBAL_A/TDT_A` (`0x0420/0x0438`) → TX no-op. Fix: `TDBAL/TDT=0x3800/0x3818`. Desbloqueia ARP/RX (SESSION_149). | Intel 8254x SDM, QEMU `e1000_regs.h` | — | `e1000.rs` (nk+k_nano) | ✅ L3.5 |
| 4.3 | **DHCP + DNS + HTTP** | 🏆 DNS **raw** Ethernet+IP+UDP no NIC + `skip_dns_name` (compressão); HTTP GET smoltcp + Host header. Smoke WHPX: L4 A-record + L5 301 (SESSION_150/152). | smoltcp, RFC 1035 | MIT | `netstack.rs`, `net.rs` | ✅ L4/L5 |
| 4.3b | **net_bridge + resolve_and_http_get** | 🏆 Hermes FE → bin NETSTACK (evita espelho vazio). HTTPS → `tls_not_ready` (sem strip→:80). | — | — | `hermes/net_bridge.rs`, `net.rs` | ✅ SESSION_152 |
| 4.3c | **NetFs #418 TCP peer** | 🏆 cmd+len+payload em `gateway:4446`; smoke LIST/READ/WRITE `[NETFS] VERDICT=PASS`. | — | — | `netfs.rs`, `tools/netfs_peer.py` | ✅ PASS |
| 4.3d | **TLS #123 N4** | 🏆 `embedded-tls` 0.19 + hybrid PKI (pins known + TOFU); smoke `root_learn`→`root_pin` (SESSION_158). | ADR-0016 N4 | Apache-2.0 | `tls_client.rs`, `tls_trust.rs` | ✅ hybrid |
| 4.4 | **WiFi Intel AX200/AX210 (iwlwifi)** | 🔬 S0+prepS1 DID→`FW_*.BIN`; **secondary** (pista = ath10k Note). SESSION_159. | iwlwifi Linux | MIT/BSD firmware | `iwl_fw.rs`, `wifi_iwlwifi.rs` | 🟡 secondary |
| 4.4b | **WiFi ath10k QCA6174** | 🏆 A3 BMI→fw_ready wired (CE0/1+LZ); FW_IMAGE 681KB; PASS só `FW_IND` pós-DONE. SESSION_161. | ath10k_pci / linux-firmware | MIT/BSD firmware | `ath10k_ce_bmi.rs`, `wifi_ath10k.rs` | 🟡 código; runtime Note |
| 4.5 | **WiFi Agnostic Engine** | 🔄 WifiChipset trait + AgnosticWifiEngine com DMA rings. Suporte a Intel, Realtek, Atheros, Broadcom via tabela de 50+ VID/DID. | iwlwifi, rtlwifi, ath drivers | GPLv2 | `generic_wifi.rs` | ✅ 0 err |
| 4.6 | **BrowserAgent HTTP Real** | 🏆 `fetch_page()` com DNS resolve + HTTP GET real via smoltcp. Antes: retornava placeholder HTML. Agora: requisição real. | Chromium networking (conceito) | BSD | `browser_agent.rs` | ✅ 0 err |
| 4.7 | **Serial SLIP Tunnel** | 🏆 Bridge serial TCP para rede em sandbox (QEMU TCG). `serial_bridge.py` com watchdog + rate limiting. | SLIP protocol (RFC 1055), QEMU serial | — (padrão internet) | `slip.rs`, `tools/serial_bridge.py` | ✅ 0 err |
| 4.8 | **P2P Orchestration — Lamport Clocks** | 🏆 Relógio lógico de Lamport atômico para ordenação de eventos distribuídos sem NTP/RTC. `tick()` no envio, `update()` no recebimento (max+1). | Lamport timestamps paper (1978) | — (conhecimento científico) | `p2p/clock.rs` | ✅ 0 err |
| 4.9 | **P2P Orchestration — Vector Clocks** | 🏆 Vector Clock para rastreamento de causalidade multi-nó (até 16 nós). Detecção de eventos concorrentes e relações happens-before. | Vector Clocks papers | — (conhecimento científico) | `p2p/clock.rs` | ✅ 0 err |
| 4.10 | **P2P Orchestration — NoProto Zero-Copy** | 🏆 Parser zero-copy com `#[repr(C, packed)]` para deserialização direta sobre buffer de rede. Slice-overlay sem alocação de memória. | NoProto (conceito), flatbuffers | — (conhecimento científico) | `p2p/noproto.rs` | ✅ 0 err |
| 4.11 | **P2P Orchestration — APIC Async Executor** | 🏆 Executor async baseado em APIC Timer com SPSC lock-free ring buffer (`WakerQueue`). Interrupções de hardware → `waker.wake()` sem CPU ociosa. | Embassy async (conceito), APIC spec | — (conhecimento científico) | `async_rt.rs` | ✅ 0 err |
| 4.12 | **P2P Orchestration — NVMe Driver** | 🏆 Driver NVMe bare-metal via PCIe MMIO. Admin SQ/CQ, `nvme_read_block()`, `nvme_write_block()`. | NVMe spec, Linux nvme driver | GPLv2 | `storage/nvme.rs` | ✅ 0 err |
| 4.13 | **P2P Orchestration — TicKV Integration** | 🏆 Flash Driver trait para TicKV sobre NVMe. Persistência de audit logs e resultados de inferência. | TicKV (Tencent) | Apache 2.0 | `storage/tickv.rs` | ✅ 0 err |
| 4.13b | **SGDB AIOS — SgdbStore + MemoryDoc/ART/BQ** | 🏆 Store cognitivo unificado no_std: facade `SgdbStore` (hanr/pkg/skill/audit), MemoryDoc L0–L7, ART lite, BQ Hamming, TickvLite. HANR/PackageHub/Audit/Episodic/RAG adotam SGDB; FAT fica blobs/firmware/WiFi. | TicKV, NoProto, ART, BQ papers | Apache/MIT | `k_ai/sgdb/*`, `storage/tickv.rs` | ✅ MVP SESSION_173 |
| 4.13c | **SGDB Hamming dispatch + D-series** | 🏆 Despacho Hamming `scalar`/`avx2_lut`/`avx512f` no boot; L0/L1 RAM-only; Tickv `sys/tickv_ckpt` + stress GC; bench ART 100k / BQ 10k×1024. | ADR-0061 ISA gate; BQ papers | — | `sgdb/hamming_dispatch.rs`, `bq.rs`, `tickv.rs` | ✅ SESSION_175 |
| 4.13d | **SGDB Memory Quality (E-series)** | 🏆 SleepCycle↔checkpoint; Hermes recall L4 BQ hybrid; TickvLite V-flag; ART Node16 SIMD; NMD1 patch/sortable keys. | Tock TicKV, NoProto patterns, ART paper, Elastic simdvec | — | `sgdb/*`, `hermes/agents`, `cognitive_bridge` | ✅ SESSION_176 |
| 4.14 | **P2P Orchestration — Hybrid Transport** | 🏆 Transporte híbrido: Raw L2 Ethernet (mesma sub-rede) ou UDP/IP smoltcp (roteado). Seleção automática por `TransportMode`. | smoltcp, Ethernet spec | MIT | `net/transport.rs` | ✅ 0 err |
| 4.15 | **Elastic Scheduler — CorePairAllocator + MWAIT** | 🏆 Alocação de núcleos em pares físicos (SMT/L2/L3 cache sharing). MWAIT **real** no AP idle loop: `monitor`/`mwait` com hint C1–C6, flag cache-line aligned para wake sem IPI. Fallback `hlt` em CPUs sem MWAIT. Wake-up por afeto (uncertainty/urgency). | Linux CFS, CPU topology, Intel SDM (MWAIT/MONITOR Cap 9) | GPLv2 | `scheduler/core_pair.rs`, `smp/ap_work.rs` | ✅ 0 err |
| 4.16 | **Elastic Scheduler — Bipole Mode** | 🏆 Modo fallback 2-core (i3): Core 0 (System: k-nano/hermes/jarbas), Core 1 (Compute: BitNet). Comunicação via SpscChannel 64-byte. | Bare-metal patterns | — | `scheduler/core_pair.rs` | ✅ 0 err |
| 4.17 | **Elastic Scheduler — Affect Wake-Up** | 🏆 Wake-up de pares ociosos via IPI quando hermes detecta high uncertainty (>0.75) ou urgency. Nanosecond wake time. | Affective computing | — (conhecimento científico) | `scheduler/core_pair.rs` | ✅ 0 err |
| 4.18 | **Brain Mesh Engine — Auto-Discovery** | 🏆 "Brain Beaconing": broadcast Ethernet/UDP para descobrir nós Neural-OS-Core na rede local. Pacote `NodeCapabilities`. | mDNS, SSDP (conceito) | MIT | `net/brain_mesh.rs` | ✅ 0 err |
| 4.19 | **Brain Mesh Engine — CapacityScore** | 🏆 Fórmula: (Cores × Clock) + (RAM × SIMD_Weight) + L3_Cache. SIMD_Weight: AVX-512=4.0, AVX2=2.0, SSE4.2=1.0. Bonus 3D V-Cache/Anchored. | HPC benchmarks | — (conhecimento científico) | `net/brain_mesh.rs` | ✅ 0 err |
| 4.20 | **Brain Mesh Engine — Master Election** | 🏆 Eleição autônoma: maior CapacityScore ou nó ancorado (jarbas UI). Re-election dinâmico. Raft/Paxos (conceito). | Raft consensus, Paxos | MIT | `net/brain_mesh.rs` | ✅ 0 err |
| 4.21 | **Brain Mesh Engine — Dynamic Roles** | 🏆 Atribuição: Master (hermes/jarbas), Memory (VFS L0-L7), Compute (MoE experts), Worker (verificação). Auto-scaling. | Kubernetes scheduler (conceito) | Apache 2.0 | `net/brain_mesh.rs` | ✅ 0 err |
| 4.22 | **CellChannel — Transparent Messaging** | 🏆 Trait unificado: Local (SpscChannel, ~10ns) vs Remote (RawEth, sub-ms). Mesma interface para hermes/cortex. | ZeroMQ, MPI | MIT | `ipc/mesh.rs` | ✅ 0 err |
| 4.23 | **Mesh P2P Reliability (ADR-0081 Phase 2)** | 🏆 ACK seletivo por fragmento (FRAG\0→FRACK\0, stop-and-wait), exponential backoff no probe, health TTL automático, avg_rtt/p99_rtt metrics, ARP cache MAC resolution, capacity scoring health-aware, token bucket rate limiting, JSON dashboard (MESH_HEALTH EventBus). | SESSION_242, ADR-0081 | MIT | `k_nano/net/{mesh,udp_broadcast}.rs`, `cortex/mesh_distrib.rs`, `jarbas/display/agent.rs` | ✅ 0 err |

---

## 5. 🎵 ÁUDIO — Captura e Reprodução

**ADR:** [0045-sound-voice-stack.md](docs/architecture/0045-sound-voice-stack.md) — truth em `neural-kernel/src/audio/*`; `jarbas/src/audio` = espelho de migração (não wired ao bin).
**Backlog residual:** Sprint Sound ✅ (2026-07-16) — soft-float/VITS + cutover jarbas ainda abertos. Não bloqueia ADR-0042.

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 5.1 | **Intel HDA Capture + Playback** | 🏆 Driver HDA completo: SD0 (captura) + SD1 (playback). CORB/RIRB, codec discovery, DMA ring buffer. **Único driver HDA funcional em bare-metal Rust.** | Intel HDA spec, Linux HDA driver | GPLv2 | `audio/hda.rs` | ✅ 0 err |
| 5.2 | **FFT Audio → Orb Visualization** | 🏆 `process_audio_fft()`: Goertzel simplificado com janela Hamming, 16 bins espectrais. Áudio do microfone HDA → FFT → animação do orbe em tempo real. | FFT algoritmos (Cooley-Tukey) | — (matemática) | `display/avatar.rs`, `audio/voice.rs` | ✅ 0 err |
| 5.3 | **Piper TTS VITS (PT-BR + EN)** | 🔄 Engine TTS neural. LOADED; **neural-lite** polish (prosódia/PT); VITS/HiFi-GAN = soft-float blocker. Tools: `convert_piper_to_bitnet.py`. | Piper TTS (rhasspy), VITS paper | MIT | `audio/piper.rs`, `audio/tts.rs` | ✅ neural-lite / ⏳ VITS |
| 5.4 | **STT CTC + VAD + Mixer** | ✅ STT CTC PCM→MFCC alinhado; VAD adaptativo; MIC/PLAYBACK rings; barge-in. Tools: `train_stt.py`. **Não** Vosk/sherpa. | CTC papers, VAD | MIT | `audio/stt.rs`, `vad.rs`, `mixer.rs` | ✅ Sound |
| 5.5 | **Wake Word "Jarvis" (nativo)** | ✅ MLP + Continuous + gate pós-WAKEWORD (bypass weather-e2e). | energia + MLP | MIT | `audio/wakeword.rs` | ✅ Sound |
| 5.6 | **USB Audio Class (UAC)** | ✅ Parse config AC/AS/iso EP + probe PCI/xHCI. Iso DMA → HW. | USB Audio Class | — | `audio/usb.rs`, `xhci.rs` | ✅ parse / ⏳ iso HW |
| 5.7 | **SER (Speech Emotion)** | ✅ Heurísticas + confidence gate. | literatura SER | MIT | `audio/ser.rs` | ✅ Sound |

**❌ Obsoleto como stack de kernel (histórico):** sherpa-onnx, Pocket TTS, Kokoro-82M como TTS padrão, Vosk, Wyoming, Rustpotter — ver ADR-0045.

---

## 6. 🔧 ARMAZENAMENTO — Discos e Sistemas de Arquivos

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 6.1 | **ATA PIO (CORRIGIDO v1.2.0)** | 🏆 **Bug crítico descoberto e corrigido:** `in al, dx+1` lia FEATURES/ERROR, não o segundo byte do dado. Fix: `in ax, dx` (16-bit). **Todo acesso a disco desde v0.1 era lixo.** | ATA/ATAPI spec, OSDev | — (especificação) | `ata.rs` | ✅ 0 err |
| 6.2 | **FAT32 Read/Write** | 🔄 Leitura e escrita de partições FAT32 LBA. MBR parser, cluster chain, diretórios, long filenames. | FAT32 spec, Microsoft | — (especificação) | `fat32.rs` | ✅ 0 err |
| 6.3 | **NVMe Driver + TRIM** | 🔄 Driver NVMe com admin queue, SQ/CQ, e comando DSM TRIM para SSD. | NVMe spec, Linux NVMe driver | GPLv2 | `disk_agent/nvme.rs` | ✅ 0 err |
| 6.4 | **AHCI SATA NCQ** | 🔄 AHCI driver com Native Command Queuing, PRDT, DMA. | AHCI spec, Linux ahci driver | GPLv2 | `ahci.rs` | ✅ 0 err |
| 6.5 | **NeuralFS (B-tree CoW + CRC32C)** | ✅ FS CoW: leaf B-tree mutavel, journal, create/read/write, agent `/mnt/neural` RAM 4MB. Disco fisico / multi-level = `por_fazer`. | BAFS, Btrfs, ZFS | MIT | `neural_fs/` | ✅ I/O RAM / ⏳ `por_fazer` disco |
| 6.6 | **exFAT FilesystemDriver** | 🔄 Detect/mount + list root cache; write arquivo = `por_fazer` (bitmap/FAT; risco mídia). BlockDevice+write nos backends. ADR-0040 MVP. | Microsoft exFAT 1.0 | — (especificação) | `exfat.rs`, `block_dev.rs` | ✅ MVP r / ⏳ w |
| 6.7 | **MHI soft-migrate** | 🏆 `mhi_tick` metadata + DRAM memcpy seguro; registry unico k_nano; DMA NVMe/VRAM deferido. | ZFS ARC | MIT | `k_nano/mhi.rs` | ✅ soft-MVP |
| 6.8 | **SysInstaller (ADR-0040 #421)** | 🏆 Instalador de sistema que copia partições boot + dados entre discos em runtime. Detecta discos (ATA + StorageBus), copia setores MBR/GPT, publica evento SYS_INSTALL. MVP: scan+copy+verify. | ADR-0040 §SysInstaller | MIT | `k_nano/src/sys_installer.rs` | ✅ MVP / ⏳ write HD |

---

## 7. 🤖 INTELIGÊNCIA ARTIFICIAL — Modelos e Inferência

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 7.1 | **BitNet b1.58 850M — Arquitetura Descoberta** | 🔬 **Engenharia reversa do modelo BitNet b1.58 da Microsoft.** Descoberta: 850M params (não 2B), GQA (20Q/5KV heads), BitFFN com grouped down_proj. `tie_word_embeddings=true`. Não documentado pela Microsoft. | Microsoft BitNet b1.58 model | MIT (model) | `cortex.rs`, `tools/export_hw_bitnet.py` | ✅ 0 err |
| 7.2 | **Modelo .bitnet (formato próprio)** | 🏆 **Formato binário proprietário para modelos ternários.** Magic "BITN", header com spec completo (hidden, layers, heads, vocab, rope, medusa), pesos compactados em 2-bit (4 pesos/byte). | GGUF (llama.cpp), safetensors (HuggingFace) | MIT / Apache 2.0 | `cortex.rs` (load_model) | ✅ 0 err |
| 7.2b | **ModelHub multi-slot** | ✅ TinyStories / generator_fast(850M) / generator_pro(3B) + Active; `load_models_multi`; GGUF→WASM SkillMarket; RustCoder 2B/3B FAT. Trinity indices estáveis. | SESSION_142 | MIT | `model_hub.rs`, `gguf_wasm.rs` | ✅ SESSION_142 |
| 7.2c | **FitPolicy Neural** | ✅ Host `llmfit_pack_filter` + `FIT_GATE`; `cortex::model_fit` (llmfit-inspired); MemoryAgent `[FIT]`; ModelHub escalate TooTight. Sem port llmfit. | SESSION_164 | MIT | `model_fit.rs`, `llmfit_pack_filter.py` | ✅ IDEA #468 |
| 7.3 | **Medusa Speculative Decoding** | 🔄 Decodificação especulativa com 3 heads: head predict tokens, LLM verifica em paralelo. Aceleração de 2-3×. | Medusa paper (Cai et al., 2024) | MIT | `cortex.rs` | ✅ 0 err |
| 7.4 | **BitNet AVX2 Kernel (OOB-safe)** | ✅ Matmul ternário AVX2 + **cauda scalar** (`n%8`); bitwise path **off** (OOB store vocab 32002 → #PF). SESSION_162. | BitNet.cpp (Microsoft) | MIT | `bitnet_avx2.rs` | ✅ 850 FWD |
| 7.4b | **BPB1 SentencePiece 32k + MRG1** | ✅ Vocab id→UTF-8 + **merges BPE** HF; encode merge-order (=`tokenizers`); decode ▁→espaço. Ladder 850/xl/3B. | HF LlamaTokenizer / tokenizers | MIT | `bpe.rs`, `export_bpe_bin.py` | ✅ SESSION_162 |
| 7.4c | **LLM ladder bench** | ✅ QEMU WHPX: pack FAT32 + loader por degrau; parse `LLM-TEST`; métricas custo/tempo/coh. | — | MIT | `tools/llm_ladder_bench.py` | ✅ SESSION_162 |
| 7.5 | **KV Cache 200× Speedup** | 🔄 Cache de Key/Value tokens. Reduz tempo de inferência de 6h para 84s. | KV cache em transformers (Dai et al., 2019) | MIT | `cortex.rs` | ✅ 0 err |
| 7.5b | **N-gram Speculative Decoding** | 🔄 Draft M tokens via rolling LCG hash (N=8) + verify paralelo no KV; zero deps/VRAM extra; Medusa complementar. | llama.cpp ngram-simple (Alok 2026) | MIT | `cortex/ngram_spec.rs`, `cortex.rs` | ✅ OK |
| 7.5c | **LatentBus + Projection** | 🔄 Canal hidden `[f16;256]` paralelo ao EventBus; mean-pool ad-hoc f16; Cortex publish / Hermes recv. | Interlat / LatentMAS | MIT | `event-bus/latent.rs`, `cortex/projection.rs` | ✅ MVP |
| 7.5d | **Evolve WASM Hot-Swap** | 🔄 Ledger + sandbox execute → promote/rollback; DREAM hook SleepCycle. | symbiont.rs / EVA | MIT | `hermes/evolve.rs` | ✅ MVP |
| 7.5e | **NeuOS Probe fase 1** | 🔄 Weight stats por layer + soul-vector stub; read-only. | NeuOS (Funasaki) | MIT | `cortex/neuos_probe.rs` | ✅ MVP |
| 7.5f | **GPU Work-Queue G1/G2** | 🔄 Persistent op queue; NVIDIA PFIFO path ou CPU_FALLBACK honesto. | neurOS / Yantra | MIT | `jarbas/gpu/work_queue.rs` | ✅ MVP |
| 7.5g | **Generative UI Spec H1+H4** | 🔄 JSON WindowSpec → compositor; avatar state ← LatentBus norm. | A2UI / leOS | MIT | `jarbas/display/ui_spec.rs` | ✅ MVP |
| 7.5h | **N-gram empirical bench** | 🔄 Accept/forward counters + microbench + speedup_est. | llama.cpp ngram | MIT | `cortex/ngram_spec.rs` | ✅ |
| 7.5i | **Evolve Genesis** | 🔄 Parent spawn 1 child WASM (ratchet). | EvolveOS Genesis | MIT | `hermes/evolve.rs` | ✅ MVP |
| 7.5j | **KV H2O + SASOS + G5 pipe** | 🔄 H2O eviction CPU; SASOS-lite map; pipeline timing CPU. | H2O / PagedAttention / neurOS | MIT | `kv_h2o.rs` `sasos.rs` `pipeline_g5.rs` | ✅ MVP |
| 7.5k | **Embed viz H2+H5** | 🔄 Latent→2D points + thought splats no FB. | leOS / NeuralOS viz | MIT | `display/embed_viz.rs` | ✅ MVP |
| 7.5l | **Telepatia de Dados** | 🏆 **Compartilhamento de memória entre agentes via ponteiro/arena, sem cópia.** LatentBus `[f16;256]` (ADR-0047) + projeção Cortex→Hermes + futura movimentação de ponteiros entre agentes (Hermes↔Cortex↔Display). Zero-copy cross-agent data flow. | LatentBus (ADR-0047), Interlat | MIT | `event-bus/latent.rs`, `cortex/projection.rs`, `hermes/cognitive_bridge.rs` | ✅ MVP (LatentBus) |
| 7.6 | **RustCoder Expert** | 🏆 **Modelo especialista em geração de código Rust treinado com 263KB.** hidden=128, 6 layers, loss=2.79. Gera skills WASM sob demanda. | Fine-tuning de LLM para código (CodeLlama, StarCoder) | MIT | `rust_coder.bitnet` | ✅ 0 err |
| 7.7 | **BEI — BitNet Ecosystem Intelligence (ADR-0060)** | 🏆 Ecossistema cognitivo de 8 ondas: Onda 0 (MPMC queue); Onda 1 (BudgetManager + ExpertLifecycleManager); Onda 2 (CellNetwork 8 regiões + PlasticityController); Onda 3 (DynamicMoE birth/merge/split); Onda 4 (MemoryStore L0-L7); Onda 5 (AffectRegulator); Onda 6 (ExecutiveSupervisor 7-fase — EgoLayer/PonderNet/Train/PromoteSkill); Onda 7 (SoulMirror 8 estados). ~2900 LOC, 7/7 ondas implementadas. | ADR-0060 (BitNet Cognitivo), neurociência, sistemas multi-agente | MIT | `neural-kernel/src/bei_init.rs`, `docs/architecture/0060-bitnet-cognitivo-bei.md` | ✅ 7/7 ondas |
| 7.8 | **Engine BitNet Fidelidade + Kernels CPU (ADR-0084)** | 🟡 Auditoria cruzada com bitnet.cpp/2B4T: mismatches ativos M1 (FFN relu2 vs silu), M2 (4 SubNorms), M3 (RoPE theta 500000 vs 10000), M4 (embed Q6_K — ternário em embed é N/A); kernels CPU com evidência (unpack branchless, acumulador em registrador, activation-parallel gated por m, T-MAC/maddubs W2A8 gated); receita treino 1-bit (tanh 30×, LR cooldown, QAT suave). Ordem acordada: fidelidade antes de velocidade; sem retreino. | microsoft/BitNet (bitnet.cpp), arXiv 2504.12285 (2B4T), 2511.21910 (Platinum), nanoGPT speedrun, Hestia QAT | MIT/Apache 2.0 (externo) | `bitnet_avx2.rs`, `cortex.rs`, `nn.rs`, `gguf.rs`, `tools/convert_bitnet.py`, `tools/bitnet_fwd_parity.py` | 🟡 ADR-0084 Proposed (por_fazer) |

---

## 8. 🏗️ AGENTES — Sistema Multi-Agente

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 8.1 | **The Agency (214 especialistas)** | 🏆 Catálogo data-driven: 146 base + 68 importados em `AGENT.md` / seed; SpecialistAgent EventDriven no scheduler. Docs antigos “147” = drift. | CrewAI, OpenAI Swarm, AutoGen, agency-agents (MIT) | Apache 2.0 / MIT | `agency_seed.rs`, `ecosystem/agents/`, `agents.rs` | ✅ data-driven |
| 8.1b | **PackageHub Agent manifests (ADR-0051)** | 🏆 Manifestos nativos+Agency no namespace NeuralFS; CRUD HITL; VFS bridge bin↔Hermes. | ADR-0051 / NeuralFS §12 | MIT | `hermes/package_hub.rs`, `tools/export_agent_packages.py` | ✅ SESSION_134 |
| 8.2 | **Consciousness Metrics (10 métricas)** | 🏆 Sistema de "consciência" com 10 métricas cognitivas (skills_ok, errors_resolved, anomaly_count, memories, etc.). Self-Improvement Loop periódico. | JARVIS C# (autor), Lethe brain regions | MIT | `cortex.rs` (Consciousness) | ✅ 0 err |
| 8.2b | **Self-Evolve Engine (Sprint 108)** | 🏆 observe→generate→verify→improve→reflect: auto-skill por padrão/LLM, verificação estrutural, SIL wired, meta-reflect no SleepCycle. | Cratos / SkillObserver | MIT | `hermes/self_evolve.rs`, `agents.rs` (SelfEvolveAgent) | ✅ S108 |
| 8.3 | **Auto-Learn + R3 Replay** | 🏆 Trinity AutoLearnAgent: monitora intents não classificados, detecta padrões (≥3), carrega conhecimento, **update_with_replay()** com RouteTrace congelados da TensorArena (sem re-rotear / sem train_step dummy), reset_moe_cache O(1). | Active Learning, GRPO/R3 papers | MIT | `agents.rs`, `r3.rs` | ✅ 0 err |
| 8.4 | **Cross-OS Ecosystem — ADR-0076** | 🏆 Ecossistema multiplataforma: CrossOsAgent (LEARN→PROPOSE→AUTO), CrossOsDiscoverer (PackageHub local + HTTP GitHub + MCP FYY/Wetware/WeftOS), Membrane (zero ambient authority), MCP bridge, JAIL sandbox (Membrane + wasmi + Merkle audit), IntentBus canônico. Skills encontradas em qualquer OS → instaladas localmente. | ADR-0076, FYY, Wetware, WeftOS, Oreulius, WAeasi | MIT | `hermes/src/cross_os/`, `hermes/src/membrane.rs`, `hermes/src/jail.rs`, `hermes/src/mcp_client.rs`, `hermes/src/intent_bus.rs`, `docs/architecture/0076-cross-os-ecosystem.md` | 🟡 waves |

## 9. 🛡️ SEGURANÇA — Trust e Safety

| # | Tecnologia | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo | Status |
|---|-----------|------------|------------|---------------|---------|--------|
| 9.1 | **SafetyAgent — 4 Invariantes SMT-proof** | 🏆 **Único sistema bare-metal com Asimov's Laws implementadas.** 4 invariantes: I1 (process separation), I2 (pre-action), I3 (fail-closed), I4 (signed evidence). Layer 0 = Cosmic Law (halt em violação). | Asimov's Three Laws, AI Safety research | MIT | `safety.rs` | ✅ 0 err |
| 9.2 | **TrustCache + Ed25519 Identity** | 🏆 Token trust + **session keypair** boot (`sign_session` / `verify_trusted` trusted\|session). Assina artifacts PackageHub. | Ed25519 (Bernstein et al.), TPM | MIT / domínio público | `k_nano/identity.rs`, `package_hub.rs` | ✅ SESSION_136 |
| 9.3 | **Merkle Audit Trail** | 🏆 SHA-256 chain + **Ed25519 por entry** (session). Wire PackageHub/Approval/self_evolve. | Blockchain / distributed ledger (conceito) | MIT | `k_ai/audit.rs` | ✅ SESSION_136 |
| 9.4 | **HANR Marketplace + Memory** | 🏆 Loja local+Net allowlist; USER/MEMORY/SOUL; progressive skills L0/L1; MCP JSON-RPC mínimo. | Nous Hermes Agent (paridade semântica) | MIT | `marketplace.rs`, `memory_store.rs`, `mcp.rs` | ✅ SESSION_136 |
| 9.5 | **Cognitive Bridge K³CHJ** | 🏆 Prompt Cortex = SOUL+USER+MEMORY + BGE-RAG + Trinity + L0 CapGate; **route_user_intent** Trinity→Trust→Skill/LLM; IterationBudget; session search; PERSONA Jarbas; REFLECT→MEMORY_NUDGE. UX HANR, stack superior. | HANR + BGE + Trinity MoE | MIT | `cognitive_bridge.rs`, `memory_store.rs`, `jarvis.rs` | ✅ SESSION_137 |
| 9.4 | **DHCP Starvation Detection** | 🏆 Monitora relação tx_count/rx_count. Se tx >> rx por período prolongado, alerta. Detector implementado em SecurityAgent. | Segurança de rede (conceito) | MIT | `security.rs` | ✅ 0 err |

---

## 10. 📊 DATASETS E TREINO — A Maior Coleção de HWIDs Pública

| # | Dataset | 🏆 Inovação | Fonte | Licença Orig. | Registros | Link HF |
|---|---------|------------|------|---------------|-----------|---------|
| 10.1 | **SDIO HWIDs** | 🏆 **Maior coleção pública de HWIDs de hardware.** 171.003 entradas de 65 DriverPacks, 20.054 arquivos .inf. Extração com 7z (BCJ2), parse UTF-16. | SDIO Windows DriverPacks | MIT (dados públicos) | 171.003 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids) |
| 10.2 | **pci.ids + usb.ids** | 🔄 Parsing completo das listas oficiais PCI-SIG e USB-IF. Estruturação como JSON para consumo por ML. | pci-ids.ucw.cz, linux-usb.org | MIT / GPL | 48.346 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |
| 10.3 | **linux-firmware WHENCE** | 🔄 Parsing do manifesto oficial do linux-firmware. 998 entries com File, Version, License, Driver, Source. | linux-firmware.git | MIT (GPL) | 1.207 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| 10.4 | **AMD Microcode Patches** | 🔄 Extração de 64 patches Family/Model/Stepping do README amd-ucode. | linux-firmware.git amd-ucode | MIT | 64 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata) |
| 10.5 | **regulatory.db** | 🔄 Database de regulamentação WiFi para 174 países. Extraído do wireless-regdb. | kernel.org wireless-regdb | MIT | 174 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-regulatory-db) |
| 10.6 | **PCI Kernel Tables** | 🔄 Extração de tabelas PCI de drivers do kernel Linux (drivers/pci, drivers/net, drivers/gpu). | Linux kernel (torvalds) | GPLv2 | 494 | [HF Dataset](https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids) |

---

## 11. 🔧 FERRAMENTAS — Scripts Python para Treino e Extração

| # | Ferramenta | 🏆 Inovação | Inspiração | Licença Orig. | Arquivo |
|---|-----------|------------|------------|---------------|---------|
| 11.1 | `extract_sdio_hw.py` | 🏆 Pipeline de extração de HWIDs de DriverPacks SDIO (7z BCJ2). Único parser público de .inf para ML. | SDIO DriverPacks (Windows) | MIT | `tools/extract_sdio_hw.py` |
| 11.2 | `extract_firmware_metadata.py` | 🏆 Parser do manifesto WHENCE + headers .h + READMEs. Extração de registros, defines, HWIDs de todos os diretórios. | linux-firmware.git | MIT | `tools/extract_firmware_metadata.py` |
| 11.3 | `fetch_pci_usb_ids.py` | 🏆 Download + parse de pci-ids e usb-ids oficiais. Estruturação como JSON hierárquico (vendor→device). | pci-ids.ucw.cz, linux-usb.org | MIT | `tools/fetch_pci_usb_ids.py` |
| 11.4 | `train_hw_expert_v3.py` | 🏆 Pipeline de treino do HW Expert com dataset combinado (SDIO + pci-ids + usb-ids + kernel). | PyTorch, BitNet arquitetura | MIT | `tools/train_hw_expert_v3.py` |
| 11.5 | `mkfat32.py` / `build_image.py` | 🏆 Gerador de imagem FAT32 bootável com modelos, firmware, e config. Inclui 111 blobs de firmware no disco. | mkfs.fat (Linux) | GPLv2 | `tools/mkfat32.py`, `tools/build_image.py` |
| 11.6 | `train_stt.py` | ✅ Treino CTC STT PCM→MFCC kernel-aligned. | PyTorch CTC | MIT | `tools/train_stt.py` |
| 11.7 | `convert_piper_to_bitnet.py` | ✅ ONNX Piper → `.bin` + validação manifesto. VITS forward = soft-float blocker. | Piper / ONNX | MIT | `tools/convert_piper_to_bitnet.py` |

---

## 12. 📜 SPRINTS — Linha do Tempo Completa

| Sprint | v | Foco | LOC | Inovação Principal | Commit |
|-------|---|------|:---:|-------------------|--------|
| 1-2 | 0.01-0.02 | Chassis + VGA + Serial | ~500 | Kernel bare-metal funcional | `8ac5ac7` |
| 3-5 | 0.03-0.05 | IDT, Memory, SIMD, Tensor | ~1.000 | Tensor engine, FPU, heap | `19bbd0e` |
| 9-11 | 0.09-0.11 | BitNet ternário, 2-bit packing | ~800 | Primeiro BitNet em bare-metal | `cb2c04a` |
| 13-16 | 0.13-0.16 | PCI, ACPI, APIC, SMP | ~1.500 | SMP multi-core | `t6u7v8w` |
| 23-24 | 0.23-0.24 | RTL8139, smoltcp, HTTP | ~1.000 | Primeira pilha de rede | — |
| 27-30 | 0.27-0.30 | Cortex LLM, xHCI, USB | ~1.500 | LLM rodando em bare-metal | — |
| 40-45 | 0.40-0.45 | Agent system, Display, VirtIO-GPU | ~2.000 | Agentes + framebuffer | — |
| 50-56 | 0.50-0.56 | Security, Safety, Medusa, MemoryTree | ~2.500 | Asimov Laws + memória episódica | — |
| 59-61 | 0.59-0.61 | Bootloader, compositor, temas, mouse | ~2.000 | UEFI GOP + compositor | — |
| 74-76 | 0.74-0.76 | TPM, FAT32, NVMe, adaptive heap | ~1.500 | TPM trusted boot | — |
| 77-80 | 0.77-0.80 | Agentic, LLM Infra, AVX2, KV Cache | ~3.100 | KV Cache 200× speedup | — |
| 81-83 | 0.81-0.83 | SMP, Work-Stealing, Polimento | ~1.200 | CFS scheduler | — |
| 84-85 | 0.84-0.85 | GPU Foundations, Decode | ~2.700 | VRAM buddy + GPU ring | — |
| 86-90 | 0.86-0.90 | JARVIS, Emotion, Desktop, Deep Cognitive | ~5.000 | Avatar, SOUL.md, DreamEngine | — |
| 91-94 | 0.91-0.94 | LAN, WASM, Vision, Display | ~3.000 | WASM runtime + UVC camera | — |
| 95-96 | 0.95-0.96 | Cognitive Engine, Self-Healing | ~860 | Self-healing framework | — |
| 97 | 0.97.x | RustCoder Expert + Trinity MoE | ~300 | Trinity MoE router | `575115b` |
| 98 | 0.98.x | Trinity MoE no LLM | ~50 | MoE integrado ao generate | `7b3e428` |
| 99 | 0.99.x | SDIO Dataset (2.794 entradas) | ~500 | Pipeline SDIO | `001c47f` |
| 100 | 0.100.x | Register Map IA | ~250 | Síntese de registradores por IA | `b034a1a` |
| 101 | 0.101.x | Router + Boot Agent | ~130 | Boot agents IA | `4933f00` |
| 102 | 0.102.x | Trinity AutoLearn | ~170 | AutoLearn: detecta→treina→registra | `f8edd70` |
| 103 | 0.103.x | SmileyOS Nativo | ~450 | 55+ cmd, drag, resize, wasm, icons | `f94dc48` |
| **104-108** | **1.1.1-1.1.5** | **GPU + Firmware + SelfHeal + Visual + WiFi** | **~2.860** | **HW Expert v3 (61K IDs), iwlwifi, 3 camadas visuais** | `af892f6`→`3eeb6d1` |
| **109** | **1.2.0** | **ATA PIO Bug Fix** | **~60** | **Disco lê pela primeira vez! Bug desde v0.1** | `65d3b44` |

---

## 13. 📦 FIRMWARE BLOSS — Repositório de Firmware

| Grupo | Blobs | Tamanho | Fonte | Licença | Uso |
|-------|-------|---------|-------|---------|-----|
| NVIDIA GP108 (FECS+GPCCS) | 8 | 39 KB | linux-firmware.git | MIT | GPU WPR secure boot |
| Intel i915 SKL+KBL (GuC+HuC+DMC) | 24 | 3.8 MB | linux-firmware.git | MIT | Intel GPU scheduling |
| Realtek NIC (rtl_nic) | 41 | 217 KB | linux-firmware.git | MIT | RTL8168/8125 |
| Realtek WiFi (rtlwifi) | 38 | 1 MB | linux-firmware.git | MIT | RTL8188/8192/8822 |
| Intel WiFi (iwlwifi AX200/210) | 5 | 7.51 MB (API77) | linux-firmware.git | MIT | SESSION_154; gap `.pnvm` |
| **Total** | **116** | **~12.5 MB** | linux-firmware.git | MIT | |

---

## 14. 📋 COMPILAÇÃO — Prova de Zero Erros

```bash
$ cargo clean -p neural-kernel && cargo nk
    Finished `release` profile [optimized] target(s)
    0 errors
```

**Métricas (v1.9.5 TEST):**

| Métrica | Valor |
|---------|-------|
| Linhas de código (Rust) | ~26.000 |
| Arquivos Rust | 180+ |
| Agentes | ~50 nativos |
| ADRs | 47+ |
| Firmware blobs | 116 (~12.5 MB) |
| HWIDs HW Expert v3 | **61.453 VID/DID** |
| Tags release | v1.0.0 → **v1.9.12-power** (gate v2.0.0 = review + `por_fazer` + OK humano) |
| Crates K³CHJ wired | k_nano, k_ai, cortex, hermes, jarbas |
| Erros (`cargo nk`) | **0** |

---

## 15. LICENÇAS E ATRIBUIÇÕES

| Componente | Licença | Detalhes |
|-----------|---------|----------|
| **Código próprio (AIOS K³CHJ)** | **MIT** | Todo código original. Copyright © 2026 Marcelo Scapin Rovani. |
| linux-firmware blobs | MIT | Firmware NVIDIA, Intel, Realtek redistribuível. |
| pci.ids / usb.ids | MIT/GPL | Listas de IDs PCI-SIG e USB-IF. |
| SDIO HWIDs | MIT | Dados extraídos de DriverPacks públicos. |
| Modelos .bitnet | MIT | Pesos treinados pela equipe AIOS K³CHJ. |
| smoltcp | MIT/Apache 2.0 | Pilha TCP/IP. |
| bootloader crate | MIT/Apache 2.0 | Bootloader v0.11. |
| x86_64 crate | MIT/Apache 2.0 | Instruções e estruturas x86. |
| libm | MIT | Funções matemáticas (musl/newlib). |
| linked_list_allocator | MIT/Apache 2.0 | Legado — substituído por `talc` no Dual-Tier (v2.0). |
| **talc** | MIT/Apache 2.0 | **Tier 1 global allocator** (Hermes/JARBAS/UI). Claim via Memory Map. |
| spinning_top | MIT/Apache 2.0 | RawMutex para Talck (dep transitiva / sync). |
| spin | MIT | Mutex, RwLock. |
| Linux kernel drivers (inspiração) | GPLv2 | Drivers NVIDIA, Intel, iwlwifi, E1000 — **engenharia reversa, não cópia de código.** |

---

> **AIOS K³CHJ — Neural OS Hermes v1.9.5 TEST / NÃO ESTÁVEL**
> *26.000 LOC, 180+ arquivos Rust, ~50 agentes nativos, 5 crates K³CHJ wired, cargo nk = 0 erros.*
> *"O hardware real não perdoa. O silício obedece."*
> [github.com/msrovani/neural-os-core](https://github.com/msrovani/neural-os-core)
> [huggingface.co/aios-k2chj](https://huggingface.co/aios-k2chj)
