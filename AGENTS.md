# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v2.0 "K²CHJ Core" 🏆
#   ~26.000 LOC, 180+ arquivos Rust, 247+ agentes, 0 erros
#   Sprints 92→100: v1.0 "Gold Master" — A Era do Silício ✅
#   Sprint 100: Code Freeze — 07/2026
#   Sprints 101→105: v2.0 "Cognição" — Kernel, Cortex, Hermes, K-IA, JARVIS
#   Sprints 106+: K²CHJ wire + ADR-0042 — base v1.8.0; consolidação v1.8.5 TEST
#   v1.8.5 = pós-1.8.0 em teste/não estável; v2.0.0 = gate após review (não declarado)
#   Gate v2.0.0 = N1–N5 + wire + review; v1.8.0 = marco adequação K²CHJ (Jul 2026); não "2.0 completo" sem review
# ════════════════════════════════════════════════════════

# NAVEGAÇÃO RÁPIDA PARA AI DEVS
# ════════════════════════════════════════════════════════
# docs/archive/sprints/sprint-plan-92-100.md → Roadmap histórico v1.0
# TECNOLOGIAS.md               → Catálogo completo de todas as tecnologias (100+)
# docs/architecture/INDEX.md   → Lifecycle e conflitos de IDs das ADRs
# docs/GOVERNANCE.md           → Ciclo IDEA→ADR→sprint→check
# docs/archive/                → Sessões, planos e notas históricas
# docs/memory/STATE.md         → Estado atual do kernel
# docs/memory/IDEA_BANK.md     → 416+ ideias catalogadas com status
# docs/memory/SESSION_INDEX.md → Índice de sessões + lições críticas
# docs/architecture/0041-*.md  → Capability PoC P0–P9
# docs/architecture/0042-*.md  → Adequação Boot OK → K²CHJ (N1–N5)
# CHANGELOG.md                 → Histórico de versões
# ROADMAP.md                   → Roadmap completo (v1.0 → v2.0)
# TODO.md                      → Checklist mestre de tarefas
# crates/k_nano/src/           → Ring 0 — HAL, drivers, PCI, memory (wired no bin)
# crates/k_ai/src/             → SelfHeal, Trust, inventário (wired N2.5)
# crates/cortex/src/           → LLM, MoE, tensores (wired N3.5)
# crates/hermes/src/           → Orquestração, WASM, rede, skills (wired N4.6)
# crates/jarbas/src/           → Display, GPU, persona (wired N5.7)
# crates/neural-kernel/src/    → Bin boot — residuals: cortex.rs, audio/*, agents.rs, net*, fs/*
# tools/                       → Scripts Python (treino, extração SDIO, bridge)
# ════════════════════════════════════════════════════════

# Role and Purpose
You are a Senior Systems and AI Engineer building "neural-os-core", an AI-native bare-metal OS from scratch. One foundational principle: **everything is an Agent or a Skill**. No tasks, no services, no standalone drivers — only agents with manifests, capabilities, and lifecycle.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** `no_std` + `no_main`. No std, no POSIX, no Linux legacy.
2. **Agent/Skill-First:** Every entity is an Agent. 247+ agents: 20 nativos + 147 The Agency + ~80 importados + HW + FS.
3. **Hardware Rings:** Ring 0 (NPU — intent routing), Ring 1 (GPU — tensor), Ring 2 (CPU — agents/skills).
4. **HW Real First:** QEMU/VirtualBox são apenas **desenvolvimento e debug**. Validação final sempre em HW real.
5. **Trinity MoE:** LLM + router treinável + experts (RustCoder, HWIdentify, etc). AutoLearn: detecta necessidade → treina → registra.
6. **Toda tecnologia nova DEVE ser registrada em `TECNOLOGIAS.md`** com ADR, IDEA, arquivo e sprint. Rodar `tools/update_tecnologias.py` após alterações.

# Agent/Skill-First Design Principles
- **Unificação Ontológica:** Tudo é Agent. Drivers → DriverAgent, Daemons → InferenceAgent/RouterAgent.
- **Manifesto Explícito:** Nome, tipo, schedule, trust tokens — nada implícito.
- **Boot = 8 fases event-driven** (SafeHarbor → MemoryCore → SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime). Cada fase publica BOOT_PHASE no EventBus.
- **Activation on Demand:** Apenas Hermes, Display, HwBridge usam Continuous (nativos Net/Input/Cortex/Cron/Security/Safety/Optimizer/Wifi também Continuous). Agency SpecialistAgent (~147) → EventDriven. Continuous não-essencial >5% ticks por 1000 ticks → rebaixado para EventDriven.
- **Trust por Agente:** (token, agent, skill) — não só (token, skill).

# Agentes Nativos (A-001 a A-025)
| Código | Agente | Tipo | Schedule | Função |
|--------|--------|------|----------|--------|
| A-001 | SystemAgent | System | Oneshot | Init, SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | System | Oneshot | Publica SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | Continuous | Scancode IRQ bridge |
| A-004 | NetAgent | Network | Continuous | smoltcp poll + HTTP |
| A-005 | InputAgent | Console | Continuous | Keyboard (PS/2 + USB xHCI) |
| A-006 | CortexAgent | Inference | Continuous | LLM + Medusa + Trinity MoE |
| A-007 | HermesAgent | Router | Continuous | Intent routing + ReAct + Skills |
| A-008 | DisplayAgent | Console | Continuous | Framebuffer BGRA32 + compositor |
| A-009 | NetDriverAgent | Driver | Oneshot | RTL8139 + VirtIO-net |
| A-010 | UsbDriverAgent | Driver | Oneshot | xHCI port scan |
| A-011 | BootSelfHealAgent | System | Oneshot | SelfHeal init |
| A-012 | BootTrustAgent | System | Oneshot | TrustCache init |
| A-013 | PlatformAgent | System | Oneshot | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | Oneshot | MHI + Adaptive Heap |
| A-015 | GpuDriverAgent | Driver | Oneshot | GPU backend detect |
| A-016 | HwDetectAgent | System | Oneshot | HwIdentifySkill + IA device tree |
| A-017 | CronAgent | System | Continuous | Cron Scheduler |
| A-018 | SecurityAgent | System | Continuous | 5 detectores + Pipeline |
| A-019 | SafetyAgent | System | Continuous | 4 invariantes I1-I4 |
| A-020 | OptimizerAgent | System | Continuous | Self-Optimization |
| A-021 | SleepCycleAgent | System | PollEvery(1000) | 5 fases REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT |
| A-022 | AutoLearnAgent | System | PollEvery(200) | Detecta necessidade → treina → registra expert |
| A-023 | WifiAgent | Network | Continuous | 802.11 scan + WPA2 + conexão |
| A-024 | WakeWordAgent | System | EventDriven | Detecção "Jarvis" por energia |
| A-025 | HdaAudioAgent | Driver | Oneshot | Intel HDA audio driver |

# Boot Sequence (v0.109.0)
```
cargo build --release → python tools/build_image.py --bios → qemu
  └─ bootloader 0.11 → kernel_main
  ├── Init: serial, framebuffer, IDT, memory, heap, SIMD
  ├── Phase 0-3: SafeHarbor→MemoryCore→SystemBringup→Diagnostics
  ├── Phase 4: HardwareDiscovery — init_platform_sync (PCI+ACPI+APIC+SMP) sync ANTES dos drivers
  ├── Phase 5: DriverInit — RTL8139/E1000, xHCI, ATA, NVMe, AHCI, GPU (apos plataforma)
  │              PlatformAgent/NetDriverAgent no registry: idempotentes se sync ja rodou
  ├── Phase 6: AgentFleet — The Agency (147 EventDriven) + HW agents (6) + FS (6)
  └── Phase 7: Runtime — HermesAgent + Trinity MoE + Cortex LLM + Display
       Agentes: Cortex, Hermes, Display, Net, Input, Cron, Security,
                Safety, Optimizer, SleepCycle, AutoLearn, Wifi, HDA
```

# Operational Rules
- **Zero Hallucination:** State explicitly if you don't know HW interaction. Don't invent no_std crates.
- **cargo check --release:** 0 errors obrigatório. Dead-code warnings são esperados (política Known Warnings).
- **⚠️ Build incremental mascara erros:** `cargo clean -p neural-kernel` antes de `cargo check --release` revela erros que o cache incremental esconde. Sempre rodar `cargo clean -p neural-kernel` quando erros somem misteriosamente ou após mudanças estruturais (PCI, ATA, imports).
- **Cargo target dirs isolados:** builds paralelos de agentes/checks usam caminhos sob `target/`, nunca `target-*` na raiz. Exemplos: `--target-dir target/agent-<nome>`, `target/check-<nome>`, `target/s106`. Equivalente: `$env:CARGO_TARGET_DIR="target/agent-<nome>"`. `target/` já está no `.gitignore`; leftovers legados `target-*/` na raiz também.
- **Boot sequence:** bootloader 0.11.15 para UEFI/BIOS handoff.
- **Busca Ativa:** Se bloqueado (🔴), busque na internet PRIMEIRO — Context7, GitHub, crates.io, arXiv. Nada de ficar bloqueado.
- **Pós-Tarefa:** Aprenda → Memorize (AGENTS.md + IDEA_BANK.md) → Documente (README, CHANGELOG, STATE, SESSION) → Versione (cargo check) → Git commit + tag.
- **Toda ideia** DEVE ter destino no IDEA_BANK.md. Estados: ✅ 🟡 ⏳ 💰 ❌.
- **Manutenção TECNOLOGIAS.md:** Toda tecnologia nova = linha nova. Avanço de status → update barra. Rodar `tools/update_tecnologias.py`.

# Memory & Documentation
- ADRs em `docs/architecture/`; lifecycle e conflitos no `docs/architecture/INDEX.md`. STATE.md contém apenas o estado atual; SESSION_NNN.md registra evidência e debug.
- **Governança:** seguir `docs/GOVERNANCE.md`: IDEA_BANK → ADR temática → sprint → TODO → implementação + STATE → SESSION → check final de IDEA/ADR. Fixes pontuais podem ir direto a TODO + SESSION.
- **Consultar TECNOLOGIAS.md antes de qualquer decisão arquitetural.**
- **Consultar SESSION_INDEX.md antes de repetir trabalho.**
- **Consultar TODO.md antes de iniciar nova sprint.**

# K²CHJ Workspace Structure (v1.5.0+)
# ═══════════════════════════════════════════════════════════════
# Monolith → 5 crates (wire N2.5–N5.7 ✅ v1.8.0):
#   k_nano ← k_ai ← cortex ← hermes ← jarbas   (dep chain, no cycles)
#   neural-kernel (bin) = integração + residuals bin-only (audio, cortex.rs, net*, …)
# ═══════════════════════════════════════════════════════════════
# Crate       | Files | Function
# ────────────|───────|──────────────────────────────────────
# k_nano      |  73   | Foundation: HAL, drivers, PCI, memory, interrupts,
#             |       | console, timer, ATA, RTC, ACPI, SMP, APIC, VGA,
#             |       | serial, VGA, AHCI, NVMe, xHCI, RDTSC, simd
# cortex      |  13   | Intelligence: LLM inference, BitNet, BPE, tensor ops,
#             |       | attention, Trinity MoE, DeltaNet, TV DSL, transformer,
#             |       | burn_flex, nn, tokenizer, medusa, reasoning
# k_ai        |  22   | Autonomy: self-heal, trust, audit, agency, cognitive,
#             |       | training, inventory, boot_log, hw_agents, shutdown,
#             |       | memory — (safety/security/optimizer/Sleep/AutoLearn → hermes)
# hermes      |  28   | Orchestration: intent routing, ReAct, skills (47),
#             |       | agents (12 FS + 6 HW + 40 repo), event bus,
#             |       | netstack, HTTP, DHCP, DNS, cron, wifi, safety, security
# jarbas      |  28   | Interaction: display compositor, HDA audio, framebuffer,
#             |       | Hermes CLI, wake word, visual 3-camadas, font,
#             |       | VirtIO-GPU, shell
# ═══════════════════════════════════════════════════════════════

# Active Dependencies (neural-kernel)
| Crate | Versão |
|---|---|
| bootloader | 0.11.15 |
| spin | 0.9 |
| lazy_static | 1.4 (spin_no_std) |
| uart_16550 | 0.2 |
| x86_64 | 0.14.11 |
| linked_list_allocator | 0.9 |
| libm | 0.2 |
| pic8259 | 0.10 |
| smoltcp | 0.13 (alloc, medium-ethernet, proto-ipv4, tcp, udp) |
| ed25519-compact | 2.3.1 |
| event-bus | workspace |
| skill-registry | workspace |
| ticket-lock | workspace |

# Key Architectural Decisions (resumo)
- VGA address: `0xB8000 + physical_memory_offset` (runtime)
- Heap: `0x_4000_0000_0000` (512MB, talc — fora do range kernel/bootloader)
- BitNet ternário: ADD/SUB apenas, zero FPU em matmul. 2-bit packing (4 pesos/byte)
- Trinity MoE: LLM + 6 experts + router_weight treinável
- SDIO MoE: 95.812 entradas .inf/.sys reais + análise pefile
- HardwareRegisterMap: gerado por IA (3 níveis: HWID→família→heurística)
- **WHPX + AVX2:** WHPX com `-cpu host` executa AVX2 **nativo**. Só bloquear AVX2 se hypervisor = TCG (QEMU sem accel). Fix em `bitnet_avx2.rs` e `tensor.rs`.
- **Capability MVP (ADR-0041 P0–P9 ✅ PoC):** Boot A+B (`init_platform_sync` **antes** drivers; Agency EventDriven). Escada: AS+CR3+SPSC+Cap+`int 0x90` → CapGate → FB → DMA/mmap → Ring3 `iretq` → #PF demand-page → VirtIO vring layout → GGUF/FAT pré-fill. Demos **non-fatal**. **Não inventar Ring3/SFI/QUEUE_NOTIFY plenos** — PoC ≠ produção. crate `hermes/` ≠ binário até wiring explícito. Detalhe: `docs/architecture/0041-k2chj-capability-rings.md`, `docs/memory/SESSION_107.md`.

# Current Sprint: estabilização v1.8.5 TEST + Sprint Net; gate v2.0.0 permanece fechado.
# Pós-v1.8.0: Sprint 108 ✅; Sound ✅ parcial; ADR-0040/0046/0047 MVPs ✅ com residuals explícitos.
# Áudio: ADR-0045 — truth=`neural-kernel/src/audio`; jarbas/audio=espelho wired mas não re-exportado no bin
# Build: soft-float + alias `cargo nk` (`.cargo/config.toml`); multicore jobs/-Z threads=16
# Não declarar v2.0.0 sem review formal ADR E sem zerar demandas `por_fazer` — só com OK explícito do maintainer.
# Lembrete: gate v2.0.0 = checklist completo (voz Sound + ADRs por_fazer + rede) + OK humano.
# Wire crates: alias `*-crate` + `pub use`; k_nano sem `global-alloc`; residuals = integração bin-only

## Roadmap v2.0 "Cognição"
| Sprint | Foco | Status |
|--------|------|--------|
| **100** | **Code Freeze** Release v1.0.0 | ✅ |
| **101** | Piper TTS + STT + HDA Capture + ATA fix | ✅ |
| **102** | GPU Compute (NVIDIA) + HW Expert v3 + Firmware | ✅ |
| **103–104** | K²CHJ Workspace Migration (5 crates) | ✅ |
| **105** | Ponytail Audit + v1.5.1..v1.5.3 | ✅ |
| **106** | Ecossistema de Anéis Lógicos (10/10 sub-sprints) | ✅ |
| **107** | Voice I/O (clima e2e + skinny EventBus) | ✅ FECHADA (PASS parcial forte+) |
| **Sound** | Pipeline voz + STT PCM + UAC parse | ✅ (soft-float/VITS abertos) |
| **ADR-42** | Adequação N1–N5 + wire crates | ✅ **v1.8.0** |
| **108** | Self-evolving agents (auto-skill generation) | ✅ |

# QEMU Launch (WHPX + VirtIO optimizado)
```powershell
.\run-qemu-whpx.ps1              # Boot normal (sobe SLIP bridge :4444; mata no exit)
.\run-qemu-whpx.ps1 -debug       # Aguarda GDB (-s -S)
.\run-qemu-whpx.ps1 -NoSerialBridge  # sem auto-bridge
# Manual se necessario: python tools\serial_bridge.py
```

Config: WHPX accel, `-cpu host`, VirtIO-GPU, VirtIO-net, 2× serial (file log + tcp COM2 SLIP client).
Disk: `if=ide` (VirtIO-blk ainda nao implementado). Ver `run-qemu-whpx.ps1`.

# Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` — 0 erros
- QEMU boot limpo (BIOS + UEFI + serial tunnel + AHCI + SMP)
- VirtualBox boot test
- Tag `v1.0.0` + release notes
- **Fim da v1.0. v2.0 "Cognição" começa na Sprint 101.**

# Sprint v1.5.0 (Jul 2026) — K²CHJ Workspace Migration
- **Crate taxonomy**: Monólito `neural-kernel` → 5 crates especializados (k_nano, k_ia, cortex, hermes, jarvis)
- **Migration tool**: `tools/migrate_k2chj.py` — mapeia 193 arquivos para crates, corrige 79 refs cross-crate
- **Dep chain**: k_nano (foundation, 73 files) → cortex (intelligence, 13) → hermes (orchestration, 28) → k_ia (autonomy, 40) → jarvis (interaction, 28)
- **k_nano compile**: crate independente — 0 erros (HAL, drivers, PCI, memory, interrupts)
- **Neural-kernel intact**: monólito compila com 0 erros, integra todos os globals via `pub use`
- **Release v1.5.0**: tag git + imagens bootáveis `disk_qemu.raw` (256MB) + `disk_hw.raw` (64MB)
- **Workspace members**: 11 crates (ticket-lock, event-bus, skill-registry, agent-core, boot, neural-kernel + 5 K²CHJ)
- **Gradual migration**: arquivos permanecem no monólito até que main.rs use explicitamente as crates externas

# Sprint v1.2 — ATA PIO Bug + Release Final
- **ATA PIO read bug** (v0.1 → v1.1.5): `read_sectors()` e `identify()` usavam `in al, dx` + `in al, dx+1` para ler palavras de 16 bits do disco ATA. O registrador `io_base+1` NÃO é o segundo byte do dado — é o registrador FEATURES/ERROR. **Todo dado lido de disco desde o início do projeto era lixo.** Fix: `in ax, dx` (16-bit) lê a palavra completa do registrador de dados.
- **Probe ATA**: agora prefere disco com partição FAT32 (type 0x0B/0x0C) sobre GPT (type 0xEE). Antes escolhia o primeiro com MBR, que era o bootloader image (uefi.img), nunca o disco de dados.
- **Impacto**: MBR, FAT32, modelos .bitnet, firmwares FW_*.BIN, credenciais WiFi — nada em disco funcionava. Agora tudo lê corretamente.

# Sprint 102 (v1.1.1–v1.1.2) — GPU Compute + HW Expert v3 + Firmware Pipeline
- **Firmware source**: GitLab mirror `kernel-firmware/linux-firmware` (kernel.org bloqueia HTTP, git clone lento). `python tools/download_firmware.py` usa GitLab.
- **Firmware paths**: NVIDIA GP108 em `firmware/nvidia/gp108/` (FECS+GPCCS, 8 blobs, 39KB). Intel i915 SKL+KBL em `firmware/i915/` (24 blobs, 3.8MB). Realtek NIC em `firmware/rtl_nic/` (41 blobs). Realtek WiFi em `firmware/rtlwifi/` (38 blobs).
- **mkfat32.py**: inclui firmware no FAT32 como `FW_FECS_BL_BIN` etc. NVIDIA mantém nome curto `FW_FECS_BL_BIN` (compatível com `firmware.rs`). Intel/Realtek usam prefixo `FW_I915_`, `FW_RTL_NIC_`, `FW_RTLWIFI_`.
- **SDIO extraction bug**: py7zr não suporta BCJ2 filter. Usar `7z.exe` (7-Zip CLI) para os DriverPacks SDIO. `extract_sdio_hw.py` alterado para usar `subprocess.run([7z, x, path, -otmpdir])`.
- **SDIO extraction**: 65 DriverPacks → 171.003 HWIDs de 20.054 .inf em ~36 min. DriverPacks de vídeo AMD/Intel >1GB frequentemente corrompidos (download incompleto) — verificar "Unexpected end of archive".
- **pci.ids fonte oficial**: `https://pci-ids.ucw.cz/v2.2/pci.ids` (1613KB, 2506 vendors, 21382 devices). Usar regex `^([0-9A-Fa-f]{4})\s+(.+)$` para vendors.
- **usb.ids fonte oficial**: `http://www.linux-usb.org/usb.ids` (713KB, 3427 vendors, 20537 devices).
- **WHENCE file**: manifesto oficial do linux-firmware (462KB, 998 entries). Cada firmware tem File/Version/License/Driver/Source. Parsing: block separator = `---` ou blank line.
- **HW Expert v3**: treinado com 61.453 VID/DID únicos (SDIO + pci.ids + usb.ids + kernel PCI tables). Modelo 128h/6L/8heads, 1M params, 259KB. Loss 3.55→0.389. Token: `hwexpert_v3_full`.
- **CUDA 13.0 dropped sm_61**: GTX 1050 (Pascal, compute cap 6.1) não é suportada pelo PyTorch 2.13+cu130 (requer sm_75+). Treino em CPU.
- **HDA audio pipeline**: `poll_hda_audio()` NUNCA era chamado — samples lidos e descartados. Fix: chamar no `JarvisVoiceAgent::tick()` + publicar `AUDIO_IN` no EventBus.
- **Skills a quente**: Skills NÃO são hardcoded no enum Intent. O fluxo correto é: usuário → WakeWord → Hermes → Chat → LLM → gera skill → SkillObserver registra → executa. Ex: "grava video" ou "imprime formulario" viram skills gerados pelo LLM sob demanda.
- **HW Expert WPR loading**: pipeline implementado em `firmware.rs` (231 LOC). WPR 2MB no topo da VRAM, upload FECS+GPCCS via BAR2, Falcon boot. Só testável em HW real (QEMU não emula NVIDIA).
- **regulatory.db**: wireless regdb (174 países, 5.4KB). Download de `https://kernel.org/pub/software/network/wireless-regdb/`.
- **Firmware metadata extraction**: `extract_firmware_metadata.py` extrai de TODOS os diretórios do linux-firmware (headers .h, WHENCE, READMEs, configs, scripts) — 1207 records, 199KB JSON.
- **HDA playback (SD1)**: HDA tem 4 stream descriptors (SD0-SD3). SD0=capture @0x80, SD1=playback @0xA0. Cada SD ocupa 0x20 bytes. `write_hda_playback()` escreve samples no DMA buffer e o codec reproduz.
- **BrowserAgent real**: `fetch_page()` agora faz HTTP GET via smoltcp TCP com DNS resolve. Antes retornava placeholder HTML. A `net::http_get()` raw existe e funciona desde v1.1.0, mas BrowserAgent não chamava.
- **DHCP starvation detection**: SecurityAgent monitora tx_count/rx_count. Se tx >> rx por período prolongado, dispara alerta. Antes era função vazia `{}`.
- **WiFi iwlwifi CSR/HBUS**: Registradores iwlwifi reais: CSR (0x000-0x07F), HBUS (0x200-0x29C), SRAM (0x400+). ucode loading: wake_ucode() → reset → upload seções (addr+len+data) → alive check → doorbell NMI. Scan via comando 0x34 no SRAM.
- **IwlWifi struct**: `bar` + `pmoff` para MMIO, `sram_write()`/`sram_read()` via HBUS_TARG_MEM_*, `send_cmd()` escreve SRAM + force NMI. Firmware blobs AX200 (cc-a0), AX201 (so-a0-gf), AX210 (so-a0-hr), AX211 (ty-a0-gf), AX101 (Qu-b0-hr) em `firmware/intel/iwlwifi/`.
- **Firmware total**: 116 blobs de firmware em `firmware/` (NVIDIA + Intel i915 + Realtek NIC/WiFi + Intel WiFi), ~12MB, todos MIT license do linux-firmware.git.
- **3 camadas visuais**: Z-order com Layer enum (OrbBackground < HermesOverlay < AppWindows < DockBar). FPS control a 60Hz. Orb reage a FFT audio (16 bins Goertzel). Hermes CLI overlay semi-transparente. Mouse PS/2 integrado (clique dock, close, drag).
- **SelfHealing I3/I4**: SelfHealAgent detecta firmware ausente e skill ausente. Publica HEALTH_ISSUE → Hermes → LLM diagnostica → NetAgent HTTP GET → firmware carregado hot. Pipeline universal pra GPU, NIC, WiFi, qualquer HW.
- **Skills a quente via LLM**: Nenhum skill é hardcoded. O LLM gera skills sob demanda e o SkillObserver registra. Ex: "grava video", "imprime formulario" viram skills gerados pelo LLM, não por enum Rust.

# Lições Críticas Aprendidas
- **Extração SDIO**: Sempre usar `7z x -r *.inf` (não sem `-r`), verificar se extraiu >0 arquivos ANTES de apagar o .7z
- **HW Expert treinado**: 95.4% com 43K devices PCI+USB. .bitnet v4 com header proprio (vocab=64, hidden=32, 4 layers)
- **.bitnet export bug**: vocab_size (u32) e num_medusa (u32) sao 4 bytes, nao 2 como os outros campos u16
- **GPU underutilization**: modelo com hidden=32 da 5% GPU. hidden=128 com batch=4096 satura a GTX 1050
- **Cargo fix**: `cargo fix --release --allow-dirty` resolve imports nao usados automaticamente
- **Cargo clean**: `cargo clean -p neural-kernel` as vezes nao limpa tudo. Usar `Remove-Item -Recurse -Force target`
- **Cargo target dirs isolados**: nunca `target-*` na raiz. Usar `target/agent-<nome>`, `target/check-<nome>`, `target/s106`. Leftover `target-s106/` na raiz fica até cargo soltar o lock.
- **Bootloader v0.11 UEFI**: BIOS image não funciona (triple fault). UEFI (OVMF) funciona. Stack top boundary bug: #PF em `0x180000000`+offset — bootloader não mapeia páginas acima do stack top. Workaround no `init_memory` com `map_to` falha porque frame allocator retorna frames em uso. Solução futura: mapear P3/P2 entries manualmente para frames no final da RAM.
- **Bootloader v0.11 build**: Usar crate `boot` separada com artifact dependency (`bindeps`). `BiosBoot` + `UefiBoot` via build.rs. Bios.img não funciona no QEMU TCG/WHPX, apenas UEFI.
- **WHPX + SMP**: "Unexpected VP exit code 4" com SMP em Windows 11. Usar `-accel tcg` para desenvolvimento.
- **Ponytail Audit (Sprint 99b)**: `embedded-graphics`, `edge-dhcp`, `buddy-alloc` removidos. 19 arquivos mortos deletados (~500 LOC). ~32 transitive crates eliminados. `dump_exception` convertido para lock-free (evita #DF cascade). IST stacks para #PF e #GP handlers. `kernel_stack_size` aumentado para 2MB.
- **Sprint 101 (v2.0 Cognição)**: Piper TTS VITS (366 tensors, PT-BR+EN). STT CTC tiny (55K params, 28 chars). HDA audio DMA capture driver. NVIDIA PUSH_BUFFER GPFIFO compute. ATA slave `read_any()` tenta master+slave. RustCoder treinado (263KB, loss=2.79). BitNet 2B baixado e convertido (202MB).
- **Piper TTS weight loading**: Converter ONNX→.bin via `tools/convert_piper_to_bitnet.py`. Carregar via QEMU loader `-device loader,file=PIPER_PT_BR.BIN,addr=0x110000000`. 366 tensores (15.6M params). ATA FAT32 loading usa `PIPER.BIN`/`PIPER_EN.BIN`/`PIPER_PT_BR.BIN`.
- **STT implementação**: `tools/train_stt.py` treina modelo CTC com PyTorch. `audio/stt.rs` carrega .bin e executa MFCC→2×LSTM→CTC decode. Vocab 28 chars (a-z+space+blank). Treino sintético 100 epochs loss=3.32.
- **HDA driver**: CORB/RIRB para comunicação com codec. SD0 configurado para captura 16-bit 48kHz mono. DMA buffer em phys 0x103000 (16KB). QEMU 11 requer `-audiodev` (não `-soundhw`).
- **NVIDIA GPU compute**: PUSH_BUFFER via GPFIFO entries funciona em HW real (GTX 1050). `pushbuffer_submit()` com doorbell + timeout. Buffer de comandos em phys 0x200000. Sem firmware ACR, VRAM fica em P8 mode (stale reads).
- **ATA PIO bug (v0.1 → v1.1.5)**: `read_sectors()` usava `in al, dx+1` para o byte alto — esse port é FEATURES/ERROR, não dado. Fix: `in ax, dx`. Impacto: TODO acesso a disco desde o início era lixo. MBR, FAT32, modelos, firmware — nada funcionava em disco.

# Referências
- ADR-0036: JARVIS Unified Interaction Layer
- ADR-0037: SMP+GPU Architecture (multi-vendor)
- ADR-0033: On-Device Micro-Learning (Self-Training MoE)
- `docs/ecosystem-analysis.md` para padrões portados (141 repos analisados)

# ════════════════════════════════════════════════════════
## Cursor Cloud specific instructions
# ════════════════════════════════════════════════════════
Contexto durável para agentes rodando no VM Linux da Cursor Cloud (não-óbvio; o
setup de dependências já foi feito pelo update script). Comandos padrão de
build/run continuam em `HOWTO.md`; aqui ficam só as ressalvas do ambiente Linux.

### Toolchain Rust (CRÍTICO — o `rust-toolchain.toml` quebra no Linux)
- `rust-toolchain.toml` fixa `channel = "nightly-x86_64-pc-windows-gnu"` (um canal
  **Windows**). No Linux isso faz `cargo`/`rustc`/`rustup` falharem com
  `target tuple in channel name ...`. **NÃO edite esse arquivo** — ele é necessário
  para o ambiente Windows do dono do projeto.
- O VM usa `nightly-2026-07-05` (rustc 1.98.0-nightly) via a env var
  `RUSTUP_TOOLCHAIN`, já exportada em `~/.bashrc`. Shells interativos pegam isso
  automaticamente; em contextos sem `~/.bashrc` prefixe os comandos, ex:
  `RUSTUP_TOOLCHAIN=nightly-2026-07-05 cargo build --release`.
- Por que **exatamente** a série nightly 1.98: nightly ≥1.99 adiciona
  `forward_overflowing`/`backward_overflowing` à trait instável `Step`, o que quebra
  o crate `x86_64` 0.14.13 (dep transitiva); nightly ≤1.97 ainda não estabilizou
  `str_from_utf16_endian`, usado por `k_nano` (`String::from_utf16le`). Só o ciclo
  1.98 satisfaz os dois. Se precisar reinstalar:
  `rustup toolchain install nightly-2026-07-05 -c rust-src -c llvm-tools-preview -t x86_64-unknown-none`.

### Build / lint / test
- Build + imagens de boot: `cargo build --release` (compila o default-member `boot`,
  que via `bindeps` compila o kernel para `x86_64-unknown-none` e gera
  `target/bios.img` + `target/uefi.img`). Lint canônico: `cargo check --release`
  (0 erros; 1 warning conhecido de import não usado é esperado).
- `cargo test` no host **não funciona** (crates `no_std` bare-metal; há bug pré-
  existente só nos testes, ex. `vec!` sem `use alloc::vec` em `k_nano`). A validação
  real é `cargo check --release` + boot no QEMU.

### Rodar o OS no QEMU (equivalente Linux dos `run-qemu-*.ps1`, que são só Windows)
- Só **UEFI/OVMF** dá boot; a imagem BIOS dá triple-fault. Use `-accel tcg`
  (KVM/WHPX indisponível/instável no VM). OVMF do sistema: `/usr/share/ovmf/OVMF.fd`.
- Gere o disco de dados FAT32 antes: `python3 tools/build_image.py` → `target/disk_qemu.raw`.
- O kernel usa COM2 como peer SLIP; suba o bridge (stdlib) antes, senão o
  `-serial tcp:` cliente do QEMU não conecta e o QEMU sai:
  `python3 tools/serial_bridge.py --port 4444 --watchdog 0 &`
  (para boot sem rede, troque `-serial tcp:127.0.0.1:4444` por `-serial null`).
- Boot headless capturando o log serial (COM1) — QEMU roda pra sempre, use `timeout`:
  ```bash
  timeout 80 qemu-system-x86_64 -m 6G -smp 4 -accel tcg \
    -drive format=raw,file=target/uefi.img,if=ide,index=0 \
    -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
    -drive if=pflash,format=raw,file=/usr/share/ovmf/OVMF.fd,readonly=on \
    -serial file:logs/boot.txt -serial tcp:127.0.0.1:4444 \
    -netdev user,id=n0 -device e1000,netdev=n0 -display none
  ```
- Screenshot do framebuffer: adicione `-vga std -monitor unix:/tmp/mon.sock,server,nowait`
  e mande `screendump /tmp/screen.ppm` no monitor (converta com `ffmpeg`).
- Boot OK ⇒ `logs/boot.txt` mostra as 8 fases, `[ATA] ... slave FAT32`,
  `AgentFleet] 259 agents`, `[SCHEDULER] 259 runtime agents` e `[TIMER] ... tick=`
  incrementando (scheduler vivo).
