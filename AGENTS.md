# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v1.0.0 🏆
#   ~19.000 LOC, 165+ arquivos Rust, 247+ agentes, 0 erros
#   Sprints 92→100: v1.0 "Gold Master" — A Era do Silício ✅
#   Sprint 100: Code Freeze — 07/2026
#   Sprints 101+: v2.0 "Cognição" — Kernel, Cortex, Hermes, JARVIS
# ════════════════════════════════════════════════════════

# NAVEGAÇÃO RÁPIDA PARA AI DEVS
# ════════════════════════════════════════════════════════
# docs/sprint-plan-92-100.md    → Roadmap v1.0 completo (Sprints 92-100)
# TECNOLOGIAS.md               → Catálogo completo de todas as tecnologias (100+)
# docs/sprint-plan-92-99.md    → Roadmap v1.0 legado
# docs/memory/STATE.md         → Estado atual do kernel
# docs/memory/IDEA_BANK.md     → 416+ ideias catalogadas com status
# docs/memory/SESSION_INDEX.md → Índice de sessões + lições críticas
# CHANGELOG.md                 → Histórico de versões
# crates/neural-kernel/src/    → Código fonte do kernel
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
6. **Toda tecnologia nova DEVE ser registrada em `docs/TECNOLOGIAS.md`** com ADR, IDEA, arquivo e sprint. Rodar `tools/update_tecnologias.py` após alterações.

# Agent/Skill-First Design Principles
- **Unificação Ontológica:** Tudo é Agent. Drivers → DriverAgent, Daemons → InferenceAgent/RouterAgent.
- **Manifesto Explícito:** Nome, tipo, schedule, trust tokens — nada implícito.
- **Boot = 8 fases event-driven** (SafeHarbor → MemoryCore → SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime). Cada fase publica BOOT_PHASE no EventBus.
- **Activation on Demand:** Apenas Hermes, Display, HwBridge usam Continuous. O resto dorme até ter evento. Continuous não-essencial >5% ticks por 1000 ticks → rebaixado para EventDriven.
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
  ├── Phase 0-3: Agents de boot (Platform, Memory, SelfHeal, Trust)
  ├── Phase 4: HardwareDiscovery — PCI scan, ACPI, SMP, GPU detect
  │              └─ HwDetectAgent.AI → HWExpert identifica cada device
  │              └─ generate_register_map() → registradores para WiFi
  ├── Phase 5: DriverInit — RTL8139, xHCI, ATA, NVMe, AHCI, WiFi
  ├── Phase 6: AgentFleet — The Agency (147) + HW agents (6) + FS (6)
  └── Phase 7: Runtime — HermesAgent + Trinity MoE + Cortex LLM + Display
       Agentes: Cortex, Hermes, Display, Net, Input, Cron, Security,
                Safety, Optimizer, SleepCycle, AutoLearn, Wifi, HDA
```

# Operational Rules
- **Zero Hallucination:** State explicitly if you don't know HW interaction. Don't invent no_std crates.
- **cargo check --release:** 0 errors obrigatório. Dead-code warnings são esperados (política Known Warnings).
- **⚠️ Build incremental mascara erros:** `cargo clean -p neural-kernel` antes de `cargo check --release` revela erros que o cache incremental esconde. Sempre rodar `cargo clean -p neural-kernel` quando erros somem misteriosamente ou após mudanças estruturais (PCI, ATA, imports).
- **Boot sequence:** bootloader 0.11.15 para UEFI/BIOS handoff.
- **Busca Ativa:** Se bloqueado (🔴), busque na internet PRIMEIRO — Context7, GitHub, crates.io, arXiv. Nada de ficar bloqueado.
- **Pós-Tarefa:** Aprenda → Memorize (AGENTS.md + IDEA_BANK.md) → Documente (README, CHANGELOG, STATE, SESSION) → Versione (cargo check) → Git commit + tag.
- **Toda ideia** DEVE ter destino no IDEA_BANK.md. Estados: ✅ 🟡 ⏳ 💰 ❌.
- **Manutenção TECNOLOGIAS.md:** Toda tecnologia nova = linha nova. Avanço de status → update barra. Rodar `tools/update_tecnologias.py`.

# Memory & Documentation
- ADRs em `docs/architecture/` (39 documentos). STATE.md com estado atual. SESSION_NNN.md para debug.
- **Consultar TECNOLOGIAS.md antes de qualquer decisão arquitetural.**
- **Consultar SESSION_INDEX.md antes de repetir trabalho.**
- **Consultar TODO.md antes de iniciar nova sprint.**

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
- Heap: `0x4444_4444_0000` (fora do range kernel/bootloader)
- BitNet ternário: ADD/SUB apenas, zero FPU em matmul. 2-bit packing (4 pesos/byte)
- Trinity MoE: LLM + 6 experts + router_weight treinável
- SDIO MoE: 95.812 entradas .inf/.sys reais + análise pefile
- HardwareRegisterMap: gerado por IA (3 níveis: HWID→família→heurística)
- **WHPX + AVX2:** WHPX com `-cpu host` executa AVX2 **nativo**. Só bloquear AVX2 se hypervisor = TCG (QEMU sem accel). Fix em `bitnet_avx2.rs` e `tensor.rs`.

# Current Sprint: Sprint 92 — Fundação Estável (~2.000 LOC)
**Roadmap v1.0:** Sprints 92→100 = Gold Master. Ver `docs/sprint-plan-92-100.md`.

## Sprints 92→100 — v1.0 "Gold Master" (A Era do Silício)
| Sprint | Foco | LOC |
|--------|------|-----|
| **92** | Fundação Estável (VirtIO, AHCI, serial, cleanup) | ~2.000 |
| **93** | WASM Runtime + IDE (wasmi, sandbox, marketplace) | ~3.200 |
| **94** | GPU Polish + Display (MSched, compositor, co-exist) | ~2.000 |
| **95** | Memory + VFS Final (BGE HNSW, MHI bridge, agents) | ~2.000 |
| **96** | GGUF + Model Loading (loader, streaming, RoPE) | ~1.500 |
| **97** | Rede + AIOS Evolution (WWW, self-update, marketplace) | ~3.000 |
| **98** | BitNet + Training Pipeline (100M params, fine-tune) | ~2.500 |
| **99** | SkillOpt + Structured Decoding + Code Freeze Prep | ~1.500 |
| **100** | **Code Freeze & Release v1.0.0** | ~500 |

# QEMU Launch (WHPX + VirtIO optimizado)
```powershell
.\run-qemu-whpx.ps1              # Boot normal
.\run-qemu-whpx.ps1 -debug       # Aguarda GDB (-s -S)
python serial_bridge.py          # Tunnel SLIP (outro terminal)
```

Config: WHPX accel, `-cpu host`, VirtIO-GPU, VirtIO-net, 2× serial (stdio + tcp tunnel).
Disk: `if=ide` (VirtIO-blk ainda nao implementado). Ver `run-qemu-whpx.ps1`.

# Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` — 0 erros
- QEMU boot limpo (BIOS + UEFI + serial tunnel + AHCI + SMP)
- VirtualBox boot test
- Tag `v1.0.0` + release notes
- **Fim da v1.0. v2.0 "Cognição" começa na Sprint 101.**

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

# Lições Críticas Aprendidas
- **Extração SDIO**: Sempre usar `7z x -r *.inf` (não sem `-r`), verificar se extraiu >0 arquivos ANTES de apagar o .7z
- **HW Expert treinado**: 95.4% com 43K devices PCI+USB. .bitnet v4 com header proprio (vocab=64, hidden=32, 4 layers)
- **.bitnet export bug**: vocab_size (u32) e num_medusa (u32) sao 4 bytes, nao 2 como os outros campos u16
- **GPU underutilization**: modelo com hidden=32 da 5% GPU. hidden=128 com batch=4096 satura a GTX 1050
- **Cargo fix**: `cargo fix --release --allow-dirty` resolve imports nao usados automaticamente
- **Cargo clean**: `cargo clean -p neural-kernel` as vezes nao limpa tudo. Usar `Remove-Item -Recurse -Force target`
- **Bootloader v0.11 UEFI**: BIOS image não funciona (triple fault). UEFI (OVMF) funciona. Stack top boundary bug: #PF em `0x180000000`+offset — bootloader não mapeia páginas acima do stack top. Workaround no `init_memory` com `map_to` falha porque frame allocator retorna frames em uso. Solução futura: mapear P3/P2 entries manualmente para frames no final da RAM.
- **Bootloader v0.11 build**: Usar crate `boot` separada com artifact dependency (`bindeps`). `BiosBoot` + `UefiBoot` via build.rs. Bios.img não funciona no QEMU TCG/WHPX, apenas UEFI.
- **WHPX + SMP**: "Unexpected VP exit code 4" com SMP em Windows 11. Usar `-accel tcg` para desenvolvimento.
- **Ponytail Audit (Sprint 99b)**: `embedded-graphics`, `edge-dhcp`, `buddy-alloc` removidos. 19 arquivos mortos deletados (~500 LOC). ~32 transitive crates eliminados. `dump_exception` convertido para lock-free (evita #DF cascade). IST stacks para #PF e #GP handlers. `kernel_stack_size` aumentado para 2MB.
- **Sprint 101 (v2.0 Cognição)**: Piper TTS VITS (366 tensors, PT-BR+EN). STT CTC tiny (55K params, 28 chars). HDA audio DMA capture driver. NVIDIA PUSH_BUFFER GPFIFO compute. ATA slave `read_any()` tenta master+slave. RustCoder treinado (263KB, loss=2.79). BitNet 2B baixado e convertido (202MB).
- **Piper TTS weight loading**: Converter ONNX→.bin via `tools/convert_piper_to_bitnet.py`. Carregar via QEMU loader `-device loader,file=PIPER_PT_BR.BIN,addr=0x110000000`. 366 tensores (15.6M params). ATA FAT32 loading usa `PIPER.BIN`/`PIPER_EN.BIN`/`PIPER_PT_BR.BIN`.
- **STT implementação**: `tools/train_stt.py` treina modelo CTC com PyTorch. `audio/stt.rs` carrega .bin e executa MFCC→2×LSTM→CTC decode. Vocab 28 chars (a-z+space+blank). Treino sintético 100 epochs loss=3.32.
- **HDA driver**: CORB/RIRB para comunicação com codec. SD0 configurado para captura 16-bit 48kHz mono. DMA buffer em phys 0x103000 (16KB). QEMU 11 requer `-audiodev` (não `-soundhw`).
- **NVIDIA GPU compute**: PUSH_BUFFER via GPFIFO entries funciona em HW real (GTX 1050). `pushbuffer_submit()` com doorbell + timeout. Buffer de comandos em phys 0x200000. Sem firmware ACR, VRAM fica em P8 mode (stale reads).

# Referências
- ADR-0036: JARVIS Unified Interaction Layer
- ADR-0037: SMP+GPU Architecture (multi-vendor)
- ADR-0033: On-Device Micro-Learning (Self-Training MoE)
- `docs/ecosystem-analysis.md` para padrões portados (141 repos analisados)
