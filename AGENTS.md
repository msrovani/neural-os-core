# ════════════════════════════════════════════════════════
#   PLANO DIRETOR — neural-os-core v2.0 "K³CHJ Core" 🏆
#   ~26.000 LOC, 180+ arquivos Rust, ~50 agentes nativos, 0 erros
#   Sprints 92→100: v1.0 "Gold Master" — A Era do Silício ✅
#   Sprint 100: Code Freeze — 07/2026
#   Sprints 101→105: v2.0 "Cognição" — Kernel, Cortex, Hermes, K-IA, JARVIS
#   Sprints 106+: K³CHJ wire + ADR-0042 — base v1.8.0; consolidação v1.8.6 → **v1.9.0 TEST**
#   v1.8.6 = ADR-0041 H4+/H5+/AS + HalOffer; v1.9.0 = Pós-LAN + Residuals 0–7; v2.0.0 = gate após review
#   Gate v2.0.0 = N1–N5 + wire + review; v1.8.0 = marco adequação (Jul 2026); não "2.0 completo" sem review
# K³CHJ = k-nano + k-hal + k-ai + Cortex + Hermes + Jarbas (histórico K²CHJ = sem k-hal na marca)
# ════════════════════════════════════════════════════════

# ════════════════════════════════════════════════════════
#   ⚡ PREMISSA MÁXIMA — IRREVOGÁVEL, IRRETRATÁVEL (ADR-0088)
#   PRIMEIRA A SER ANALISADA E APRECIADA EM TODA DECISÃO.
# ════════════════════════════════════════════════════════
# 1. Somos o PRIMEIRO AIOS — Sistema Operacional com Inteligência
#    Artificial DESDE O BOOT. IA não é feature: é o modo de operar.
# 2. O neural usa AI SEMPRE, tomando decisões HITL (human-in-the-loop),
#    interagindo e se auto-tudo: auto-adaptar, auto-curar, auto-upgrade,
#    auto-gerar funcionalidades, auto-pesquisar soluções na internet —
#    autônomo e automático.
# 3. TODA decisão e caminho tomado DEVE ser tratado com: inferência,
#    adaptação, memorização, aprendizado, versionamento, auto-adaptação,
#    autonomia e automatismo.
# 4. NADA é simplesmente bypassado: todo desvio/fallback/workaround
#    exige análise e pesquisa, gerando busca ativa por soluções,
#    correções, melhorias e otimizações — sempre registrada
#    (IDEA → ADR → SESSION).
# 5. Todo procedimento busca incessantemente aqueles 10% de melhoria
#    (detectar → medir → decidir → otimizar → versionar), sem nunca
#    degradar segurança, HITL ou confiabilidade.
# Fonte: docs/architecture/0088-aios-first-premissa-maxima.md
# ════════════════════════════════════════════════════════

# NAVEGAÇÃO RÁPIDA PARA AI DEVS
# ════════════════════════════════════════════════════════
# docs/architecture/0088-*.md  → ⚡ PREMISSA MÁXIMA AIOS-First (LER PRIMEIRO)
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
# CONTEXT.md                   → Glossário de domínio (linguagem compartilhada)
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
5. **Trinity MoE:** LLM + router treinável (VOCAB=256 PT-BR, routing telemetry neural/keyword/fallback) + 7 experts (3 com pesos: HWEXPRT, RUSTCDR, PIPER). Expert on-demand load via `get_or_mmap_expert` na Cortex Arena. AutoLearn: detecta necessidade → treina → registra.
6. **Toda tecnologia nova DEVE ser registrada em `TECNOLOGIAS.md`** com ADR, IDEA, arquivo e sprint. Rodar `tools/update_tecnologias.py` após alterações.
7. **Lições S292-S294:** TTS split_into_sentences com core::mem::replace (ownership-safe state machine); compositor hot path: fill scanline + isqrt_u64 (não sqrtf); TARGET_FRAME_TICKS=1 (PIT ~18Hz).

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
- Trinity MoE: LLM + 7 kind (HwIdentify, HwControl, RustCoder, DiskDiag, Security, Generator, SpeechSynth — 3 wired HWEXPRT/RUSTCDR/PIPER, 4 keyword→Generator) + router_weight treinável (ROUTER.BITNET VOCAB=256 HIDDEN=64; moe_router=LOADED vs ABSENT keyword+FALLBACK_GENERATOR; routing telemetry neural/keyword/fallback counters)
- SDIO MoE: 95.812 entradas .inf/.sys reais + análise pefile
- HardwareRegisterMap: gerado por IA (3 níveis: HWID→família→heurística)
- **WHPX + AVX2:** WHPX com `-cpu host` executa AVX2 **nativo**. Só bloquear AVX2 se hypervisor = TCG (QEMU sem accel). Fix em `bitnet_avx2.rs` e `tensor.rs`.
- **Capability MVP (ADR-0041 P0–P9 ✅ PoC):** Boot A+B (`init_platform_sync` **antes** drivers; Agency EventDriven). Escada: AS+CR3+SPSC+Cap+`int 0x90` → CapGate → FB → DMA/mmap → Ring3 `iretq` → #PF demand-page → VirtIO vring layout → GGUF/FAT pré-fill. Demos **non-fatal**. **Não inventar Ring3/SFI/QUEUE_NOTIFY plenos** — PoC ≠ produção. crate `hermes/` ≠ binário até wiring explícito. Detalhe: `docs/architecture/0041-k2chj-capability-rings.md`, `docs/memory/SESSION_107.md`.

# Current Sprint: **v1.9.99-s308 TEST** — SMP anti-churn (redistribute/inflight/IPI0→1) + Memory N≥5 + steal_burst;
# s307 SMP AIOS N-cores (roles∝N, MAX_CORES=256 RQ, smp-runqueue);

# s306 Dual QEMU 4c mesh Master/Worker; s305 4c P6+Jarbas; s302 Ring3 Onda 6;
# s294 compositor hot path (TTS streaming); s293 OVMF/Falcon3; s292 TTS sentence-level.
# v279: SMP AIOS MADT inventário; trampoline jmp@IP=0; IDs u32 (s278 Ring3 TCG iretq+CPL3; B/C **não** liberados).
# ADR-0081 mesh (SESSION_242): **REASSEMBLY 2→16 slots** + **ACK seletivo** (FRAG\0→FRACK\0 stop-and-wait, 3 retries, 50 ticks) + timeout 500→2000 ticks; **probe_node exponential backoff** 50→3200 ticks; **cleanup_peer_health_ttl** (>60s a cada 500 ticks); PeerHealth expandido (avg_rtt EWMA α=1/8, rtt_samples[32], `peer_p99_rtt` via `(count*99+99)/100` — no_std sem f32::ceil); **ARP cache** PEER_MAC_CACHE + `recv_*_with_mac` expõe src_mac; **capacity_weighted_assign health-aware** (unreachable→0, latency/p99 factors); **token bucket** rate limiting (1/tick, burst 20; heartbeat=1, ROLE=2, dados=3); **JSON dashboard**: `PeerHealth::to_json` + `publish_mesh_health` emite JSON array no tópico `MESH_HEALTH`, `mesh_health_json::parse` no_std no Jarbas + lazy subscribe no DisplayAgent. Transporte vive em **k_nano R0**. Commit 7a97556.
# ADR-0081 mesh (s238+): transporte (udp_broadcast frame/send/recv) + serviço (mesh::p2p_tick) vivem em **k_nano R0** (bin re-exporta statics NIC: `pub use k_nano::nic_globals::{RTL8139,E1000,VIRTIO_DEV}`). **Segurança Fase A (s238):** RX fail-closed (assinatura vs pk vinculada → DROP), TOFU via `PK\0`+pk no heartbeat (`PEER_KEYS[16]`, seam SKYNET `peer_public_key()`), anti-replay (clock ≤ last → DROP), todos TX assinam — sec=0/0/0 validado. **Fragmentação MTU (s238):** `send_fragmented`/`recv_fragmented` (`FRAG\0` header 21B, fora-de-ordem OK, timeout 2000 ticks); gate 1200B removido — matmul 64×64 ~17.5KB round-trip OK. Non-heartbeat → EventBus `P2P_PACKET`; skill_sync/marketplace consomem via poll_p2p. BitTorrent: NÃO implementar (veredicto s238 — merkle piece verification quando modelos). `run-qemu-p2p-mesh.ps1`: ASCII puro, socket listen/connect, 8G, OVMF 8.3, -smp 2 MTTCG, -NoDisk. Commits: f240fa4→916d155 (s234-s238).
# SGDB = path cognitivo (HANR/Audit/Pkg meta/Skills/Episodic/RAG); FAT = blobs/firmware/WIFI.CFG/BOOT.LOG. Ver SESSION_172–173 + ADR-0063/0064.
# Emagreçer: lógica nova nas crates; bin só wire/`pub use` — `.cursor/rules/neural-emagrecer-bin.mdc`.
# ADR-0057 Compute Dispatch SMP+GPU+NPU: WS-A wake multi-AP (SIPI direcionado sequencial + stack/PerCpu por-AP + retry; `-smp 4`→APs=3, CorePools r0=1 r1=2 r2=1; contador unificado; bin::smp emagrecido); WS-B/C `cortex::compute` dispatcher (gated `ap_pollable`, deadlock-proof); WS-D GPU só se canário `Ready`; WS-E NPU XDNA/Intel detecção+fallback software; WS-G #412 `cortex::decode` (self-test PASS). On-demand AP-worker (IDT/IPI) + GPU W2A8 + driver NPU = Layer S/HW.
# ADR-0059 Runtime App Factory — **Caminho A ✅**: runtime WASM real **wasmi** (no_std, fuel) roda `.wasm` no bare-metal (`hermes::wasmi_rt`; self-test `add(2,3)=5` PASS). Seletor por IA **A/B/C** (`hermes::app_factory`): A=wasmi (sandbox, default IA não-confiável); B=Cranelift JIT wasm→nativo; C=Rust-subset nativo (rustc-lite-like). `cranelift-codegen` no_std compila (feature `jit-cranelift`, opt-in) mas execução nativa (B/C) é **GATED** por ring de isolamento (ADR-0077 Ring3) + **HITL forte** — segurança primeiro. CapGate nos host-imports `aios::*`. Bare-metal SEM rustc completo (LLVM); **mas Cranelift no_std permite Rust-subset on-device (C)**. Motor do self-improve/heal/update. Supersede ADR-0031(WASM)/0032; aposenta `Op` VM + `wasm.rs` (após bridges). **F7 arena W^X ✅** (`crate::exec_arena`: nativo `mov eax,42`→42 PASS — base JIT; é Ring 0, NÃO isolado). **F6 Ring3 → ADR-0077** (Onda 6 T-051–T-057; `TRY_ENTER_RING3=true` em `k_nano::paging`; demos boot stub — ADR-0102 H2). **Porto seguro:** ring gated, boot OK; nenhum código nativo não-confiável roda (só wasmi A). **Conectores no código:** `hermes::app_factory::register_native_ring` (seam) + `neural-kernel::isolation_ring::{init_connectors,ring3_run_native}` (site de impl; NÃO registra até ADR-0077 §6 passar em HW). `isolation_ring_available()` reflete o registro. **F4 ✅** `hermes::wasm_build` (op-IR `Op`→wasm válido + `validate`; alvo da gramática #412 — self-test `a*b+7`→49). **F3 ✅** `app_factory::generate_and_run(op-IR)` monta+roda no wasmi (self-test `(3+4)*2`→14). Restam: LLM emitir op-IR (integração #412) + registrar `Skill`/`agent-wasm` persistente, F5 promover, e F6 Ring3 (ADR-0077 + ADR-0102, Onda 6).
# ADR-0058 Generative Card Desktop (UI/Jarbas) — **S1–S4 ✅**: `embedded-graphics` `DrawTarget` (`FbTarget`, `jarbas/src/display/eg.rs`) sobre `DoubleBuffer` + `UiDeclaration`/`UiRenderer` (`card.rs`: Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel). `CardWindow` retido no compositor + `UI_SPEC` spawn/close + mouse (close/drag/botão→`CARD_ACTION`). **Orb responsivo e barra de relógios/HUD preservados.** Cards gerados por LLM (#412 `card_json_schema_hint`) ou skill WASM (RustCoder/Codex, ADR-0052) + Cron. Supersede parcial ADR-0047-HMI (H3 ❌); ADR-0036 persona inalterada. QEMU: 3 cards demo (Sistema/Clima/Chamada de Vídeo). S5 (widgets ricos/tema/TTF) + A/V real (HDA/UVC) = residual.
# Residuals 0–7 ✅ + fila lan: net_bridge · NetFs PASS · SelfUpdate HTTP · TLS smoke (SESSION_157).
# Onda 7 LAN: e1000 TX 0x3800/0x3818; DNS raw + HTTP; SESSION_149/150/152.
# Abertos: WiFi ath10k A3 Note AWAITING; LLM coh semântica (#466); /model-fetch e2e; GPU/UAC/DMA AWAITING_HW.
# Áudio: ADR-0045 — truth=`crates/jarbas/src/audio` (cutover e51a48b); `neural-kernel/src/audio` = facade `pub use jarbas_crate::audio::*`
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
- **Mesh P2P slog `info` = eleição invisível (SESSION_306 / ADR-0081+0092):** `MESH_ENGINE`/`mesh role=`/`TX heartbeat`/`TOFU` com `sub=info` → Trace mudo — GOAL2 podia passar sem linha na serial. Fix: `ok`/`warn`. **STATIC socket** sem gw: skip L3.5/L4/L5; detectar flag em addrs canônicos ≥4 GiB (`0x13E000000`). **`target1/uefi.img` stale** engole rebuild em `target/` — sync após `cargo build -p boot`. Dual Falcon3 + 2×4c exige FreeGB host >>5.
- **Boot log sem sev = dump, não dmesg (SESSION_289 / ADR-0092):** `slog` `[sub]` livre (`info`/`e1000`/`ckpt`) torna TRACE indistinguível de OK. Contrato: 4º campo = `ok|warn|fail|trace`; desconhecido = TRACE (mudo na consola). FB = produto (fases + HUD); serial = dmesg; `BOOT SCORE` é o que a IA lê. `boot_ckpt` K* no ecrã compete com o compositor — ramlog só. PnP não vai a `HERMES_RESPONSE` (vira TTS).
- **AIOS SMP (SESSION_281 / ADR-0088):** K22 metal i5=#GP ICR x2APIC (bits 14/15 reservados); 240H=freeze N×retry + deassert ilegal. ICR = dest+delivery+vector. GDT crate 8 slots ≠ silício — tabela própria 1 TSS/CPU + IST heap + sti. Teto de crate é bypass, não política. Aceite = HW real K23.
- **AIOS SMP (SESSION_279 / ADR-0088):** MADT Enabled é Observe, não “teto”. RAM não capam cores. `MAX_APS=7` / `.min(8)` / “¼ do heap” = doutrina falsa. SIPI IP=0 no header de patch = AP nunca entra no Rust (copiar jmp+ready do Redox, não o NASM). x2APIC ID = valor 32-bit do MSR 802 (não `>>24`). **QEMU é só DEV/TEST; aceite SMP = HW real.**
- **Ring3: GDT fantasma ≠ GDT carregada (SESSION_278):** seletores `user_code/data` em `interrupts_ext` vinham de lazy_static GDT **nunca** `lgdt` — índices colidiam com TSS da GDT `k_nano` → `#GP(0x20)` no iretq. User segments vivem na GDT que `init_idt` carrega.
- **Ring3: CR3 user sem HHDM (SESSION_278):** stack Limine/kernel em HHDM; `create_sandbox_as` só P4[511] → `#PF` pós-CR3 pré-iretq. Copiar P4[HHDM] **supervisor-only** (CPL=3 não USER).
- **Ring3: int 0x90 exige TSS.RSP0 (SESSION_278):** iretq CPL=3 OK mas `privilege_stack_table[0]==0` → `#PF` no syscall. Inicializar RSP0 no TSS BSP.
- **Ring3: RAX no handler `x86-interrupt` não é o RAX do user (SESSION_278):** prologue clobbera; mailbox em página USER (ou frame salvo) para nr. `from_user` via CS no frame OK; ler `rax` no handler não.
- **Ring3: SSE em CPL=3 com CR0.EM=1 → #UD (SESSION_278):** qemu64 tem OSFXSR; `xorps` só é #UD com EM. Patch IDT #UD → `fault_abort` no demo.
- **Heap dual-range demand-page (SESSION_299):** `try_fault_in_heap` DEVE cobrir AMBOS os ranges: `HEAP_START` (0x_4000_0000_0000 = TALC pós-boot) E `HEAP_BUFFER` linker address (bump boot+runtime). O `.kheap` NOLOAD pode ter páginas não mapeadas. Sem check duplo, #PF loop em CR2=0xffffffffa0cea000 após ATA+FAT32 mount.
- **skip_measure ≠ skip_probe (SESSION_299):** A distinção entre "skip benchmark" e "skip probe" é fundamental. Benchmarks que travam ≠ probes que funcionam. Regra: se a função mede performance (256 setores), skip em TCG. Se identifica hardware (identify + 1 setor), SEMPRE roda.
- **slog severity é contrato de visibilidade (SESSION_299):** Sub como `"ramdisk"`, `"Asset"`, `"BGE"` mapeiam para `Sev::Trace` (hidden). Mensagens que o operador PRECISA ver ("por que X falhou?") devem usar sub `"ok"` ou `"warn"`. Regra: se responde diagnostic question, deve ser visível.
- **Kernel virtual ≠ HHDM (SESSION_301):** Kernel virtual addresses (0xffffffff80000000+) SÃO MAPEAMENTOS SEPARADOS do HHDM (0xffff800000000000+). `cr2 - HHDM_OFFSET` NÃO funciona para kernel virtual — dá 140 PB (errado). A fórmula correta é `kernel_phys + (cr2 - kernel_virt)`. Fix: armazenar `KERNEL_PHYS_BASE`/`KERNEL_VIRT_BASE` no boot e usar na #PF handler. Resultado: 11 #PFs → 0.
- **`serial_print!` deadlock em interrupt handlers (SESSION_301):** UART spinlock deadlock dentro de #PF handler. Usar `puts`/`puthex` (lock-free raw I/O) ou contadores atômicos (`PF_DIAG_*`) para diagnóstico em handlers. `serial_print!` só é seguro fora de interrupt context.
- **Per-layer head_dim obrigatório em cognitive.rs (SESSION_301):** `model.head_dim` / `model.kv_dim` pode não corresponder ao shape real do tensor Q/K/V após `matmul_hybrid`. Derivar `hd = q.shape.1 / num_heads` POR LAYER dentro do loop. Usar `min()` para clamp de `hd` em `gqa_attn_forward`. `rms_backward` precisa de bounds checking: `len = min(x, dy, dx)`.
- **Colisão SESSION_275 mesh ≠ jarbas → port vira SESSION_276 (SESSION_277):** `main` já tinha SESSION_275 = mesh P2P GOAL1–3; branch aios-chj rotulava jarbas como 275. Nunca sobrescrever o número — renumerar o port (276) e documentar a colisão no SESSION_INDEX.
- **Não merge/rebase branch 10 commits atrás (SESSION_277):** `aios-chj` / tip `c234138` divergia (stack/mesh/LAPIC já em main). Port seletivo do commit útil (`ac4e853` → compositor/HDA/`infer_in_flight`); descartar o tip inteiro.
- **Specs órfãs overclaimam — corrigir ao wire (SESSION_277):** diretivas silício/GPU afirmavam AMX/`work_queue`/`trinity_inject` prontos; ao wirar TSC/CachePadded/ReBAR report, reescrever specs com honesty (detect vs compute, WIP vs wired).
- **`busy_wait_us` fixo (`us*40`) → TSC calibrado (SESSION_277):** spin constante mentia em TCG/WHPX/HW. `k_nano::tsc` (HPET→PIT→CPUID) + `sleep_us`; SMP wake usa tempo real.
- **Fat32Io absorb no canônico, não dual-module (SESSION_277):** trazer `Fat32Io`/`format_fat32_bps` para `k_nano/src/fat32.rs` + `BlockDevice::sector_size`; órfão `neural_fs/fat32` NÃO wired (evita segunda verdade FAT).
- **Bump heap: stack cresce para baixo — registry DEPOIS da stack+guard (SESSION_275):** `AgentRegistry` no bump abaixo da stack do scheduler → overflow no `BootSelfHealAgent::tick` esmagava Vec/BTree (`#PF CR2=0x18` / PANIC btree). Fix: alocar stack **8MB + guard 64KB** e só então `Box::leak(registry)`. Cache `name`/`auto_start` no `AgentInstance` + restaurar `on_activate`.
- **TOFU settle ≠ só bind de pk (SESSION_275 / ADR-0081):** Master virava Master no 1º HB e disparava ROLE/SkillSync antes de B vincular `PK\0` → fail-closed dropava Sync; Master marcava `synced` e nunca re-pushava. Fix: `tofu_settled()` (130 ticks) + `FORCE_HEARTBEAT`; gate ROLE/SkillSync/MKTP; `clear_synced_for_resync`; `become_worker` preserva Memory/Compute.
- **MicroPython path sem bytecode = stub wasmi (SESSION_275):** marketplace “python” fingia runtime. Fix mínimo: MVP WASM (`build_micropython_wasm.py` → `MICROPY.WASM`) carregado no wasmi (CapGate intacto) + probe `mesh_g3_probe` Master→Worker. Full emcc = residual.
- **“IA desde o boot” = Observe→Plan→Act→Verify→Remember, não CortexAgent cedo (SESSION_272):** martelar ATA/xHCI/E1000 “porque sempre foi assim” é bypass, mesmo com log. Fluxo: DeviceTree (H1) observa silício → k_ai planeja com Trust `(1,boot_observe,plan)` + recipe HITL → k_nano executa só o que existe (NVMe>AHCI>USB>ATA) → SelfHeal verifica a mesma árvore → HANR `hydrate_memory`. Escalate ≠ Auto. SLIP = DEGRADED+I5, não Net gate. Cortex sem pesos no T+0 = log honesto, não “LLM decide HW”.
- **k_hal H1 tardio = inventário vazio no SelfHeal (SESSION_271):** `k_hal::init` depois dos drivers + probe E1000-first ignorava o silício já visto. Fix: H1 idempotente pós-PCI; `boot_bind`+`boot_observe` ANTES dos drivers; SelfHeal `from_khal` sem rescan ATA. Bin só executa o plano.
- **Dois TRINITY + LCG fingindo MoE = telemetria mentirosa (SESSION_273):** fonte única `cortex::trinity::TRINITY`; `router_trained` gate; LCG seed=42 não roteia; HEALTH I5/Escalate observe-only (`runtime_observe`); HUD no `render()` com `net_hud_label` + MoE posture.
- **GPU matmul na CPU contado como GPU = auditoria falsa (SESSION_274):** `nvidia_matmul`/Intel `gpu_matmul` → `None` até KernelPack W2A8; `boot_report.gpu_ok` via `note_gpu`; MHI tier0 wired com CE canário + rollback CoW.
- **Feature do bin não propaga para crates — espelhar `foo = ["dep/foo"]` (SESSION_264):** `fat-boot-log` existia só em `neural-kernel`; o `persist_now` real em `k_nano::boot_logger` estava atrás de `#[cfg(feature = "fat-boot-log")]` mas **k_nano não tinha a feature** → o stub `persist_now → false` era o que sempre compilava e o BOOT.LOG nunca gravava no pendrive (QEMU mascarava via COM1). Regra: cfg de feature numa crate exige a feature DEFINIDA na própria crate; o bin propaga com `fat-boot-log = ["k-nano/fat-boot-log"]`. Verificar com `cargo check -p <crate> --features <feat>` + teste host que o path real (não o stub) compila.
- **PRs em cima de origin/main antigo conflitam com sessões locais (SESSION_264):** o PR #7 criava `SESSION_262.md` (Early BOOT.LOG) mas o HEAD local já tinha `SESSION_262.md` (SMP/x2APIC). Regra: ao integrar PR, comparar docs por número de sessão — renomear a do PR (262→264), nunca sobrescrever o registro local. Mesma regra para STATE.md/SESSION_INDEX.md (aditivo, não substituição).
- **Dir FAT não-atômico + crash = corrupção agravada (SESSION_264/260):** reescrever o dir cluster a cada flush rasgava o FAT se o boot crashasse no meio (triple fault → Windows não montava o volume). Fix: `overwrite_boot_log` data-only — escreve só clusters de dados (+padding zero); dirent só se size < 512 (1 WRITE de setor). Flushes seguintes = data-only.
- **`model.q_dim` do HW Expert é contrato de predição, não shape (SESSION_255):** o forward `predict_hw_v4` usa `model.q_dim` para TRUNCAR a atenção (treinado com qd = h/heads = 32). Ao converter v5→v6 (ADR-0085 §3.2 "q_dim==hidden"), colapsar para hidden muda predições — `tools/check_hwexpert_qdim.py` provou 7/10 devices DIFF com qd=128. Regra: preservar q_dim treinado no header v6 e o loader lê shapes FIXOS hwexpert q/k/v/o=(h,h), g/u=(h,ff), d=(ff,h). Converter hwexpert v5→v6 é mecânico e fiel: v5 (prefixos u32 len+scale + rope) → v6 (sem prefixos, feat=0x03, sem rope); bytes packed idênticos — validar com parity tensor-a-tensor + predições (`tools/check_hwexpert_v6_parity.py`).
- **Teste de loader com ARQUIVOS REAIS, não sintéticos (SESSION_255):** o teste host `hwexpert_v6_matches_v5_predictions` usa include_bytes do v5 E do v6 convertido e compara as 5 saídas × 10 devices — prova que o loader lê o MESMO modelo (parse + layout + forward), não apenas que parseia. Padrão p/ qualquer novo loader de formato.
- **`target1/` é o canônico de modelos v6 (SESSION_254+):** mkfat32 `find_file` prioriza `target1/` e nomes `.v6` (FALCON3.V6, AGENT.v6…); `models/` vira fallback. Imagem HW: `PACK_LLM=2b python tools/build_image.py --hw --unified --size 6144` → `target/usb_hw.img` (ESP FAT + dados FAT32; Rufus DD). Verificar o resultado dentro do FAT32 (nome 8.3 sem ponto: `HWEXPRT4BIN`, `BITNET2BBIN`).
- **Free-stack LIFO corrompe contiguidade de extent (SESSION_252, F1):** o alocador que popa blocos de uma stack (LIFO) entrega ordem INVERTIDA ao extent — reescrever um arquivo (ex: /models/) gravava blocos lixo e corrompia silenciosamente, sem erro reportado. Regra: se o formato assume blocos contíguos (data_block+count), o alocador DEVE validar contiguidade (ordenar + checar `w[i+1]==w[i]+1`, fallback bump). O BAFS corrigiu isso em `b81b43f` ("reclaim intra-transação é corrupção") e o port inicial NÃO trouxe o fix — sempre comparar o port com os bug-fixes upstream.
- **Ordem CoW: dados novos → commit → SÓ ENTÃO reclaim antigos (SESSION_252, F2):** reclaimar o extent antigo antes de os dados novos existirem destrói a versão boa em power-loss/ENOSPC. É a disciplina canônica de BAFS e LiberFS ("freeing adiado por 1 commit"). No erro, devolver os blocos recém-alocados (sem leak).
- **Nunca formatar um volume que existe mas não monta (SESSION_252, F3):** `probe_magic` lendo só o bloco 1 + format automático no ATA = wipe de /models/ quando o superbloco primário corrompe (backup bom ignorado). Fix: probe com fallback ao backup; se probe true mas mount falhou → recusa format (exige fsck explícito). LiberFS corrigiu o MESMO bug hoje (`86d8cb4`).
- **`sfence` (CPU) ≠ flush de dispositivo (SESSION_252, F16):** QEMU/RAM mascaram ausência de flush no commit — no HW real com write cache, o header do journal pode chegar à mídia antes dos data blocks. Fix: `sync_cache()`/flush entre dados→header→superbloco. É o bug clássico "funciona no emulador".
- **Região crua de KV não pode colidir com partições GPT (SESSION_252, C1):** TickvLite gravava no LBA 2048 hardcoded ("pula MBR/GPT" — errado: 2048 é onde a ESP começa). No dev sem NVMe cai em RAM (bug invisível); no 1º boot NVMe real = brick. Regra: qualquer storage cru (KV/flash) usa região calculada no fim do disco ou partição GPT própria — nunca LBA fixo perto das partições.
- **Persistência que se finge de pronta quando é volátil é pior que ausente (SESSION_252, C2):** `put_kv` com backend RAM retorna Ok — SELF.STATE/memória episódica evaporam sem erro. Reportar backend "ram" como não-persistente (log CRÍTICO) + write-through de dados críticos.
- **Cache morto = dívida (SESSION_252, C6):** ArcCache + readahead_cache do disk_agent têm `new()`/`tick()` mas zero `get`/`insert` — três "caches" (bloco, SGDB L0/L1, Hermes) das quais uma não serve ninguém. Verificar callers antes de aceitar uma abstração de cache.
- **Hardcoded de ambiente ≠ contrato de layout (SESSION_252):** nomes 8.3 do FAT (`KERNEL~1`) e path do Limine (`kernel.elf`) são **contratos** — configurá-los quebra o sistema. IP de server (`10.0.2.2:8080`) é **dado** — deve viver no config file (`UPDATE.CFG`). O AIOS não carrega endereço como constante: stub morto com IP de QEMU (`CHANNEL_MANIFEST_URL`/`poll_channel`, sem caller) foi **apagado**, não configurado (YAGNI).
- **Resolver gap = escolher o alvo mais simples (SESSION_252, U6):** o gap pedia suportar NeuralFS no update (mount + create_file + nomes = centenas de LOC); a **ESP já é FAT32 real (`0xEF`)** — adicionar `0xEF` ao filtro resolve o GPT instalado com ~15 linhas. `fat32.rs` mapeia GPT_ESP→0xEF, GPT_NEURALFS→0x7F, GPT_BASIC_DATA→0x0C. Antes de montar um FS novo, procurar a partição que já fala o protocolo.
- **Agente que só loga o evento = dívida (SESSION_252, I3/I6):** o AutoInstallerAgent escutava `SYS_INSTALL` mas nunca instalava (o `tick` só logava). O gap real era o **wiring evento→execução** (`run_install_from_bus`: source=boot ATA, target=AHCI→NVMe→USB via globals), não a API — que já estava pronta. Registrar no fleet + comando shell é o que destrava.
- **Bootloader "instalado" = ESP copiada crua (SESSION_252, I3):** SysInstaller copia a ESP setor a setor — qualquer arquivo nela (Limine, kernel.elf, UPDATE.CFG) vai junto ao target automaticamente. Gravar config na ESP no build (`build.rs`) evita mudança no instalador.
- **`NeuralVolume` exige `&mut dyn BlockDevice` (SESSION_252):** `mount`/`resolve_path`/`read_file` pedem mut; `ATA_DRIVER.lock().as_mut()` dá `Option<&mut AtaDriver>` → cast `let dev: &mut dyn BlockDevice = g`. `lock().as_ref()` (imutável) não compila. Erro comum: `&mut **g` (deref duplo) em vez de `&mut *g`/cast.
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
- **Matriz QEMU mesh GOAL1-3 (SESSION_280):** TCG 1c/4G/NoDisk Both PASS (325KB/4499 linhas A + 326KB/4525 B; Runtime tick1120, Master 2/Worker 3, TOFU settle T+1505, mesh_g3_probe→Worker, MKTP 18 skills, parser `tools/mesh_log_parser.py` ao vivo); 2c TCG FAIL hang `INIT-SIPI-SIPI ap_ids=[0x01]` 168 linhas (regressão 7d8116a); WHPX FAIL `#GP RIP 0834EEE OvmfPkg/PlatformPei`; host 2×6G estoura 6.5GB free → 4G teto. Loop 5 relaunches + `Stop-Process qemu*`. NoDisk = boot rápido sem BGE PIO 135MB.
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
- **GGUF type IDs são CONTRATO da spec, não enum livre (SESSION_309):** tensor types: `0=F32 1=F16 2=BF16 3=Q4_0 4=Q4_1 5=Q5_0 6=Q5_1 7=Q8_0 8=Q8_1 10=Q2_K … 25=TQ1_0/TQ2_0`; metadata value types: `0=UINT8 1=INT8 2=UINT16 3=INT16 4=UINT32 5=INT32 6=FLOAT32 7=BOOL 8=STRING 9=ARRAY 10=FLOAT64`. O enum do kernel estava **deslocado** (`2=Q4_0` em vez de BF16; `6=uint64` em vez de FLOAT32) → qualquer GGUF real corrompia offset ("n_dims fora de 1..=4"). Gate: SEMPRE validar loader contra arquivo GGUF REAL (não sintético com IDs do próprio código), e o sintético DEVE usar `tools/gen_test_gguf.py` com IDs da spec.
- **"1.58bit" no nome do Falcon3 é método de TREINAMENTO, não formato do arquivo (SESSION_309):** `Falcon3-*-1.58bit-q2b0.gguf` tem 155 tensors BF16 + 45 F16 — GGUF ternário (TQ2_0/TQ1_0) só em quantizações nativas ternárias. Não assumir o tipo pelo nome; ler o header.
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

- **PMM 64GiB na stack = overflow (SESSION_290):** `BitmapFrameAllocator` ~4MB. `empty()` no boot/stack Limine ou `NumaFrameAllocator::new` crasha. Fix: PMM já no static `GLOBAL_ALLOCATOR`; NUMA usa `Box`. Teste host: thread 32MB.
- **Pack Falcon3 (AIOS, SESSION_298):** lab / Active = **3B 1.58-bit**. Se o budget ainda couber, 7B/10B em GeneratorPro e 1B como comparativo. Não é “todo o FAT”; é inventário + fit. (SESSION_291 dizia 7B-first — superseded como *lab*, não como slot Pro.)
- **grow_bump_auto sem budget cap = OOM (SESSION_287):** `grow_bump_auto` cresce 256MB/passo até OOM se `HEAP_LIMIT` não tem teto. Fix: `HEAP_BUDGET_MB` + `set_heap_budget_mb`. **SESSION_290:** o valor vem de `heap_budget_mb(ram)` (75% − 2GB se ≥8GB), **sem** cap 1536. O budget ainda só limita o bump; TALC LLM é caminho próprio.
- **QEMU 8G falha em boot com Limine (SESSION_287):** `-m 8G` causa Limine "Loading executable" mas kernel nao inicia (sem output serial). Possivel: HHDM offset com 8G excede o kernel .bss heap layout. Usar 6G como teto QEMU. Testado: 6G funciona, 8G nao.
- **ATA TCG skip e intencional (boot_observe.rs:60):** ~~sob `-accel tcg`, o boot_observe.rs pula ATA do probe plan~~ **CORRIGIDO (SESSION_299):** `allow_probe()` permite probe ATA em TCG (identify + 1 setor ≈16ms). `skip_measure()` continua skipando BENCHMARK (256 setores). FAT32 mount funciona em TCG. Resultado: BOOT.LOG persistente + cross-boot NSGDB desbloqueado.
- **QEMU loader scan limitado a 2 endereços (SESSION_293):** kernel procura BitNet magic `0xBE11BE11` apenas em `0x100000000` e `0x120000000`. Modelos carregados via QEMU loader em outros endereços (ex: hw_expert @0x108400000) são ignorados. Fix necessário: scan iterativo `[0x100000000..0x180000000)` step=1MB ou reordenar loaders para BitNet primeiro.
- **OVMF pflash dual-file obrigatório (SESSION_293):** `-bios ovmf.fd` (combinado CODE+VARS) não funciona com QEMU pflash. Usar `-drive if=pflash,format=raw,file=ovmf_code.fd,readonly=on` + `-drive if=pflash,format=raw,file=ovmf_vars.fd`. O VARS precisa de NVRAM entries para boot fresh.
- **QEMU launcher Python (SESSION_293):** `tools/qemu_boot_stdio.py` — pflash OVMF + chardev serial file + model auto-discovery. Bugs corrigidos: addr sem `0x` prefix, SIGALRM inexistente no Windows, smp override por args posicionais.
- **virtio_gpu stale reference no hermes/agents.rs (SESSION_286):** `k_nano::virtio_gpu::init_driver_virtio_gpu()` foi removido no emagrecer (s261) mas a referencia ficou em `hermes/src/agents.rs:2967` (GpuDriverAgent::tick). Fix: substituir por log de status. **Regra:** quando uma crate e movida/removida, varrer TODOS os callers em OTRAS crates. O `cargo check` passa mas o `cargo build --release` crasha se o simbolo nao existe no binario final.
- **v6_file_size autodescritivo (boot 2B):** o QEMU-loader não recebe o tamanho do arquivo; o const `BITNET_2B_V4_BYTES` (604MB do v4) truncava o 2B v6 (792MB) → parse lixo. Fix: `cortex::model::v6_file_size(data)` deriva o tamanho total do header (autodescritivo) — o scan nunca mais hardcoda tamanho de modelo.
- **AMD BAR roles ≠ NVIDIA (ADR-0087 pré-req 4a, SESSION_252 §9):** amdgpu (Bonaire+) mapeia **VRAM→BAR0, doorbell→BAR2, MMIO→BAR5** — o `detect.rs`/`amd.rs` locais assumiam VRAM=BAR2/MMIO=BAR0 (o oposto). Como AMD era AWAITING_HW, o bug era invisível. Fix de raiz AIOS: **medir o tamanho real dos 6 BARs em runtime** (`k_nano::pci::read_bar_size`, técnica 0xFFFFFFFF) e atribuir roles por evidência — VRAM = maior BAR ≥64MB, MMIO = BAR0 exceto quando BAR0 É a aperture (AMD dGPU → BAR5, APU → BAR5). APU sem BAR grande → DRAM compartilhada (honesto). Regra: BAR roles se detectam, não se assumem por tabela DID.
- **Intel BCS encodings (ADR-0087 F3, i915 source):** `BLT_RING_BASE` = **0x22000** (não 0x220000 — hex a mais); TAIL = base+**0x30** (0x22038 é **RING_START**!); `RING_CTL` = `RING_CTL_SIZE(16K)|VALID` = **0x3001** (não 4096 dwords); XY_SRC_COPY_BLT = **0x54F00008** (opcode 0x53 — **0x41 é XY_COLOR_BLT**!), depth 32bpp no **DW1** bits 25:24 (não no header), precisa DW2 x/y + **DW3 x2/y2** + src_pitch; **MI_FLUSH_DW = 0x4C000001** (3 dwords; 0x02000000 é o MI_FLUSH pré-gen6). Sempre confirmar encoding em i915 source (`intel_gpu_commands.h`/`intel_engine_regs.h`/i-g-t) antes de escrever registradores Intel.
- **Submissão por ring ≠ batch buffer (ADR-0087 F3):** NÃO usar MI_BATCH_BUFFER_END dentro de ring engine — o engine para nele e HEAD nunca alcança TAIL → `wait_idle(HEAD==TAIL)` dá timeout. Ring vazio ⟺ HEAD==TAIL; o i915 preenche com MI_NOOP e só atualiza TAIL.
- **NVMe PRP (ADR-0087 F1):** um único PRP1 (cdw6/7) só vale se o transfer inteiro cabe numa página; cruzar fronteira exige PRP2 (cdw8/9, 2ª página) ou **lista PRP** (512 entradas × 8B por página). O driver local nunca setava cdw8/9 → transfer multi-página silenciosamente quebrado (só a identidade contígua mascarava). Regras: offset = dma_addr & (page-1); lista quando remaining > page após 2ª página. Testável em QEMU `-device nvme`.
- **SASOS + CE são complementares (ADR-0087 §2.0.1):** VRAM no heap por ponteiro (SASOS, acesso pontual/aleatório — KV pages, tensores <1MB) ≠ DMA bulk via engine (CE/SDMA/BCS, pesos 792MB). SASOS decide ONDE o dado vive, CE decide COMO moves bulk acontecem. Ordem: SASOS primeiro (dá o ponteiro), CE depois (dá a velocidade). WC p/ gravar VRAM via CPU (`movntdq`), UC p/ ler.
- **`record_access` sem callers = política é ruído (ADR-0087 F2):** o MHI tinha `record_access` sem nenhum caller → `access_count=0` para tudo → `arc_suggest_tier` era ruído. O fix não é a política — é o **wiring** (disk write `io_scheduler_flush`, disk read `readahead_hint` com convenção lba*512, `vram_alloc`/`vram_free`/`msched_record`). Política só vale quando os dados chegam.
- **Scheduler rate-limita agentes passivos — input/rede morrem de fome (SESSION_252/OTA):** `if urgency == 0 && consecutive_pending > 50 && tick_id % 5 != 0 { continue }` (agent-core/lib.rs:417) — InputAgent/HwBridgeAgent/NetAgent retornam `Pending` sempre → após 50 ticks são skipados 80% → `polled=1` no log → teclado e rede param de rodar. Fix: `AgentRegistry::set_urgency(name, u8)` — agents interativos (hw_bridge=200, net=180, input=200, mouse=150) com urgency > 0 são isentos. Bug real de HW: input interativo morre após ~50 ticks ociosos.
- **`BudgetManager.reset_all()` sem callers → `polled=0` (SESSION_252/OTA):** o `check_budget` consome 1 tick por poll; `ticks_used` acumulava para sempre (reset_all nunca era chamado) → após ~103 polls cada agente Continuous estourava o budget 100 e virava `Paused` → scheduler parava de rodar tudo (polled cai 35→22→5→4→0). Fix: `budget_manager.reset_all()` no início de cada ciclo do `run()`. O watchdog anti-runaway continua via `consecutive_pending > 10000`.
- **smoltcp clock: TIMER_TICKS ≠ ms (SESSION_252/OTA):** o PIT incrementa TIMER_TICKS a ~18.2Hz (~55ms/tick). Passar `Instant::from_millis(now)` direto faz o relógio do smoltcp rodar ~55× mais devagar: delayed-ACK (~40ms smoltcp) vira ~2.2s reais → slirp (RTO 1s) retransmite, backoff estoura e **aborta com RST** → downloads grandes truncam ~1748 bytes no fim (hash_mismatch no OTA). Fix: `Instant::from_millis(now.saturating_mul(55))`. Qualquer uso de tick como ms em timer de rede é bug latente.
- **http_poll aceitava encerramento precoce como Done (SESSION_252/OTA):** CloseWait/Closed → `Done(data)` sem validar corpo. RST do slirp (ou FIN precoce) virava "download completo" truncado. Fix: parse `Content-Length` (header_len + expect_len) + `http_complete` (body >= Content-Length) + drenar `while tcp.can_recv()` no CloseWait/Closed — corpo truncado vira `Failed`, nunca sucesso.
- **Checksum TCP RX ignorado → payload corrompido aceito (SESSION_252/OTA):** `ChecksumCapabilities::ignored()` + `tcp = Checksum::Tx` valida só TX — segmentos TCP corrompidos (checksum errado) eram aceitos, o TCP não retransmitia, e o conteúdo chegava corrompido com tamanho exato (hash não-determinístico). Fix: `csum.tcp = Checksum::Both` (RX validado, segmento ruim descartado e retransmitido). Sempre validar checksum RX de TCP em download.
- **ESP GUID: ordem textual ≠ on-disk LE (SESSION_252/OTA):** `mk_esp_fat.py` usava `bytes.fromhex("C12A7328...")` (ordem textual/BE do GUID) mas GPT on-disk exige little-endian misto → o kernel (fat32.rs `GPT_ESP`) não reconhecia a partição → ESP virava `0xEE` em vez de `0xEF` → `with_fat_reader` rejeitava → UPDATE.CFG "missing" no OTA. Fix: `uuid.UUID("C12A7328-...").bytes_le` (mesmo do build_usb_unified.py:38). GUID em GPT = sempre `.bytes_le`, nunca a string crua.
- **FAT32 short-write: arquivo < 512B em cluster spc=2 → PANIC (SESSION_252/OTA):** `write_cluster_chain` grava o cluster inteiro (spc×bps); no setor s=1 com `data.len() < 512`, `&chunk[512..]` estourava (`range start index 512 out of range for slice of length N`). Fix: setor sem dados = zeros (cluster é maior que o arquivo; o size no dirent limita a leitura). Bug real de HW: WIFI.CFG/BOOT.LOG/TLSPINS.BIN pequenos gravados na ESP panicavam.
- **PIC fallback mascarava o teclado (SESSION_252/OTA):** `remap_pic_pit_fallback` gravava `0xFA` (1111_1010) no PIC master → **IRQ1 (teclado) mascarado** — o mouse funciona por polling do status 0x64, mas o teclado depende de IRQ1 → sendkey/scancode nunca chegava. Fix: `0xF8` (IRQ0 PIT + IRQ1 teclado + IRQ2 cascade abertos). `0xFA` deixa o timer e cascade mas **esquece o teclado**.
- **json_field: json.dumps Python emite `": "` com espaço (SESSION_252/OTA):** o parser procurava `"version":"` mas o json.dumps gera `"version": "` → manifest nunca parseado (`manifest=no_version`). Fix: aceitar `: ` opcional. Qualquer parse de JSON gerado por Python precisa tolerar espaço após `:`.
- **Frame allocator não exclui kernel/heap → DMA sobrescreve memória viva (SESSION_252/OTA, residual):** `init_from_usable_ranges` (memory.rs:56-98) marca como livre tudo que o Limine reporta `MEMMAP_USABLE` — sem excluir a imagem do kernel, `.bss.heap` (HEAP_BUFFER) nem page tables. Quando um `deallocate_frame` devolve um frame ainda vivo (gguf_mmap.rs:121, dma.rs, virtio_net.rs), o e1000 aloca esse frame como buffer RX e o DMA do NIC **sobrescreve o heap (conn.buf do download) depois do checksum validar** — tamanho exato + hash não-determinístico. Fix proposto (ora-1): excluir kernel/heap/page tables no init + auditar deallocs (unmap antes do free).
- **Corrupção com tamanho exato + hash determinístico = bug no hash, não na transmissão (SESSION_252/OTA):** o download do KERNEL.BIN (17MB) tinha tamanho exato mas hash errado — e o `got` era **idêntico entre rodadas** com o mesmo arquivo (determinístico). A hipótese de frame allocator/DMA race estava **errada** (seria não-determinística). A causa raiz era o `k_nano::tpm::sha256`: o byte `0x80` do padding SHA-256 ficava no índice 0 do bloco de pad em vez de `remaining` → para `len % 64 != 0` (kernel.elf: `17415976 % 64 = 40`) o hash saía errado deterministicamente. **Sempre valide a implementação criptográfica contra vetores FIPS antes de investigar a rede.** Fix: padding inline correto (`last[remaining] = 0x80` + bloco extra quando `remaining >= 56`) + 3 vetores FIPS de teste em `tpm.rs`. O mesh/TLS "funcionavam" com o sha256 bugado porque eram self-consistent (dois nós com o mesmo bug = mesmo hash errado).

- **Subagente não escreve fora do workspace (SESSION_256):** 2 fixers retornaram **vazio** (task "completed", zero arquivos) ao tentar criar `C:\DEV\neural-sgdb\src\*` — o alvo fica fora de `C:\DEV\neural-os-core` e o sandbox de escrita do subagente bloqueia. Sintoma: resultado vazio + nada gravado = caminho errado, NÃO falta de trabalho. Verificar o alvo de escrita antes de delegar; orquestrador executa direto quando o repo alvo está fora do workspace.
- **`f32::sqrt` NÃO existe no core p/ `x86_64-unknown-none` (SESSION_256):** confirmado empiricamente com rustc (`E0599: no method named sqrt found for type f32`). Por isso o kernel usa `libm::sqrtf`. Em crate zero-deps: Newton-Raphson 10 iterações (~6 LOC). `deny(warnings)` em no_std eleva dead-code a erro — `#[allow(dead_code)]` explícito onde o port deixa API não usada (ex: `TOMBSTONE` só em std).
- **MCP server: handshake legado `2025-11-25` (SESSION_256):** JSON-RPC 2.0 sobre stdio, UMA mensagem por linha `\n` (stdout SÓ JSON-RPC; logs→stderr). `-32601` em `server/discover` faz client moderno (2026-07-28) cair para `initialize` — responder erro e continuar vivo. Claude Code envia `tools/list` **sem esperar** `notifications/initialized` (health check) — NÃO gatear tools no initialized. `initialize` result exige `protocolVersion`+`capabilities`+`serverInfo`. Embedding: crate standalone não tem BGE — demo por hash de trigramas rotulado (recall real exige embeddings próprios).
- **CRDT rate-limit com `Option<u64>`, não sentinela 0 (SESSION_256):** guard `last != 0 && now-last < interval` falha quando o primeiro sync acontece em `now=0` (nunca mais rate-limita). No kernel o tick nunca é 0 na prática; em crate host `now=0` é legítimo. Port de lógica kernel → crate: reexaminar sentinelas assumidas.
- **ART upstream não suporta chave-prefixo (SESSION_256):** chave onde uma é prefixo de outra (ex: `k0000/1` e `k0000/10`) → split silencioso (get retorna None). O kernel nunca insere isso (sufixos largura fixa); documentado como limitação herdada no README/api.md do neural-sgdb. Testes com chaves contract-compliant.
- **Stack do bootloader é memória viva não reservada (SESSION_254 → corrigida em 258/260):** o kernel pede stack de 2MB ao Limine (`StackSizeRequest`) mas NÃO reserva onde ela foi alocada no frame allocator → com loader 4GB (BITNET2B) ou HW real (16GB), o watermark das alocações sobe até a região da stack e o allocator entrega frames da PRÓPRIA stack do kernel → return address corrompido → `#PF ip=0x0` (QEMU) / triple fault + reboot (HW real, bloco K33). ⚠️ **O `StackSizeResponse` do Limine NÃO tem campo `address`** — o protocolo define só `{ revision }` (verificado na fonte oficial limine-protocol/PROTOCOL.md). O fix do 254 que lia `resp.address` (adicionado ao struct Rust) era **fantasma**: lia .bss zerado = 0 → `reserve_range(0, 2MB)` = no-op silencioso — QEMU passava por acaso (watermark menor), HW real crashava. **Fix real (57ad20a):** derivar a stack do RSP atual (o kernel EXECUTA nela; RSP virtual = phys + pm_offset no HHDM) e reservar `(rsp & ~2MB) − 2MB, 4MB` (margem p/ stack não-alinhada). Log valida: `reserva stack via RSP 0x98000000 len=4MB` (antes `0x0`). Lição: **nunca confiar em campo de struct de bootloader sem conferir o protocolo oficial** — o campo "obviamente útil" que lê 0 é bug latente que só explode em HW com mais RAM. Heap eager (`resize_bump_heap` 1024/1536MB no T+0) PIORA: sobe o watermark e expõe o bug; piso 512MB + `grow_bump_auto` (OOM, allocator.rs:46) é o comportamento AIOS.
- **MBR de pendrive UEFI exige ESP 0xEF visível no MBR (SESSÃO_258/260, empírico notebook real):** o firmware do notebook (UEFI, SB OFF) **só lista o stick se o MBR tiver a partição ESP (tipo 0xEF)** apontando para `\EFI\BOOT\BOOTX64.EFI` — GPT sozinha (MBR só 0xEE protetora) **não basta** para firmware real (OVMF tolera, notebook não lista). Regressões que falharam: `df88cc0` (MBR só dados 0x0C → firmware não lista; Windows monta), `2dd6ffc` (MBR 0xEE+dados → firmware real não lista; Windows vê "FS não reconhecido"). **Fix canônico:** MBR `slot0=dados 0x0C` + `slot1=ESP 0xEF` **SEM flag ativa 0x80** (a 0x80 fazia o Windows tratar o stick como disco de sistema e não montar NEURAL-OS; firmware UEFI acha o ESP pelo tipo, não precisa de 0x80). GPT continua com ESP+dados. Validado QEMU/OVMF. **Não confiar em OVMF sozinho para boot USB** — firmware real é mais estrito; testar o layout em HW.
- **`cargo clean -p neural-kernel` remove 0 arquivos p/ kernel no_std (SESSION_258):** o binário vive em `target/x86_64-unknown-none/`, não em `target/release/` — o clean por package não limpa o target específico. Para rebuild real: `Remove-Item -Recurse -Force target/x86_64-unknown-none` (check 1m19s revelou o que o cache de 0.36s mascarava).
- **`set_urgency` com nome errado = fix morto (SESSION_258):** `set_urgency("net", 180)` não aplicava ao NetAgent porque o manifest é `"network_agent"` (agents.rs:221) — o fix de starvation do s252 nunca surtiu efeito na rede. Antes de aplicar urgency/registro, confirmar o `name` do manifest (`set_urgency` retorna bool e o retorno era ignorado).
- **Watchdog que não distingue idle de runaway mata fleet (SESSION_258):** `consecutive_pending > 10000 → Crashed` sem recuperação (RESPAWN_QUEUE sem writers) crashava agentes interativos (urgency>0, Pending por design) em ~9 min. Fix: `watchdog_should_crash(urgency, consec) = urgency==0 && consec>10000` (espelha a isenção do rate-limit). Fleet idle sem urgency ainda crasha em ~46min — se "nunca crashar idle" virar requisito, remover a transição Pending→Crashed (o rate-limit já limita CPU).
- **EventDriven com contador self-referential dorme para sempre (SESSION_258):** `has_event = consecutive < 20` (lib.rs:440) sem caminho de reset externo = 147 Agency specialists + AutoInstallerAgent (SYS_INSTALL nunca consumido) inertes. Fix: trait `Agent::has_pending()` (default false) + `event_driven_has_event(last_poll, has_pending) = last_poll==0 || has_pending` — o `Receiver::has_pending()` do event-bus já existia; os agents só nunca eram pollados.
- **Falso HIGH em auditoria: verificar medindo o artefato (SESSION_258):** oráculo reportou "feat=0x42/act=10 rejeita 3 de 4 .v6" — errado; leu offsets de header errados. Medição própria com offsets canônicos do `v6_file_size` (hidden@18, tok_len@45, act/emb/feat pós-tokenizer) provou todos passam no gate. Qualquer achado HIGH em parse/loader merece medição direta do arquivo antes de reportar.
- **include_bytes! de modelo gitignored quebra teste em clone fresco (SESSION_249 reaplicada em 258):** o teste de paridade hwexpert v6 ficou quebrado de 372afd6 até 258 porque os paths apontavam p/ modelos movidos a `legacy/` e `target1/` (gitignored). Fix: un-ignore explícito (`!legacy/hw_expert_v4.bitnet`, `!target1/hw_expert_v6.bitnet`) + `git add -f`.
- **Output buffered em build longo parece travado (SESSION_258):** `Select-Object -Last` segura todo stdout até o fim — em build de minutos o usuário aborta achando que travou. Em comandos longos, stream direto (sem -Last) ou `2>&1 | Tee-Object`.
- **Modifiers de teclado: função pura + estado no agente, nunca tabela só-lowercase (SESSION_259):** `scancode_to_ascii` era pura `(u8)->Option<char>` só-lowercase e o InputAgent dropava break codes (`if scancode>=0x80 return`) sem rastrear Shift/CapsLock → sem maiúsculas, sem shifted symbols (`!@#{}:…`), CapsLock inerte. Fix: tabela pura `(scancode, shift, caps)->Option<char>` (letras uppercase iff `shift != caps` XOR, dígitos/símbolos shiftados, teclas faltantes `[ ] ; ' \` \ ,` absorvidas da cópia morta do bin) + estado `shift/caps` no InputAgent (breaks `0xAA/0xB6` limpam shift, toggle caps só no make `0x3A`). Break code = make|0x80 em set 1; match em `key = scancode & 0x7F` cobre make e break juntos. Fonte única: cópia morta `pub(crate) fn scancode_to_ascii` no bin (main.rs:4019, zero callers) DELETADA. Referência completa (set 1/2, 8042, encoder 0xED/0xF0/0xF3, status 0x64): minerada do BrokenThorn OSDev series no mempalace room `neural-os-core/brokenthorn-osdev` (site brokenthorn.com dá 403 direto — usar Wayback `web.archive.org/web/2024/...`).

- **Desenho fora do `render()` do compositor é apagado (SESSION_261):** `JarbasDesktop::render()` é o ÚNICO pintor de frame e o único que chama `fb.swap()` — ele apaga o back buffer inteiro (compositor.rs:394) e só swap() dentro dele (577). Qualquer draw em `desktop.fb` fora do render (ex: no `tick()` do DisplayAgent) é apagado no frame seguinte — os cards de status de mesh (agent.rs:717-760, s242) existiram assim e **nunca apareceram na tela** (o log serial funcionava, a UI não). Padrão correto: dados → static compartilhado (`MESH_GRAPH: IrqSafeLock<Vec<_>>`), render() desenha. Regra visual: orb (nível "um eu", AffectVector) pode virar o nó central de um grafo (nível "sistema", topologia de peers) — substituir nunca, centralizar sempre.
- **Compositor hot path (SESSION_294):** `fill_rect` via `set_pixel` ≈ 1M writes/frame; `fill_rect_fast` bpp=4 com `aw/4` pintava 25%; `fill_circle_glow` O(r²) `sqrtf` no orb r≈264 ≈ 280k sqrts; `TARGET_FRAME_TICKS=3` + PIT ~18 Hz ≈ 6 FPS e cada frame faminto o scheduler (serial congelava). Fix: fill por linha + doubling memcpy, glow scanline+isqrt, 1 frame/tick, orb menor. ADR-0090 “glow integer-only” mentia — medir o código. ESP: `python` (não `python3` Store). Guest QEMU aberto não recarrega `uefi.img`.
- **HW pendrive BOOT.LOG skip ≠ hang fatal (SESSION_295):** `init_after_usb` sem MSC/ATA/AHCI só imprime skip — o freeze vinha **depois**: P24a/b `bringup_hid_*` sem MSC, K27 `verify_kernel_from_disk` (ATA PIO minutos), K71 `run_deferred` completo. Fix: `live_usb_no_msc` + `run_deferred_usb_live` + defer HID. **Limine-only no reboot** = `uefi.img` stale vs `kernel.elf` (`build.rs` falha mk_esp no Windows) — sync `limine-esp.img` antes de `usb_hw.img`; `build_image --build-boot` obrigatório. BOOT.LOG Notepad = BOM UTF-8. Display splash congela sem `set_urgency("display", 220)`.
- **Splash freeze com BOOT.LOG intacto (SESSION_296):** `E:\BOOT.LOG`/`NSGDB.BIN` placeholder/zeros = Runtime sem MSC write no stick. Splash = tick 1 `claim_graphics`; compositor só tick 2 (SESSION_168). Fix: `render()` imediato pós-claim em `agent.rs`. Imagem regerada 22:22.
- **PIT tick ≠ 5 ms (SESSION_294):** gate de FPS não pode assumir 200 Hz. `TIMER_TICKS` ≈ 18.2 Hz. Comentário “60 FPS” sem TSC é doutrina falsa (mesma classe do smoltcp clock s252).
- **Scans de memória fixa devem checar PRESENT (SESSION_262):** scans do QEMU-loader (`read_volatile` em ranges fixos 0x100000000+/0x129000000+) liam hole não-mapeado → #PF storm em máquina com RAM insuficiente. Fix: `k_nano::memory::is_page_present(virt)` (walk PML4→PT com HUGE_PAGE 1GB/2MB) + guard nos 3 scans. AIOS mede e pula, nunca crasha em máquina menor.
- **Duas cópias de módulo divergem silenciosamente (SESSION_262):** o bin `smp/mod.rs` é cópia do k_nano com checkpoints K22 próprios — o fix no k_nano `init_smp` (IDs do MADT) nunca rodou porque `crate::smp::init_smp` resolve para o bin. Sempre verificar qual cópia o bin realmente chama antes de assumir que o fix da crate base vale.
- **LAPIC IDs com HT não são sequenciais (SESSION_262):** wake SMP deve usar os IDs reais do MADT (`BOOT_APIC_IDS`), não guess `bsp+1, bsp+2...`. No i5-7300HQ (4C/8T) os IDs são ex `0,1,4,5` — INIT-SIPI para ID inexistente → 0 APs acordam (`madt_lapics=4` mas `total_cores=1`).
- **Dump do ramlog roda antes do init_phase (SESSION_262):** trace de init_phase precisa imprimir no FB direto (`console_print`), não só no ramlog — o dump `take(40)` do main.rs:3948 roda antes do `init_phase`. Padrão: `AgentRegistry.init_trace` (fn pointer zero-dep no agent-core) loga `INIT1: r<N> poll <agente>` no FB antes de cada tick de Oneshot.
- **Freeze de agente Oneshot no metal sem ATA (SESSION_262):** `BootSelfHealAgent` fazia `scan_pci` + inventário pesado que travava no boot USB (ATA_DRIVER=None). Fix: gate `has_ata = ATA_DRIVER.lock().is_some()` — sem ATA, pula scan e faz honest noop (`run_vid_gated_scan(&[])` + `SystemArchitecture` vazio).

- **Instalador com source só-ATA quebra boot USB (SESSION_292):** `run_install_from_bus` exigia `ATA_DRIVER` como source — boot por pendrive em HW real popula `USB_MSC`, não ATA → `"sem ATA (boot device ausente)"` sempre. Fix: fallback ATA→USB_MSC na leitura do kernel.elf E no source da cópia ESP + **guarda target≠source por endereço** (`ptr::eq` sobre `*const u8`) — com source USB, o auto-pick (AHCI→NVMe→USB) podia escolher o próprio boot device e formatá-lo. `Fat32Reader`/`read_mbr` são tipados em `&AtaDriver` (I/O `&self`); generalizar quebraria 45 callers em 7 crates — para devices dinâmicos usar `fat32::read_root_file_dev(dev, part, name)` (mesmos gates do `read_file`; padrão boot_logger).
- **CONFIG.TXT só-exFAT = flag morta na imagem unified (SESSION_292):** `peek_config_txt` lia apenas partições exFAT; a imagem unified (`--hw --unified`) tem dados em FAT32 `0x0C` → `NEURALFS_USB_FORMAT=1` nunca era lida e o NeuralFS USB ficava preso em RAM 4MB no release. Fix: ramo FAT32 (`0x0B/0x0C/0x1C/0xEF`) via `read_root_file_dev`. Regra: config lida on-device deve cobrir TODOS os filesystems que o build produz.

- **`shell::execute` é código morto sem caller (SESSION_293):** `crates/neural-kernel/src/shell.rs` e `crates/hermes/src/shell.rs` definem `execute(cmd)`, mas **nenhum dos dois é invocado pelo HermesChat** — input do chat vai para `HermesAgent` → `parse_command` → `Command::Chat` (ou variantes nomeadas). Antes do fix, `install` no HermesChat caía em `Command::Chat("install")` → LLM, e o `shell.rs` handler da própria hermes era um stub que só imprimia texto (não publicava `SYS_INSTALL_UI`). Resultado: card 7902 do instalador nunca aparecia. Regra: para cada `shell::execute` handler, rastrear o caller até o ponto onde o input do usuário entra — se não houver, é feature desligada. Fix: variante `Command::Install` em `hermes.rs::Command`, parse `/install`/bare `install`, dispatch em `HermesAgent` publica `TOPIC_SYS_INSTALL_UI` (parity com shell.rs).

- **Descritor virtio encadeado exige `VRING_DESC_F_NEXT` no flags, não só no `next` (SESSION_297):** `virtio-blk missing headers` no QEMU 11.1.0 — os descritores (hdr→dado→status) setavam `next=1,2,0` mas **flags=0x0/0x2** sem o bit 0 (`VRING_DESC_F_NEXT=1`). O QEMU só segue o campo `next` SE esse bit estiver setado (`virtqueue_split_read_next_desc`); sem ele lê só o header → `out_num=1, in_num=0` → `virtio_error` marca o device BROKEN e todos os requests timeout. O `virtio_net.rs` (referência que funciona) **não encadeia** descritores (1 pacote = 1 desc, `next=0`), então o bug nunca aparecia lá — copiar o padrão do net para o blk foi o erro. Fix: `desc[0].flags = DESC_F_NEXT`, `desc[1].flags = DESC_F_NEXT | DESC_F_WRITE` (se read), `desc[2]` fim (sem NEXT). Diagnóstico decisivo: `page_leaf_phys()` (walk P4→PT) provou que a HHDM apontava pro frame físico correto — o problema era o **conteúdo** do descritor, não o mapeamento. Lição: `read_bar_value` (pci.rs) mascara o bit 0 de I/O BAR (`low & !0xFF`) — drivers de device I/O-legacy devem re-ler o BAR raw via `read_config_dword`. FileFlash ganhou `FlashDev::VirtioBlk` (QEMU dev/test) → `TICKV backend=file dev=virtio` (NSGDB persiste em disco, antes `backend=RAM VOLATIL`).

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
- **op-IR WASM: LogicalAnd/LogicalOr não podem ser emitidos como select simples (v2.0, sessão atual):** `select` em WASM MVP não tem acesso ao segundo-da-stack sem locals/dup/swap. `i32.const 0; select` simplesmente escolhe um dos operandos, ignorando o outro. Fix: transformar `&&`/`||` em **blocos If/Else/End no parse** (não no emit). `a && b` → `left; I32Eqz; If(Some(I32)) { 0 } Else { right }`. `a || b` → `left; If(Some(I32)) { 1 } Else { right; eqz; eqz }`. O padrão `ops.drain(left_end..)` separa operandos esquerdo/direito no parser antes de montar o bloco. **Semântica assimétrica:** `&&` retorna o valor de `b` (short-circuit), `||` retorna booleano 0/1 — definido pelos testes, não por convenção C.
- **If arity em WASM: `If(Some(ValType::I32))` vs `If(None)` (v2.0):** blocos que produzem valor na stack precisam de `Some(I32)` (arity=1); blocos side-effect-only usam `None` (arity=0). Usar `None` num bloco que deveria produzir valor causa "deve sobrar exatamente 1 valor" no validate. Sempre checar a arity do resultado esperado antes de emitir If/Else/End.
- **ADR-0089 Per-CPU Run-Queues (sessão atual):** Slot-based MPMC com `SyncCell<[RqSlot;128]>` + HEAD/TAIL atômicos (padrão `ap_work.rs`). `steal_agent` usa round-robin com min-1 threshold para evitar starvation. `CPU_COUNT` em testes host precisa ser setado explicitamente (default=1 — `steal_agent`/`total_pending` retornam cedo se `n<=1`). Testes compartilham statics → `TEST_LOCK` + `clear_all_queues()` obrigatórios.
- **SyncCell pattern para statics no_std:** `SyncCell(UnsafeCell<T>)` com `unsafe impl Sync` — permite writes em `static` arrays sem heap. Padrão canônico em `ap_work.rs`. `[T; N]::new()` em statics exige `T: Copy`; arrays com `UnsafeCell` precisam de wrapper que implementa `Copy` manualmente ou `#[derive(Copy)]` no conteúdo.

- **Falcon3-3B ≠ 7B (SESSION_298 / ADR-0101, configs HF 2026-08-31):** compartilham família Llama + GQA 12Q/4KV + head_dim 256 + hidden 3072 + vocab ~131K + SwiGLU/RMSNorm. **Não** compartilham profundidade nem FFN: 3B = **22 layers / intermediate 9216**; 7B = **28 layers / 23040**. 1.58bit 3B declara **ctx 4096** (o Instruct denso 3B é que tem 32768). 1B = 18L / hidden 2048 / 8Q/4KV. Falcon-Edge-3B é **outro** modelo (32L / hidden 2048 / vocab 32K). Lab canônico = 3B 1.58-bit (`FALCON3.V6`). Converter: `python tools/convert_falcon3_bitnet.py --hf-repo tiiuae/Falcon3-3B-Instruct-1.58bit --output target1/FALCON3.V6`. 7B continua `PRO.v6` (GeneratorPro), não o lab.
- **Hardcoded Falcon3 no bin era o SKU errado (SESSION_288 vs HF SESSION_298):** constantes tipo `FALCON3_NUM_LAYERS=22` estão **certas para o 3B** e **erradas para o 7B** (28L / FFN 23040 / ~1.86GB). `FALCON3_FILE_SIZE=1.03GB` e `MAX_SEQ=4096` não descrevem o 7B (32K) nem o Instruct denso 3B (32K) — o 1.58bit 3B é que declara 4096. Fix: `ModelHeader` + `parse_model_header()` no v6. Nunca colar shapes do 7B no 3B.
- **ModelHeader pattern (Fase 1):** `parse_model_header(data)` retorna `ModelHeader { hidden, num_layers, vocab, ... file_size }` sem carregar o modelo completo. Static `LOADED_HEADER` + `set_model_header()`/`loaded_model_header()` para acesso global. `model_info()` e `slot_footprint_mb()` leem do header em vez de constantes.
- **Arena auto-size (Fase 1):** `auto_arena_size()` calcula 50% da RAM detectada, clamp [512MB, 4GB]. `CORTEX_ARENA_MAX_SIZE=4GB` suporta Falcon3-10B (2.5GB). Fallback 2GB se RAM não detectada.
- **slot_footprint_mb() dinâmico (Fase 1):** Para slots "active"/"generator"/"pro" tenta `loaded_model_header().file_size_mb()` primeiro; fallback para constantes known-small (4MB smoke, 1MB hwexpert). Nunca mais assume tamanho fixo.

- **neural-sgdb não é KV — é substrato de memória (SESSION_284):** o `Hit` tem 12 campos (key, text, dist, path, content_type, payload_type, score, matched_terms, validity, rel, provenance, score_breakdown). Reduzir a `(String, u32)` perde 80% da informação. O `recall_lexical` (BM25) é o default do MCP (ADR-0008) e funciona SEM embedding. O `Sgdb` do neural-sgdb não é genérico — usa `Box<dyn Storage>` internamente, requer wrapper `unsafe impl Send + Sync` (SafeSgdb). ContentType awareness muda o RAG: Embedding/Binary são skip, Json/Text/Code renderizam verbatim. Lifecycle é determinístico (sem wall clock). OsEmbedder conecta BGE ao Embedder trait. Dual-write durante migração (TickvLite + neural-sgdb ART/BQ redundantes até Fase 3).

- **ATA 4Kn detecta sector size do hardware (SESSION_285):**  agora lê words 117-118 do IDENTIFY para detectar  (512/1024/2048/4096).  adapta: 4Kn → múltiplos reads de 512B por setor lógico.  retorna o valor real.  traduz count de 512B para sectors do device. Testes host validam 512 e 4096.

- **DMA map_page_uc antes de set_page_uc (SESSION_285):**  só altera flags em mapeamento EXISTENTE — sem  antes, o MMIO fica stale e o device leitura 0. Padrão:  →  em todo driver DMA.

- **TSC sleep substitui spin loops no ATA (SESSION_285):**  é ~10ms fixo e impreciso. Substituído por  calibrado via TSC/HPET. Funciona em HW real e QEMU.

- **Dead code excluído ≠ removido (SESSION_285/286):** módulos sem callers são comentados no  (preservados no disco) em vez de deletados. Reduz build time ~23% (7,446 LOC) sem perder referência para futura reativação.

- **Jarbas: 40 locks eliminados por frame (ADR-0093):** Theme (), Mouse (), HUD string (cache), dirty-rects (skip background fill 1M px). Render loop passou de ~40 lock() calls/frame a ~0.

- **Hermes: 48 testes host adicionados em 3 módulos (ADR-0094):** cognitive_bridge (22), memory_store (11), self_evolve (15). Base para refactor seguro dos 32K LOC.

- **MHI migration executor real (SESSION_285):**  agora executa  em demote (Dram→Hdd) e  em promote (Hdd→Dram). Antes era metadata-only sem cópia de dados.

- **PT-BR TTS fonemas extras (ADR-0093):** 8 fonemas adicionados ao formant: lh, nh, ã nasal, õ nasal, rr, s final, ss, x → sh. TTS agora cobre a maioria dos sons PT-BR comuns.

- **HDA QEMU: GCTL funciona mas ICW/ICR não respondem (SESSION_286):** MMIO no BAR0 do intel-hda QEMU funciona para GCTL (offset 0x08) mas ICW (offset 0x60) retorna sempre 0x0. CORB/RIRB DMA também não recebe response (RIRB WP=0). DIAGNÓSTICO: GCTL read/write OK, CORBCTL=0x3 (running), mas controller ignora comandos. Causa provável: QEMU intel-hda partial emulation ou BAR mapping issue com1GB huge pages do HHDM. Fix: testar em HW real (HDA funciona em hardware nativo).

- **VAD compartilhado vs duplicado (SESSION_286):** AudioPipelineAgent e JarbasVoiceAgent instanciavam VAD separados. Pipeline VAD era usado só para barge-in — simplificado para threshold de amplitude sem VAD completo. Voice agent mantém VAD real (SpeechStart/SpeechEnd). Economiza ~2K LOC + CPU.

- **EWMA emotion decay (SESSION_286):** LAST_VOICE_EMOTION agora usa α=0.3 EWMA em vez de substituição direta. Evita oscilação brusca de emoção entre frases. Orb e persona mudam suavemente.

- **Piper smoke test (SESSION_286):**  valida que pesos geram audio com amplitude > 100. Detecta pesos corrompidos no boot. Chamar após .

- **xHCI HID QEMU: event ring all zeros — controller ignores command doorbell (SESSION_290):** `qemu-xhci` em TCG mode aceita MMIO reads (PORTSC funciona, CCS=1 nos ports P5/P6) mas NÃO processa command ring — `cmd_enable_slot()` sempre dá timeout com event ring todo zero (`evt=0x0 0x0 0x0 0x0`). Port Reset funciona (escreve PR no PORTSC), mas EnableSlot nunca completa. Guard `qemu_hid` foi removido (estava bloqueando o bringup inteiro). Odiagnostic agora é: PORTSC dump + reset FAIL/OK + EnableSlot FAIL com event ring peek. Funciona em HW real. **AWAITING_HW** para validação completa do mouse USB.
- **xHCI metal: Runtime Interrupter começa em +0x20; IOC/CC são contrato (SESSION_313):** `RTSOFF+0x00` é MFINDEX e o Interrupter Set 0 começa em `RTSOFF+0x20`; gravar ERSTSZ/ERSTBA/ERDP diretamente em `RTSOFF+0x08/+0x10/+0x18` escreve reservado e deixa o Event Ring mudo. Normal TRB: length no DW2 bits 0..16; **IOC é DW3 bit 5** (não DW2); completion `Success=1`, `Short Packet=13` (`0` não é sucesso). Takeover real também exige PCI BusMaster, USBLEGSUP OS-owned + SMI off/RW1C, `HCCPARAMS1.CSZ` 32/64, scratchpads em DCBAA[0], PAGESIZE/CNR, WPR/WRC em SuperSpeed, Protocol Slot Type por root port e ERDP.EHB. QEMU pode mascarar todos.
- **UI metal: hlt morto + early MSC sem teto = orb congelado / tela preta (SESSION_315/315b):** (1) `hlt` se TIMER_TICKS não avança → soft-halt `scheduler_idle_halt` + `wall_ticks()`. (2) HID defer **não** pode exigir `USB_MSC`. (3) PIC fallback slave `0xEF` (IRQ12). (4) Compositor orb-only **não** repinta HUD/dock. (5) Limine→preto: hub MSC EP0/reset sem budget + `boot_ckpt` ramlog-only — budget **3s** MSC + TSC 50–100ms + `boot_progress_line` no FB. UI first > hang early USB.
- **USB HW BE em k_hal (SESSION_313/314):** política hub→MSC (route string + TT, padrão Redox/Chitti) vive em `k_hal::usb`; `k_nano::xhci` só primitivos R0 + hook. Notebooks (Alienware) quase sempre têm o stick atrás de hub interno — root-only = BOOT.LOG placeholder.
- **Orb JARVIS MCU rendering (SESSION_291):** hex grid (dots nas intersecoes, 48px spacing, pulse por distancia), 5 camadas de glow (outer 2.8r -> body -> inner ring 0.7r -> core 0.18r -> specular), 24 particulas deterministicas, aneis rotativos com perspectiva eliptica (flatten Y x0.3), scanlines a cada 4px. Paleta: cyan #00D4FF em navy #080C18. Distance2 + quadratic falloff (zero sqrtf/expf). sinf/cosf so para ~24 posicoes/frame.

- **Anti-flicker compositor (SESSION_291):** o root cause do flicker era fill_rect(0,0,w,h) a CADA frame (limpa tela inteira). Fix: bounding box do orb apenas (+16px margin para particulas). dirty_hud NAO cascata mais apos orb/mesh redraw. dirty_orb = true a cada 2 ticks para manter animacao.

- **Theme JARVIS navy (SESSION_291):** Dark theme bg atualizado de (15,15,18) cinza para (8,12,24) navy. Consiste com o background do orb. fill_rect_fast para HUD bar em vez de fill_rect (8x mais rapido).

- **snapshot vs clone persistente (SESSION_291+):** ferramentas de edicao (write_file, str_replace) apontam para o SNAPSHOT, NAO para o clone. Apos editar, SEMPRE copiar do snapshot para o clone. SEMPRE testar com git rev-parse --is-inside-work-tree antes de assumir qual e qual.

- **pre-existing broken changes em working tree (SESSION_291):** sessoes anteriores deixaram virtio_blk.rs (457 LOC, erros de build) e storage_bus.rs (VirtioBlk variant sem match) no working tree. Revertidos com git checkout -- para desbloquear build. Regra: NUNCA commitar codigo que nao compila; se esta WIP, stash ou branch separada.

- **TTS streaming por frases (SESSION_292):** split_into_sentences() divide texto por pontuação (. ! ? ;). StreamingTtsState simplificado de 4 variantes para 2 (Idle/Streaming com queue). Síntese incremental: primeira frase em ~50-200ms vs ~500-2000ms bloqueio completo. Borrow-checker: core::mem::replace para ownership seguro no tick (evita conflito ref mut + reassign).

- **StreamingTtsState ownership (SESSION_292):** match self.stream_tts { ref mut ... } impede self.stream_tts = ... dentro do match. Solução: let prev = core::mem::replace(&mut self.stream_tts, Idle); match prev { ... } — take ownership, process, reassign. Padrão Rust idiomático para state machines com reassign.

- **Trinity MoE Fase 1 (SESSION_293):** hermes::globals::TRINITY era vazio (nunca populado). Populate_trinity_from_bin() copia experts do bin para hermes no boot. Bridge TRINITY_MMAP_BRIDGE agora instalado corretamente. trinity_inject foi reabilitado (estava comentado como dead code pelo HERMES_AUDIT). 168 testes (+22 novos).

- **Int8Router vs TrinityRouter não são duplicatas (SESSION_293):** MoELayer/Int8Router (moe.rs) = computação neural MoE (shared_expert + top_k inference). TrinityRouter (trinity.rs) = classificador de intents (keyword + neural routing). Servem propósitos diferentes — não unificar.

- **TrinityRouter experts() getter (SESSION_293):** campo experts era privado sem getter público. Adicionado pub fn experts() -> &[Expert] para permitir que o boot copie expert info para hermes.

- **Routing telemetry AtomicU64 (SESSION_293):** contadores stats_neural/stats_keyword/stats_fallback no TrinityRouter. Lock-free, zero overhead. neural_route_ratio() dá 0.0-1.0 de rotas neurais. Base para MonitorAgent/SelfHeal detectarem degradação.

- **Ring3 Onda 6 wired (SESSION_302 / ADR-0102):** `k_nano::ring3` = mailbox USER (N4) + `verify_blob_no_simd` (T-056) + `ring3_can_register_native()` (metal + `ring3_mark_hw_gate_passed`, separado de `ring3_is_safe`). P6 demos reais em `k_nano::paging` (não stub `user_mode`). `register_native_ring` só quando `can_register_native()` — TCG/WHPX nunca. `hermes::app_factory` precisa estar `pub mod` ou seam morto. `set_bsp_rsp0` = `k_nano::interrupts::set_bsp_rsp0` (TSS carregado), não `interrupts_ext` fantasma. `fault_abort` → `HEALTH_ISSUE:ring3:sandbox_fault`. Delete `neural-kernel/src/smp/percpu.rs` (espelho divergente).
- **P6 int 0x90 + marker mailbox (SESSION_305):** `#GP @ USER_CODE+0x19` = `int 0x90` com gate DPL=0 — instalar `idt[0x90]` **DPL=3** em `k_nano::interrupts` (belt-and-suspenders com `patch_idt`). Demo marker em `USER_MAILBOX_VA+48`, **não +32** (`result` da mailbox N4 é zerado por `syscall_finish_ok`). `syscall_stage_from_mailbox`: se `nr==0 && cap==0`, preservar stage de `enter_user_mode`. QEMU 4c: `tools/run-qemu-4c-loop.ps1` → saudacao+TTS+`ring3_can_iretq=true`+desktop_ready; fault/SSE demos ainda emitem `#PF/#UD` contidos (non-fatal). Magic `0xBE11BE11` @ loader ≠ BGE — log honesto no scan.
