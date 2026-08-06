# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v2.0 "K³CHJ Core" 🏆
#   ~26.000 LOC, 180+ arquivos Rust, ~50 agentes nativos, 0 erros
#   Sprints 92→100: v1.0 "Gold Master" — A Era do Silício ✅
#   Sprint 100: Code Freeze — 07/2026
#   Sprints 101→105: v2.0 "Cognição" — Kernel, Cortex, Hermes, K-IA, JARVIS
#   Sprints 106+: K³CHJ wire + ADR-0042 — base v1.8.0; consolidação v1.8.6 → **v1.9.0 TEST**
#   v1.8.6 = ADR-0041 H4+/H5+/AS + HalOffer; v1.9.0 = Pós-LAN + Residuals 0–7; v2.0.0 = gate após review
#   Gate v2.0.0 = N1–N5 + wire + review; v1.8.0 = marco adequação (Jul 2026); não "2.0 completo" sem review
#   K³CHJ = k-nano + k-hal + k-ai + Cortex + Hermes + Jarbas (histórico K²CHJ = sem k-hal na marca)
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
# docs/architecture/0042-*.md  → Adequação Boot OK → K³CHJ (N1–N5)
# CHANGELOG.md                 → Histórico de versões
# ROADMAP.md                   → Roadmap completo (v1.0 → v2.0)
# TODO.md                      → Checklist mestre de tarefas
# crates/k_nano/src/           → Ring 0 — HAL base, drivers, PCI, memory (wired no bin)
# crates/k_hal/src/            → Ring 1 — DeviceCap, HalOffer, MMIO BE, VirtIO transporte
# crates/k_ai/src/             → SelfHeal, Trust, inventário (wired N2.5)
# crates/cortex/src/           → LLM, MoE, tensores (wired N3.5)
# crates/hermes/src/           → Orquestração, WASM, rede, skills (wired N4.6)
# crates/jarbas/src/           → Display FE, persona (wired N5.7; GPU BE em k_hal)
# crates/neural-kernel/src/    → Bin boot — residuals: cortex.rs, audio/*, agents.rs, net*, fs/*
# tools/                       → Scripts Python (treino, extração SDIO, bridge)
# ════════════════════════════════════════════════════════

# Role and Purpose
You are a Senior Systems and AI Engineer building "neural-os-core", an AI-native bare-metal OS from scratch. One foundational principle: **everything is an Agent or a Skill**. No tasks, no services, no standalone drivers — only agents with manifests, capabilities, and lifecycle.

# Core Architecture & Constraints
1. **Bare-Metal Rust:** `no_std` + `no_main`. No std, no POSIX, no Linux legacy.
2. **Agent/Skill-First:** Every entity is an Agent. ~50 native agents with manifests, plus variable HW agents at runtime.
3. **Hardware Rings:** Ring 0 (NPU — intent routing), Ring 1 (GPU — tensor), Ring 2 (CPU — agents/skills). ⚠️ **Anéis são organização de código, NÃO fronteira de segurança imposta pelo processador** — tudo executa em Ring 0 real; isolamento efetivo hoje = wasmi (Caminho A) + Ring3 gated (ADR-0077, não registrado).
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

# K³CHJ Workspace Structure (v1.5.0+ → v1.8.6)
# ═══════════════════════════════════════════════════════════════
# Monolith → 6 crates de produto (wire N2.5–N5.7 ✅ v1.8.0; k_hal ✅ v1.8.6):
#   k_nano ← k_hal ← cortex
#                  ← k_ai
#                  ← hermes ← jarbas
#   neural-kernel (bin) = integração + residuals bin-only
# ═══════════════════════════════════════════════════════════════
# Crate       | Anel | Function
# ────────────|------|──────────────────────────────────────
# k_nano      | R0   | Foundation: mem, IRQ, PCI cfg, traps, serial
# k_hal       | R1   | DeviceCap, HalOffer, MMIO BE, VirtIO transporte
# k_ai        | R2   | SelfHeal, Trust, inventário, Agency
# cortex      | R2   | LLM BitNet, Trinity MoE, tensores
# hermes      | R3   | Orquestração, WASM, rede, skills, HalOffer client
# jarbas      | R3   | Display FE, persona (GPU BE em k_hal)
# neural-kernel | —  | Bin boot — residuals integração
# ⚠️ Anel aqui = camada lógica de dependência (R0 fundação → R3 aplicação),
#    NÃO privilégio do processador: todo o código roda em Ring 0 (CPL=0).
#    Fronteira de execução não-confiável = wasmi (A) + Ring3 (ADR-0077, gated).
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

# Current Sprint: **v1.9.9 TEST** (SESSION_242) — Mesh P2P Reliability (ADR-0081 Phase 2). Mesh AEAD Tier F (s241); Fase B gate cripto L/F (s240); Fase C completa (s239); Segurança Fase A + FRAG\0 ok (s238); Mesh apps 1+2+3+4 ok (s235).
# ADR-0081 mesh (SESSION_242): **REASSEMBLY 2→16 slots** + **ACK seletivo** (FRAG\0→FRACK\0 stop-and-wait, 3 retries, 50 ticks) + timeout 500→2000 ticks; **probe_node exponential backoff** 50→3200 ticks; **cleanup_peer_health_ttl** (>60s a cada 500 ticks); PeerHealth expandido (avg_rtt EWMA α=1/8, rtt_samples[32], `peer_p99_rtt` via `(count*99+99)/100` — no_std sem f32::ceil); **ARP cache** PEER_MAC_CACHE + `recv_*_with_mac` expõe src_mac; **capacity_weighted_assign health-aware** (unreachable→0, latency/p99 factors); **token bucket** rate limiting (1/tick, burst 20; heartbeat=1, ROLE=2, dados=3); **JSON dashboard**: `PeerHealth::to_json` + `publish_mesh_health` emite JSON array no tópico `MESH_HEALTH`, `mesh_health_json::parse` no_std no Jarbas + lazy subscribe no DisplayAgent. Transporte vive em **k_nano R0**. Commit 7a97556.
# ADR-0081 mesh (s238+): transporte (udp_broadcast frame/send/recv) + serviço (mesh::p2p_tick) vivem em **k_nano R0** (bin re-exporta statics NIC: `pub use k_nano::nic_globals::{RTL8139,E1000,VIRTIO_DEV}`). **Segurança Fase A (s238):** RX fail-closed (assinatura vs pk vinculada → DROP), TOFU via `PK\0`+pk no heartbeat (`PEER_KEYS[16]`, seam SKYNET `peer_public_key()`), anti-replay (clock ≤ last → DROP), todos TX assinam — sec=0/0/0 validado. **Fragmentação MTU (s238):** `send_fragmented`/`recv_fragmented` (`FRAG\0` header 21B, fora-de-ordem OK, timeout 2000 ticks); gate 1200B removido — matmul 64×64 ~17.5KB round-trip OK. Non-heartbeat → EventBus `P2P_PACKET`; skill_sync/marketplace consomem via poll_p2p. BitTorrent: NÃO implementar (veredicto s238 — merkle piece verification quando modelos). `run-qemu-p2p-mesh.ps1`: ASCII puro, socket listen/connect, 8G, OVMF 8.3, -smp 2 MTTCG, -NoDisk. Commits: f240fa4→916d155 (s234-s238).
# SGDB = path cognitivo (HANR/Audit/Pkg meta/Skills/Episodic/RAG); FAT = blobs/firmware/WIFI.CFG/BOOT.LOG. Ver SESSION_172–173 + ADR-0063/0064.
# Emagreçer: lógica nova nas crates; bin só wire/`pub use` — `.cursor/rules/neural-emagrecer-bin.mdc`.
# ADR-0057 Compute Dispatch SMP+GPU+NPU: WS-A wake multi-AP (SIPI direcionado sequencial + stack/PerCpu por-AP + retry; `-smp 4`→APs=3, CorePools r0=1 r1=2 r2=1; contador unificado; bin::smp emagrecido); WS-B/C `cortex::compute` dispatcher (gated `ap_pollable`, deadlock-proof); WS-D GPU só se canário `Ready`; WS-E NPU XDNA/Intel detecção+fallback software; WS-G #412 `cortex::decode` (self-test PASS). On-demand AP-worker (IDT/IPI) + GPU W2A8 + driver NPU = Layer S/HW.
# ADR-0059 Runtime App Factory — **Caminho A ✅**: runtime WASM real **wasmi** (no_std, fuel) roda `.wasm` no bare-metal (`hermes::wasmi_rt`; self-test `add(2,3)=5` PASS). Seletor por IA **A/B/C** (`hermes::app_factory`): A=wasmi (sandbox, default IA não-confiável); B=Cranelift JIT wasm→nativo; C=Rust-subset nativo (rustc-lite-like). `cranelift-codegen` no_std compila (feature `jit-cranelift`, opt-in) mas execução nativa (B/C) é **GATED** por ring de isolamento (ADR-0041 Ring3) + **HITL forte** — segurança primeiro. CapGate nos host-imports `aios::*`. Bare-metal SEM rustc completo (LLVM); **mas Cranelift no_std permite Rust-subset on-device (C)**. Motor do self-improve/heal/update. Supersede ADR-0031(WASM)/0032; aposenta `Op` VM + `wasm.rs` (após bridges). **F7 arena W^X ✅** (`crate::exec_arena`: nativo `mov eax,42`→42 PASS — base JIT; é Ring 0, NÃO isolado). **F6 Ring3 → ADR-0060 dedicada** (BLOQUEADOR: habilitar=triple-fault reboot loop; `TRY_ENTER_RING3=false`). **Porto seguro:** ring gated, boot OK; nenhum código nativo não-confiável roda (só wasmi A). **Conectores no código:** `hermes::app_factory::register_native_ring` (seam) + `neural-kernel::isolation_ring::{init_connectors,ring3_run_native}` (site de impl; NÃO registra até ADR-0060 §6 passar). `isolation_ring_available()` reflete o registro. **F4 ✅** `hermes::wasm_build` (op-IR `Op`→wasm válido + `validate`; alvo da gramática #412 — self-test `a*b+7`→49). **F3 ✅** `app_factory::generate_and_run(op-IR)` monta+roda no wasmi (self-test `(3+4)*2`→14). Restam: LLM emitir op-IR (integração #412) + registrar `Skill`/`agent-wasm` persistente, F5 promover, e F6 Ring3 (ADR-0060, sessão debug).
# ADR-0058 Generative Card Desktop (UI/Jarbas) — **S1–S4 ✅**: `embedded-graphics` `DrawTarget` (`FbTarget`, `jarbas/src/display/eg.rs`) sobre `DoubleBuffer` + `UiDeclaration`/`UiRenderer` (`card.rs`: Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel). `CardWindow` retido no compositor + `UI_SPEC` spawn/close + mouse (close/drag/botão→`CARD_ACTION`). **Orb responsivo e barra de relógios/HUD preservados.** Cards gerados por LLM (#412 `card_json_schema_hint`) ou skill WASM (RustCoder/Codex, ADR-0052) + Cron. Supersede parcial ADR-0047-HMI (H3 ❌); ADR-0036 persona inalterada. QEMU: 3 cards demo (Sistema/Clima/Chamada de Vídeo). S5 (widgets ricos/tema/TTF) + A/V real (HDA/UVC) = residual.
# Residuals 0–7 ✅ + fila lan: net_bridge · NetFs PASS · SelfUpdate HTTP · TLS smoke (SESSION_157).
# Onda 7 LAN: e1000 TX 0x3800/0x3818; DNS raw + HTTP; SESSION_149/150/152.
# Abertos: WiFi ath10k A3 Note AWAITING; LLM coh semântica (#466); /model-fetch e2e; GPU/UAC/DMA AWAITING_HW.
# Áudio: ADR-0045 — truth=`neural-kernel/src/audio`; jarbas/audio=espelho wired mas não re-exportado no bin
# Build: soft-float + alias `cargo nk` (`.cargo/config.toml`); multicore jobs/-Z threads=16
# Não declarar v2.0.0 sem review formal ADR E sem zerar demandas `por_fazer` — só com OK explícito do maintainer.
# Lembrete: gate v2.0.0 = checklist completo (voz Sound + ADRs por_fazer + WiFi/TLS) + OK humano.
# Wire crates: alias `*-crate` + `pub use`; k_nano sem `global-alloc`; residuals = integração bin-only
# **Emagreçer bin:** lógica nova nas crates K³CHJ; neural-kernel só wire/bridge. Ver `.cursor/rules/neural-emagrecer-bin.mdc`.
# Net gate canônico = e1000 + smoltcp (SLIP/COM2 = frozen debug; não é path do gate)

## Roadmap v2.0 "Cognição"
| Sprint | Foco | Status |
|--------|------|--------|
| **100** | **Code Freeze** Release v1.0.0 | ✅ |
| **101** | Piper TTS + STT + HDA Capture + ATA fix | ✅ |
| **102** | GPU Compute (NVIDIA) + HW Expert v3 + Firmware | ✅ |
| **103–104** | K³CHJ Workspace Migration (5 crates) | ✅ |
| **105** | Ponytail Audit + v1.5.1..v1.5.3 | ✅ |
| **106** | Ecossistema de Anéis Lógicos (10/10 sub-sprints) | ✅ |
| **107** | Voice I/O (clima e2e + skinny EventBus) | ✅ FECHADA (PASS parcial forte+) |
| **Sound** | Pipeline voz + STT PCM + UAC parse | ✅ (soft-float/VITS abertos) |
| **ADR-42** | Adequação N1–N5 + wire crates | ✅ **v1.8.0** |
| **108** | Self-evolving agents (auto-skill generation) | ✅ |

# QEMU Launch (WHPX; Net gate = e1000 + user/slirp)
```powershell
.\run-qemu-whpx.ps1              # Boot: e1000 + user net (SLIP default OFF)
.\run-qemu-whpx.ps1 -Window      # com display
.\run-qemu-whpx.ps1 -Bridge      # TAP + ICS/bridge (internet real via WiFi host)
.\run-qemu-whpx.ps1 -SerialBridge  # opt-in SLIP legado (não é Net gate)
python tools\preflight_wave.py --wave 7   # PreFlight residuals
```

Config: WHPX + Haswell/TCG, e1000 + `-netdev user`, VirtIO-GPU. SLIP/COM2 frozen.
Disk: `if=ide`. Ver `run-qemu-whpx.ps1` + `SESSION_149`/`150`.

# HW Boot — Pendrive Unified (Limine UEFI + dados FAT32)
Gera imagem USB bootável para notebook real (Secure Boot OFF).

```powershell
# Limpeza e build (0 erros)
cargo clean -p neural-kernel && cargo build --release -p boot

# Gera target/usb_hw.img (~3.2GB) — GPT com:
#   Part 0:   ESP FAT32 (Limine BOOTX64.EFI + kernel.elf) @ LBA 34
#   Part 1:   Dados FAT32 (modelos, firmware, CONFIG.TXT, BOOT.LOG) @ LBA 262144
#   MBR:      Protective 0xEE (GPT legacy guard, NÃO bootável)
#   Windows:  Monta partição de dados como NEURAL-OS (letra E:)
python tools/build_image.py --hw --unified

# Gravar no pendrive (≥8GB) via Rufus — modo Imagem DD
# Rufus → selecionar target/usb_hw.img → Imagem DD → gravar
# Conectar no notebook → F2/F12 → selecionar USB → Secure Boot OFF → boot
```

**Pipeline interno:** `crates/boot/build.rs` → `tools/limine/mk_esp_fat.py` (gera GPT,
não MBR) → `limine-esp.img` → `uefi.img` → `build_usb_unified.py` → `usb_hw.img`.
`mk_esp_fat.py` foi migrado de MBR-only para GPT completo (protective MBR 0xEE +
EFI PART header + entries + backup) em SESSION_228 — essencial para pendrive real.

**BOOT.LOG:** A partição de dados tem `BOOT.LOG` pré-alocado (256KB). O kernel
tenta escrever via USB-MSC/ATA/AHCI/NVMe (persist_now, ordem). Em HW real sem
ATA, o flush pode falhar se USB-MSC não enumerar a tempo. SysInfoAgent (card
ID=9001) retry periódico até FAT_READY=true.

# Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` — 0 erros
- QEMU boot limpo (BIOS + UEFI + serial tunnel + AHCI + SMP)
- VirtualBox boot test
- Tag `v1.0.0` + release notes
- **Fim da v1.0. v2.0 "Cognição" começa na Sprint 101.**

# Sprint v1.5.0 (Jul 2026) — K³CHJ Workspace Migration
- **Crate taxonomy**: Monólito `neural-kernel` → 5 crates especializados (k_nano, k_ia, cortex, hermes, jarvis)
- **Migration tool**: `tools/migrate_k2chj.py` — mapeia 193 arquivos para crates, corrige 79 refs cross-crate
- **Dep chain**: k_nano (foundation, 73 files) → cortex (intelligence, 13) → hermes (orchestration, 28) → k_ia (autonomy, 40) → jarvis (interaction, 28)
- **k_nano compile**: crate independente — 0 erros (HAL, drivers, PCI, memory, interrupts)
- **Neural-kernel intact**: monólito compila com 0 erros, integra todos os globals via `pub use`
- **Release v1.5.0**: tag git + imagens bootáveis `disk_qemu.raw` (256MB) + `disk_hw.raw` (64MB)
- **Workspace members**: 11 crates (ticket-lock, event-bus, skill-registry, agent-core, boot, neural-kernel + 5 K³CHJ)
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
- **SDIO extraction**: 65 DriverPacks → 171.003 HWID strings raw de 20.054 .inf em ~36 min. Após colapso de variantes SUBSYS/REV → 16.126 únicos; (vid,did) únicos → ~1.005. A maior parte (~44K unique vid/did) vem de pci.ids + usb.ids. DriverPacks de vídeo AMD/Intel >1GB frequentemente corrompidos (download incompleto) — verificar "Unexpected end of archive".
- **171K é o número bruto de strings HWID antes de qualquer colapso.** O dataset real de treino (v4) tem ~44K devices únicos (vid,did) + ~16K variantes = ~60K amostras. Usar "44K unique devices" em vez de "171K" em comunicações precisas.
- **pci.ids fonte oficial**: `https://pci-ids.ucw.cz/v2.2/pci.ids` (1613KB, 2506 vendors, 21382 devices). Usar regex `^([0-9A-Fa-f]{4})\s+(.+)$` para vendors.
- **usb.ids fonte oficial**: `http://www.linux-usb.org/usb.ids` (713KB, 3427 vendors, 20537 devices).
- **WHENCE file**: manifesto oficial do linux-firmware (462KB, 998 entries). Cada firmware tem File/Version/License/Driver/Source. Parsing: block separator = `---` ou blank line.
- **HW Expert v3**: treinado com 61.453 VID/DID únicos (SDIO + pci.ids + usb.ids + kernel PCI tables). Modelo 128h/6L/8heads, 1M params, ~345KB (v3; v4 ≈260KB). Loss 3.55→0.389. Token: `hwexpert_v3_full`.
- **CUDA 13.0 dropped sm_61**: GTX 1050 (Pascal, compute cap 6.1) não é suportada pelo PyTorch 2.13+cu130 (requer sm_75+). Treino em CPU.
- **GTX 1050 FUNCIONA com torch 2.13+cu126 (SESSION_249b):** o drop sm_61 era do cu130, **não do cu126** — `torch.cuda.get_arch_list()` do cu126 inclui `sm_61`. O sintoma "No CUDA GPUs available / device_count=0" com a GPU presente é problema de **detecção**: fix `$env:CUDA_VISIBLE_DEVICES="0"` (o `train_models_gpu.py` já seta na linha 28). Com a env var: GTX 1050 4.3GB, cuda=12.6, treino real funciona. SEMPRE checar `get_arch_list()` antes de assumir incompatibilidade de arquitetura.
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
- **Validar o ARTEFATO exportado, não só o treino em memória (SESSION_247):** o holdout em memória do HW Expert v4 passava (70.6%) mas o arquivo `.bitnet` era 100% zeros — `export_v4` quantiza com threshold 0.5 e `nn.Linear` inicializa ±1/√128≈±0.088 → todo peso vira 0. Gate p/ qualquer pipeline de export de modelo: fração não-zero ≥1% + predições não-constantes + holdout do ARQUIVO via port Rust-exact do loader (`tools/validate_hw_expert_v4.py`). **Formato de export é contrato com o loader:** `num_params` u32 (não u64), prefixo `u32 len + u32 scale` por tensor, embed row-major `wt(f, embed.weight)` (não `.T` — o loader lê índice flat `col*h + row`). Tabela curada > ML: `build_card` deve dar precedência à tabela HWID, nunca deixar o modelo sobrepor PnP conhecido.
- **Gate de host em crate no_std = `#[cfg(target_os = "none")]`, não `cfg(test)` (SESSION_247):** quando a crate é dependência, ela compila SEM `cfg(test)` — um gate só de teste não cobre. Ex.: `probe_port` stub host, IPI no-op host, IDT `cfg(not(windows))` (repr(C, align(16)) quebra codegen MSVC/COFF), p2p_sim gated `feature="p2p-sim"`. `cargo test --workspace --exclude neural-kernel --exclude boot` = 139 testes no host desde 08/2026.
- **SSE/AVX tails: `n%4 != 0` é real (SESSION_247):** heads do HW Expert v4 têm 17/9/10 colunas — clamp do último bloco (`lanes = min(4, n-j)`) para não ler além de n. Antes disso o SSE path assumia n%4==0 e lia além.
- **Dead code por case (SESSION_247):** padrões mistos (`[INST]`) contra input lowercased (`lower.contains`) nunca casavam — era o TESTE que estava errado, não o gate. Se um assert de gate não dispara, confira o case do input.
- **Mesh P2P reliability (SESSION_242):** (1) **2 slots de reassembly + fire-and-forget = perda silenciosa** — 17KB matmul → 18 fragmentos; qualquer perda = reassembly falho. Fix: 16 slots + ACK seletivo (FRAG\0→FRACK\0, stop-and-wait). (2) **no_std sem `f32::ceil`** — p99 index via aritmética inteira `(count*99+99)/100`. (3) **`recv_*` precisa expor src_mac** para ACK direto — `recv_unicast_with_mac()`/`udp_broadcast_recv_with_mac()` retornam `(payload, mac)`. (4) **Jarbas DisplayAgent tem métodos fora do `impl`** (`handle_pointer_click`, `apply_ui_spec` eram `fn` soltas) — falha ao adicionar métodos; dashboard integrado com lazy subscribe + parser JSON externo. (5) **`fill_rect`/`draw_text` APIs RGB**: `fill_rect(x,y,w,h,r,g,b)` 7 args, `draw_text(fb,x,y,text,w,r,g,b)` 8 args — não compactar cor em 1 arg. (6) **Precedência de cast**: `expr as u64.method()` → `(expr as u64).method()`. (7) **param `node_id` sombreia `node_id()` fn** — renomear param (`target_id`).
- **TLS bridge wiring (SESSION_241):** Módulo declarado + implementado ≠ funcional. O padrão bridge (function pointer registrado no boot) exige: (1) tipo da function pointer na crate FE, (2) `register_*()` na crate FE, (3) chamada de `register_*()` no boot com cast explícito, (4) consumers chamando a API da crate FE. O kernel já tinha TLS 1.3 completo (`embedded-tls 0.19`, `HybridProvider`, ECDSA+RSA-PSS) — o gap era exclusivamente o wiring `hermes↔kernel`. Fallback `http://host:443/path` (HTTP na porta TLS) é bug silencioso. Ver `crates/hermes/src/tls.rs` → `fetch_url()`.
- **Custo cripto é inverso à latência onde importa (SESSION_240):** Ed25519 verify ~26-46µs/pacote (custo FIXO, ~0.3 Gbps/core) domina o budget por ~2 ordens vs HMAC ~1.3µs @1.2KB (~8 Gbps). Em datacenter (RTT 0.1-0.5ms) +40µs = +8-40% do RTT (visível); em WAN é invisível. Onde dá pra relativizar o custo é alto; onde não dá a rede engole. **Decisão ADR-0081 Fase B:** mesmo range/subnet provisiona `set_segment_key()` → DADOS com HMAC-SHA256 (tag 32B, `k_nano::crypto`, reusa `tpm::sha256`, sem dep nova); controle/TOFU (heartbeat/ROLE/PK\0) SEMPRE Ed25519 (`sign_packet_authentic`) — é a âncora de confiança e é raro (~1.1s). Fail-closed: sem chave = Full = zero regressão. Anti-replay de dados em Tier L é follow-up (senders usam clock=0). Implementação importa 3-4x (OpenSSL EVP ~100µs vs lib25519 ~32µs; `ed25519-compact` sem SIMD — calibrar no target).
- **`#[target_feature]` funciona em build soft-float:** `#[target_feature(enable = "avx2")]` compila o kernel SIMD mesmo com `-C target-feature=-sse,-sse2,-avx2` no `.cargo/config.toml`. O runtime check `allow_avx2()` decide se chama. SEM `#[cfg(...)]` gate — compila todos os kernels e deixa o runtime escolher. AIOS adaptativo, não recompilação.
- **WHPX filtra CPUID xsave:** `allow_avx2()` não pode depender de `isa.xsave` porque WHPX expõe AVX2 mas esconde o bit XSAVE. Remover `xsave` da gate: `isa.avx2 && isa.avx && !tcg`.
- **`find_child_byte16_sse` com cfg ausente corrompe ART:** A função SSE2 era chamada com `#[cfg(target_arch = "x86_64")]` + runtime `allow_avx2()`, sem `cfg(target_feature = "sse2")`. LLVM com build soft-float gerava código SSE2 inconsistente — `_mm_movemask_epi8` retornava máscara errada. Sintoma: `art_ok=false` mas `art_len==n_art`. Fix: remover cfg e deixar `#[target_feature(enable = "sse2")]` proteger.
- **171K é HWID strings raw, não devices únicos:** O número 171K do SDIO conta TODAS as strings, incluindo variantes SUBSYS/REV. Devices únicos (vid,did) = ~44K. Amostras de treino com variantes = ~60K. Usar "44K unique devices" em precisas, "60K training samples" em contexto de ML.
- **Windows DriverStore exige admin:** .inf em `C:\Windows\System32\DriverStore\FileRepository` são protegidos por TrustedInstaller. `Get-Content` falha sem elevação. Script `extract_wdm_hwids.py` precisa `Start-Process -Verb RunAs`.
- **Cursor auto-checkpoint ao trocar de branch/agente:** mensagem `checkpoint before checking out cursor/…` engole **todo** o working tree dirty (código + `.cursor/plans/*`) num commit com mensagem ruim. Ritual: `git status` limpo (commit nomeado ou stash) **antes** de checkout de outra sessão/agente. Tag pode apontar pro commit errado se criada na branch errada — sempre `git show <tag>`. Ver SESSION_176 “Pós-release — commit intermediário”.
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
- **Framebuffer bpp hardcoded (tela = "chuviscos")**: `probe_uefi_framebuffer` (`crates/jarbas/src/display/fb.rs`) fixava `bpp=3` para `PixelFormat::Bgr`/`Rgb` e derivava `fb_stride = info.stride * 3`. Em framebuffer de 32 bits (QEMU/OVMF e a maioria do HW real reportam `bytes_per_pixel=4`, ex. stride 1280px = 5120 bytes) cada pixel escrevia 3 bytes num slot de 4 e cada linha usava 3840 em vez de 5120 → imagem "escorrega" na diagonal e vira chuviscos; só ficaria certo num painel real de 24-bit. Fix: usar `info.bytes_per_pixel` (cobre 24-bit=3 e 32-bit=4), com `fb_stride = info.stride * bytes_per_pixel`. Todos os consumidores (console fb em `vga_buffer.rs`, splash, P4 `jarbas_fb.rs`) leem `fb_bpp`/`fb_stride` do `GpuDevice`, então corrigir a fonte conserta tudo.
- **FB texto ilegível / sobrescrita (SESSION_139):** TRACE do bootloader + `boot_ckpt` + `fb_print` sem limpar faixa → “fantasma”. Fix: `console_clear` no probe; `console_print` limpa banda da linha; `fb_print`→`console_print`. Stick Windows: MBR FAT dados `0x0C` + ESP `0xEF` (não 0xEE-first). `BOOT.LOG` só pós-heap. GOP `BltOnly` → vendor bootloader SetMode Rgb/Bgr.
- **e1000 TX aliases QEMU (SESSION_149):** `TDBAL/TDT` em `0x0420/0x0438` são aliases Intel **não wired** no QEMU → write no-op → ARP nunca sai → RX=0. **Usar `0x3800/0x3818`.**
- **DNS compressão (SESSION_150):** ao pular nomes DNS, **não seguir** pointer `0xC0xx` no offset de continuação — só +2 bytes no wire (`skip_dns_name`). DNS bootstrap = raw Ethernet+IP+UDP no NIC (smoltcp perde 1º UDP no ARP).
- **Hermes net espelho (SESSION_152):** Browser/Search/Market **não** usam `hermes::net` (NETSTACK vazio). Registrar `hermes::net_bridge` no boot → `neural-kernel::net::resolve_and_http_get_safe`.
- **Deadlock NETSTACK (SESSION_152):** nunca chamar `tcp_exchange` / NetFs smoke **dentro** de `NETSTACK.lock()` em `bootstrap_early` — hang pós-L5. Smoke só após return do bootstrap.
- **HTTPS:** deny `tls_not_ready` até TLS real; **nunca** strip `https://` → porta 80. Stub: `[TLS] VERDICT=BLOCKED reason=softfloat_or_crate`.
- **SMP wake multi-AP (ADR-0057 WS-A, SESSION_163):** SIPI **broadcast** (`all-excl-self`) + stack real/32b/64b e GS.base compartilhados só acorda **1 AP** (0 com ≥2 — corrompem a stack na transição de modo). Fix: IPI **direcionado** por LAPIC ID + wake **sequencial** + **stack/PerCpu por-AP** (`AP_PCPU`) + **retry INIT-SIPI-SIPI 3x** (TCG é flaky). `-smp 4`→APs=3. Contador único = `k_nano::smp::AP_ENTRY_COUNTER` (bin reusa `k_nano::smp::ap_entry`).
- **APs sem IDT (ADR-0057 WS-F):** AP sobe com IF=0 e sem `lidt`; `hlt` sem trabalho **trava o AP**. Por isso `cortex::parallel_*` (WS-B) é **gated por `k_nano::smp::ap_pollable()`** (hoje false) → BSP faz o matmul, sem deadlock. Usar APs como workers vivos exige IDT compartilhada + reschedule-IPI (Layer S/HW).
- **embedded-graphics em bare-metal (ADR-0058):** `embedded-graphics` 0.8 compila limpo no `x86_64-unknown-none` soft-float. Seam = implementar `DrawTarget` p/ o `DoubleBuffer` (`jarbas/src/display/eg.rs`); UI declarativa em `card.rs` (`UiDeclaration`/`UiRenderer`). Orb (`draw_orb_layer`) e HUD (`gauges::draw_status_gauges`) **não** são substituídos — cards ficam por cima (Layer 2b). Fontes embedded-graphics são **ASCII 0x20–0x7E**: evitar `—`/acentos nos títulos (viram `?`).
- **SKILL_REGISTRY shadow (SESSION_217, P001):** `lazy_static! { static ref SKILL_REGISTRY }` privado no main.rs shadowing o `pub` de k_nano = **silenciosamente invisível** p/ hermes/k_ai. Skills registradas no bin nunca chegavam às crates. Fix: remover shadow, criar `register_builtin_skills()` que registra no `k_nano::SKILL_REGISTRY` antes do Phase 6. Sempre usar o singleton da crate base.
- **Statics duplicados bin↔crate (SESSION_237, BGE case):** `neural-kernel::memory_systems` era cópia de `k_ai::memory_systems` — o boot carregava o BGE nos statics do bin e `k_ai` nunca via o modelo → semantic recall rodava **silenciosamente** em pseudo-hashes 64d. Fix: bin delega com `pub use k_ai::memory_systems::*;`. Fonte única sempre na crate base. **Guarda (SESSION_244):** `tools/check_duplication.py` — exit 1 se o mesmo `.rs` (não-facade) existe em ≥2 crates; rodar após mudanças estruturais. NeuralFS consolidado em k_nano (era triplicata k_nano/hermes/bin); hermes e bin são facades `pub use k_nano::neural_fs::*` + adapter do trait `FilesystemAgent` (trait é ring-local, tipo é canônico). Ainda pendente da mesma consolidação: camada fs/ (ata_agent, proc_fs_agent, etc.), camada net (netstack, network_agent), espelhos cortex/k_ai — listados pelo guarda.
- **`set_page_uc` NÃO cria mapeamento (SESSION_237, xHCI #PF):** `apic::set_page_uc(phys, pmoff)` só seta flags UC em mapeamento **existente** (retorna cedo se a entrada não é PRESENT). Drivers MMIO precisam de `map_page_uc`/`map_mmio_page` (cria L4→L3→L2→L1→PTE). `k_nano::xhci::init_xhci` usava `set_page_uc` no BAR → 1º `r32(base,0)` **#PF** (`CR2 = pmoff + bar0 + RTSOFF`), mascarado por WHPX e exposto sob TCG (WHPX falhou nesta máquina: "Ignoring request for interrupt vector 0"). Fix: loop `map_page_uc(mmio + page*0x1000, pmoff)` p/ as 16 páginas do BAR (padrão e1000).
- **Drift de tipo struct (SESSION_217):** Não mover `impl Trait for Struct` para crate se o bin tem sua própria cópia do struct — tipos distintos não compartilham impls. Verificar com `fc.exe /A` antes. Caso: `usb_msc::UsbMassStorage` existe em ambos mas usa `crate::xhci` que resolve p/ módulos diferentes.
- **`return` em match arm (SESSION_217):** `return String` dentro de fn que retorna `AgentTickResult` = erro de tipo (E0308). Use `Result<String,String>` p/ early-exit dentro de match arm — `Err(msg) => msg` vira o valor do arm.
- **`ToString` em no_std (SESSION_217):** Importar explicitamente `use alloc::string::ToString;` — não está no prelude. Erros `method not found for &str` = trait faltando.
- **Checkpoint SelfHeal v2 (SESSION_217):** `restore_checkpoint` agora captura CR3/PML4 (`x86_64::registers::control::Cr3::read().0.start_address()`), heap addr (0x_4000_0000_0000 + 512MB), FNV-1a hash de driver init flags. Ainda não restaura page tables (P09 pendente) — log diagnóstico apenas.
- **Boot path Agency (SESSION_217):** Agency specs vazio quando sem AGENT.md assinados — fallback cria 2 AgentSpecs (`SystemDiagnostics`, `HwMonitor`) p/ garantir >0 agentes no boot log.
- **Free function > method para render com borrow conflitante (SESSION_219):** `draw_window_fb(fb, win, theme, scr_w)` — função livre separa o borrow de `fb` (mutable) do borrow de `win` (immutable), evitando E0502 quando `&mut self` e `&self.windows[idx]` precisam coexistir.
- **Index antes de mutable borrow (SESSION_219):** `self.apps.iter().position(|a| ...)` + acesso por índice resolve conflito `iter_mut` + `iter` no `toggle_app`. Computar count visível primeiro, depois mutar.
- **const Theme com const fn (SESSION_219):** `Theme::new(...)` é `const fn` — `const COSMIC_DARK: Theme = Theme::new(...)` evita o `&temporary` pattern (E0515) de `current_theme()`.
- **k_hal não tem `crate::memory::` (SESSION_219/ADR-0065 FASE 2.2):** k_hal é R1 e depende de k_nano (R0). `GLOBAL_ALLOCATOR`, `PHYS_MEM_OFFSET`, etc. vivem em `k_nano::memory::`. Em k_hal usar `use k_nano::memory::GLOBAL_ALLOCATOR` — nunca `crate::memory::` (não resolve). Padrão segue blit.rs, ring.rs, intel.rs que já usam `k_nano::memory::`.
- **GDT lazy_static + TSS multi-AP (ADR-0065 FASE 3.1):** GDT é `lazy_static` — não dá pra adicionar entradas em runtime. Solução: pré-alocar `TSS_ARRAY: [TaskStateSegment; MAX_APS+1]` + `tss_selectors: [u16; MAX_APS+1]` no init. `init_ap_tss(ap_index, ist_tops)` preenche slot pré-alocado e monta o descriptor no GDT. APs chamam `ap_load_idt_and_tss(ap_index)` → `lidt` + `ltr tss_selectors[ap_index]` + `sti`.
- **AP sem IDT trava (ADR-0065 FASE 3.1):** AP sobe com IF=0 e sem `lidt`. `hlt` sem trabalho trava o AP. Por isso `cortex::parallel_*` é gated por `k_nano::smp::ap_pollable()` (hoje false até AP_IDT_READY barrier). ap_entry faz: init_ap_ist → init_ap_tss → ap_load_idt_and_tss → AP_IDT_READY barrier → set_ap_pollable(true).
- **TimerFuture poll não pode registrar future dentro do poll (ADR-0065 FASE 3.2):** `TimerFuture::poll` chamar `register_future(Box::pin(async {}))` só pra pegar um índice é feio mas funciona. Padrão correto: registrar o TimerFuture no `init_async_rt` ANTES do primeiro poll, e o poll só decrementa `ticks_remaining` + `wake_by_ref` quando >0. `process_wakes` (chamado do timer IRQ) faz o wake.
- **AutoInstaller (ADR-0079, SESSION_227):** O único self-installer no ecossistema AIOS no_std.
  `scan_pci()` é unsafe — precisa `unsafe {}`. `PciDevice` tem `bar0..bar5` individuais, não `bars[]`.
  `NeuralVolume::write_file(dev, ino, data)` requer device + inode + dados — não é `write_file(path, data)`.
  `SkillRegistry::list_skills()` retorna `Vec<(String, ToolPolicy)>`, não String.
  `StorageBus::entries()` devolve `&[StorageEntry]` com campos nomeados, não tuplas.
  Sempre sincronizar `N_SLOTS` com o número de variantes de `ModelSlot` ao adicionar novo slot.
  Fazer o boot frame allocator já setar `TOTAL_RAM_MB` ao final de `init_from_usable_ranges()`.
  `format_fat32_esp()` exige ≥65525 clusters (~32MB) — FAT32 real não funciona com menos.
  Nenhum projeto AIOS no_std pesquisado (ClaudioOS, FYY, Wetware, WeftOS, Oreulius, WAeasi, coconutOS, ArceOS) tem self-installer.
  ADR-0079 + plano de implementação em `docs/architecture/0079-neural-auto-installer.md`.
- **Seed agents: Ed25519 + VFS I/O desperdício no boot (SESSION_230):** `seed_embedded_agents()` chamava `sign_artifact_md()` (Ed25519 signing ~50-100ms cada) + `read_vfs`+`write_vfs` (NeuralFS I/O) para cada um dos 41 agentes nativos. Só que seeds são `trusted-by-compilation` — não precisam de assinatura runtime nem de persistência VFS (já estão no binário). Custo total: ~8.5s de boot. Fix: pular signing e VFS I/O quando `tier == "native"`. Ver `crates/hermes/src/package_hub.rs` linha 847.
- **Bootloader 0.11 cleanup (SESSION_232):** Migração completa para Limine exigiu remoção de ~65 arquivos (vendor crate), 3 dependências `bootloader_api`, feature `limine-boot`, entry point 0.11 (`kernel_main`), `BootloaderHandoff`, `probe_uefi_framebuffer`, `raw_boot_info()` do trait, `BitmapFrameAllocator::init()`, ramdisk bootloader path, LEGACY builders, e `[patch.crates-io]` bootloader. A `bootloader_api` tinha tentáculos profundos: tipo `MemoryRegionKind` em `k_nano::memory`, `PixelFormat` em `jarbas::display::fb`, `BootInfo` no bin. Cada crate consumidora precisou ser limpa individualmente. Lição: ao eliminar um bootloader, varrer a árvore de dependências inteira — o tipo do bootloader vaza para todas as camadas via `BootHandoff` trait.
- **Ring3 CR3 switch order (SESSION_233):** O triple-fault ao habilitar `TRY_ENTER_RING3=true` era causado por `mov cr3` DENTRO do bloco `asm!` que faz `iretq`. Após `mov cr3`, o TLB é flushado e as instruções seguintes (`push`, `iretq`) são fetch via NOVO CR3. Se alguma entrada de PT intermediária não estiver PRESENTE no clone raso → #PF → handler #PF inacessível → triple fault. **Fix (Moros pattern):** `address_space::restore_cr3(user_l4)` como statement Rust **antes** do asm block. Enquanto CPL=0, supervisor-only pages são acessíveis → kernel text compartilhado via P4[511] funciona. O asm block só faz segmentos + push + iretq. O `SAVED_RSP` para return path é salvo em Rust antes do CR3 switch. Testado em Moros, aplicado aqui. Ver `crates/neural-kernel/src/user_mode.rs:enter_user_mode()`.
- **UnsafeCell não é Sync em lazy_static (SESSION_233):** `lazy_static!` exige `Sync` para o tipo. `UnsafeCell<TaskStateSegment>` não implementa `Sync`. Solução: wrapper `TssCell(TaskStateSegment)` com `unsafe impl Sync` (justificado: TSS só mutado single-threaded durante Ring3, CLI state). Ver `crates/neural-kernel/src/interrupts.rs`.
- **Ring3 triple-fault — RSP=0 por `xor ax, ax` clobbering operando asm (SESSION_233):** O `jump_back_to_kernel` usava `"xor ax, ax; mov ds, ax; mov es, ax; mov ss, ax"` para zerar segmentos no retorno do syscall. Em long mode isso era **desnecessário** (ds/es/ss base ignorada; SS.RPL=0 já vem do TSS no `int 0x90`). Pior: o compilador escolheu **RAX** para o operando `{rsp}` — o `xor ax, ax` zerou RAX → `mov rsp, rax` com RAX=0 → **RSP=0** → `ret` para RIP=0 → #PF storm (CR2=rodata, err=0x3). **Lição:** nunca clobberar registradores dentro de `asm!` que o compilador pode usar para operandos `in(reg)` — use um registrador dedicado ou `options(nostack)` + clobbers explícitos. Diagnóstico via `-d int,cpu_reset` do QEMU (dump RIP/CR2) + `llvm-objdump` da função.
- **Callee-saved clobbered por handler `extern "x86-interrupt"` (SESSION_233):** o handler de syscall salva rbx/rbp/r12-r15 na stack RSP0; `jump_back_to_kernel` faz `jmp` direto ao return point **pulando o epilogue do handler** → callee-saved lixo → epilogue de `enter_user_mode` usa RBP corrompido. **Fix:** salvar callee-saved em static antes do iretq e **restaurar em `jump_back_to_kernel`** (CPL=0 + kernel CR3, statics acessíveis) — NÃO no return point do asm (lá o registrador com o ponteiro foi clobberado).
- **HEAP_BUFFER em `.bss` corrompe statics adjacentes (SESSION_233):** `resize_bump_heap(2048)` estende o `HEAP_LIMIT` para além do array `HEAP_BUFFER` de 512MB em `.bss` → o bump allocator entrega endereços que colidem com statics `.bss` seguintes (`GLOBAL_ALLOCATOR`, `PHYS_MEM_OFFSET`, `TOTAL_RAM_MB`) → `total_frames=0` → falsa exaustão de frames ("sem frame CoW" nos demos P4-P9) + "RAM: 0 MB". **Fix duplo:** (1) mover statics críticos para `.data` com `#[link_section = ".data"]`; (2) mover `HEAP_BUFFER` para seção própria `.bss.heap` colocada no FIM da imagem (limine.ld) — assim a extensão além dela só toca espaço livre. O limite `HEAP_LIMIT` default = `HEAP_SIZE` (512MB).
- **NeuralFS formata disco de boot (SESSION_233):** `try_format_gpt_virgin` só bloqueava tipos MBR 0x0B/0x0C/0x07/0x1C/0x7F/0xEF — NÃO incluía **0xEE (protective GPT do ESP Limine)** → o kernel formatava o `uefi.img` como NeuralFS durante o boot QEMU, destruindo o ESP → próximo boot: OVMF "Not Found" → shell UEFI. **Fix:** `has_data = parts.iter().any(|p| p.type_code != 0)` — qualquer partição bloqueia format destrutivo.
- **`build.rs` do boot sem `rerun-if-changed` (SESSION_233):** sem `cargo:rerun-if-changed`, quando o kernel não muda o cargo não reroda o build.rs → `uefi.img` fica stale (pode estar corrompido por boot anterior). Fix: rerun-if-changed em `build.rs`, `kernel.elf`, `limine.ld`.
- **Feature que escreve MSR no boot precisa do hypervisor REAL (SESSION_243):** `platform_probe::hypervisor()` retorna `HypervisorKind::None` (default 0) **antes** do `detect()` rodar — indistinguível de HW real. Gate de features sensíveis (ex: `wrmsr` dos MSRs SYSCALL LSTAR/STAR/FMASK) exige `probe_done()` + hv real, senão libera o caminho errado e o WHPX dá #GP no boot.
- **WHPX rejeita `wrmsr` dos MSRs SYSCALL (SESSION_243):** `#GP ip=...` no 1º `wrmsr` de `init_syscall_fast_path`; TCG permite no-op (bug mascarado no TCG). Fix: gate `probe_done() && hv ∈ {None, Kvm}`; WHPX/TCG/VBox/VMware → fallback `int 0x90` (DPL=3 já ativo). `[SYSCALL] gated off (probe=true hv=MicrosoftHv)`.
- **Escrever em VA que só existe no sandbox AS, com CR3 do kernel, dá #PF (SESSION_243):** `jit_write_exec_user` escrevia via `ARENA_VA` mas a página só está no sandbox AS → `#PF CR2=0x500000000000`. Fix: escrever via **HHDM no frame físico** (`hhdm_mut::<u8>(frame)`), nunca pelo VA do sandbox a partir do CR3 do kernel.
- **Self-test que executa código USER com CR3 do kernel dá #PF (SESSION_243):** `user_arena_self_test` transmutava o VA do arena e executava em Ring 0 → #PF (VA inexistente no CR3 atual). Fix: validar folha USER mapeada + bytes via HHDM; execução real é do `ring3_run_native` em CPL=3.
- **Boot TCG trava no ATA PIO/FAT32 (SESSION_243, conhecido):** validar runtime com `-NoDisk` (script mesh omite o 2º drive: "ATA PIO sob TCG atrasa/trava o boot"). TCG 2 cores 8G sem disk → boot completo 8 fases + Runtime + tick.
- **ELF64 header offsets (SESSION_243):** e_phentsize@54 (2B), e_phnum@56 (2B), e_shentsize@58, e_shnum@60. Erro de offset → "no program headers".
- **Sobrescrever arquivo existente do bin (SESSION_243):** o bin já tinha `elf_loader.rs` (ADR-0076, 277 linhas) — `write` sobrescreveu. Sempre `git show HEAD:<path>` antes de sobrescrever; fundir preservando a API existente (`ElfLoader::load`/`is_valid_elf`/`load_and_spawn`).
- **Artefato exportado é o contrato, não o modelo em memória (SESSION_247/248):** in-mem 73% holdout ≠ runtime 5% (sweep QEMU). O export era degenerado (threshold 0.5 vs init ±0.088 → pesos zero). Fix: treinar com forward Rust-exato (BitNetRustExact) + STE + validar o ARQUIVO com port Rust-exato (`validate_hw_expert_v4.py`: parse_end==size, nonzero gate, holdout do arquivo). SEMPRE medir o artefato, nunca métricas em memória.
- **Forward de treino deve espelhar o kernel (SESSION_247/248):** o forward de treino (BitNetLMv4) divergia em 5 pontos (scale vs rms_norm, g·u vs SwiGLU, heads com bias, atenção full-dim vs q_dim=32, residual pré-norm vs residual-sobre-normalizado) + embed .T embaralhado → artefato lia lixo. Lição: o forward de TREINO deve usar a MESMA matemática do kernel — aplica-se a qualquer modelo host (HW Expert v6, router v2).
- **Controle decisivo — mesmo arch fp32 responde em 30 min (SESSION_248):** se o modelo ternário colapsa para majoritário, NUNCA assuma que é a quantização. Rodar o mesmo forward com matmuls fp32 (sem ternário, mesmo split) — se o colapso persiste, a ARQUITETURA é a vilã (atenção truncada/mean pool) e QAT não resolve. Foi assim que descobrimos que o transformer é a ferramenta errada para HW identification (60.58% fp32 = 60.67% ternário = majoritário).
- **Teto de sinal > arquitetura > treino (SESSION_248):** vid:did → família de driver em devices nunca vistos tem teto ~59-63% (não é imbalance, não é capacidade, não é tokenizer — provado rodando todas as variantes de loss + arquiteturas). Antes de otimizar, medir o teto com um MLP simples treinado em ground-truth SEM imbalance — se o stage-2 placa abaixo do gate, nenhum investimento em arquitetura ou treino cruza o gate.
- **Labels circulares produzem números falsos (SESSION_248):** treinar com labels do `classify_by_vendor` (mesma heurística do kernel) dá train 72% ≈ holdout 73% mas runtime 5% — o modelo aprendeu padrões de vendor que não conferem com hardware real. Ground-truth independente (nomes pci.ids, WDM class) é NECESSÁRIO mas não suficiente (cobertura 54.7% nos dados do repo).
- **Tabela curada > NN para devices conhecidos (SESSION_248):** a precedência tabela-primeiro é a política correta — a tabela (18 pares curados) faz 100% nos devices que ela cobre; a NN nunca deve sobrepor a tabela. Para devices fora da tabela, a NN só se justifica se medidamente ≥65% específico; senão a heurística de class byte (hardware fornece) é confiável.
- **Infra de prova/refutação é o ativo durável (SESSION_248):** sweep QEMU (tools/hw_sweep/), validator Rust-exato, split honesto 90/10 por device seed 42, controle contínuo, MLP probes — qualquer modelo futuro é provado ou refutado em 30-90 min sem tocar no kernel.
- **`f16_to_f32`: nunca derivar o sinal com `(bit>>15 as f32) * -1.0` (SESSION_249):** quando o bit de sinal é 0, `0.0 * -1.0 = -0.0` e `-0.0 * mant * powf = -0.0` — **todo f16 positivo decodificava como -0.0**, zerando silenciosamente TODOS os dequants GGUF (Q4_0/Q5_0/Q6_K). Fix: `sign = if bit==1 {-1.0} else {1.0}`. Gate: qualquer dequant f16 com escala positiva deve produzir ≠0 (teste Q6_K cross-check pegou `d=-0`).
- **Paridade Python↔Rust de PRNG exige u64 (SESSION_249):** LCG `(x*1103515245+12345) & 0x7FFFFFFF` em Python usa inteiros arbitrários; em Rust `u32::wrapping_mul` trunca ANTES do mask → valores diferentes (sintoma: 1º f32 divergia no offset 184 da paridade). Usar u64 no estado e `as u32` na saída.
- **Encoder Q6_K (SESSION_249):** `d = block_max/(31*127)` + `scale_i = round(127*sub_max/block_max)` → `eff = d*scale_i ≈ sub_max/31` (reconstrução exata no ponto de máximo; erro ≤ sub_max/62). Layout espelhado de `dequantize_q6_k_block` (gguf.rs): ql[128]+qh[64]+scales[16]+d(f16) por 256 pesos; element decode = half/lane/l/is; `rms_ffn_norm` canônico = `intermediate_size`.
- **`include_bytes!` de goldens vs `.gitignore` `*.bin` (SESSION_249):** goldens usados por `include_bytes!` nos testes (ex. `tools/golden_v6.bin`) são engolidos por `*.bin` → clone fresco compila mas o teste falha por arquivo ausente. Un-ignorar explicitamente (`!tools/golden_*.bin` + ref `.f32`).
- **v6: feat é inventário, não flag de arquitetura (SESSION_249/ADR-0085 D5):** feat computado do que foi ESCRITO (nunca hardcoded); `tie⇒seção unembed NÃO existe` (D3, loader nunca inventa zeros); `num_params` u64 é informativo (loader nunca infere layout dele); act_type/embed_type no header — forward escolhe ativação por arquivo (2B4T=relu2, modelos próprios=silu).
- **lazy_static que inicializa outro static pode nunca rodar → ISTs zerados → triple fault (SESSION_251):** o GDT do k_nano usava `Descriptor::tss_segment(&TSS_ARRAY[0])` (TaskStateSegment cru = interrupt_stack_table zerados) e o lazy_static `TSS` que preenche os ISTs **nunca era dereferenciado** → gate #PF/#GP/timer com IST≠0 entrega a exceção empurrando o frame para VA 0 → #DF → triple (sintoma QEMU `-d int`: `check_exception old: 0xe new: 0xe`, `CR2=0xfffffffffffffff8`). Fix: `&*TSS` no ponto de uso força o lazy_static. Regra: se um lazy_static só existe p/ inicializar outra static, derefereciar explicitamente no caminho de boot. Foi o reboot loop do commit 2662d50 (SESSION_250 veio com boot quebrado — só o working tree com HEAP_EXT_BASE foi testado).
- **Run-qemu-whpx.ps1 só troca WHPX→TCG em erro de launch; hang/triple-fault silencioso NÃO aciona fallback (SESSION_251):** validar boot com `-d int,cpu_reset` + greppar `Triple fault`/`check_exception`. Boot TCG é lento e não-determinístico (~1900+ ticks até DriverInit); evidência de aceite de boot deve usar **WHPX** quando disponível.
- **Intrins 256-bit com split não compilam no target no_std (SESSION_249b):** `_mm256_maddubs_epi16` (pmaddubsw 256-bit) exige split LLVM para `pmaddubsw` 128-bit (SSSE3), e o target `x86_64-unknown-none` desabilita `-ssse3` no NÍVEL DO TARGET — `#[target_feature(enable="avx2,ssse3")]` por função não re-legaliza (LLVM ERROR "Do not know how to split the result of this operator!"). O `bitnet_avx2.rs` compila porque usa só intrins AVX2 puras sem split (`_mm256_cvtepi8_epi32`, `_mm256_fmadd_ps`). Fix: gate por target, não por cfg(test) — kernel real `#[cfg(all(x86_64, not(target_os="none")))]` (host/test onde SSE nativo) + stub no_std gated (nunca chamado — runtime gate false).
- **Paridade W2A8 vs ref: quantizar a referência (SESSION_249b):** comparar kernel int8 com ref f32 puro dá rel-err ~1.0 quando as linhas têm soma≈0 (erro de quantização ≈0.28 domina want≈0). O ref correto replica a MESMA quantização (si + round) e deve bater ~exato (folga 1%); o ref f32 puro documenta o erro esperado (≤5%). Tail escalar do kernel deve aplicar o MESMO desconto do viés (xq=q+128 ⇒ q = xq-128) que o caminho SIMD faz via `corr = r - bias`.
- **Wrap 2⁶⁴ no bump heap (SESSION_249b/ADR-0085 boot 2B):** `HEAP_BUFFER` vive no fim da imagem (high-half, ex. 0xffffffff809c59d8). `heap_start + offset` envolve para VAs baixas no offset ≈2044MB — o 2B v6 (cópia 755MB + embed Q6_K 257MB → offset ~2158MB) cruzava o wrap e o `memcpy` do embed escrevia em VA 0 (não-mapeada) → **#PF CR2=0 em `rep movsq`**. Fix proposto (HEAP_EXT_BASE em p4[508]) falha no `map_page_direct` (sem check de HUGE_PAGE em certo nível → lê P2 garbage e early-returns → páginas não-mapeadas). **Decisão:** reverter p/ `heap_start + offset` (boot-time resize 512→1536MB não cruza o wrap; só o grow runtime do 2B cruza) e documentar como known-issue. Lição: `map_page_direct` precisa de check HUGE_PAGE em TODOS os níveis antes de confiar no mapeamento.
- **AIOS self-adapting heap (premissa 4):** o heap deve derivar da RAM física detectada em runtime (`TOTAL_RAM_MB` populado em `init_from_usable_ranges`) — `heap_initial_mb = clamp(75% RAM, 512..1536)` no boot + `grow_bump_auto` (auto-grow 256MB/passo sob demanda, com verificação `heap_pte_present` pós-mapeamento). Eliminou o `resize_bump_heap(2048)` hardcoded. **NÃO mapear eager 6GB em TCG** (exaure frames → reboot loop): usar piso inicial modesto + crescimento preguiçoso. `needs_airllm(params, file_mb)` em model_fit decide residente vs AirLLM (layer streaming) quando modelo + heap > 75% RAM.
- **v6_file_size autodescritivo (boot 2B):** o QEMU-loader não recebe o tamanho do arquivo; o const `BITNET_2B_V4_BYTES` (604MB do v4) truncava o 2B v6 (792MB) → parse lixo. Fix: `cortex::model::v6_file_size(data)` deriva o tamanho total do header (autodescritivo) — o scan nunca mais hardcoda tamanho de modelo.

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

### Toolchain Rust (histórico — `rust-toolchain.toml` agora é cross-platform)
- Desde 08/2026 o `rust-toolchain.toml` usa `channel = "nightly-2026-07-05"` **sem
  sufixo de target** (o antigo `...-x86_64-pc-windows-gnu/msvc` quebrava Linux/macOS
  com `target tuple in channel name ...`). O rustup resolve o host nativo de cada
  plataforma; no Windows do dono resolve o mesmo toolchain de sempre. **Não
  reverta** para o formato com sufixo.
- A env var `RUSTUP_TOOLCHAIN=nightly-2026-07-05` (exportada em `~/.bashrc` do VM)
  continua como fallback inofensivo — ela tem precedência sobre o arquivo e aponta
  para o mesmo nightly 1.98. Em contextos sem `~/.bashrc` o arquivo sozinho resolve.
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
- `cargo test` no host **funciona** desde 08/2026 (SESSION de testes): os lib crates
  usam `#![cfg_attr(not(test), no_std)]` e HW-only items são gated com
  `#[cfg(target_os = "none")]` (NÃO `cfg(test)` — é inerte em builds de dependência).
  Comando: `cargo test --workspace --exclude neural-kernel --exclude boot`
  (139 testes; os 2 bins bare-metal nunca são testados no host). A validação real
  continua sendo `cargo check --release` + boot no QEMU (agora também em CI).

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

## Repository Map

A full codemap is available at `codemap.md` in the project root.

Before working on any task, read `codemap.md` to understand:
- Project architecture and entry points
- Directory responsibilities and design patterns
- Data flow and integration points between modules

For deep work on a specific folder, also read that folder's `codemap.md`.
