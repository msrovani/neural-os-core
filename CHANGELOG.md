# Changelog — neural-os-core v2.0 "Ring Buffer Refactor"

## [Unreleased]

### SESSION_235 (item 4): Compute distribuído Worker→Master via P2P real (2026-07-31) ✅
- **feat(mesh): matmul ternário distribuído** — cortex feature `p2p` nova; o bloco
  `#[cfg(feature="p2p")]` do `dispatch_ternary` (que existia mas nunca compilava) agora
  funciona: Worker serializa w+x (`MW\0` + shapes u32 LE + packed_data 2-bit + x f32 LE,
  gate MTU 1200B), envia via udp_broadcast (TaskType::Inference), espera síncrona
  (~200 TIMER_TICKS) a resposta `MR\0` (filtro dest_id); timeout → fallback local.
- **feat(mesh): Master responde requests** — `poll_mesh_requests()` drena EventBus
  P2P_PACKET, computa com `ternary_matmul_adaptive` e responde. Gate "só Master"
  removido (responde mesmo Undecided — sob TCG o Master pode ainda não ter eleito).
- **feat(mesh): self-test distribuído** — `mesh_matmul_self_test()` 16×16 (1107B ≤ MTU)
  + retry 5x no bei_tick (DIAG do boot roda antes da eleição — nunca pegava o P2P).
- **VALIDADO QEMU dual**: `[B] matmul request node=3 size=1107 sent=true` →
  `[A] matmul resposta node=3 sent=true` → `[B] matmul resposta node=3 ok
  shape=(16,16) primeiro=120.0 (mesh dispatch)`. Commit `b6ab13b`. 0 erros.

### SESSION_235: Mesh P2P aplicações reais — Marketplace + PROMOTE + Papéis (2026-07-31) ✅
- **feat(marketplace): broadcast real** — `activate_global` popula `local_skills` do
  SKILL_REGISTRY canônico (14 skills, dedupe); antes nunca era chamado → nada enviado.
  Throttle por TIMER_TICKS (scheduler rate-limited: 200 CALLS demoravam minutos sob TCG).
- **feat(promote): PROMOTE_SKILL real** — Worker envia `PROMOTE\0name\0desc` (NoProto Sync
  via k_nano); Master detecta prefixo e registra `DynamicSkill("promoted from mesh worker")`.
- **feat(roles): propagação de papéis real** — `assign_roles` envia `ROLE\0target\0role_u8`
  (broadcast, throttle 110 ticks); receptor filtra por `node_id()` e aplica via `set_role`
  (era ponytail "send role-assignment"). Primeiro uso de destino no mesh.
- **fix(eleição): todos-Worker** — lazy-init do MESH_ENGINE usava MAC completo vs peers
  `[source_id,0,..]` → comparação lexicográfica sempre favorecia o peer. Fix: local usa
  `[node_id(),0,0,0,0,0]` (mesmo formato).
- **VALIDADO (2 QEMUs)**: A=Master node=2 (15 skills push + 14 offers broadcast sent=true),
  B=Worker node=3 (RX type=4 ModelUpdate + `role aplicado node=3 role=Memory`). 0 erros.
- Commits: `50bdf6b` (1+2+3), `e4917c1` (fix .data), `9239ac9` (node_id+tie-break).

### SESSION_234: P2P Mesh real entre 2 QEMUs + migração transporte→k_nano (2026-07-31) 🏆
- **feat(mesh): descoberta P2P funcionando de verdade** — duas instâncias QEMU
  (10.0.3.2/10.0.3.3) trocam heartbeats via broadcast UDP 42069 na NIC real
  (e1000), com RX cruzado (A recebeu `clock=4796` enviado por B) e eleição.
  Commits `f240fa4` (Fase A) + `0eec18f` (migração).
- **feat(skillsync): Master push → Worker apply** — 15 skills empurradas do
  Master (`broadcast=true`) e processadas no Worker via `poll_p2p` (EventBus
  topic `P2P_PACKET`). skill_sync (R3 hermes) e marketplace (R3 hermes+jarbas)
  consomem sem inversão de dependência.
- **refactor(mesh): transporte+serviço movidos do bin → k_nano (R0)** —
  decisão do oracle (a intuição do maintainer estava certa: mesh é camada
  baixa de sistema). `udp_broadcast::{build_frame,send,recv}` + `mesh::p2p_tick`
  agora vivem em k_nano (que já tinha smoltcp+e1000+nic_globals). O bin
  `net.rs` re-exporta os statics NIC de k_nano (transporte R0 usa o MESMO NIC);
  non-heartbeat publicado no EVENT_BUS; `net_bridge` P2P removido (HTTP/TCP/DNS
  permanecem). `set_nic_config(mac,ip)` só pós-configuração (set_static_ip/DHCP).
- **fix(script): run-qemu-p2p-mesh.ps1** — ASCII puro (PS 5.1 lê sem BOM como
  ANSI), `$Root = $PSScriptRoot`, OVMF via caminho 8.3 (`C:\PROGRA~1\...`),
  `-m 8G` + `-smp 2` (MTTCG), switch `-NoDisk` (teste P2P é rede pura — a
  leitura FAT32 dos modelos via ATA PIO sob TCG travava o boot).
- **cargo check --release**: 0 erros
- **Known**: `nodes=1` na eleição (node_id = `local_role()` colide entre nós) —
  next: derivar node_id do MAC/IP real (10.0.3.2→2, .3→3).

### SESSION_233b: Ring3 triple-fault resolvido — boot QEMU 100% (2026-07-30)
- **fix(ring3): RSP=0 no `jump_back_to_kernel`** — `"xor ax, ax"` para zerar
  ds/es/ss clobberava o registro RAX que o compilador escolheu para o operando
  `{rsp}` → `mov rsp, rax` com RAX=0 → ret para RIP=0 → #PF storm (CR2=rodata).
  Em long mode zerar segmentos era desnecessário (SS.RPL=0 vem do TSS no int 0x90).
- **fix(ring3): callee-saved restore** — handler `extern "x86-interrupt"` salva
  rbx/rbp/r12-r15 na stack RSP0 e o `jmp` pulava o epilogue → restaurar em
  `jump_back_to_kernel` (CPL=0 + kernel CR3, statics acessíveis).
- **RESULTADO**: `P6 SUCCESS iretq+CPL3 marker=3352494e470001 Cap::ENTER_USER`
  + `BOOT: P6 Ring3 OK` + scheduler vivo (tick=1 agents=53 polled=32).
  Boot QEMU 8GB + OVMF + janela completa sem reboot loop. ✅
- **fix(mem): statics .bss corrompidas pelo bump heap** — `resize_bump_heap(2048)`
  entregava endereços além do HEAP_BUFFER (512MB) → sobrescrevia
  GLOBAL_ALLOCATOR/PHYS_MEM_OFFSET/TOTAL_RAM_MB → `total_frames=0` → falsa
  exaustão de frames ("sem frame CoW"). Fix: statics → `.data` +
  HEAP_BUFFER → seção `.bss.heap` no fim da imagem (limine.ld).
- **fix(neuralfs): nunca formatar disco com partições** — `try_format_gpt_virgin`
  não bloqueava 0xEE (protective GPT do ESP Limine) → kernel formatava o
  uefi.img como NeuralFS → OVMF "Not Found" no boot seguinte.
- **fix(boot/build.rs): rerun-if-changed** — sem isso uefi.img ficava stale
  (corrompido por boot anterior).
- **cargo check --release**: 0 erros

### SESSION_233: Ring3 Isolation (ADR-0077) — 6 fases (2026-07-30)
- **Phase 0 (fix)**: CR3 switch BEFORE iretq asm (Moros pattern). Moveu `mov cr3` do inline asm para `address_space::restore_cr3()` em Rust, eliminando triple-fault após switch de page table.
- **Phase 1 (feature)**: `user_mode::run_process(pid)` — conecta ELF loader + ProcessManager + `enter_user_mode()`. Comando shell `run <pid>`.
- **Phase 2 (feature)**: TSS mutável via `TssCell` (wrapper Sync) + `set_rsp0()`. Per-process kernel stack para traps CPL=3→0.
- **Phase 3 (feature)**: Syscall ABI por registrador (RAX=nr, RDI=arg, RDX=caps) + fallback atomics. Handler lê registradores quando `stage_syscall()` não foi chamado.
- **Phase 4 (feature)**: `address_space::create_sandbox_as()` — AS do zero que só copia entries P4≥256 (kernel+HHDM). Sem tabelas L3/L2/L1 compartilhadas com kernel.
- **Phase 5 (feature)**: Hypervisor-aware gating em `isolation_ring.rs`. `ring3_is_safe()` = true só em KVM; TCG/WHPX/HW real = gated. `init_connectors()` registra native ring via `register_native_ring()` quando seguro.
- **fix: TssCell wrapper Sync** — substitui `UnsafeCell` por `TssCell(TaskStateSegment)` com `unsafe impl Sync` (single-threaded durante Ring3).
- **cargo check --release**: 0 erros

### SESSION_232: Bootloader 0.11 cleanup — Limine path único (2026-07-30)
- **clean: vendor/bootloader/** — crate do image builder 0.11 removida (~1.8MB, 65+ arquivos)
- **clean: bootloader_api dep** — removida de k_nano, neural-kernel, jarbas Cargo.tomls
- **clean: limine-boot feature** — removida; Limine é agora unconditional (sem feature gates)
- **clean: bootloader 0.11 entry point** — `kernel_main(boot_info)`, `BootloaderConfig`, `entry_point!` removidos
- **clean: BootloaderHandoff** — `neural-kernel/src/boot_handoff.rs` deletado (wrapper `bootloader_api::BootInfo`)
- **clean: probe_uefi_framebuffer** — `jarbas/src/display/fb.rs` — só chamada no entry 0.11
- **clean: raw_boot_info()** — removido do trait `BootHandoff` em `k_nano/src/boot_handoff.rs`
- **clean: BitmapFrameAllocator::init()** — método morto (só `init_from_usable_ranges` usado)
- **clean: ramdisk bootloader path** — código que destrinchava `bootloader_api::Optional` (nunca dispara no Limine)
- **clean: LEGACY/build-tools/mk_uefi/** + **build_usb_bios.py** — builders 0.11 deletados
- **clean: [patch.crates-io] bootloader** — patch morto removido do workspace Cargo.toml
- **cargo check --release**: 0 erros

### SESSION_231: HW Expert v4 + ADR-0082 — HardwareInfo Registry (2026-07-30)
- **ADR-0082** — criada e implementada: HardwareInfo Registry, 489 linhas, Anexo A pesquisa de mercado
- **feat: HardwareInfo struct** — `platform_probe.rs`: registro público de HW unificado. `hw_info()` accessor. `avx2_ready()`, `avx512_ready()`.
- **feat: HW Expert v4 multi-head** — 5 heads (family, fw, agent, caps, next_action). 59.905 amostras de treino. 260KB. v5 .bitnet format.
- **feat: Rust v5 loader** — `cortex.rs`: `HwExpertV4Model`, `load_hwexpert_v5()`, `predict_hw_v4()`, `hwexpert_v4_predict()` API pública.
- **feat: build_card() integrado com ML** — `hw_capability.rs`: tenta HW Expert v4 → tabela → heurística.
- **feat: Boot loading HWEXPRT4.BIN** — QEMU loader scan + FAT32 fallback.
- **feat: SGDB /hw/pci/** — `predict_all_pci()` escreve predições do HW Expert v4 no SGDB por device PCI.
- **fix: xsave gate AVX2** — WHPX filtra CPUID xsave. `allow_avx2` agora só depende de `isa.avx2 && isa.avx && !tcg`.
- **fix: find_child_byte16_sse runtime dispatch** — ART: `art_ok=false` com `art_len==n_art` por SSE2 mal compilado em soft-float. Agora usa `#[target_feature(enable = "sse2")]` + runtime check.
- **feat: Windows DriverStore extractor** — `tools/extract_wdm_hwids.py`: 478 HWIDs extraídos.
- **feat: Q-jump per-step logging** — `mod.rs`: cada passo Q1-Q7 loga PASS/FAIL individualmente.
- **feat: ART benchmark monitorado** — `bench.rs`: `art_len=` no output mostra quantas chaves realmente inseridas.
- **tools**: `train_hw_expert_v4.py` (multi-head training), `unify_hwids_v4.py` (59.905 amostras), `extract_wdm_hwids.py`
- **models**: `hw_expert_v4.bitnet` (260KB), `dataset.json` (59.905 amostras), `vocab.json`
- **docs**: ADR-0082 completa com anexo de mercado, mapa fornecedores/consumidores, ring isolation.
- **cargo check --release**: 0 erros

### SESSION_230: Boot acelerado — skip Ed25519 + VFS I/O para seed agents (2026-07-30)
- **perf: seed_agent()** — pula `sign_artifact_md()` (Ed25519, ~50-100ms/agent) e
  `read_vfs`+`write_vfs` (NeuralFS I/O) quando `tier == "native"`. Seeds são
  trusted-by-compilation, não precisam de assinatura runtime nem persistência VFS
  (já estão no binário). Economia: ~8.5s de boot (T+810→T+9386 → T+810→~T+900).
- **ponytail comment** — marcado com `// ponytail: ...` no código.
- O fix está em `crates/hermes/src/package_hub.rs` `seed_agent()`.

### SESSION_229: Turing Test — JARBAS Plenitude + LLM 8 slots + BEI (2026-07-30)
- **feat: JARBAS Rung 4** — Ring3 (TRY_ENTER_RING3=true), TTF Latin-1 (à á â ã é ê í ó ô õ ú ç), alpha blending real
- **feat: Sprint 80** — fail-closed safety (ConsentGate deny por padrão), emotion classifier 16-feature
- **feat: Streaming TTS** — primeiro áudio em ~50ms via StreamingTtsState, PLAYBACK_RING streaming
- **feat: HW→Persona** — 4 perfis (StandardUma→Tool, AsymmetricCcd→Coach, IntelHybrid→Tutor, MultiDomainNuma→Auto)
- **feat: AutoSkillGen→AppFactory** — gera WASM real no 3º matching
- **feat: Matrix learning #311f** — OnDemandLearning + MatrixLearningAgent (454 LOC)
- **feat: 8 modelos no ModelHub** — BITNET2B, VISION, LLAMA8B, RERANKER, RUSTCDR3, HWEXPRT, LEARNER, AGENT
- **feat: dispatch_expert** — RUSTCODER_MODEL + HWEXPERT_MODEL + Agent slot roteados para modelos dedicados
- **feat: MoE router neural** — load_router() no boot substitui keyword matching
- **feat: Fine-tuning #312b** — FineTuningPipeline (DataCollector→TrainingAgent→BitNetTrainer)
- **feat: Self-Learning OS #313** — SelfLearningAgent (PollEvery 5000, DataCollector→Hub)
- **feat: SleepCycle #314a-f** — 6/6 itens (EWC, ring buffer 1000, Dream sintético, Pruning, Confidence, Ciclo)
- **feat: Structured Decoding #412** — OutputGrammar, mask_logits, generate_structured, 10 self-tests
- **feat: BudgetManager→scheduler** — watchdog Normal→Warning→Paused→Crashed
- **fix: Model loading honesty** — ModelStatus, NO_MODEL_MSG, CortexAgent não cria toy model no boot
- **fix: wasmi unwrap→Trap** — 6 call sites convertidos de panic para Trap seguro
- **refactor: LEGACY migrations** — hardware/, adaptation/, p2p/, brain_mesh, core_pair, budget, hooks, wasm* (7.845 LOC)
- **restore: LEGACY gems** — hardware topology, adaptation engine, MPMC queue, budget watchdog, dedup, HAL trait (3.500 LOC)
- **docs: ADR INDEX** — ADR-0057 completa, ADR-0075 completa(parcial)

### SESSION_228: Hardware Boot + SysInfo Debug Card + Mouse Fix (2026-07-28)
- **hardware boot**: pendrive unified (GPT/ESP) bootou Limine UEFI até Jarbas em notebook real
- **fix(compositor)**: `render_app_content` agora renderiza `WindowContent::Card` via
  `card::render_card()`. Cards existiam como código mas nunca apareciam na tela.
- **feat: SysInfoAgent** — agente `PollEvery(50)` que coleta CPU/cores, memória/RAM/heap,
  agentes totais, uptime, rede e storage de fontes lock-free e exibe como card Jarbas.
  Card ID=9001, atualizado in-place a cada ~2.7s.
- **feat: status bar** — linha de status agora mostra mais info (implícito no SysInfoAgent)
- **fix(mouse): `ps2_check_exists()`** — detecta controlador 8042 antes de init PS/2
- **fix(boot): `mk_esp_fat.py` GPT** — migrado de MBR-only para GPT completo
- **Hardware boot em notebook real**: pendrive unified (GPT/ESP + dados FAT32) bootou
  Limine UEFI até interface Jarbas. Primeiro boot HW real da história do projeto.
- **fix(mouse): `ps2_check_exists()`** — detecta controlador 8042 antes de init PS/2.
  Em notebook moderno sem 8042, `enable_ps2_mouse()` fazia 100K-loop timeouts em
  cada operação de porta 0x64/0x60, tornando o sistema lentíssimo e sem mouse.
  Agora: self-test 0xAA→0x55 com timeout curto 5K loops. Fallback para USB HID.
- **fix(boot): `mk_esp_fat.py` GPT** — migrado de MBR-only para GPT completo
  (protective MBR 0xEE + EFI PART header + partition entries + backup GPT).
  Limine bootloader exige GPT/ESP padrão UEFI. `build_usb_unified.py` depende
  de GPT no `uefi.img` para criar pendrive bootável unificado (`usb_hw.img`).
- **Decks (diário de bordo)**: identificados challenges do bare-metal real:
  sem 8042, sem ATA PIO (USB boot), trackpad I2C-HID sem driver, xHCI depende
  de enumeração bem-sucedida. SMP com retry 3×250ms adequado.

### SESSION_227: ADR-0079 Neural AutoInstaller — M0 a M4 (2026-07-27)
- **ADR-0079 Neural AutoInstaller**: Instalador inteligente pendrive→HD/SSD/NVMe com IA.
  Detecta HW alvo, copia só o que a máquina precisa, cria GPT dual (ESP+NeuralFS).
  Nenhum projeto AIOS no_std pesquisado (ClaudioOS, FYY, Wetware, WeftOS, Oreulius,
  WAeasi, coconutOS, ArceOS) tem self-installer — território inédito.
- **M0 — SysInstaller reativado**: `pub mod sys_installer` linkado na lib.rs.
  `install(target, kernel_elf)` aceita `&mut dyn BlockDevice` genérico em vez de ATA hardcoded.
  Cria GPT via `gpt_format_single()`, copia kernel.elf setor a setor, verifica MBR+GPT.
  demo() com MemoryDisk testa o ciclo completo.
- **M0.1 — gpt_format_multi()**: Cria GPT com N partições (128 entradas, header+backup, CRC32C).
  `GptPartitionDef` struct para definir type_guid, start, end, label.
- **M1 — Dual partition + ESP copy**: `install(source, target, kernel_elf)` — cria GPT com
  ESP (FAT32, 512MB) + NeuralFS (restante). Copia ESP setor a setor do source (pendrive).
  Formata NeuralFS, copia kernel.elf via `NeuralVolume::create_file()+write_file()`.
  Verifica MBR + GPT header + FAT32 BPB.
- **M2 — AutoInstallerAgent + HwProfiler + Jarbas card**: `HwProfile` com PCI scan + RAM detect +
  GPU/NIC/WiFi flags. `AutoInstallerAgent` EventDriven que orquestra instalação completa,
  copia catálogo de skills `/skills/CATALOG.MD` e perfil HW `/config/hw_profile.txt` para o target.
  `install_progress_card()` no Jarbas com gauge + step + botão Reboot.
  Hermes shell comando `install` exibe perfil HW.
- **M3 — AI-Native installation**: `Cortex::install_adviser` gera recomendação via ModelHub slots
  (GeneratorPro→Active→fallback). `self_check` salva/verifica CRC32C dos arquivos instalados.
  `rollback` com 3 tentativas + fallback pendrive.
- **M4 — HW Swap + Recovery**: `hw_change` detecta troca de GPU/NIC/WiFi comparando perfil salvo.
  `self_heal_disk` escaneia StorageBus, escolhe maior disco alternativo, propõe migração.
  `net_fallback` busca firmware ausente via NetFs→GitHub→HuggingFace aios-k2chj.
- **detect_ram_mb() real**: `TOTAL_RAM_MB` atomic populado pelo frame allocator no boot.
  Substitui hardcoded 512MB.
- **format_fat32_esp()**: Cria FAT32 válido do zero (BPB, FSInfo, FATs, root dir).
  Partição ≥ 65525 clusters (~32MB mínimo). Sem depender de source ESP.
- **Ajuste hub multi-LLM**: InstallAdviser roteia via `model_hub::generate_from_slot()`.
  `N_SLOTS=8` para comportar `ModelSlot::Agent`.
- **Lições:** `scan_pci()` é unsafe (precisa `unsafe {}`). `PciDevice` tem `bar0..bar5` individuais,
  não `bars[]`. `NeuralVolume::write_file()` precisa de `dev + ino + data`. `list_skills()` retorna
  `Vec<(String, ToolPolicy)>`, não String. `StorageBus.entries()` devolve `&[StorageEntry]` com
  campos nomeados, não tuplas. Sempre verificar `N_SLOTS` ao adicionar `ModelSlot`.
- **cargo check --release: 0 erros**.

### SESSION_226: Onyx ChatWindow + StreamPacket Protocol + Render Registry + COSMIC UI (2026-07-27)
- **StreamPacket protocol** (`hermes/src/stream_packet.rs`): 14 typed packet types (ReasoningStart/Delta/Done, ToolStart/Delta/Done, MessageStart/Delta, Stop, etc.) com encode/decode compacto para EventBus.
- **ChatSession tree** (`hermes/src/chat_tree.rs`): Árvore de conversa com branching (parent/children), ChatNode, display_nodes().
- **ChatWindow** (`jarbas/src/display/chat_window.rs`): UI Onyx-style com timeline de tools expansível/colapsável, mensagens streaming, histórico, input bar, botão mic toggle `[MIC]`/`[REC]`.
- **Áudio integrado**: `MIC_ACTIVE` flag → VoiceAgent escuta sem wake word → STT transcreve → texto no input buffer. TTS automático na resposta.
- **FocusMode** (`compositor.rs`): Chat (clique no chat → teclado vai pro input) vs Ambient (fundo → wake-word "Jarvis").
- **COSMIC visual refinements**: `decorations.rs` com `draw_rounded_rect(r=4)`, gaps entre tiles (4px), painel Hermes translúcido (bg_alt/2 + r=8), barra de status estilo COSMIC.
- **Render Registry** (`jarbas/src/display/render_registry.rs`): `RENDER_REGISTER` / `RENDER_WINDOW` topics. Agentes registram `RenderFn` e publicam janelas dinâmicas sem modificar compositor.
- **Cleanup**: NeuralConsole removido (~287 LOC), F-keys legados, Settings/Power/Ide/Camera/AudioViz AppIds descartados. render_app_content só trata HermesChat.
- **cargo check --release: 0 erros**.

### SESSION_225: Limine Migration + Higher-Half Fixes + Desktop Jarbas na Tela + Soft Power Off (2026-07-27)
- **Limine boot (uefi.img):** Migração bootloader 0.11 → Limine 6.x. Kernel higher-half 0xffffffff80000000+. Framebuffer @0xffff8000c0000000 via HHDM.
- **PHYS_MEM_OFFSET early store:** main.rs:1268 — setado ANTES de qualquer driver. e1000/HDA/NetAgent enxergam offset correto em vez de 0.
- **P6 raw_vec capacity overflow fix:** TRY_ENTER_RING3=false. Subtração entre VA higher-half e user-space (0x7000..) estourava isize::MAX → Vec::with_capacity overflow.
- **e1000 RX #PF loop fix:** ponytail guard `if pmoff == 0 { return None }` em recv()/any_rx_dd(). Buffer overflow corrompe o static PHYS_MEM_OFFSET.
- **BPE scan bound fix:** cortex/src/bpe.rs:485 — 0x200000000→0x180000000 (RAM 6GB, não 8GB).
- **Desktop Jarbas na tela do QEMU:** Compositor, 3 apps (HermesChat + Settings + Power), 55 agentes, scheduler rodando.
- **Soft power off OK:** Botão Power → confirmação → ACPI PM1a_CNT (0xb004) → shutdown.
- **Cleanup:** build_esp.ps1 removido (build.rs gera ESP). limine.cfg removido. .gitignore em tools/limine/esp/.
- **WHPX:** "Ignoring request for interrupt vector 0" — pendente investigação ACIP/IDT.

### SESSION_224: ADR-0076 Implementação Pesada — 23 entregas (2026-07-27)
- **Skill Manifest FYY canônico (Onda 1):** struct expandida com RemoteConfig, Pricing, QualityIndicators, Interop (a2a/clawhub/skillnet), parser from_slice/from_json_str, 12 testes. 25 manifests agentes nativos A-001 a A-025.
- **WASM Runtime expansion (Onda 2):** host functions 1→6 (aios, aios_net, aios_fs), 11 cap constants + check_cap() com cap check. WASI Preview 1 stubs conectados ao linker. WAT test suite com 18 testes. Telemetry ring lock-free SPSC 4096 slots + shell trace cmd.
- **Segurança (Onda 3):** Membrane two-layer gate (bitmask + Membrane::check). Permission Gate com HITL (RiskLevel, Approve/Deny spin-wait). Quarantine Gate sanitization (pattern/length/repetition/structural + 8 testes). WIT-typed ABI (aios.wit).
- **Live capsule lifecycle (Onda 3.6):** PKG_CHANGED events no EventBus para upgrade sem reboot.
- **Cascading capability revoke (Onda 4.2):** CapRegistry com create/delegate/revoke em cascata.
- **Goal-aware scheduler (Onda 4.3):** goal_urgency + novelty_score + coherence_partner. Sort por goal_urgency×2 + novelty_score. Rate-limiting exclui agentes com urgência >0. Novelty decay 1/tick.
- **Intent Bus canônico (Onda 4.4):** Intent enum com 33 variantes, 10 categorias, describe().
- **Glass Box inspect (Onda 4.5):** inspect command mostra estado vivo dos 25 agentes.
- **Syscalls consolidados 13→9 (Onda 4.4.1-3):** removeu SEND_TCP + VRING_SETUP, unificou WRITE_RING+READ_RING→RING_OP. 6 arquivos atualizados.
- **GEMM benchmark golden checksum (Onda 8.1):** ternário 64×64 FNV-1a (Folkering pattern).
- **SYS_MAP_FB real:** page table walk no syscall dispatch, mapeia BAR físicas no AS atual.
- **Proof-gated mutations (Item 3):** ruvix-proof crate integrado (ProofGate, 3-tier proof, 6 testes).
- **Kernel HNSW (Item 4):** ruvix-vecgraph crate integrado (KernelHnsw, HNSW slab-allocated, patches no_std).
- **Ring-3 Userspace (Item 1):** ELF loader (elf_loader.rs), ProcessManager (process.rs), SYS_DEMAND_PAGE real, TRY_ENTER_RING3=true, shell `run` cmd.
- **Lições:** Fixers paralelos sobrescrevem lib.rs — verificar módulos após cada fork. ruvix-vecgraph precisa de patches no_std (f32::sqrt). Merge conflicts em rust-toolchain.toml e wasm_build.rs. `AgentTickResult::Continue` não existe — usar Pending.

### SESSION_223: Cross-OS Ecosystem + BEI + P01 Drift + TLS + ADR-0040 (2026-07-26)
- **ADR-0076 Cross-OS Ecosystem (7 fases):** Skill Manifest (skill_manifest.rs: RiskLevel, SkillType, Permissions, SkillManifest, validate, to_json, office_spreadsheet factory). Membrane + CapGate (membrane.rs: Membrane struct, Operation, Capability, Verdict Allow/Deny/Escalate, for_legacy/for_wasm, glob FS, net allowlist, demo self-test). JAIL sandbox (jail.rs: Jail struct, Membrane::check, Merkle audit trail, check_file_read/write/net/capability, report, demo). WASI Preview 2 (wasi_host.rs: 15 wasi_snapshot_preview1 stubs — fd_write, fd_close, fd_seek, fd_prestat_get, environ_*, args_*, proc_exit, random_get, clock_time_get, path_open). MCP bridge (mcp_client.rs: search_marketplace/search_fyy/search_weftos → mcp_server.rs: SearchFyy/WeftOS/Skills MCP methods + search_skills/search_fyy_skills/search_weftos_skills). Ciclo aprendizado (cross_os/agent.rs: LearningState Learn→Propose→Auto, WorkflowLearner pattern_registered integration).
- **ADR-0060 BEI (7 ondas completas):** Onda 3 Dynamic MoE (cortex::moe: try_birth/try_merge/try_split + stale_indices/high_entropy_indices + self_test). Onda 4 Memory L0-L7 (hermes::memory: MemoryLevel 8 tiers, MemoryTier trait, InMemoryTier, MemoryStore read/write/promote/tick_advance). Onda 7 Soul Mirror (jarbas::display: SoulMirrorState/SoulMirrorRenderer, Avatar8State 8 estados Idle/Listening/Processing/Speaking/Thinking/Dreaming/Alert/Updating). Lifecycle INDEX.md atualizado Accepted/completa.
- **ADR-0040 Residuals:** SysInstaller #421 (k_nano::sys_installer: scan_disks, install ATA copy, verify, EventBus SYS_INSTALL publish). Storage UI #419 (jarbas::cards::storage_card: gauge, disk list, format button via UiDeclaration/UiRenderer).
- **TLS:** embedded-tls integration (hermes::tls: TlsStatus, https_get/https_get_fallback bridges, feature-gated `tls`). Cargo.toml feature + kernel bridge registration.
- **P01 Type-drift fix:** NIC globals unificados (k_nano::nic_globals → pub use no bin). BSP_PCPU unificado (BspPcpu wrapper). wasmi Error type fix (wasmi::core::Trap → wasmi::Error).
- **P08 SELF_HEAL/TRUST_CACHE:** Movidos para k_ai como singletons. Bin agora pub use.
- **Drift massivo (14 módulos):** boot_logger (FAT persistence bin→k_nano), serial (pub use), allocator (LazyBumpAllocator bin→k_nano), vfs (append/exists/mkdir bin→k_nano), smp/trampoline (pub use), smp/work_stealing (pub use), fs/ata_agent (pub use), disk_power (pub use), usb_trust (pub use), NeuralFS (9 arquivos idênticos deletados do bin), hnsw/multi_user (cópias em k_nano + cortex/k_ai).
- **RTC driver:** k_nano::rtc (CMOS MC146818: cmos_read, bcd_to_bin, read_rtc, RtcDateTime, format_rtc, demo).
- **BGE alignment:** static mut BGE_WEIGHTS→spin::Mutex, BGE_VOCAB/HIDDEN→AtomicUsize, f32 alignment chunks_exact(4)+from_le_bytes safe copy.
- **HwRegistry detect_all:** PCI class_name() helper + slog_kai! log por device no boot.
- **restore_checkpoint:** save_count field, best-effort doc table, v3 serialization format. Boot log FAT32 validation log.
- **Ring 1 ownership:** safety/security/optimizer/SleepCycle/AutoLearn documentados como permanência em hermes.
- **Toys no-op:** CandleSidecar/TaskSpawner/ReActLoop ponytail comments (k_ai::cognitive).
- **Trust validation:** check_or_cache wired em 3 execute_skill diretos + audit 17/17 paths.
- **debug_rl! deprecation** em favor de slog_bin!.
- **TECNOLOGIAS.md:** 4 novas entradas (RTC, SysInstaller, BEI, Cross-OS).
- **59+ arquivos modificados, 8 deletados, 5 novos.** cargo check --release = 0 erros.

### Power Management completo — P-state, C-state, S3 Suspend/Resume (2026-07-26) — SESSION_222
- **cpufreq.rs (novo):** MSR IA32_PERF_CTL (0x199) + IA32_PERF_STATUS (0x198) + IA32_ENERGY_PERF_BIAS (0x1B0). Governor Performance/Powersave/Ondemand. CPUID leaf 0x16 + probe MSR write-take-effect. APERF/MPERF actual_ratio() via MSR 0xE8/0xE7 para frequência real.
- **MWAIT real:** AP idle loop usa `monitor`/`mwait` quando CPU suporta (CPUID.1:ECX[3]), fallback `hlt`. MONITOR_FLAG (AtomicU8, cache-line aligned) escrito no enqueue() para wake sem IPI. `set_mwait_hint(cstate)` para C1–C6.
- **S3 suspend:** ACPI _S3 DSDT parser + FACS waking vector. `suspend()` salva CR3/RSP, set FACS wake vector → trampoline 64-bit em 0x7000, park APs, set powersave, write SLP_TYP=3+SLP_EN via PM1a_CNT.
- **S3 resume trampoline:** Blob de 64 bytes na posição física 0x7000: restaura CR3 + RSP, jump para `s3_resume_entry()`. Handler re-inicializa APIC, PIT, EPB. Save/restore e1000 (16 regs + MTA 128 entradas).
- **Scheduler integration:** Ondemand tick no closure `halt` do `registry.run()` — chama `cpufreq::ondemand_tick(ap_work::has_pending())`.
- **10 arquivos modificados:** cpufreq.rs (novo), suspend_resume.rs (novo), acpi.rs (+_S3 parse +FACS), ap_work.rs (+MWAIT), platform_probe.rs (+mwait), apic.rs (+send_ipi_reschedule_to), e1000.rs (regs pub), core_pair.rs (send_wake_ipi real), lib.rs (+2 mod), hardware/probe.rs (+cpufreq init). 0 erros.
- Refs: wasmi, AAGT, GBNF/Outlines/XGrammar, arXiv SelfEvolve/ARISE/Tool-Making/MCP-SandboxScan

### Generative Card Desktop (UI/Desktop Jarbas) — ADR-0058 (2026-07-21) — S1–S4 ✅
- Planejamento unificado do UI/desktop: fundação **embedded-graphics** (`DrawTarget` sobre `DoubleBuffer`) + toolkit no_std (matrix-gui/embedded-gui/kolibri, MIT/Apache) + camada declarativa **`UiDeclaration`/`UiRenderer`** (cards)
- Cards gerados como **dados** por Hermes/Trinity/Cortex (constrangidos pelo structured decoding ADR-0057 #412) ou por **skill WASM** (RustCoder/Codex, ADR-0052) + repetição Cron. Ex.: "clima de amanhã" → WeatherCard
- WM stacking mantido (árvore de janelas retida; aposenta enum `AppId` hardcoded)
- **Supersede parcial** ADR-0047-HMI (H1/H2/H4/H5 absorvidos; H3 ❌); ADR-0036 persona inalterada
- **S1–S4 ✅ implementados** (QEMU: 3 cards + orb responsivo + barra de relógios/HUD preservados; self-tests S1/S2 PASS; clique fecha card; `cargo check` 0 erros). S5 (widgets ricos/tema/TTF) + A/V real (mic/alto-falante/vídeo via HDA/UVC) = residual. Cards demo: Sistema, Clima ("clima de amanhã"), Chamada de Vídeo (Atender/Microfone/Alto-falante/Encerrar)

### Compute Dispatch SMP+GPU+NPU — ADR-0057 (2026-07-20)
- **WS-A wake multi-AP:** SIPI direcionado sequencial por LAPIC ID + stack/PerCpu por-AP + retry INIT-SIPI-SIPI 3x. QEMU `-smp 4` → **APs acordados: 3**, `CorePools r0=1 r1=2 r2=1` (antes: máx 1 AP; ≥2 → 0). Contador `AP_ENTRY_COUNTER` unificado; `neural-kernel::smp` emagrecido (delega a `k_nano::smp`)
- **WS-B:** `parallel_ternary_matmul` (particiona colunas; decode `m=1` escala) + `Tensor::matmul` f32 nos APs — **gated por `ap_pollable`** (deadlock-proof: BSP faz o matmul enquanto APs em `hlt`)
- **WS-C:** `cortex::compute` — dispatcher único (`NPU→GPU→CPU-SMP→AVX2→scalar`) nos choke points; backends via fn-pointer
- **WS-D:** `k_hal::gpu::compute_dispatch` registra GPU só se `BackendState::Ready` (canário silício); kernel W2A8 = Layer S/HW
- **WS-E:** `k_hal::npu` — detecção PCI XDNA/Intel + `[NPU-HW] VERDICT=SOFTWARE` honesto + fallback software (Ring0 MLP CPU). Driver/firmware = Layer S/sponsor
- **WS-F:** wake robusto (retry) + `hlt` idle + gate `ap_pollable` + seam `install_wake_fn`/`wake_aps`. On-demand AP-worker (IDT+reschedule-IPI) = residual HW
- **WS-G #412:** `cortex::decode` structured decoding (máscara de tokens antes do argmax); default no-op; self-test de boot **PASS**. Medusa/FlashAttention/PagedAttention/huge-pages/burn-flex/codebook = residual (validação com modelo)

### Rebrand K³CHJ (2026-07-18)
- Nome canônico **K³CHJ** = `k_nano` + `k_hal` + `k_ai` + Cortex + Hermes + Jarbas
- Histórico **K²CHJ** = 5 crates (sem `k_hal` na marca); paths ADR `*k2chj*` inalterados
- Glossário: ADR-0042 §0; INDEX “Nome do produto”

## [1.9.5] — 2026-07-19 — Emagrecer neural-kernel cutover (TEST)

**Versão:** v1.9.5 TEST / NÃO ESTÁVEL — **não** v2.0.0.

### Emagrecer neural-kernel (SESSION_163 / IDEA #467)
- Cutover seguro bin→crates K³CHJ (ondas 0–6): stubs `pub use` + promote truth do bin
- Gate: `tools/diff_bin_crate.py` + `docs/memory/BIN_CRATE_DIFF.md`
- Unificados: `ATA_DRIVER`, `TIMER_TICKS`/`MOUSE_ABS_*`, `global_arena` pending_route
- Promovidos a k_nano: `fat32`/`ata`/`e1000` (probe exFAT, prove_rx)
- Residuals honestos no bin: `cortex.rs`/`bpe`/`agents`/`net*`/`audio/*`/`boot_logger`/`smp`
- `cargo nk` = 0 erros; ~−7k LOC no monólito (sem perda de lógica)

## [1.9.1] — 2026-07-19 — BitNet 850 generate + BPE SP32 + TLS/WiFi/LEGO (TEST)

**Versão:** v1.9.1 TEST / NÃO ESTÁVEL — **não** v2.0.0.

### BitNet ladder 850 (SESSION_162)
- **#PF fix:** AVX2 ternary matmul — desactivar bitwise OOB (`n%4`); cauda scalar `n%8` (`bitnet_avx2.rs`)
- **Loader:** size = blob chat FAT; hub skip PIO se Active QEMU-loader; copy+`Box::leak`
- **Layout v4:** `has_basic_rms=true` (evita rms=0)
- **BPE SP32:** `export_bpe_bin.py --sp32` → BPB1+**MRG1** (61249 merges); load **antes** LLM-TEST
- **LLM-TEST:** `ola` → encode HF `[1,288,433]`; resposta BPE (coerência semântica residual)
- **Harness:** `tools/llm_ladder_bench.py`; FAT default **3072 MB**; `PACK_LLM=850|13|2b|3b|all`

### TLS / WiFi / Device LEGO (SESSION_154–161)
- TLS #123: embedded-tls soft-float; https_get; smoke google; PKI pins+TOFU
- WiFi ath10k QCA6174 A0–A3 BMI→fw_ready (Note AWAITING)
- ADR-0056 DeviceRecipe / UnlockDAG H1 + specs `docs/specs/device-lego/`

## [1.9.0] — 2026-07-18 — Pós-LAN B-01 + Residuals 0–7 (TEST)

**Versão:** v1.9.0 TEST / NÃO ESTÁVEL — base v1.8.6; **não** v2.0.0.

### Plano Residuals 0–7 FECHADO (SESSION_142–151)
- **PreFlight:** `tools/preflight_wave.py` + cache `.preflight_cache/` + `pass_marker` anti-contaminação + anti-fake Ready
- **Ondas 0–6:** docs/IDEA; NeuralFS smokes; exFAT write `#417`; USB Trust/UAC-HW; GPU/MHI AWAITING; AirLLM ATA + AIRLLM-DMA; soft-float defer (Trilha R)
- **Onda 7 LAN:** e1000 TX canônico `0x3800/0x3818` (aliases QEMU no-op); L3.5 ARP/RX; DNS raw + HTTP 301 smoke WHPX
- **Tags:** `depends_on: lan` liberado; WiFi AWAITING; TLS BLOCKED; #418 peer PASS
- **Política:** `▶️ AWAITING_HW` — sem fake Ready

### Pós-LAN B-01 unlock (SESSION_152)
- **net_bridge** Hermes→bin NETSTACK; `resolve_and_http_get` + Host header; HTTPS deny até TLS
- **Agents:** `/fetch`, Browser, Search, RSS, Market, AutoLearn sem stub B-01; Email SMTP residual honesto
- **AirLLM Net:** DNS+hostname + Range/stream; `tools/serve_tiny_gguf.py`
- **#418 NetFs:** TCP `gateway:4446` + `tools/netfs_peer.py` + `[NETFS] VERDICT=PASS`
- **#308 SelfUpdate:** `fetch_update` HTTP + FNV + slot A/B
- **#123 TLS:** `[TLS] VERDICT=BLOCKED reason=softfloat_or_crate` (sem fake HTTPS)
- **Fix:** deadlock NETSTACK — NetFs smoke fora de `NETSTACK.lock()` pós-L5
- **Hygiene:** TODO BLOQUEADORES / STATE / IDEA / INDEX alinhados SESSION_152

## [1.8.6] — 2026-07-18 — ADR-0041 H4+/H5+/AS + HalOffer Cap (TEST)

### ADR-0041 restante (SESSION_140)
- **H4+ QUEUE_NOTIFY:** `k_hal::virtio` map UC VirtIO-PCI + `try_pci_queue_notify` → `NotifySent` / `NotifySkipped` honesto
- **Residual MMIO:** hermes/jarbas FE (HalOffer); VGACNTRL → `k_hal::gpu::backend::disable_intel_vga_plane`; `virtio_gpu` / `link_watcher` sem BAR
- **H5+ Cap:** `grant_fe` no `offer::bind`; ports `fe_*` + `check_fe_bound`; demo R3 Deny / Bound Allow
- **AS shallow:** `address_space::demo_as_r1_r3_shallow` (CR3 + touch BAR + restore; ≠ isolamento produção)
- **HalOffer:** API R3 genérica; VirtIO = transporte BE; slog canônico `[T+n] [Rn] [k-xxx]`
- **Versão:** v1.8.6 TEST — **não** v2.0.0; ADR lifecycle `fazendo`

### HW USB boot diagnostics (SESSION_139)
- **Console FB legível:** `console_clear` / `console_print` em `jarbas/display/fb.rs`; `boot_ckpt`/`boot_splash` e `vga_buffer::fb_print` usam o mesmo cursor (limpa faixa — sem TRACE/ghost).
- **BOOT.LOG:** `fat-boot-log` no artifact boot; overwrite 8.3 + `heap_ready`; `init_after_usb` + ckpts K0–K17.
- **USB Windows mount:** MBR dados FAT32 `0x0C` + ESP `0xEF`; `mkfat32` BPB/seed; `inspect_usb_layout.py`.
- **Bootloader BltOnly:** `[patch.crates-io]` → `vendor/bootloader` (SetMode Rgb/Bgr; sem panic em HD 620).

### ADR-0048 / 0049 / 0050 — GPU Multivendor Unlock (SESSION_138)
- Fundação: `compute_abi`, detect `has_compute=false` até canário, `display_coex` dirige `init_backend_with_plan`
- KernelPack NKP1 (`kernel_pack.rs`) + packers host NVIDIA/AMD/Intel + `tools/gpu_kernels` (CPU golden)
- Bring-up stubs: LegacyAcr vs Gsp; Gen9 vs Arc; AMD KiQ/Mes; canário FailDispatch → CPU (display intacto)
- Hardening: ACR só Pascal + BAR2+pmoff; AMD doorbell noop; gate ADR-0047 só `Ready`
- **Não alegado:** QMD/walker/PM4 golden em silicon; NKP ainda unsigned (sig zeros)

### ADR-0051 / Agency data-driven (SESSION_134)
- **255 AGENT.md** em `ecosystem/agents/` (214 Agency + 41 nativos) via `tools/export_agent_packages.py`
- Seed embutido `k_ai::{agency_seed,native_agent_seed}`; `Agency::from_specs` + registro via PackageHub
- Kinds `Agent` + `Workflow`; alias legado `/agents/*.wasm`
- VFS bridge Hermes → `neural-kernel::fs`; `NeuralFsAgent` cria árvore `ecosystem/`
- Disco exFAT **não** recebe nested ecosystem (residual honesto)

### NeuralFS / storage (SESSION_133)
- USB format lock (opt-in: `NEURALFS_USB_FORMAT=1` / `debug_assertions` / `allow_usb_format`)
- GPT dedicada NeuralFS (`GPT_TYPE_NEURALFS` + virgin `gpt_format_single`)
- `build_usb_unified.py`: partição de dados exFAT default (`--fat32` legado)
- `mkexfat.py`: boot Microsoft checksum + backup; bitmap/upcase; root 0x81/0x82/0x83

### NeuralFS / storage (SESSION_132)
- B-tree multi-nivel (split interno + path CoW); USB-MSC format/mount `0x7F` para pendrive de teste
- Volume de dados de boot: `mkexfat.py` default; `read_file_from_dev` prefere exFAT (fallback FAT32)
- Fix exFAT `VolumeLength` @ offset 72 (spec Microsoft)

### Fixed
- **hermes `wasm_rt::SkillMarket::top`:** replace `partial_cmp(...).unwrap()` ranking with `f32::total_cmp` (total order, NaN-safe), aligned with `skill_market::SkillMarket::top_skills`. Truth path is hermes only (monolith `wasm_rt` mirror removed at N4.6); LEGACY snapshot untouched.
- **Release build warning cleanup:** remove three unused imports, two unreachable match arms and the informational `cargo:warning` emitted by `boot/build.rs`; clean `cargo check --release` now completes with 0 errors and 0 warnings.
- **Framebuffer bpp dinâmico:** `GpuDevice::from_probe` / `resolve_bytes_per_pixel` usam `info.bytes_per_pixel` do GOP como fonte única; `DoubleBuffer::from_gpu` e consumidores (DisplayAgent, splash, console, avatar, P4) atuam sobre o valor coletado — sem hardcode Bgr→3.
- **HW PnP / HwCapabilityCard:** remove free-text `generate_via_hwexpert("identifique…")` (lixo `OA5US…`); card tipado `k_ai::hw_capability` (family/fw/agent/caps/next_action) publicado em `HW_CAPABILITY` + `HW_PNP_ACTION`; hooks honestos (NEED_FW→HEALTH_ISSUE, wifi→NET_IFACE_AVAILABLE). Seed treino `tools/train_hw_expert_v4.py` no mesmo schema.
- **Hermes agentico PnP:** `hermes::hw_pnp` — card → `observe_intent` + skill efêmera (SkillOpt) → com ≥3 usos/70% promove WASM via `evolve::promote_ephemeral_to_wasm`; Cortex só em `bind_wifi_scan`/`bind_gpu_compute` (hint ≠ ordem). Detect deixa de dump free-text da árvore em `LLM_REQUEST`.
- **ADR-0051 Package Hub:** namespace NeuralFS §12 `/mnt/neural/ecosystem/{skills,agents,plugins,mcp,models,firmware}`; `package_hub` CRUD+HITL+assinatura embutida; `/pkg *`; catalog no system prompt Cortex; seed `skills/*/SKILL.md`.

## [1.8.5] — 2026-07-16 — Consolidação pós-v1.8.0 (teste / não estável)

> Canal de integração e testes. Os MVPs abaixo não constituem validação
> production-grade nem liberam o gate de `v2.0.0`.

### Agentes e voz
- **Sprint 108:** Self-Evolve `observe→generate→verify→improve→reflect`, verificação de skills, SIL e reflexão no SleepCycle
- **Sprint Sound:** pipeline Mic→Wake→STT→LLM→TTS, STT PCM→MFCC, UAC descriptor parse, VAD/SER e Piper neural-lite
- Residuais honestos: soft-float/VITS, CTC WER, UAC isócrono e cutover `jarbas::audio`

### Filesystems e modelos
- **NeuralFS:** I/O RAM, B-tree leaf com reclaim/split, ATA MBR opcional e agente `/mnt/neural`
- **ADR-0040:** exFAT read-MVP + MHI soft-migrate; writes e DMA físico permanecem `por_fazer`
- **ADR-0046:** AirLLM GGUF layer-wise, prefetch soft e hot-swap ATA/Net→FAT→`set_model`
- **Cortex:** N-gram speculative decoding com benchmark empírico e rollback de KV

### Latent/GPU/HMI
- **ADR-0047:** LatentBus, Evolve hot-swap/Genesis, NeuOS Probe, GPU work queue/SASOS/H2O/G5 e HMI embedding/splats
- **ADRs 0048–0050:** propostas multigeração NVIDIA/AMD/Intel registradas como `por_fazer`

### Estado
- Versão **v1.8.5 de teste, não estável**
- `v2.0.0` continua bloqueada por review formal, demandas `por_fazer` e aprovação explícita do maintainer
- Memória consolidada em `docs/memory/SESSION_121.md`–`SESSION_129.md`

## [1.8.0] — 2026-07-16 — Marco K³CHJ: ADR-0042 adequação + wire crates completo

### Marco
- **ADR-0042 N1–N5 ✅** — cadeia funcional `k-nano → k-ai → cortex → hermes → jarbas` verificada em QEMU
- **Wire crates N2.5→N5.7 ✅** — monólito boot linka os 5 crates K³CHJ (commits `8740bfd`…`95f8967`)
- **Sprint 107 Voice ✅** — PASS parcial forte+ (`'O tempo esta'` + Piper neural-lite + EventBus skinny)
- **Pista ativa pós-1.8.0:** Sprint Sound (voz production-grade) + review gate `v2.0.0` (não declarar automaticamente)

### Wire summary (N2.5→N5.7)
| Fase | Versão | Crate | Espelhos removidos |
|------|--------|-------|-------------------|
| N2.5 | v1.7.8 | `k_ai` + `k_nano` | `trust.rs`, `self_heal.rs` |
| N3.5 | v1.7.9 | `cortex` | 9 (tensor, trinity, arena, r3, …) |
| N4.6 | v1.7.10 | `hermes` | ~37 (cron, wasm*, wifi*, apps/, …) |
| N5.7 | v1.7.11 | `jarbas` | 29 (display/*, gpu/*, jarvis, …) |

### Padrão de migração (lição)
- Alias dep `*-crate` evita conflito com `mod` re-exportado
- `k_nano` sem feature `global-alloc` → único `#[global_allocator]` no bin
- Bridge `memory` + `EVENT_BUS` → `k_nano::globals`
- Residual monólito = integração bin-only (`cortex.rs`, `audio/*`, `agents.rs`, `net*`, `fs/*`, `jarbas_fb.rs`)

### Residual (não bloqueia 1.8.0)
- `audio/*` — ADR-0045 truth path (Sprint Sound)
- `cortex.rs` / `bpe.rs` — generate path + weather-e2e loader
- `agents.rs` / `net*` / `fs/*` — fleet + NETSTACK singleton
- Qualidade voz: STT CTC fraco, Mic→Wake runtime, Piper VITS pleno, soft-float latency

### HW real
- `target/usb_hw.img` unificado (ESP + FAT dados; Rufus DD)
- BITNET2B + PIPER + HWEXPRT + RUSTCDR + BGE + STT + 116 firmware blobs

### Build
- `cargo clean -p neural-kernel && cargo nk` — **0 erros** (2026-07-16)

### Docs
- STATE v1.8.0, SESSION_120, SESSION_INDEX, TODO, AGENTS, ADR-0042 policy, IDEA_BANK #439–#442

## [1.7.11] — 2026-07-16 — ADR-0042 N5.7 (jarbas wired no bin)

### Wired
- **N5.7** `neural-kernel` → `jarbas-crate` (dep direta `package = "jarbas"`)
- `pub use jarbas_crate::{display, gpu, jarvis, virtio_gpu, uvc_driver, vision_agent}`
- Removidos 29 espelhos monólito (display/*, gpu/*, jarvis, virtio_gpu, uvc_driver, vision_agent)
- Feature `jarbas-bridge` removida — wire always-on (`k_nano` sem `global-alloc` resolve conflito allocator)
- `jarbas_bridge.rs` compara TOPIC_* via `jarbas_crate::audio` (audio truth permanece monólito)
- Gate `[N5-JARBAS] full_wire=OK(jarbas-crate)`
- `paint_tts_response` / `boot_splash` portados para `jarbas/src/display/fb.rs`

### Residual monólito (integração bin)
- `audio/*` — ADR-0045 truth path + Sprint107 wakeword (`voice.rs` diverge do espelho jarbas)
- `jarbas_fb.rs` — CapGate P4 FB demo (bin-only)
- `jarbas_bridge.rs` — cross-check TOPIC_* monólito vs jarbas-crate

### Marco
- **Wire crates N2.5→N5.7 ✅** — cadeia K³CHJ linkada no bin; qualidade voz → Sprint Sound

## [1.7.10] — 2026-07-16 — ADR-0042 N4.6 (hermes wired no bin)

### Wired
- **N4.6** `neural-kernel` → `hermes-crate` (dep direta `package = "hermes"`)
- `pub use hermes_crate::{actor_registry, apps, cron, hermes, safety, security, wasm*, wifi*, …}`
- Removidos 37 espelhos monólito (hermes, cron, wasm_rt, skill_*, wifi_*, apps/, …)
- Alias `hermes-crate` evita conflito com módulos re-exportados
- Gate `[N4-HERMES] full_wire=OK(hermes-crate)`

### Residual monólito (integração bin)
- `agents.rs` — fleet nativo + HermesAgent; globals em `main.rs`
- `cognitive.rs` — engine Sprint 95 (não no crate)
- `net*` + `rtl8139`/`e1000`/`virtio_net` — NETSTACK singleton + virtio init
- `fs/*` — VFS monólito (`inference_fs_agent`, `mhi_scheduler`)
- `aios_api.rs` — CapGate P3
- `micropython_wasm.rs` — loader via `crate::fs`

### Próximo
- Sprint Sound — qualidade voz (STT/Piper/soft-float); Mic→Wake runtime

## [1.7.9] — 2026-07-16 — ADR-0042 N3.5 (cortex wired no bin)

### Wired
- **N3.5** `neural-kernel` → `cortex-crate` (dep direta `package = "cortex"`)
- `pub use cortex_crate::{arena, bitnet_avx2, burn_flex, delta, nn, r3, tensor, trinity, tv_dsl}`
- Removidos 9 espelhos monólito (tensor, trinity, arena, r3, …)
- Alias `cortex-crate` evita conflito com `mod cortex` (integração LLM/EventBus/load_status)
- Trinity crate sync: Sprint 107 generator-first + `moe_router_loaded` / `has_generator`

### Residual monólito (integração bin)
- `cortex.rs` — generate path, EVENT_BUS, demo_flags, allocator resize
- `bpe.rs` — BPB1 + weather-e2e lexicon + FAT/QEMU loader
- `global_arena.rs` — pending route Hermes→Cortex
- `cortex_mmap.rs` — ADR-0041 P5/P7 (não no crate)

### Próximo
- **N4.6** wire `hermes` crate

## [1.7.8] — 2026-07-16 — ADR-0042 N2.5 (k_ai wired no bin)

### Wired
- **N2.5** `neural-kernel` → `k_ai` + `k-nano` (sem feature `global-alloc`)
- Removidos espelhos `trust.rs` / `self_heal.rs`; `pub use k_ai::{trust, self_heal}`
- Bridge `memory` → `k_nano::memory` (GLOBAL_ALLOCATOR único para boot + SelfHeal)
- Bridge `EVENT_BUS` → `k_nano::globals::EVENT_BUS` (HEALTH_ISSUE no mesmo bus)
- `k_nano` feature `global-alloc` (default OFF) gateia `#[global_allocator]` no crate lib

### Próximo
- **N3.5** wire `cortex` — remover espelhos `cortex.rs`, `bpe.rs`, `tensor.rs`, `trinity.rs`, …

## [1.7.7] — 2026-07-16 — ADR-0042 N5 CLOSED (jarbas ego/UI)

### Closed
- **N5.1–N5.6** funcionais: DisplayAgent compositor + GPU FB + P4 jarbas_fb; JarvisAgent persona 16-stage; voice agents (`jarvis_voice`/`wakeword`/`audio_mixer`) via Hermes only; `paint_tts_response` FB; voice e2e (GATED boot default + prior Sprint107 TTS+FB); IPC←hermes topics mirror honesto
- Serial gate `[N5-JARBAS] … criteria=MET` — evidência `logs/boot_n5_20260716_145943.txt`
- **N5.7** link crate `jarbas` no bin = deferred (espelho monólito; padrão N2.5/N3.5/N4.6)
- STT/Piper/soft-float quality → Sprint Sound (não bloqueia N5)

### Marco
- **N1–N5 funcionais ✅** — gate `v2.0.0` pode ser **discutido**; wire crates N2.5–N5.7 e qualidade voz permanecem deferred

### Não é
- Crate `jarbas` wired no bin, voz production-grade, ou declaração automática de `v2.0.0` sem review de qualidade ADR

## [1.7.6] — 2026-07-16 — ADR-0042 N4 CLOSED (hermes orquestra)

### Closed
- **N4.1–N4.5** funcionais: HermesAgent intent routing (`USER_INTENT`/`HERMES_RESPONSE`), ReAct 7 fases + `SKILL_REGISTRY` + WASM SFI hub, `global_arena`→`generate_via_model`, EventBus intent e2e (GATED boot default + prior weather-e2e L5), IPC→jarbas topics mirror honesto
- Serial gate `[N4-HERMES] … criteria=MET` — evidência `logs/boot_n4_20260716_144651.txt`
- **N4.6** link crate `hermes` no bin = deferred (espelho monólito; padrão N2.5/N3.5)
- Voz/STT quality → Sprint Sound (não bloqueia N4)

### Não é
- Crate `hermes` wired no bin, jarbas ego pleno, ou `v2.0.0` (falta N5)

## [1.7.5] — 2026-07-16 — ADR-0042 N3 CLOSED (cortex cérebro)

### Closed
- **N3.1–N3.4** funcionais: BitNet 2B `llm=LOADED`, Cap MAP_WEIGHTS (P5), Trinity 6 experts + HWEXPERT/RustCoder, generate path (GATED soft-float no boot default + prior weather-e2e HIT)
- Serial gate `[N3-CORTEX] … criteria=MET` — evidência `logs/boot_n3_20260716_132753.txt`
- **N3.5** link crate `cortex` no bin = deferred (espelho monólito; padrão N2.5)
- Soft-float fluency / TTS quality → Sprint Sound (não bloqueia N3)

### Não é
- Chat fluente 24/7, float/AVX path pleno, ou `v2.0.0` (falta N4–N5)

## [1.7.4] — 2026-07-16 — ADR-0042 N2 CLOSED (SelfHeal VID-gated + Trust)

Critérios funcionais N2 ✅. Package Cargo permanece `1.0.0` (tag-only). **Não** é v2.0.0.

### N2 (k-ai HW-AI / SelfHeal)
- Boot path: Trust `(token,agent,skill)` → inventário PCI → `run_vid_gated_scan` com heal/noop + HEALTH_ISSUE
- Fine-gate: Intel net `8086` class 02/0D **exclui** Ethernet nativo (`subclass==0x00`, ex. e1000) — alinhado à política NVIDIA (sem falso positivo)
- Honest noop quando `fw_gated=0`; hermes residual gate commitado (usa `k_ai` real)
- **N2.5:** link crate `k_ai` no bin ainda bloqueado por `#[global_allocator]` — comportamento via espelho `neural-kernel` até então

### Evidência
- `logs/boot_n2_20260716_131837.txt` — `[TRUST] allow` + `[N2-SELFHEAL]` inventory/honest noop/gate complete
- `cargo nk` = 0 erros

### Docs
- ADR-0042 checklist N2 ✅; STATE; SESSION_112; IDEA #435 ✅

## [1.7.3] — 2026-07-16 — Docs: handoff voz 107 → Sprint Sound + pista ADR-0042

Docs-only. Sem mudança de runtime. Package Cargo permanece `1.0.0` (hábito tag-only).

### Docs
- Sprint **107 Voice** marcada **FECHADA** (PASS parcial forte+) — entregas permanecem; gaps de voz **não** são mais 107
- Backlog voz migrado para **Sprint Sound (reaberta)**: STT retrain, Mic→Wake runtime, Piper VITS pleno, soft-float latency, UAC, jarbas wire, VAD/SER/Wake polish
- **Pista ativa** = ADR-0042 N2→N5 (voz não bloqueia)
- STATE / TODO / ROADMAP / AGENTS / IDEA_BANK / TECNOLOGIAS §5 / ADR-0045 / SESSION_111 alinhados

## [1.7.2] — 2026-07-16 — Sprint 107 loops 1–5 (clima PASS parcial forte)

Marco funcional pós-ADR-0045. Package Cargo permanece `1.0.0` (hábito tag-only).

### Clima e2e (Loop 5 — `logs/boot_whpx_20260716_033322.txt`)
- GEN: `decoded_len=12 text='O tempo esta'` — frase PT climática (logits + máscara; não canned)
- TTS: Piper neural-lite (`emb.weight`) · `pcm_samples=15428` + FB paint
- WakeWordAgent registrado no AgentFleet
- STT CTC LOADED (path real) mas `ctc=''` → seed prompt
- Experts: RUSTCODER/STT/BGE OK; HWEXPERT parse FAILED (header vocab u16)

### Code (loops 1–5)
- cortex/bpe generate constrained weather + chat frame Llama
- Piper neural-lite + convert_piper; STT path hardening
- WakeWord register; QEMU loaders BPE/HW/RustCoder/STT; weather e2e scripts

### Known gaps (→ Sprint Sound reaberta / v1.7.3 docs)
- Soft-float tkn/s; STT CTC retrain; Mic→WakeWord→STT→LLM→TTS runtime e2e; Piper VITS pleno; jarbas/audio wiring; UAC

## [1.7.1] — 2026-07-16 — ADR-0045 Sound Voice Stack (docs)

Documentação do stack de voz **real** no boot. Sem mudança de runtime. Package Cargo permanece `1.0.0` (hábito tag-only).

### Docs / ADR
- **ADR-0045** `docs/architecture/0045-sound-voice-stack.md` — truth = `neural-kernel/src/audio/*`; `jarbas/audio` = espelho não wired
- Stack canônico: HDA + Piper VITS (+ formant fallback) + STT CTC + VAD + mixer + FB TTS paint
- Supersede como primário: sherpa-onnx, Pocket TTS, Kokoro-82M, Vosk, Wyoming, Rustpotter
- IDEA_BANK: #75/#83 ✅; #84 UAC 🟡 futuro; #315.21–25 / #315.N+1 / #360 ❌ supersedido; B-01 voz desbloqueado (SLIP #415)
- STATE / SESSION_109 / SESSION_INDEX / TECNOLOGIAS §5 / TODO Sprint 107 alinhados
- Gaps Sprint 107 documentados (WakeWord não registrado; Piper neural fraco; loop TTS↔STT↔LLM aberto) — **superseded by 1.7.2** (WakeWord registrado; GEN+TTS neural-lite)

## [1.7.0] — 2026-07-15 — N1 ✅ + BitNet 2B LOADED (N3 parcial)

Marco QEMU além de “só N1”. Linha **1.6.0-dev absorvida/superseded** (sem tag `v1.6.0` vazia). Package Cargo permanece `1.0.0` (hábito tag-only, como 1.5.7). **Não** é `v2.0.0` (ADR-0042 gate = N1–N5).

### N1 — k-nano legível ✅
- **N1.1** `load_status::{LoadStatus,AssetKind}` + banner `[STATUS] llm=… bge=… piper=… fw_gpu=…`
- Removido log falso `modelo 2B carregado da FAT32` sem prova → `LLM ABSENT` / LOADED coerente com `[LLM-TEST]`
- **N1.2** Probe NVIDIA FW (`test_load_firmware` / ACR) **só** se `GpuVendor::Nvidia`; QEMU 1234:1111 → skip; CapGate bootstrap documentado (DENY demos esperados)
- **N1.3** Hook `agent_core::set_sched_metrics_hook` → log periódico `[SCHED] tick= agents= polled=`

### N3 parcial — BitNet 2B LOADED de verdade
- Evidência `logs/boot_whpx_20260715_112049.txt`: QEMU-loader @0x100000000, **~590MB**, ver=4 h=2560 **L=30**, `LLM LOADED file=BITNET2B`
- Path STT-sim → Hermes → **FWD layers 0…29/30**
- Export/convert v4 (`tools/convert_bitnet.py`) alinhado a `load_model` dims

### QEMU / ops / FAT
- Soft-float + multicore: `.cargo/config.toml` (`jobs`/`-Z threads`, `-sse*`, alias `cargo nk`)
- Disco/FAT: free-cluster scan por setor; `mkfat32` / `BITNET2B.BIN`; scripts `-RamGB 6 -Smp 4`
- Disco slim: `tools/mkfat32_slim_qemu.py`; full via `build_image.py`

### Known issue (e2e clima PARCIAL)
- `[JARBAS-TTS] FAILED empty generate` — generate/TTS ainda aberto; LOADED+FWD ≠ resposta falada

## [1.5.7] — 2026-07-14 — Boot A/B + ADR-0041 Capability Ladder P0–P9

PoC capability no monólito `neural-kernel` (commits `9bb1382`…`49c4301`). Package Cargo permanece `1.0.0` (hábito do repo); release via CHANGELOG + tag git.

### Pacote A — Boot endurecido
- STI/PIC, stack heap ≥2MB, `init_phase` round-robin, `BOOT_PHASE` + consumer, DiagnosticSkill, logs/docs de heap

### Pacote B — Ordem de bring-up
- `init_platform_sync` (PCI+ACPI+APIC+SMP) **antes** dos probes de driver
- PlatformAgent / NetDriverAgent idempotentes se sync já rodou
- Agency SpecialistAgent: Continuous → EventDriven

### MVP C — ADR-0041 Capability Rings (PoC)
- `AddressSpace` + CR3 switch A→B→kernel (IRQ mascaradas na janela)
- `SharedSpscRing` em página compartilhada; Cap bitflags + trap `int 0x90`
- Demo pós-DriverInit non-fatal (WARN + boot continua)
- Arquivos: `address_space.rs`, `syscall.rs`, `ipc/*`

### Docs
- ADR-0041, STATE/IDEA_BANK/SESSION_107, TECNOLOGIAS 2.10

### P3 — Hermes CapabilityGate (ADR-0041)
- `capability_gate.rs`: gate `aios_send_tcp` / `aios_write_ring` por `Cap::{SEND_TCP,WRITE_RING}`
- Hermes skills net/* + `wasm_rt::host_call_gated`; demo boot non-fatal
- Deny sem Cap → log serial `[CapGate] DENY`

### P4 — JARBAS FB MMIO + double-buffer (ADR-0041)
- `jarbas_fb.rs`: contrato `FbContract` (bootloader FB), map AS JARBAS (`JARBAS_FB_VA`), Cap `MAP_FB`/`WRITE_FB`
- Double-buffer heap + `present` + stub vsync (`TIMER_TICKS`/`sfence`); demo boot non-fatal pós-P3
- Sem FB físico → Cap-only path SUCCESS; falha → WARN, boot continua

### P5 — K-IA DMA pin + Cortex weight mmap (ADR-0041)
- `k_ia_dma.rs`: pin frames não-reclaimáveis + map AS (`K_IA_DMA_VA`), Cap `PIN_DMA`/`MAP_DMA`; VirtIO phys stub
- `cortex_mmap.rs`: mmap N páginas peso simuladas em `CORTEX_WEIGHT_VA` (eager), Cap `MAP_WEIGHTS`; demand-paging/GGUF TODO
- Demo boot non-fatal pós-P4; falha frame alloc → Cap-only / WARN, sem panic

### P6 — Ring3 user-mode real (ADR-0041)
- GDT user code/data + TSS RSP0; IDT `int 0x90` DPL=3
- `user_mode.rs`: `enter_user_mode` via `iretq`, stub USER (marker + EXIT), return jmp kernel; Cap `ENTER_USER`
- `map_user_page` com USER em toda a cadeia PT; demo boot non-fatal pós-P5; #GP/#PF abort durante demo
- Flag `TRY_ENTER_RING3` para disable se necessário

### P7 — Demand-paging via #PF (ADR-0041)
- `demand_page.rs`: registry lazy (frames pré-alocados); `#PF` instala leaf PRESENT e retorna (retry)
- `cortex_mmap::mmap_weights_lazy` + Cap `DEMAND_PAGE` / `SYS_DEMAND_PAGE`; `AddressSpace::reserve_page`
- Demo boot non-fatal pós-P6: first-touch R/W curado; deny sem Cap; falha → WARN

### P8 — VirtIO vring + DMA pin (ADR-0041)
- `virtio_vring.rs`: Virtqueue layout-compatible (desc+avail+used) sobre `k_ia_dma::pin_frames`; Cap `VRING_SETUP`
- `Desc.addr` aponta para página pinnada (zero-copy); path paralelo — NIC live observe-only
- Sem VirtIO device → layout-only SUCCESS; demo boot non-fatal pós-P7

### P9 — GGUF/FAT file-backed mmap (ADR-0041)
- `gguf_mmap.rs`: pré-lê 1–4 páginas de `BITNET.BIN`/`HWEXPRT.BIN`/… via FAT `read_file_range`; Cap `MAP_FILE`
- Frames pré-preenchidos + `demand_page::register_lazy` (`FILE_WEIGHT_VA`); #PF só PRESENT (sem I/O no fault)
- Fallback stub `NFIL` se arquivo ausente; demo boot non-fatal pós-P8 (deny → mmap → touch magic → restore)

## v2.0.0 — 2026-07-13 — Sprint 106: Ecossistema de Anéis Lógicos

### Sprint 106-11: Correção de boot em HW real
- **Heap address:** Alterado de `0x4444_4444_0000` para `0x4000_0000_0000` (1TB) — endereço mais seguro para hardware real
- **AHCI/SATA:** Verificado suporte AHCI já implementado em `ahci.rs` — sistema suporta tanto ISA ATA quanto SATA AHCI
- **Display/Framebuffer:** Sistema requer UEFI GOP ativo para framebuffer gráfico. Sem GOP, fallback para VGA text mode (80x25)
- **Diagnóstico vídeo:** Logs mostram "Sem framebuffer UEFI — VGA text mode" em QEMU. Bootloader 0.11 não expõe configuração de framebuffer via API. GOP depende do firmware UEFI/OVMF.
- **Solução:** Para HW real, garantir UEFI GOP ativo no firmware. Para QEMU, usar OVMF com `-vga std` para framebuffer gráfico.
- **Validação:** `cargo check --release` com 0 erros (2 warnings menores em ata.rs, não críticos)
- **Motivo:** Endereço de heap muito alto (0x4444_4444_0000) pode causar problemas de mapeamento de memória em hardware real

### ADR v2.0 — Refatoração para Workspace Estrito
- **workspace Cargo:** 11 membros (ticket-lock, neural-kernel, agent-core, skill-registry, event-bus, boot, k_nano, k_ai, cortex, hermes, jarbas) com `resolver = "2"`
- **Rename:** k_ia → k_ai (Ring 1 Lógico), jarvis → jarbas (Ring 2 HCI)
- **Backups:** Pastas antigas preservadas (LEGACY/k_ia, LEGACY/jarvis)

### Sprint 106-1: Estruturar Cargo workspace estrito
- **Cargo.toml raiz:** `members = ["crates/k_nano", "crates/k_ai", "crates/cortex", "crates/hermes", "crates/jarbas"]` + dependências auxiliares
- **Isolamento:** Dependências não vazam entre camadas lógicas
- **Cargo.lock:** Regenerado com resolver = "2" para dependências transativas
- **Validação:** `cargo check --release` com 0 erros

### Sprint 106-2: Cargo clean + Workspace Sanity Check
- **cargo clean -p neural-kernel:** Build artifacts removidos (target/), código fonte preservado
- **cargo check --release:** Validado 0 erros (2 warnings menores em ata.rs, não críticos)
- **Preservação:** Nenhum arquivo fonte deletado — apenas build cache

### Sprint 106-3: Corrigir SOUL.md parser (dependência ring2→ring0)
- **Cargo.toml jarbas:** adicionado `neural-kernel = { path = "../neural-kernel" }`
- **jarvis.rs:** `load_from_fat32()` usa `neural_kernel::fs::read_vfs("/SOUL.MD")` em vez de `k_nano::ATA_DRIVER.lock()` + `crate::fat32::Fat32Reader`
- **Isolamento:** jarbas (ring2) não acessa mais k_nano (ring0) diretamente para hardware — apenas serviços comuns (serial_println, EVENT_BUS, AUDIT_TRAIL)
- **Validação:** `cargo check --release` com 0 erros

### Sprint 106-4: Corrigir Trinity MoE Router
- **Investigação:** `trinity.rs` usa apenas `k_nano::serial_println!()` para logging (aceitável)
- **ExpertKind enum:** Simples, sem dependências externas
- **Trinity Router:** Classifica intents via ML/keyword matching — **não roteia para hardware específico**
- **Validação:** Build com 0 erros, nenhuma dependência circular detectada

### Sprint 106-5: RustPython no_std (Rota Nativa)
- **Viabilidade investigada:** RustPython **NÃO é no_std nativo** — depende de `std` para alocação dinâmica
- **Rota principal WASM (106-6):** Compilar RustPython para .wasm via `cargo build --target wasm32-wasip1`
- **Alternativa documentada:** Bridge C via `abi_x86_interrupt` exigiria portar RustPython para no_std (trabalho enorme)

### Sprint 106-6: MicroPython via WASM (Rota Sandbox)
- **Compilação:** MicroPython para .wasm
- **Sandbox:** Hermes executor com isolamento

### Sprint 106-7: Corrigir page faults (ordem de inicialização)
- **Ordem correta:** allocator → events → agents
- **lazy_init!():** Macro para agentes dependentes de heap
- **Validado:** `cargo run --release` sem page faults

### Sprint 106-8: AIOS API para Python (RAG + System Prompt)
- **Bibliotecas:** aios_net, aios_fs
- **Injeção:** Via RAG/System Prompt no RustPython

### Sprint 106-9: Escalonamento Evolutivo de Código (JIT Cognitivo)
- **SkillOpt + Knowledge Graph:** Python efêmero → WASM cravado em pedra
- **Evolução:** Código evolve de JIT para JIT Cognitivo

### Sprint 106-10: SkillOpt - Tradução Python→Rust no_std
- **Geração:** Rust no_std a partir de Python via Cortex LLM
- **Automatizado:** Pipeline de tradução integrado

### Refactor
- **`RingBufStore` extraído** em `fs/mod.rs` — tipo genérico com evicção FIFO por quota
- **`ram_fs_agent.rs`** delegado para `RingBufStore::new(1MB)` — ~40 LOC eliminados
- **`log_fs_agent.rs`** delegado para `RingBufStore::new(256KB)` — ~50 LOC eliminados
- **`hermes/src/fs/`** também atualizado com `RingBufStore` (consistency com monólito)

### Safety
- **`LEGACY/v1.5-neural-kernel-src/`** — snapshot de todo `crates/neural-kernel/src/` antes da migração v2.0
- Nada foi deletado — refactor puro por extração

# Changelog — neural-os-core v1.5.2 "Ring Buffer Refactor"

## v1.5.2 — 2026-07-13 — Ring Buffer Refactor

### Refactor
- **`RingBufStore` extraído** em `fs/mod.rs` — tipo genérico com evicção FIFO por quota
- **`ram_fs_agent.rs`** delegado para `RingBufStore::new(1MB)` — ~40 LOC eliminados
- **`log_fs_agent.rs`** delegado para `RingBufStore::new(256KB)` — ~50 LOC eliminados
- **`hermes/src/fs/`** também atualizado com `RingBufStore` (consistency com monólito)

### Safety
- **`LEGACY/v1.5-neural-kernel-src/`** — snapshot de todo `crates/neural-kernel/src/` antes da migração v2.0
- Nada foi deletado — refactor puro por extração

# Changelog — neural-os-core v1.5.1 "Ponytail Audit"

## v1.5.1 — 2026-07-13 — Ponytail Audit

### Cleanup (600 LOC removidos, 11 dep entries)
- **Deps removidas:** `pic8259` de 4 Cargo.tomls; `ed25519-compact`, `linked_list_allocator`, `bootloader_api` de crates que não usam
- **smoltcp features podadas:** `socket-dns`, `proto-dns` removidas (nunca usadas — DNS via UDP raw)
- **6 arquivos deletados:** `cfs.rs`, `hal.rs`, `time_utils.rs`, `wifi_aer.rs`, `wifi_dma.rs`, `wifi_apic.rs` — todos `#[allow(dead_code)]` sem chamadores
- **3 funções mortas removidas:** `ram_used_bytes()`, `agent_for_mount()` (stub), `scheduler_stats()` (stub)
- **14 branches `#[cfg(not(target_arch = "x86_64"))]` removidos** de `tensor.rs`, `simd.rs`, `bitnet_avx2.rs` — portabilidade especulativa para arquiteturas inexistentes
- **Trait `Architecture` + static `ARCH` removidos** de `hal.rs` — marcado `@dead` pelo autor, zero chamadores
- **`PICS` lazy_static + `init_pics()` removidos** de `interrupts.rs` — kernel só usa APIC

### K³CHJ Workspace Migration (v1.5.0)
- Monólito `neural-kernel` → 5 crates (k_nano, cortex, hermes, k_ia, jarvis)
- `tools/migrate_k2chj.py`: 193 arquivos mapeados, 79 refs cross-crate corrigidas
- k_nano compila independentemente (0 erros)
- neural-kernel intacto como bin crate (build 0 erros)

## v1.2.0 — 2026-07-12 — ATA Liberation

### ATA PIO Bug Fix (crítico — afeta v0.1 até v1.1.5)

- **Root cause:** `read_sectors()` e `identify()` usavam `in al, dx` + `in al, dx+1` para ler palavras de 16 bits do disco ATA. O port `io_base+1` não contém o segundo byte do dado — é o registrador FEATURES/ERROR. Correção: usar `in ax, dx` para ler a palavra completa do registrador de dados.
- **Impacto:** TODO acesso a disco desde o início do projeto (v0.1, 2026-05) era lixo. MBR, FAT32, modelos .bitnet, firmwares, credenciais WiFi — nada era lido corretamente. Apenas discos detectados como "presentes" mas dados corrompidos.
- **Probe ATA:** Agora prefere disco com partição FAT32 (type 0x0B/0x0C) sobre GPT (type 0xEE). Antes escolhia o primeiro com MBR — que era o bootloader (uefi.img), nunca o disco de dados.
- **Log QEMU confirmado:** `[ATA] ISA 496: slave FAT32! (type=0xc)` + `[FAT32] BPB: bps=512 spc=1`

## v1.1.5 — 2026-07-12 — Silicon Afterlife

### Sprints v1.1.x: GPU Compute + WiFi + Visual 3-Camadas + SelfHealing

- **v1.1.1 — GPU + Firmware + HW Expert v3** (1.200 LOC)
  - Firmware ACR loading: pipeline WPR implementado, blobs NVIDIA GP108 em firmware/
  - HW Expert v3 treinado: 61.453 VID/DID únicos (SDIO + pci.ids + usb.ids + kernel)
  - Modelo 128h/6L/8heads, 1M params, 259KB, loss 0.389
  - 171.003 HWIDs SDIO de 65 DriverPacks, 20.054 .inf
  - 48.346 registros oficiais pci-ids + usb-ids + kernel PCI tables
  - Firmware metadata: WHENCE (998) + headers + AMD ucode (64 patches)
  - regulatory.db: 174 países WiFi

- **v1.1.2 — SelfHealing + HWID Datasets** (800 LOC)
  - SelfHeal I3: firmware ausente → HEALTH_ISSUE
  - SelfHeal I4: skill ausente → HEALTH_ISSUE  
  - firmware.rs: hot_load_firmware(vid, did, class) universal
  - HermesAgent: inscrito em HEALTH_ISSUE → LLM diagnostica
  - mkfat32.py: firmware incluso como FW_* no FAT32

- **v1.1.3 — 3 Camadas Visuais + Audio + Rede** (600 LOC)
  - Z-order real: Layer enum (OrbBackground < HermesOverlay < AppWindows < DockBar)
  - FPS control a 60Hz (LAST_FRAME_TICK)
  - Hermes CLI overlay semi-transparente sempre visível
  - FFT audio (Goertzel 16 bins) → animação do Orbe
  - Mouse PS/2 integrado: dock bar, close, drag
  - HDA playback (SD1): TTS finalmente chega ao auto-falante
  - BrowserAgent real: HTTP GET via smoltcp TCP com DNS resolve
  - DHCP starvation detection (SecurityAgent)

- **v1.1.4 — WiFi Intel AX200** (260 LOC)
  - iwlwifi CSR/HBUS/SRAM registers (0x000-0x29C)
  - ucode loading pipeline: wake → reset → seções → alive
  - Command/response via SRAM + doorbell NMI
  - Scan via comando 0x34
  - 5 firmware blobs: cc-a0 (AX200), Qu (AX101), so-a0-gf/hr (AX201/210), ty-a0-gf (AX211)
  - ~7.5MB firmware Intel WiFi em firmware/intel/iwlwifi/

- **v1.1.5 — Integração + Documentação** (50 LOC)
  - Sprint plan atualizado com progresso real
  - AGENTS.md expandido com lições v1.1.x
  - Build release: 0 erros, ~26.000 LOC, 180+ arquivos

## v1.0.0 — 2026-07-11 — A Era do Silício

### Sprints 92-100: Fundação Estável → Code Freeze

- **Sprint 92** — Fundação Estável: VirtIO-MMIO, AHCI probe, Zero-Trust Syscall, WHPX+AVX2 fix
- **Sprint 93** — WASM Runtime + IDE: WasmExec VM, PluginHub, SkillMarket, BitNet IDE
- **Sprint 94** — GPU Polish: MSched Belady, Observability, Human-in-the-Loop, Actor Registry
- **Sprint 95** — Memory + VFS: HNSW index, MHI+FS Bridge, Inference/Hermes/RamFs agents
- **Sprint 96** — GGUF + Model Loading: GGUF parser, RoPE, .bitnet v3/v4, /model swap
- **Sprint 97** — Rede + AIOS: http_get real, SearchAgent, SelfUpdate A/B slots, ContextWindow
- **Sprint 98** — Training: TrainingAgent, DataCollector, WakeWordML, Intel compute dispatch
- **Sprint 99** — SkillOpt + Structured Decoding: Compressed FSM, 6 decode modes, SkillOptimizer
- **Sprint 92b** — Code Cleanup + ZT Syscall: 94 warnings→0, check_syscall() wireado, serial bridge watchdog
- **Sprint 93b** — WASM refinements: parse_description refatorado, auto-rollback via snapshot
- **Sprint 94b** — LLM Icons + Human-in-the-Loop: generate_llm_icon() integrado, /approve/deny/pending
- **Sprint 96b** — GGUF Streaming + FAT32 chunked: load_gguf_header_from_disk(), read_file_range()
- **Sprint 97b** — RssAgent + EmailAgent: RSS/Atom parse + SMTP via http_get_raw()
- **Sprint 98b** — HW Expert GPU Training: 43.339 PCI+USB devices, loss 0.097, 95.4% accuracy
- **Sprint 99b** — Ponytail Audit: removed 19 dead files (~500 LOC), 3 dead deps (edge-dhcp, embedded-graphics, buddy-alloc), ~32 transitive crate nodes cleaned
- **Sprint 100** — **Code Freeze & Release v1.0.0**
- **Sprint 101** — **v2.0 Cognição**: Piper TTS VITS multilíngue, STT CTC engine, HDA audio DMA, NVIDIA PUSH_BUFFER GPU, ATA slave, RustCoder treinado

### Funcionalidades Principais

- **Bare-metal Rust kernel**: bootloader 0.11.15, IDT, GDT, TSS, SMP, APIC, ACPI, PCI
- **GPU**: VirtIO-GPU, Intel ring buffer, NVIDIA/AMD probe, VRAM buddy, display coexistence
- **LLM**: BitNet ternary (~850M params), 4-layer transformer, Medusa speculative decoding
- **Trinity MoE**: 5 experts + router (hw_identify, rust_coder, disk_diag, security, generator)
- **Rede**: smoltcp TCP/IP, VirtIO-net, RTL8139, E1000, serial tunnel SLIP, DNS, HTTP
- **WASM Runtime**: Custom VM with fuel, sandbox, 9 built-in skills, PluginHub
- **Filesystem**: FAT32, VFS, 7 agents (ATA, dev, proc, inference, hermes, ramfs, logfs)
- **HNSW**: Hierarchical Navigable Small World for approximate nearest neighbor search
- **Áudio**: HDA driver, pocket TTS (neural), formant synth, VAD, wake word, mixer
- **Segurança**: Ed25519 signing, TPM 2.0, TrustCache, Zero-Trust syscall, Audit Trail

### Mudanças desde v0.109.x

- 165+ arquivos Rust, ~19.000 LOC, ~50 agentes nativos, 0 erros de compilação
- 461 commits desde o primeiro boot

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/)
with [Conventional Commits](https://www.conventionalcommits.org/).

## [0.109.3-b01-morto] — 2026-07-09 — 🏆🔥 B-01 MORTO! Serial tunnel TCP bridge

### O bloqueador de 18 sprints finalmente caiu — B-01
**O kernel recebeu dados reais pela primeira vez:**
```
[BRIDGE] RX #1: 304 bytes ← KERNEL RESPONDEU!
[BRIDGE] RX #2: 42 bytes
[BRIDGE] RX #3: 42 bytes
[BRIDGE] RX #4: 42 bytes
```
Comunicação bidirecional entre kernel bare-metal e host Windows via serial tunnel.

### Causa raiz do B-01
Não era bug no kernel. Era incompatibilidade entre:
- **Windows 11**: firewall loopback bloqueia TCP inbound, named pipes têm chicken-and-egg
- **QEMU TCG**: emulação de NIC (RTL8139/E1000) RX não injeta DMA de forma confiável
- **Kernel**: código Rust correto desde o início — TX funcionava, RX=0 por isolamento físico

### Solução: bypass serial TCP (inversão de topologia)
- `slip.rs` (82 LOC): driver serial COM2 com framing length-prefix, non-blocking
- `serial_bridge.py`: bridge TCP **servidor** (Python escuta, QEMU conecta como cliente)
- `-serial tcp:127.0.0.1:4444`: QEMU como cliente TCP, não servidor
- `nic_send/nic_recv`: serial tunnel como fallback universal no pipeline

### Arquitetura final do pipeline de rede
```
Browser/curl → WiFi Windows → localhost:4444 → QEMU TCP → COM2 serial
  → slip::recv() → nic_recv() → NetPhy::receive() → smoltcp → socket TCP
```

### SystemEnv — kernel sabe onde está rodando
- `env.rs`: SystemEnv enum (QemuSandbox/VBoxSandbox/HwReal/Offline)
- Detectado no boot por CPUID hypervisor + presença de NIC
- Serial tunnel só ativo em sandbox (QEMU/VBox) sem NIC
- Cortex, Hermes, JARVIS consultam via `crate::env::get()`

## [0.109.2-rtl8139-rx-fix] — 2026-07-08 — 🐛🔧 RTL8139 RE bit + AHCI BlockDevice

### Raiz do B-01 (RX=0) encontrada — RTL8139 CR_RE bit ausente
- **Bug**: `const CR_RE: u8 = 0x01` (Receiver Enable) nunca era escrito no registrador CR (offset 0x37). O MAC da Realtek ficava desligado — pacotes descartados na borda do chip antes do DMA.
- **Log confirmou**: `cr=0x0c` (só RXE+TXE), bit 0 (RE) = 0.
- **Correção**: todas as 3 escritas do CR agora usam `CR_RE | CR_RXE | CR_TXE` (0x0D).
- **E1000** funciona porque tem registradores diferentes — não depende desse bit.
- **Aprendizado**: dumps brutos de registradores na telemetria salvam dias de debug.

### scan_pci_cb() — Scanner PCI zero-allocation com callback
- `scan_pci_cb(cb)`: varre 256 buses × 32 slots com Header Type optimization, executa callback `(bus,slot,func,vid,did) → bool`, zero alocação.
- `find_device_by_class(class, subclass)`: busca early-return por class/subclass.
- AHCI probe em `main.rs` refatorado de `scan_pci()` (Vec heap) para `scan_pci_cb()` (zero alloc).

### AHCI + BlockDevice trait — Integração com pipeline FAT32
- `block_dev.rs`: trait `BlockDevice` com `read_sectors(lba, buf)`, implementada para `AtaDriver` e `AhciDriver`.
- `AHCI_DRIVER` global: armazena driver AHCI encontrado.
- Model loading tenta AHCI primeiro, fallback ATA legado.
- QEMU sem disco SATA anexado → `[BOOT] No storage device found` (esperado).

### SkillOpt viability analysis
- Paper Microsoft Research analisado: SkillOpt como optimizer de skills em espaço textual.
- Viabilidade confirmada para neural-os-core (~145 LOC, sem dependências externas).
- Recomendado para Sprint 99.

### SGLang Structured Decoding viability analysis
- Paper Stanford/Berkeley analisado: FSM comprimido para geração constraint.
- RadixAttention inviável (memória), PrefixCache parcial viável (~80 LOC).
- **Compressed FSM**: viável e alto impacto (~120 LOC) — máscara logits no BitNet decoder para JSON/SKILL.md/shell.
- Recomendado para Sprint 99 (junto com SkillOpt).

### vLLM PagedAttention viability analysis
- Paper UC Berkeley (SOSP 2023): KV cache paginado com COW entre prefixos.
- Conceito implementável com frame allocator + page table existentes (~100 LOC).
- Ganho marginal para single-user (sem batch de LLM).
- Recomendado para Sprint 100+, após SkillOpt + FSM.

### FlashAttention viability analysis
- Paper Stanford (NeurIPS 2022): IO-aware exact attention com tiling no cache L1.
- Aplica-se ao BitNet CPU: blocos de 16 tokens cabem no L1 (32 KB).
- ~3-5× speedup para sequências >256 tokens.
- Recomendado para Sprint 100+, ~100 LOC em cortex.rs.

## [0.109.1-compilation-fix] — 2026-07-08 — ✅ 32 erros de compilação eliminados

### Correção em massa — cache incremental mascarava 32 erros
- `cargo clean -p neural-kernel` revelou 32 erros que o build incremental escondia por meses
- **Causa raiz**: múltiplos imports faltando (`alloc::vec`, `Vec`, `String`, `ToString`), APIs trocadas (slab, VFS, jarvis), format string não escapada
- **shell.rs**: commas faltando, VFS methods inexistentes → `lookup`/`list_dir`, `current_dir` removido
- **cortex.rs**: `{` não escapado no `format!`; `Event` → `crate::Event`
- **agents.rs**: `}` extra pós-match arm; `train_step` esperava `&mut [i8]` não `&[i8]`
- **alloc_adapter.rs**: `SlabAllocator::new()` → `::empty()`, `allocate` → `slab_alloc` (retorna `*mut u8`)
- **burn_flex.rs**: `matmul_hybrid` (TernaryTensor) → `matmul` (Tensor); imports faltando
- **trinity.rs / memory_systems.rs**: `.sqrt()` → `libm::sqrtf()` (sem trait F32Ext em no_std)
- **jarvis.rs**: 4 APIs erradas (dream, ego, heartbeat, babel)
- **main.rs**: `AuditTrail::new()` não-const → `const fn` c/ `Vec::new()`; AHCI com PCI scan
- **Aprendizado chave**: `cargo clean -p neural-kernel` antes de `cargo check --release` é obrigatório quando erros somem misteriosamente

## [0.109.0-sprint91-sound] — 2026-07-08 — 🎵 Sprint 91 + Sound completos

### Sprint 91 — Ecosystem + Polimento 🏁
- **burn-flex backend**: `FlexBackend::gemm/quantize/pack` com testes unitários
- **MSched VRAM eviction**: Predictor Belady/OPT para working set de VRAM
- **GPU Display Co-existence**: iGPU display + dGPU compute assignment planner
- **SkillManifest macro**: `skill_manifest!()` para declarar manifests estaticamente

### Sprint Sound — Áudio completo 🎤 (16 módulos, ~2.000 LOC)
- **Intel HDA**: Driver real com PCI probe, GCTL reset, BAR0 mapping
- **USB Audio (UAC)**: Probe de dispositivos UAC via xHCI
- **Pocket TTS 100M**: Engine neural com GPU offload, FAT32/QEMU loader
- **Formant TTS**: Klatt-style sintetizador completo (36 fonemas, IIR resonators)
- **VAD**: Voice Activity Detection com RMS+ZCR e hysteresis
- **SER**: Speech Emotion Recognition (8 emoções) com Skill exposure
- **Wake Word**: Detector "jarvis" por energia, cooldown 100 ticks
- **Audio Ring Buffer**: SPSC lock-free PCM (16384 samples)
- **Audio Mixer**: Volume scaling agent com `AUDIO_VOLUME` atomic
- **Audio Context**: Construtor de contexto emocional para LLM injection

## [0.108.0-sprint89] — 2026-07-08 — 🧠 Sprint 89: SleepCycle + Memória + BGE

### Sprints 86-89 — JARVIS completo
- **Sprint 86** (JARVIS Persona): SOUL.md FAT32, 4 compressões, Notification 4 urgências, SlabBuddy
- **Sprint 87** (JARVIS Security+AHCI): I1-I4 invariantes, AUDIT_TRAIL global, AHCI instanciado
- **Sprint 88** (JARVIS Emotion+Cache): ADE real, Persona Pipeline 16 stages, edge-dhcp
- **Sprint 89** (SleepCycle+Memory): SleepCycle 5 fases, KG bitemporal, BGE semantic_search

### Pendentes resolvidos
- **#314 SleepCycleAgent**: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT com BitNetTrainer
- **#225 KG Bitemporal**: valid_from/valid_to, tx_from/tx_to, as_of(tick)
- **#359 BGE semantic_search**: `index_embedding()` + cosine similarity
- **#333 burn-flex**: stub com gemm/quantize/pack + testes

## [0.102.0-trinity-learn] — 2026-07-08 — 🤖 Trinity AutoLearn + SmileyOS Nativo

### Trinity AutoLearn
- **AutoLearnAgent**: Detecta intent não classificado 3x → gera necessidade de aprendizado
- **Ciclo completo**: necessidade → FAT32/CVE.BIN → BitNetTrainer → expert registrado
- **`generate_via_model()`**: Reporta intents "generator" via EventBus para AutoLearnAgent

### SmileyOS Padrões Nativos (IDEA #279, sem repo original)
- **Shell 55+ comandos**: +17 comandos (touch, mkdir, top, dmesg, netstat, fetch, etc)
- **Compositor drag/resize/close**: `drag_window()`, `resize_window()`, [X] close button, dock bar
- **WASM Executor**: VM stack-based com 20+ opcodes (Push, Add, Sub, Call, Br, Print, Halt)
- **LLM Icons**: Geração de bitmap 8x8 via HWEXPERT_MODEL com fallback hash

## [0.100.0-ai-regmap] — 2026-07-08 — 🧩 Saltos: RegMap IA + MoE Router + Boot Agent

### Salto 1 — HardwareRegisterMap via IA
- **`generate_register_map(vid, did)`**: 3 níveis de inferência
  - Nível 1: mapa direto por HWID (40+ dispositivos conhecidos)
  - Nível 2: IA classifica família → aplica mapa correspondente
  - Nível 3: heurística por vendor ID → mapa genérico funcional
- `runtime_probe_and_bind()` usa IA quando não acha mapa fixo

### Salto 2 — TrinityRouter com pesos treinados (MoE real)
- `router_embed`: tabela VOCAB×HIDDEN para embedding de tokens
- `router_weight`: PackedTernaryTensor (HIDDEN×NUM_EXPERTS)
- `classify_intent()`: ML → softmax → argmax, fallback keyword se score < 15%

### Salto 3 — Boot Agent com IA generativa
- HwDetectAgent reescrito: PCI scan → HWExpert identifica → generate_register_map() → device tree

## [0.99.0-sdio-complete] — 2026-07-08 — 💾 SDIO Pipeline: 45 packs, 95.812 entradas

### SDIO DriverPacks
- **45 packs processados** (18.6 GB) via watcher automático
- **95.812 entradas** JSONL geradas (de 2.794 → 95.812, 34× crescimento)
- Extração completa: `.inf` + `.sys` + `.cat` + `.dll` + `.txt` + `.html`
- Análise `pefile`: imports IAT por DLL, exports, strings de hardware
- Modelo re-treinado: `hw_expert.bitnet` loss 3.05 → 0.38

### Ferramentas
- `extract_full_hw.py`: extrator completo de TODOS os formatos, watcher automático
- `samdrivers_full.py`: pipeline com --resume/--retrain/--check
- `publish_hf_dataset.py`: sanitiza e publica dataset no HuggingFace
- `update_tecnologias.py`: mantém barras de progresso do catálogo automaticamente

## [0.97.0-rustcoder] — 2026-07-08 — 🦀 Sprint 97: RustCoder Expert + Trinity MoE

### Sprint 97 — RustCoder Expert (~300 LOC, 3 arquivos alterados)
- **Treino**: Expert Rust (hidden=128, 6 layers, 1.6M params) treinado com 41.200 amostras de código Rust na GTX 1050 (loss 0.34)
- **tools/finetune_rust_llm.py**: Script de fine-tuning completo com export bitnet v2
- **tools/rust_coder.bitnet**: Modelo exportado em 444 KB
- **RUSTCODER_MODEL**: Nova static global em cortex.rs — `set_rustcoder_model()` + `generate_via_rustcoder()`
- **Fast-path HermesAgent**: Trinity classifica "rust_coder" → geração direta pelo expert sem LLM principal
- **Fallback silencioso**: Se RUSTCDR.BITNET não existir na FAT32, usa LLM principal normalmente
- **Boot FAT32**: Kernel carrega RUSTCDR.BITNET da partição FAT32 durante boot
- **build_image.py**: Copia rust_coder.bitnet → RUSTCDR.BITNET na imagem HW
- **Aprendizado chave**: bitnet v2 (packed ternary) é o formato correto para load_model() do kernel

## [0.95.0-cog+v0.96.0-heal] — 2026-07-06 — 🧠🛡️ Sprints 95+96: Cognitive + Self-Heal

### Sprint 95 — Cognitive Engine (510+ LOC, 25 structs/funcs)
- **#105 Intent Planner** — SkillSteps com params, goal-based plan generation
- **#106 Success Engine** — win/loss tracking, streak, recent_rate (64-window)
- **#107 Neural Cache** — TTL, LRU evicção (max 4096), hit/miss tracking
- **#108 MatMul-Free LM** — RWKV-style WKV forward sem multiplicação de matrizes
- **#149 Feedback Loop** — rating (0-10) + comment attachment
- **#150 Ternary Weight Update** — gradiente → {-1,0,+1} com threshold lr
- **#151 Experience Replay Buffer** — ring buffer (10K cap), sample por index
- **#152 Weight Consolidation** — snapshot export com metadata
- **#158 Workflow Predictor** — confidence scoring por task, top prediction
- **#159 Auto-Skill Generator** — WASM templates (echo, hello), generate bytes
- **#160 Dynamic Resource Scaling** — heap_target ajustável por pressure
- **#161 Self-Optimizing Scheduler** — timeslice dinâmico baseado em latência
- **#162 Workflow Profile** — JSON export com steps + avg_duration
- **#169 Codebook VQ** — nearest-neighbor quantization (256 codes × 64 dim)
- **#170 KV Cache Codebook** — compress/decompress KV cache via codebook
- **#171 ReAct Loop** — Thought → Action → Observation, max_iter guard
- **#172 MCP Server** — tools/list, tools/call, session tracking
- **#173 Codebook Finetune** — centroid adjustment via learning rate
- **#174 Delta Branches** — speculative decode draft/verify, acceptance rate
- **#175 Workspace Isolation** — sandbox heap per agent (BTreeMap alloc)
- **M2 Episodic Memory** — ring buffer (max 1000), replay API
- **M37 SleepCycle Guard Rails** — blocked words per phase (replay/dream)
- **M38 BitNetTrainer** — train_step com ternary_update, loss tracking
- **M39 Candle Trainer sidecar** — stub com connect/train/loss
- **M40 Task Spawner** — spawn tracking (max 16 children)
- **M41 Three Data Sources** — replay_buffer, user_feedback, episodic_memory

### Sprint 96 — Self-Healing Avançado (~350 LOC em self_heal.rs + vfs + memory)
- **#226-227 Team Memory + Snapshots** — agent-shared BTreeMap com versionamento
- **#265-266 Vector FS** — VectorFs com dot product search (384-dim)
- **#267 OverlayFS** — VfsRegistry::mount_overlay() multi-layer
- **M1 Zero-Copy SFS** — slice references, directory index em 256 bytes
- **M3 Skills-as-Modules** — fn pointer import + version control
- **M6 Failure Taxonomy** — classify_by_code (5 classes + range mapping)
- **M7 Exception Self-Heal** — auto recovery via SelfHeal::analyze()
- **M8 Corrective Prompting** — context-aware suggestion with escalation
- **M9 Verifier Pós-Recovery** — fn check: bool, label reporting
- **M10 Erros no EventLog** — format + persist stub
- **M11 Budgeted Recovery** — attempts/daemon com max per window
- **M12 Silent Failure Detection** — heartbeat + threshold detection
- **M13 Multi-level Failure Assessment** — Ok/Warning/Error/Critical
- **M14 Failure Prediction** — trend analysis via window diff
- **M29 Notification Gate** — allow list por agent + type, block/deliver counters

### Changed
- `cognitive.rs` — reescrito de 86 LOC para 510+ LOC com todos os 25+ itens
- `self_heal.rs` — Sprint 96 completo: M1-M29, ZeroCopySfs, SkillModule, BudgetedRecovery, SilentFailureDetector, NotificationGate
- `memory_systems.rs` — Team memory with snapshot versioning
- `vfs/mod.rs` — Vector FS semantic search + OverlayFS mount
- `main.rs` — 22 new `lazy_static` instances para cognitive + self-heal modules
- `fs/ata_agent.rs` — Fixed pre-existing unreachable match arm bug

## [0.94.0-vision] — 2026-07-06 — 👁️ Sprint 94: Vision + Display

### Added
- **#79 Font rendering escalado** — `draw_text_scaled()` com scale=1,2,3... para alta resolução
- **#80 Texto em negrito** — `draw_text_bold()` com desenho duplicado para destaque
- **#81 VirtIO-GPU** — Aceleração 2D via VirtIO (QEMU) já funcional desde Sprint 45
- **#82 Tensor visualization** — `draw_tensor_heatmap()` + `draw_attention_graph()` no desktop
- **Painel Vision** — Attention Map + Token Scores no canto superior direito do desktop

### Changed
- `font.rs` — Adicionadas 4 novas funções de renderização
- `compositor.rs` — Tensor viz overlay integrado ao desktop JARVIS

### Tested
- QEMU -smp 1 TCG: 0 panics, Desktop 1280×720, Vision panel, 248 agents

## [0.93.0-wasm] — 2026-07-06 — ⚡ Sprint 93: WASM Runtime + IDE

### Added
- `wasm_rt.rs`: WASM Skill Runtime, MemoryPool (256KB/skill), 15 WASI→Skill mappings, HybridRegistry
- **BitNet IDE** (F4): Gera WASM skills via `[GEN]` → publica como ícone no desktop
- **Ícones WASM dinâmicos**: Skills aparecem como quadrados no desktop, clicáveis
- `app_store.rs`: AppForge (install/uninstall/search)
- `multi_user.rs`: Multi-User com trust tiers
- `workflow.rs`: Workflow Builder (DAG) + Federated Cluster
- `hub.rs`: Observability + Hub Discovery
- `elf_loader.rs`: Cross-OS loaders (ELF/PE/Mach-O/APK)
- Compositor: suporte a AppId::Ide, AppId::WasmSkill, ícones dinâmicos

### Tested
- QEMU -smp 2 WHPX: 0 panics, Desktop 1280×720, 248 agents

## [0.92.0-lan] — 2026-07-06 — 🌐 Sprint 92: LAN + Dependências

### Added
- B-01/#117-120: Network stack (smoltcp DHCP/ARP, /ping)
- #186-189: AppForge, Multi-User, Workflow, Federated
- #241-247: Observability, Hub, HITL, Marketplace, Compaction
- #306a-d: ELF/PE/Mach-O/APK loaders
- M4-M5: Syscall Categories, Neural Cache

## [0.91.0-ui] — 2026-07-06 — 🖥️ Sprint 91: JARVIS Desktop UI

### Added
- **JarvisDesktop** — Compositor multi-window com status bar + app switcher
- **Hermes Chat App** — Janela de chat com histórico de comandos
- **Settings App** — Configurações: tema, voz, memória, avatar, rede
- **Power App** — Shutdown, Reboot, Hibernate, Sleep
- **JARVIS avatar overlay** — Canto inferior direito com pulso animado
- **`display/compositor.rs`** — Reescrito: `JarvisDesktop` + `draw_text()` + `render_app_content()`

### Changed
- `display/agent.rs` — DisplayAgent agora gerencia Desktop + apps + avatar
- `compositor.rs` — Substitui wrapper NeuralConsole por JarvisDesktop completo

### Tested
- QEMU -smp 2 WHPX: 0 panics, Desktop 1280×720, 248 agents

## [0.90.0-cognitive] — 2026-07-06 — 🧠 Sprint 90: JARVIS Deep Cognitive

### Added
- **#315.12 Dreaming/Consolidation** — `DreamEngine`: insights sintéticos, agrupamento por tópico
- **#315.13 Ego Layer** — `EgoLayer`: confidence tracking por domínio, `can_answer()`
- **#315.14 Proactive Heartbeats** — `Heartbeat`: JARVIS alerta proativamente (disk, mem, net)
- **#315.15 Tool-State Save Game** — `ToolState`: snapshot + rollback de ferramentas
- **#315.16 Auto-Skill Generation** — `AutoSkillGen`: gera skill ao detectar padrão ≥3 repetições
- **#315.17 Babel-Index** — `BabelIndex`: monitora entropia, contradictions, staleness

### Tested
- QEMU -smp 2 WHPX: 0 panics, 248 agents, JARVIS cognitive engine OK

## [0.89.0-memory] — 2026-07-06 — 🧠 Sprint 89: SleepCycle + Advanced Memory + BGE

### Added
- **#314 SleepCycle Agent** — 5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT, agendado por tick
- **#214 SHA-256 Memory Dedup** — Sliding window 5min, SHA-256 hash check
- **#215 Privacy Filter** — Strip API keys, secrets, tokens antes de armazenar
- **#216 Memory TTL/Eviction** — Auto-evict por TTL + importância, fallback LRU
- **#219 Ebbinghaus Decay** — `strength = importance × e^(-λ·days) × (1 + recall_count × 0.2)`
- **#217 Hybrid Search (BM25 + MLP)** — BM25 score com RRF fusion, avg_len normalizado
- **#218 4-Tier Memory Consolidation** — Working→Episodic→Semantic→Procedural pipeline
- **#222 Metacognitive Guard** — Verifica erros passados antes de executar skill
- **#223 Draft→Review→Merge Memory** — Workflow de aprovação de memória
- **#224 Atkinson-Shiffrin 3-tier** — Sensory Register (48h) → STM (7d) → LTM (permanent)
- **#225 Bi-temporal Knowledge Graph** — Triplas (sujeito, predicado, objeto) com validade temporal
- **#359 BGE-Small-EN-v1.5** — Embedding stub 384-dim (ONNX→.bitnet pendente)

### New File
- `memory_systems.rs` — Todos os 12 itens em um módulo coeso (~470 LOC)

### Tested
- QEMU -smp 2 WHPX: 0 panics, 248 agents, JARVIS avatar OK

## [0.88.0-emotion] — 2026-07-06 — 🎭 Sprint 88: JARVIS Emotion + Cache + Pipeline + DHCP

### Added
- **#315.6 Emotion Analysis** — `EmotionAnalysis` com 7 emoções + sarcasmo, análise por palavra-chave
- **#315.7 Capability Contract + Consent Gates** — `ConsentGate` com 3 níveis (Safe/Moderate/Dangerous)
- **#315.8 Skill Discovery** — `SkillDiscovery` — observa padrões de tarefa, propõe skills em ≥3 repetições
- **#315.9 ADE Pipeline** — `ade_pipeline()` — 4 fases: Spec→Execute→Review→Recover
- **#315.10 Semantic Cache** — `SemanticCache` — 5 tiers (exact→pattern→fallback), hit/miss tracking
- **#315.11 Persona Pipeline** — `persona_pipeline()` — 16 stages da OVOS
- **#356 edge-dhcp integration** — `dhcp.rs` — ponte para crate edge-dhcp (no_std + no-alloc DHCP)

### Changed
- `jarvis.rs` unificado com todos os 16+ componentes da Sprint 86-88

### Tested
- QEMU -smp 2 WHPX: 0 panics, 214 agency agents, JARVIS avatar OK

## [0.87.0-security] — 2026-07-06 — 🛡️ Sprint 87: JARVIS Security + AHCI

### Added
- **#315.18 Fail-Closed Safety Invariant** — `safety.rs`: `SafetyInvariants` com 4 invariantes SMT-proof (I1-I4). Padrão é negar.
- **#315.19 Merkle Audit Trail** — `audit.rs`: `AuditTrail` com SHA-256 chain, ring 4096, verificação de integridade.
- **#315.20 Fluid Persona** — `jarvis.rs`: `SoulProfile::fluid_update()` adapta tom por emoção/urgência. 3 modos (Coach/Tutor/Tool).
- **AHCI driver** — `ahci.rs`: Driver SATA 6G NCQ via MMIO. Suporta ATAPI, PRDT, DMA READ/WRITE. PCI class 0x01/0x06.

### Changed
- `tpm.rs`: `sha256()` agora é `pub` (usado pelo audit trail)

### Tested
- QEMU -smp 2 WHPX: 0 panics. SafetyAgent registrado, Hermes Chat OK.

## [0.86.3-persona] — 2026-07-06 — 🧑 Sprint 86: JARVIS Persona + Alloc Adapter

### Added
- **#315.1 SOUL.md** — `SoulProfile` com name/tone/humor/formality/empathy, parser markdown
- **#315.2 IPW Monitor** — `IpwMonitor` lê RAPL MSR 0x610 (PKG_ENERGY_STATUS), calcula tokens/watt
- **#315.3 Session Compression** — `SessionHistory` com 4 estratégias (summarize/drop_lowest/merge_similar/segment_means)
- **#315.4 Notification Gate** — `NotificationGate` com 4 urgency levels, dedup, rate limit
- **#315.5 Sessionless Thread** — `SessionlessThread` conversa contínua sem reset, stale detection
- **#355 Alloc Adapter** — `alloc_adapter.rs` ponte para buddy-slab-allocator (feature opcional)
- **`jarvis.rs`** — Engine unificada integra todos os 5 componentes + tick loop

### Tested
- QEMU -smp 2 WHPX: 0 panics, JARVIS avatar + Hermes Chat OK

## [0.86.2-embedding] — 2026-07-06 — 🧠 ADR-0038 v2: BGE Embedding + Kokoro TTS

### Added
- **ADR-0038 v2** — Seção 5: Modelos de Embedding e TTS. BGE-Small-EN-v1.5 (Sprint 89) e Kokoro-82M (Sprint 92+).
- **IDEA_BANK:** #359 (BGE-Small-EN-v1.5), #360 (Kokoro-82M TTS)
- **Sprint 89 expandido:** +300 LOC para BGE embedding → busca semântica real no Hermes
- **Sprint 92+ expandido:** Kokoro-82M substitui Piper como TTS padrão (82M params vs 300M+)

### Viability
| Modelo | Params | Licença | Tamanho | Uso | Sprint |
|--------|--------|---------|---------|-----|--------|
| BGE-Small-EN-v1.5 | 33.4M | MIT | 33 MB | Embedding semântico | 89 |
| Kokoro-82M-ONNX | 82M | Apache-2.0 | 86 MB Q8 | TTS | 92+ |

## [0.86.1-ecosystem] — 2026-07-06 — 📊 ADR-0038: Otimização do Ecossistema (Hugging Bay + crates.io)

### Added
- **ADR-0038** — `docs/architecture/0038-ecosystem-optimization.md`: Decisões de substituição baseadas em pesquisa Hugging Bay + crates.io.
- **IDEA_BANK:** #355 (buddy-slab-allocator), #356 (edge-dhcp), #357 (khal-std), #358 (ruvix-net)
- **`tools/huggingbay_search.py`** — Busca no Hugging Bay por artefatos AI.
- **`tools/huggingbay_item.py`** — Detalhes de artefato por ID.

### Changed
- **`docs/sprint-plan-84-95.md`** — Sprint 86 expandido (+buddy-slab-allocator), Sprint 88 expandido (+edge-dhcp).
- **`docs/memory/IDEA_BANK.md`** — +4 ideias (#355-#358), total 358.

### Decisions (ADR-0038)
| Tecnologia | Ação | Sprint | Motivo |
|---|---|---|---|
| buddy-slab-allocator | Substituir slab.rs + vram.rs backend | 86 | 30K downloads, no_std, per-CPU slab, ArceOS |
| edge-dhcp (edge-net) | Fallback DHCP p/ B-01 | 88 | no_std + no-alloc, 225★ GitHub |
| khal-std | ❌ Inviável (requer wgpu/std) | — | Inspiração arquitetural apenas |
| ruvix-net | 🔵 Referência | — | Kernel cognitivo similar |

## [0.86.0-jarvis] — 2026-07-06 — 🏆 JARVIS Avatar + Cognição (port do .NET MAUI)

### Added
- **`display/avatar.rs`** — JARVIS Avatar com partículas animadas, 4 estados (Idle/Listening/Processing/Speaking), port do `AvatarDrawable.cs` do .NET MAUI. Renderiza sobre framebuffer via `DoubleBuffer::set_pixel()`.
- **`jarvis.rs`** — JARVIS Engine unificada: personalidade (`JarvisPersonality`), análise emocional (`detect_emotion` com 7 emoções + sarcasmo), memória contextual (`JarvisMemory` com ring buffer 256), avatar state machine. Port dos conceitos `TextProcessor`, `EmotionalAnalysisService`, `VectorStorageService`, `UserProfile` do .NET MAUI.
- **`display/agent.rs`** — DisplayAgent integra JARVIS avatar + engine + Hermes Chat Console.

### Arquitetura JARVIS (port .NET MAUI → bare-metal)
| Conceito .NET MAUI | Equivalente Rust | Arquivo |
|---|---|---|
| AvatarDrawable (SkiaSharp) | JarvisAvatar + Particle | `display/avatar.rs` |
| EmotionalAnalysisService | `detect_emotion()` (BitNet fallback) | `jarvis.rs` |
| VectorStorageService + SQLite | JarvisMemory (ring buffer) | `jarvis.rs` |
| UserProfile | JarvisPersonality (aprendizado contínuo) | `jarvis.rs` |
| Semantic Kernel | Hermes Cognitive + ReAct | (existente) |
| MainPage (Avatar+Chat) | DisplayAgent + NeuralConsole | `display/agent.rs` |
| VoiceService | Piper + Vosk (pós B-01) | (futuro) |

## [0.85.0-design] — 2026-07-06 — 🏆 Sprint 85: GPU Decode (XPU split + DMA + XQueue)

### Added
- **`gpu/xpu.rs`** — Agent.xpu prefill/decode split (#329, ~90 LOC): CPU prefill via forward_with_kv, GPU decode stub, generate() com timing. Referência arXiv 2506.24045.
- **`gpu/kv_dma.rs`** — CPU→GPU KV cache DMA (#331, ~90 LOC): KvDmaTransfer, kv_transfer_layer(). Copia KV cache entre RAM e VRAM com sfence. Referência dmaplane.
- **`gpu/xqueue.rs`** — XQueue preemptível 3 níveis (#332, ~125 LOC): pending/in-flight/running com timeout. Preempt rebaixa in-flight para pending. Referência XSched (OSDI 2025).

### Changed
- **`gpu/mod.rs`** — Adicionado `pub mod xpu`, `pub mod kv_dma`, `pub mod xqueue`.

### Tested
- QEMU (-smp 2, WHPX): 0 panics, 0 errors. GPU-BACKEND, SECURE-BOOT, Hermes Chat OK.
- VirtualBox (1 CPU, VirtIO-net): 0 panics, 0 errors. Hermes Chat OK.

### Sprint 85 Total: ~305 LOC (4 itens, est. 1500 LOC — stubs para quando GPU compute estiver pronto)

## [0.84.1-gpu] — 2026-07-06 — 🏆 Sprint 84: GPU Foundations (BAR mapping + Job Ring + VRAM Buddy + Secure Boot)

### Added
- **`gpu/ring.rs`** — SPSC job ring genérico para 3 vendors (Intel RENDER_RING_TAIL, NVIDIA PFIFO, AMD PM4). Doorbell, push, poll, submit_and_wait. Ring buffer em páginas UC.
- **`gpu/firmware.rs`** — Secure boot GPU: NVIDIA ACR, AMD PSP, Intel GuC. Pipeline: linux-firmware → kernel → BAR0 → GPU engine. Blobs stub (firmware disponível em linux-firmware, loading futuro).
- **`gpu/vram.rs`** — Upgrade para buddy allocator power-of-2 (4KB a 4GB). Splitting/merging de blocos. Substitui first-fit BTreeMap.

### Changed
- **`gpu/backend.rs`** — `init_backend()` agora: (1) mapeia BARs UC, (2) valida BAR0, (3) cria SPSC job ring, (4) secure boot, (5) vendor init.
- **`gpu/mod.rs`** — Adicionado `pub mod ring` e `pub mod firmware`.
- **`memory_agent.rs`** — `VRAM_STATE` → `VRAM_BUDDY` (novo allocator).

### Tested
- QEMU (-smp 2, WHPX): boot OK, GPU-BAR mapping, SECURE-BOOT, CPU fallback. ✅
- VirtualBox (1 CPU, VirtIO-net): boot OK, Hermes Chat, GPU-BACKEND. ✅
- 0 erros, 446 warnings (dead code esperados).

## [0.84.0-design] — 2026-07-05 — 📚 Documentação Reestruturada: HW Real First + Multi-Vendor + Sprint Plan 84-95

### Added
- **`docs/sprint-plan-84-95.md`** — Plano mestre de 9 sprints (84-95). Todos os 354+ items do IDEA_BANK assignados a sprints/blocos. HW Real, multi-vendor GPU/NVIDIA/AMD/Intel, busca ativa na internet para bloqueios.
- **`docs/memory/SESSION_INDEX.md`** — Catálogo de 43 sessões com títulos, sprints, descobertas. Seção "Lições Críticas (NÃO REPETIR)" com 10 dead-ends documentados.
- **`docs/TODO.md`** — Reescrito como checklist multissprint. Cada sprint com checkboxes, goals, sub-itens, dificuldades, dependências, fontes. Status flags: ✅ 🟡 ⏳ 🔴 💰 ❌.

### Changed (docs)
- **`docs/memory/STATE.md`** — Roadmap expandido para 84-95 (9 sprints). Seção "Navegação Rápida para AI DEVs". Pendentes por sprint com #ID do IDEA_BANK.
- **`docs/memory/IDEA_BANK.md`** — 9 items orphan atualizados com sprints específicos. Seção 6 expandida: Bloco 28 desmembrado em 21c+21d.
- **`docs/roadmap.md`** — Multi-vendor GPU/NPU, firmware ACR/PSP/GuC, QEMU loader removido, bloqueios com busca na internet.
- **`docs/architecture/0037-smp-gpu-architecture.md`** — GTX 1050→genérico NVIDIA/AMD/Intel. QEMU/VBox→HW Real.
- **`docs/architecture/0029-gpu-architecture.md`** — Tabela HW expandida, firmware multi-vendor, hardware layer genérico.
- **`docs/architecture/0016-network-strategy.md`** — RTL8139 dev + e1000/r8169 HW real + busca por NIC.
- **`docs/architecture/0001-initial-architecture-and-toolchain.md`** — QEMU→dev/debug.

### Changed (root)
- **`AGENTS.md`** — HW Real First (princípio #4). Busca ativa na internet para bloqueios. Navegação rápida AI-first. ~200 linhas de sessões históricas inline removidas (apontam para SESSION_*.md). MemPalace integration. Sprint: 84.

## [0.81.0] — 2026-07-05 — 🏆 Sprint 81: SMP Foundation + GPU Improvements

### Added (neural-kernel)
- **`smp/spsc.rs`** — SPSC (Single Producer Single Consumer) queue lock-free para comunicação entre cores. Baseado em MPMC de Dmitry Vyukov, simplificado para SPSC. Capacidade potência de 2, atomic head/tail.
- **`smp/mod.rs`** — Adiciona módulo `spsc` ao SMP.
- **`interrupts.rs`** — IPI handlers para SMP: `ipi_reschedule_handler` (vetor 0x80), `ipi_halt_handler` (vetor 0x81), `ipi_call_function_handler` (vetor 0x82). Contadores globais `IPI_RESCHEDULE`, `IPI_HALT`, `IPI_CALL_FUNCTION`.
- **`apic.rs`** — Funções de envio de IPI: `send_ipi_reschedule()`, `send_ipi_halt()`, `send_ipi_call_function()`. Compatível xAPIC/x2APIC. Shorthand=all_excl_self (0x180000).
- **`gpu/intel.rs`** — Infraestrutura para Intel GEN shader assembly: constantes `MEDIA_OBJECT`, `PIPELINE_SELECT`, `STATE_BASE_ADDRESS`. Campos `shader_pa` e `shader_loaded` em `IntelRing`. Funções `load_gen_matmul_shader()` e `execute_gen_shader()` (stubs preparados para shader real NDA Intel).
- **`gpu/backend.rs`** — Separa BCS Blitter do RCS ring: `GpuAccel::Intel(IntelRing, Option<BcsRing>)`. `init_backend()` inicializa BCS ring se disponível. `gpu_status()` reflete estado RCS+BCS. `gpu_matmul()` usa `as_mut()` para ring mutável.

### Changed
- **`gpu/backend.rs`** — `gpu_matmul()` agora usa `as_mut()` para permitir mutação do ring durante matmul.

### Tasks Completadas
- B-05: Integrar GPU no boot (já existia em main.rs)
- B-07: Implementar GTT setup para Intel GPU (já existia em intel.rs)
- B-02: Implementar Intel GEN shader assembly para matmul (infraestrutura + stub)
- B-08: Separar BCS Blitter do RCS ring
- B-09: Implementar VRAM Free List (já existia em vram.rs)
- B-10: Implementar driver e1000/r8169 para NIC real (e1000 já existia)
- B-14: Implementar WASM Sandbox (já existia em wasm.rs)
- B-15: Implementar GGUF Model Swap (já existia em gguf.rs)
- Bloco 21a: SMP Foundation (SPSC + IPI + PerCpu)

### Notes
- GEN assembly é NDA da Intel. `load_gen_matmul_shader()` aloca 1 página e escreve NOOPs como stub. Shader real requer engenharia reversa do i915 driver ou assembler externo.
- IPI handlers configurados nos vetores 0x80-0x82. PerCpu já existia com GS.base e `cpu_id()`.
- SPSC queue usa atomic head/tail com memory ordering Acquire/Release.

## [0.80.0] — 2026-07-05 — 🏆 Sprint 80: AVX2 Debug + WHPX Detection + Forward Pass

### Added (neural-kernel)
- **`bitnet_avx2.rs`** — `unpack_row_into()`: descompacta 1 linha de PackedTernaryTensor em buffer i8 reutilizável (n bytes em vez de k*n). WHPX detection via CPUID leaf 0x40000000: vendor "Microsoft Hv" → `avx2_available()` retorna false. `avx2_ternary_matmul_impl()` reescrito: row buffer + acumulação direta.
- **`tensor.rs`** — `has_avx2()` com WHPX detection (hypervisor bit + vendor string check).
- **`cortex.rs`** — Per-layer timing: `[FWD] L0 qkv:... attn:... proj:... ffn_gateup:... down:... total:...`. Unembed timing. `generate_speculative()` limitado a 8 tokens (antes 64).
- **`agents.rs`** — Timing em `generate_via_model()`: `[CORTEX-LLM] generate_via_model took X ticks (~Ys)`.

### Fixed
- **AVX2 `matmul_hybrid` para TernaryTensor (Q,K,V,O):** era scalar puro — agora usa AVX2 dispatch quando disponível (`matmul_hybrid_avx2`).
- **Tail handling AVX2:** K/V têm n=100 (não múltiplo de 8). `matmul_hybrid_avx2` e `matmul_avx2_inner` processam blocos de 8 com AVX2 e colunas restantes com scalar.
- **`avx2_ternary_matmul_impl`:** Revertido broadcast-per-t (correto para matmul) — outer product com `step_by(8)` estava incorreto.
- **Removido gate `m >= 4`:** tokens únicos (m=1) agora usam AVX2 via `tensor.matmul()`.
- **`unpack_all()` removido:** substituído por `unpack_row_into()` que aloca apenas n bytes (6.9 KB) em vez de k*n bytes (17.7 MB) por matmul.

### Performance (WHPX, 2.4B model, seq_len=64)

| Modo | ticks/layer | tempo/layer | 30 layers |
|---|---|---|---|
| AVX2 (VEX emulado) | ~4443 | ~4.4s | ~132s |
| **Scalar puro** | **~2218** | **~2.2s** | **~66s** |

### Lessons
- **WHPX + AVX2 = pior performance:** WHPX emula cada VEX instruction como VM exit. Scalar GP instructions rodam nativos. AVX2 sob WHPX é 2x MAIS LENTO que scalar.
- **`has_avx2()` detection:** CPUID leaf 0x40000000 vendor string "Microsoft Hv" identifica WHPX. Hypervisor presente check (CPUID leaf 1, ECX bit 31) como pré-requisito.
- **`unpack_all` não era o gargalo:** 17.7 MB allocation é barata comparada a 17M operações de bit unpacking + matmul.
- **Forward pass BitNet b1.58 sob WHPX:** ~60s para 64 tokens × 30 layers. Inviável para autogeneração sem KV cache ou bare metal.

### Known Issues
- Forward pass BitNet b1.58 sob WHPX: ~2.2s/layer = ~60s/forward pass. Generate 8 tokens: ~6h.
- Solução: KV cache + bare metal ou QEMU+KVM.

## [0.79.2] — 2026-07-05 — 🐛 Xuvisco v2: VGA Sequencer Screen Off (0x3C4/0x3C5)

### Fixed (neural-kernel)
- **`vga_buffer.rs`** — `clear_physical_buffer()` substituída por `disable_vga_plane()` que usa o sequenciador VGA (porta 0x3C4/0x3C5) para setar bit 5 (Screen Off) do Clocking Mode Register. Não acessa CRTC (0x3D4/0x3D5) nem memória 0xB8000.
- **`main.rs`** — Chama `vga_buffer::disable_vga_plane()` em vez de `clear_physical_buffer()`.

### Root Cause (v0.79.1 regression)
`clear_physical_buffer()` escrevia em 0xB8000 via `write_bytes`, mas o bootloader (UEFI/OVMF) não mapeia o legacy VGA hole 0xA0000-0xBFFFF no memory map. Escrever em 0xB8000 causa page fault antes da IDT ser inicializada (main.rs linha 454) → triple fault → reset → xuvisco.

### Lesson
"VGA text buffer" não está magicamente mapeado em todo hardware. UEFI/OVMF não inclui a VGA hole no mapa de páginas. I/O ports (0x3C4/0x3C5) são a única forma segura de desligar o VGA plane antes da IDT.

## [0.79.1] — 2026-07-05 — 🐛 Display Xuvisco Fix (VGA buffer + framebuffer clear)

### Fixed (neural-kernel)
- **`display/fb.rs`** — Framebuffer é limpo para preto imediatamente após `probe_uefi_framebuffer()`, eliminando artefatos do bootloader na tela.
- **`vga_buffer.rs`** — Nova função `clear_physical_buffer()` que limpa 0xB8000 (4000 bytes) via `write_bytes` sem acessar registros CRTC (0x3D4/0x3D5). Segura para Intel 6xx com UEFI GOP.
- **`main.rs`** — `vga_buffer::clear_physical_buffer(pm_offset)` chamado quando framebuffer presente, antes de qualquer mensagem de boot.

### Root Cause
`[BOOT] FB ativo — VGA text mode desligado` nunca executava VGA disable real: `hide_cursor()` e `clear_vga_buffer()` estavam definidos mas nunca chamados (orfãos desde Sprint 71). VGA text overlay e framebuffer sujo coexistiam, causando xuvisco em QEMU e hardware real.

## [0.79.0] — 2026-07-04 — 🏆 Sprint 79: LLM Infrastructure (BitNet-b1.58 Integration)

### Added (neural-kernel)
- **`bitnet_avx2.rs`** — AVX2 ternary matmul kernel (`ternary_matmul()`) with scalar fallback. Unpacks 2-bit packed ternary → `_mm256_cvtepi8_epi32` → `_mm256_cvtepi32_ps` → FMA. Called by `PackedTernaryTensor::matmul_hybrid()`.
- **`trinity.rs`** — `TrinityRouter` MoE stub. `register_expert()` adds named experts; `classify_intent()` rule-based dispatch across 5 classes (code/hw/chat/file/system). Real ML router deferred.
- **`bpe.rs`** — `BpeTokenizer` with JSON parser for HuggingFace `tokenizer.json`. `encode()`/`decode()`/`init_from_json()` global functions. Subword tokenization with BPE merge rules.

### Changed (neural-kernel)
- **`cortex.rs`** — `vocab_size` migrated from `u16` to `u32` (supports vocab 128K). `load_model()` v2: initializes BPE tokenizer automatically via `bpe::init_from_json()`. `TransformerModel` with dynamic `hidden`, `num_layers`, `max_seq`, `vocab_size: u32`. `LayerWeights.rms_attn/rms_ffn` as `Vec<f32>` (vectorial RMSNorm). `generate_speculative()` uses `model.max_seq` and BPE when loaded.
- **`gguf.rs`** — `vocab_size()` returns `u32`. Field `vocab_size` as `u32` in constructor.
- **`tensor.rs`** — Removed inline scalar matmul fallback; `matmul_hybrid()` delegates exclusively to `bitnet_avx2::ternary_matmul()`.
- **`main.rs`** — `mod bitnet_avx2`, `mod trinity`. Ramdisk loading section: checks `boot_info.ramdisk_addr` for bootloader ramdisk. QEMU loader fallback: probes physical address `0x100000000` (4GB) for `.bitnet` magic. Maps up to 1.5GB and calls `load_model()` if found.

### Changed (tools)
- **`download_bitnet.py`** — Header `.bitnet` v2 fixed: `vocab_size` as u32 (u16 overflowed at 128K). `ffn_dim` field added. `tok_type`/`tok_len` for BPE tokenizer embedding. BPE `tokenizer.json` extracted alongside `.bitnet`.
- **`build_image.py`** — Simplified: removed LBA append logic (provisional). Changed bootloader dependency to `default-features=false, features=["bios"]` to avoid UEFI compile error.

### Model
- Downloaded `microsoft/BitNet-b1.58-2B-4T` (real: 850M params). Converted to `.bitnet` v2: 1,464 MB, magic `0xBE11BE11`. Architecture: hidden=2560, layers=30, heads=20 (GQA=5 KV), vocab=128256, intermediate=6912.
- `micro.bitnet` (71KB) synthetic model preserved as fallback.

### Fixed
- `#[allow(dead_code)]` policy confirmed for production code (399 expected warnings)
- `mod shell` remains dead with `@dead` annotation (prevents accidental revival)

### Known Issues
- Forward pass broken for BitNet b1.58: GQA (20→5 KV heads) + BitFFN grouped down_proj (640→6912) not supported by standard FFN path. Sprint 80 needed.
- QEMU loader requires 6GB RAM + WHPX. 2GB fails (model at 512MB conflicts with boot allocator).
- Ramdisk via bootloader impossible for 1.46GB (FAT partition ~64MB). QEMU loader at 4GB is workaround.

## [0.78.1] — 2026-07-04 — 🧹 Code Review: Dead Modules Audit

### Added
- `#![allow(dead_code)]` + `@dead` annotations em 8 módulos mortos (shell, voice_skill, bench, verify, orchestrator, tracer, skill_market, hal) — cada um documentado com motivo e sprint futuro alvo
- Seção "DEAD MODULES" em `main.rs` com tabela de referência para IA devs

### Changed
- 36 warnings eliminados (426→390) nos 8 módulos anotados
- Política confirmada: `#[allow(dead_code)]` só em módulos mortos conhecidos; código em desenvolvimento mantém warnings como esperado

## [0.78.0] — 2026-07-04 — 🏆 Sprint 78: Agentic Evolution

### Added (neural-kernel)
- **IntentCache wiring** (`agents.rs`): HermesAgent now instantiates `IntentCache`, checks cache before `parse_command()`, and caches results. LRU with 64-entry limit.
- **OutputCache wiring** (`agents.rs`): `execute_skill()` checks `OutputCache` before calling `SkillRegistry::execute_skill_unchecked()`. Skills marked `idempotent: true` have outputs cached.
- **WorkflowEngine wiring** (`agents.rs`): HermesAgent field + tick loop checks `is_active()` and advances phases. Started for Chat→LLM commands and advanced on LLM response.
- **SelfCritique** (`hermes.rs`): `SelfCritique::evaluate()` e `SelfCritique::check_command()` — verifica output vazio, erros, placeholders, respostas curtas.
- **GgufBackedModel** (`gguf.rs`): Implementa `cortex::Model` trait. Converte pesos GGUF (FP32/Q4_0) para `TransformerModel` via `try_build_transformer()`. Suporta busca de tensores por nome (blk.N.attn_q, ffn_gate etc.).
- **FsBridgeAgent** (`agents.rs`): Agente `PollEvery(500)` que escaneia `MHI_REGISTRY` por alocações candidatas à promoção (access_count > 5, idle < 500) e executa migração HDD→DRAM via VFS.
- **WasmExecutor** (`wasm.rs`): Interpretador stack-based com suporte a i32.const/add/sub/mul/eqz/load/store, block/loop/if/else/end, br/br_if, call, select, memory.size/grow. 35+ opcodes WASM.
- **WasmSkill** (`wasm.rs`): Implementa `Skill` trait. `verify()` parseia bytecode, `execute()` carrega e executa função exportada (main/_start). WASI stub para bridge futura.
- **register_wasm_skill()** (`wasm.rs`): Registra uma skill WASM no SkillRegistry a partir de bytecode.

### Added (agent-core)
- **AgentTier** (`lib.rs`): Enum com `Permanent/System/User/Periodic/Learning`, cada um com `priority()`.
- **AgentInstance.tier**: Campo `tier: AgentTier` default `Permanent`.
- **migrate_to_tier()**: `AgentRegistry::migrate_to_tier(idx, new_tier)` e `migrate_to_tier_by_name(name, new_tier)`.
- **agents_by_tier()**: Filtra agentes por tier.

### Fixed
- **execute_skill** borrow fix: Mudança de `&self` para `&mut self` para permitir cache writes. Clona skill_names antes de chamar.

### Sources
- Sprint 78 plan (8 items: Flow/Crew, Cache, Workflow, StateGraph, Tier, MHI-FS, GGUF, WASM)
- v0.72.0 base (Crew, FlowTrigger, StateGraph, IntentCache, OutputCache, WorkflowEngine, GGUF parser)

---

## [0.77.0] — 2026-07-04 — 🏆 Sprint 77: Foundation Quick Wins

### Added (skill-registry)
- **Skill::verify()** (`skill.rs`): Pre-flight verification trait method. Skills podem checar precondições antes de executar. `SystemStatusSkill` verifica MHI, `HardwareInfoSkill` verifica SystemArchitecture.
- **CompletionContract** (`contract.rs`): `CONTRACT_NONEMPTY` e `CONTRACT_UTF8` com validação pós-execução. Suporta `WarnOnly`, `RejectOutput`, `RetrySkill`.
- **TaskSchema** (`task.rs`): `TaskSchema`, `JobPreconditions`, `TaskStatus` — tipos para schema de tarefas estruturadas com precondições, timeout, retries.
- **DynamicSkill** (`dynskill.rs`): Skill registrável em runtime via `/learn`. Implementa `Skill` trait diretamente, sem LLM.
- **FanOutPool** (`fanout.rs`): Pool de sub-tarefas assíncronas. `spawn()`/`poll_all()`/`take_result()`. Sub-tasks como `Box<dyn FnOnce + Send>`.
- **SkillIndex.find()** (`index.rs`): Busca textual por nome/desc/capabilities.
- **McpCatalog** (`index.rs`): Catálogo público de skills com `search()`, `register()`, `CatalogEntry`.

### Changed
- `McpManifest` ganha campo `contracts: Vec<&'static CompletionContract>`
- `SkillRegistry::execute_skill()` e `execute_skill_unchecked()` chamam `verify()` + contratos pós-exec
- Todas as 7 implementações de `Skill` ganham `verify()` e `contracts: Vec::new()`
- `Command::Learn` separado de `Command::AddSkill` — registro direto sem LLM

### Fixed
- **VirtualBox SMP**: Novo `AP_COUNT` static lido do MADT `lapic_count`. Se 0 APs, `init_smp()` retorna sem INIT-SIPI-SIPI. 2 vCPUs no VBox agora bootam confiavelmente.

### Added (neural-kernel/hermes)
- **60.1b**: Prompt `>` interativo — `show_prompt` default `true` no NeuralConsole
- **67.2.1**: `/learn <nome> <desc>` cria `DynamicSkill` + registra em SkillRegistry + SkillLoader
- **72.6**: `McpCatalog` populado via `SkillRegistry.list_skills()`

### Deprecated
- N/A

### Sources
- Sprint 60, 67, 72 plans
- ADR-0036 (JARVIS)

---

## [0.72.0] — 2026-07-02 — 🏆 Evolução Agêntica: Crew + FlowTrigger + StateGraph

### Added (agent-core)
- **Crew** (`crew.rs`): `Crew`, `CrewPool`, `ScheduledTask`, `OutputSchema`, `ProcessType` (Sequential/Hierarchical). Times de agentes com objetivo comum, tasks com dependências, kickoff/delegation pattern (CrewAI-inspired).
- **FlowTrigger** (`flow.rs`): `FlowTrigger::Start/Listen/Router` — quando e como um agente acorda. `RouterRegistry` para roteamento baseado em payload do EventBus. `should_poll_flow()` substitui `match schedule` no scheduler.
- **StateGraph** (`state_graph.rs`): `StateGraph` com nós (agentes) e arestas (condições de transição). Substitui scheduler round-robin por grafo de estados (LangGraph-inspired).
- **CrewManifest**: Extensão opcional do `AgentManifest` com `role`, `goal`, `backstory`, `flow`, `crew_id`. Sem modificar o struct original (evita quebrar 24+ const definitions).
- **CrewAgent trait**: Agente que implementa role semantics.
- **AgentRegistry**: `create_crew()`, `assign_to_crew()`, `init_graph()` — integração com CrewPool + StateGraph.

### Changed
- `AgentInstance` ganha campo `crew: CrewManifest`
- `AgentRegistry::run()` suporta FlowTrigger e StateGraph como alternativas ao round-robin

### Added (skill-registry)
- **OutputSchema** (`mcp.rs`): enum `Any, String, Json(Vec<String>)` com validação de output de skills. `McpManifest` ganha `preconditions` (caminhos VFS para contexto), `context_links` (skills relacionadas), `output_schema`, `idempotent` (cacheável).
- **OutputCache** (`cache.rs`): Cache de outputs de skills idempotentes com hash(input) e TTL. Evita re-execução de `system_status`, `echo`, etc. Suporta `get()`, `set()`, `evict_expired()`.
- **SkillIndex** (`index.rs`): Catálogo de skills por domínio (`by_domain`) e capacidade (`by_capability`). `relevant(capabilities)` para progressive disclosure via Hermes.

### Added (neural-kernel/hermes)
- **IntentCache** (`hermes.rs`): Cacheia intents (hash do input → Command) com TTL de 1000 ticks. HermesAgent consulta antes de chamar `cortex.think()`. Evita re-classificação de comandos repetidos.
- **WorkflowEngine** (`hermes.rs`): Máquina de estados THINK→PLAN→EXECUTE→VERIFY→REFINE→DONE. Suporta retry com `max_retries`. Usado por HermesAgent para workflows multi-passo.

### Changed
- Todas as 6 implementações de `Skill` atualizadas para os novos campos `McpManifest`.
- `Command` agora é `Clone` (necessário para IntentCache).

### Sources
- CrewAI (link 5): TaskSchema + OutputSchema
- AI Memory Vault (link 8): JobPreconditions, context_links
- Hermes Agent 10x (link 10): IntentCache, OutputCache, WorkflowEngine
- MCP Catalog (link 4): SkillIndex por domínio/capacidade

## [0.71.1] — 2026-07-02 — 🏆 Xuvisco exterminado: 3 bugs em cascata corrigidos

### Fixed
- **Xuvisco (causa raiz #3 — framebuffer race)**: `_print()` agora verifica se o compositor está ativo antes de chamar `fb_write_text()`. Quando o DisplayAgent está rodando, apenas o compositor escreve no framebuffer via `DoubleBuffer::swap()`. Elimina a briga entre `println!` (texto amarelo) e o compositor (tela completa) que causava flicker e sobreposição.
- **VGA text mode totalmente desligado com framebuffer**: Quando o framebuffer da UEFI está disponível, `vga_buffer::init()` NÃO é chamado. Zero escritas em 0xB8000, zero toques nos registros VGA CRTC (0x3D4/0x3D5). A camada de texto VGA não é mais ativada.
- **`_print()` seguro sem Writer**: `write_fmt()` usa `let _ = ...` em vez de `.unwrap()` para evitar panic se o Writer VGA não foi inicializado.

### Changed
- **`vga_buffer::_print()`**: Só chama `fb_print()` quando o compositor NÃO está ativo. Com compositor ativo, todo output de tela passa pelo DisplayAgent.
- **`main.rs`**: `vga_buffer::init()` condicional — só executado se não há framebuffer.

## [0.71.0] — 2026-07-02 — 🏆 Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 Log + Xuvisco Fix

### Fixed
- **Xuvisco (VGA CRTC corruption em Intel 6xx)**: `probe_uefi_framebuffer()` movido para ANTES do VGA text mode init. `println!` não escreve mais nos registros VGA CRTC (0x3D4/0x3D5) em modo UEFI GOP, eliminando a corrupção do display no boot.
- **FAT12 log não era gravado**: `boot_logger.rs` só aceitava FAT32 (type_code 0x0B/0x0C). Adicionado suporte a FAT12 (type_code 0x01). `write_boot_log()` agora usa `Fat12Writer` para FAT12, `Fat32Writer` para FAT32.
- **BootLogAgent ignorava FAT12**: `read_last_boot_log()` só procurava B<TICK>.LOG em FAT32. Agora lê BOOT.LOG de partições FAT12 também.
- **fb_write_text sem bounds check**: Adicionada verificação de limite do buffer para evitar escrita fora do framebuffer.
- **fb_write_text division by zero**: `max_lines == 0` tratado para evitar panic em resoluções muito baixas.
- **fb_write_text LINE wrap**: `static mut LINE` agora incrementa corretamente sem pular a linha 0.

### Changed
- **Boot vira sequência de agentes**: `BOOT_PHASE` events publicados no EventBus (SafeHarbor→MemoryCore→SystemBringup→Diagnostics→HardwareDiscovery→DriverInit→AgentFleet→Runtime). HermesAgent, CortexAgent e BootLogAgent podem subscrever.
- **90+ linhas de teste inline → DiagnosticSkill**: Box/Vec/Tensor/SiLU/RMSNorm/BitNet movidos para `DiagnosticSkill` em `agents.rs`. SystemAgent executa durante fase Diagnostics.
- **CortexAgent acorda antes do HW discovery**: Modelo LLM carregado e agente instanciado antes do PCI scan, RTL8139, ATA, xHCI. O sistema nervoso participa das decisões de hardware.
- **BootLogAgent agora contínuo**: `auto_start=true`, `persist=true`, `ScheduleKind::Continuous`. Monitora boot logs em tempo real.
- **`Fat12Writer::root_lba()` e `data_lba()` agora pub**: Para BootLogAgent acessar a geometria da partição.
- **`display/fb.rs`**: Removido VGA FIX que modificava stride do framebuffer (causava mismatch). Stride original da UEFI é preservado.
- **`vga_buffer.rs`**: `fb_write_text()` com bounds check e divisão por zero tratada.

### Added
- **`BootPhase` enum + `publish_boot_phase()`**: 8 fases de boot com eventos no EventBus.
- **`DiagnosticSkill`**: Skill de diagnóstico que substitui os testes inline. SystemAgent executa na fase Diagnostics.
- **`TOPIC_BOOT_PHASE`**: Constante do tópico EventBus para fases de boot.

## [0.65.0] — 2026-06-30 — COSMIC UI Patterns + AxiomOS Verifier + HAL + Bench

### Added
- **Workspace manager** (COSMIC): `display/workspace.rs` — 3 workspaces, LayoutMode (Floating/Tiled/Grid/Maximized)
- **Notification overlay** (COSMIC): `display/notifications.rs` — temporárias, 3 severidades, expire
- **Auto-tiling layout** (COSMIC): `display/layout.rs` — Tile, Grid, Maximize, Floating
- **Skill verifier** (AxiomOS): `verify.rs` — eBPF-style opcodes, verify_skill(), execute_verified()
- **HAL trait** (AxiomOS): `hal.rs` — `trait Architecture` + impl X86_64
- **Benchmark framework** (AxiomOS): `bench.rs` — start/end_bench, alloc_throughput

## [0.64.0] — 2026-06-30 — Voice skill + Gbrain reranker + BrowserAgent

### Added
- **Voice skill**: `voice_skill.rs` — speak(text, profile), 8 preset voices, display fallback
- **Gbrain reranker**: `kgraph.rs` — `ranked_query()` combina label match + edge scores
- **BrowserAgent**: `browser_agent.rs` — fetch_page, extract_text (HTML tag-stripper), PageViewerApp, cache
- PageViewerApp: janela no compositor que mostra conteúdo de páginas web

## [0.63.1] — 2026-06-30 — MegaTrain patterns + Self-skill generation

### Added
- **MegaTrain streaming**: `mhi.rs` — MEGATRAIN_QUEUE, enqueue_prefetch(), megatrain_tick()
- **Self-skill generation**: `skill_gen.rs` — TaskPattern registry, generate_skill(), auto após 3 usos

## [0.63.0] — 2026-06-30 — Cortex Evolution + PTRM + Kanerva + Anatomy

### Added
- **Model trait**: `cortex.rs` — `pub trait Model: Send`, `set_model()`, `generate_via_model()`
- **PTRM**: `cortex.rs` — `gaussian_noise()`, `ptrm_generate()`, Q-head, 3 trajetórias
- **Kanerva Memory**: `kanerva.rs` — sparse_read, distributed_write, bayesian_update, hamming_distance
- **Hard blocklist**: `safety.rs` — 12 comandos que NUNCA rodam, check_command()
- **Curated memory**: `conversation.rs` — curated_context() com budget 4KB

## [0.61.0] — 2026-06-30 — Sprint 61 Desktop completo (7/7 sub-sprints)

### Added
- **MouseAgent (61.0)**: PS/2 mouse driver como agente A-021. IRQ12 handler, pacote 3 bytes, EventBus MOUSE_MOVED/MOUSE_CLICK/MOUSE_DRAG. 5 skills. ~200 LOC.
- **Theme Engine (61.1)**: 5 temas (hermes-dark, dracula, matrix, solarized, hermes-light). Hot-swap via `theme.apply()`. Integrado ao console. ~120 LOC.
- **Compositor (61.2)**: Multi-window com z-order, dock bar 36px com botoes + relogio, drag de janelas via title bar, cursor cross. Subscreve MouseAgent events. ~300 LOC.
- **Shell (61.3)**: 15 comandos (help, echo, clear, uptime, ps, meminfo, pci, theme, profile, shutdown, reboot, date, uname, cpuinfo, ls). ~100 LOC.
- **3 Desktop Apps (61.4)**: Hermes App (chat+shell), Settings App (theme+profile picker), Power App (shutdown+reboot+confirmacao). AppRegistry estatico. ~250 LOC.
- **LLM Icons (61.5)**: IconCache com fallback geometrico por hint hash. Render 16×16 (2-bit palette). ~80 LOC.
- **WASM Sandbox (61.6)**: WasmSandbox com load/execute stub, scan_exports. Preparado para wasmi. ~80 LOC.

### Fixed
- **Status bar height**: `fill_rect` usava `status_y + ch + 2` (22px) em vez de `ch + 3` (19px) — invadia area de conversa
- **Prompt height**: `fill_rect` usava `prompt_y + ch + 1` (737px!) em vez de `ch + 3` (19px)
- **conv_y**: realinhado para comecar logo apos a status bar (sem overlap)
- ConsoleAgent: removeu `println!` (VGA) — display framebuffer e suficiente
- DisplayAgent: filtro de mensagens — apenas `[Hermes]`, `Hermes v`, `>`, `/` aparecem

## [0.62.2] — 2026-06-30 — InferenceFS, HermesFS, RamFS, MHI Scheduler

### Added
- **InferenceFsAgent**: `/inference/` — arquivos gerados sob demanda via LLM, buffer de treino
- **HermesFsAgent**: `/chat/` — send (write→LLM), last_response, history, clear, count
- **RamFsAgent**: `/mnt/ram/` — cache DRAM com quota 1MB, LRU eviction
- **MhiScheduler**: scan MHI_REGISTRY a cada 1000 ticks, promove/demove tiers por acesso
- MhiScheduler integrado ao OptimizerAgent.tick()
- AllocTier::UsbMsc adicionado ao mhi.rs

## [0.62.1] — 2026-06-30 — Storage Agents: Ata, DevFs, ProcFs

### Added
- **AtaAgent**: `/mnt/hdd/sda` — ATA block device como arquivo
- **DevFsAgent**: `/dev/pci/list`, `/dev/pci/<vid:did>`, `/dev/rtl8139`, `/dev/xhci`, `/dev/mem`
- **ProcFsAgent**: `/proc/agents`, `/proc/meminfo`, `/proc/uptime`, `/proc/cpuinfo`, `/proc/version`, `/proc/profile`, `/proc/mhi`
- **FilesystemAgent trait**: `read()`, `write()`, `list()`, `mount_point()` — interface padrao para FS agents
- **VFS bridge**: `read_vfs()`, `write_vfs()`, `list_vfs()` — resolve mount e delega ao agente
- VFS init + 8 mounts no boot: `/`, `/mnt/ram`, `/mnt/hdd`, `/mnt/sdhc`, `/chat`, `/dev`, `/proc`, `/system`, `/inference`

## [0.62.0] — 2026-06-30 — VFS Layer + MHI ARC-style Tier Suggestion

### Added
- **VFS Layer**: `VfsRegistry` (mount, resolve, lookup, list_dir), `VfsNode` (arvore de diretorios), `VfsMount`
- **Path utils**: `canonicalize()`, `split()`, `join()`, `filename()`, `parent()`, `match_mount()`
- **MHI ARC-style**: `arc_suggest_tier()` — ZFS-ARC-inspired (MFU→Dram, MRU→Nvme, cold→Hdd)
- **AllocTier::UsbMsc**: novo tier para USB Mass Storage
- Sprint plan atualizado: `docs/sprint-062-fs.md` com MHI+VFS+StorageAgents unificado

## [0.60.5] — 2026-06-30 — RTL8139 early init 32KB RX

### Fixed
- RTL8139 init movido para kernel_main (antes da fragmentacao do frame allocator)
- `alloc_pages(8)` para RX buffer de 32KB contiguo
- `init_driver_rtl8139()` idempotente (chamado 2x: boot + NetDriverAgent)

## [0.60.4] — 2026-06-30 — RTL8139 TX + iPXE buffer sync

### Fixed
- **TSD_SIZE_SHIFT 16→0**: SIZE nos bits 0-12 (correto). TX funcionando com TOK=1
- **iPXE RX buffer**: `rx_offset = CAPR` apos init — pula dados do bootloader
- **smoltcp tight poll**: loop `poll_delay()` para DHCP multi-step
- **IP estatico imediato**: 10.0.2.15/24 no tick 11 (bypass DHCP)

### Added
- Plano Desktop: `docs/sprint-061-desktop.md` (6 sub-sprints, ~2800 LOC)
- Plano FS: `docs/sprint-062-fs.md` (6 sub-sprints, ~2700 LOC)
- Plano WWW: `docs/sprint-063-www.md` (7 sub-sprints, ~2600 LOC)

## [0.60.3] — 2026-06-30 — e1000 TX non-blocking + mmio_virt + map_page_uc

### Fixed
- **e1000 Page Fault**: `map_page_uc()` mapeia PCI MMIO (cria page table entries)
- **e1000 TX non-blocking**: TDT=(idx+1)%64, sem wait loop (QEMU TCG nao processa TX while spinning)


### Added
- **Ecosystem Batch 3 (12 repos, 8 arquivos, 601 LOC)**:
  - redox-os/redox (16.4k★) → `scheme.rs`: SchemeHandler trait para namespace I/O
  - theseus-os/Theseus (3.2k★) → `state.rs`: TypedAgent<Boot|Running|Faulted|Done>
  - embassy-rs/embassy (9.5k★) → `timer_wheel.rs`: 64-slot TimerWheel
  - openai/swarm (21.8k★) → HermesAgent: Handoff enum (SwitchTo/Escalate/Delegate)
  - tock/tock (5.3k★) → `mmio.rs`: Register<T> + RegisterField<OFFSET,WIDTH>
  - raga-ai-hub/RagaAI-Catalyst (16k★) → `tracer.rs`: 256-span ring buffer
  - kyegomez/swarms (6.9k★) → `orchestrator.rs`: task decomposition
  - TransformerOptimus/SuperAGI (16k★) → `skill_market.rs`: SkillScore scoring table
- `cargo check --release`: 0 errors ✅

## [0.59.1] — 2026-06-29 — HW Agents + Native Agent Fleet

### Added
- **HW Agents**: `hw_agents.rs` — HwRegistry por PCI, HwAgent por dispositivo, `class_to_capabilities()`, `activate_for_intent()`
- **Especialistas nativos**: `agency.rs` — 12 divisões (engineering, design, product, qa, support, marketing, infra, data-science, creative, legal, spatial, research)
- **SpecialistAgent** struct genérica com missão, skills, entregável
- `register_agency_agents()` registra todos no boot

## [0.59.0] — 2026-06-29 — 🏆 Bootloader 0.11 + Framebuffer UEFI + Hermes Gráfico

### Added
- **Bootloader 0.11.15**: `bootloader_api` substitui `bootloader::bootinfo`, `BootloaderConfig` com `physical_memory=Dynamic`, stack 512KB
- **Framebuffer 1280×720**: `probe_uefi_framebuffer()` via `BootInfo::framebuffer`, BGR pixel suporte, stride em BYTES
- **Serial Fallback**: `Mutex<Option<SerialPort>>`, `probe_port()` em 4 endereços (0x3F8/0x2F8/0x3E8/0x2E8)
- `fb_print()` escreve no framebuffer quando serial ausente
- DisplayAgent renderiza NeuralConsole com framebuffer ativo
- `tools/build_image.py` via `bootloader::BiosBoot` + BIOS/UEFI modes

### Changed
- Branch `test-bootloader-0.11` promovida a `main` (force push)
- `kernel_stack_size=512KB` previne triple fault no stack probe
- `mov ss, 0` após init_idt() evita #GP no breakpoint handler
- `vga_buffer::_print()` pula VGA quando framebuffer ativo
- `.cargo/config.toml`: rustflags `[]` (sem relocation-model=static)

### Fixed
- #GP no breakpoint handler: SS não era recarregado após GDT
- Triple fault: stack 256KB → 512KB
- Serial detection: porta 0x3F8 falha em notebooks modernos → fallback 0x2F8/0x3E8/0x2E8

## [0.58.0] — 2026-06-28 — 🏆 Boot em Hardware Real + USB + FAT12 + ATA

### Added
- **🏆 Primeiro boot do Neural OS Hermes em notebook físico via SDHC USB** (2.7MB imagem, Rufus DD+MBR+CSM)
- **xHCI USB HID Keyboard Driver**: `init_xhci()`, `poll_keyboard()` com Event Ring, HID→PS/2 scancode (68 teclas), CAD via USB
- **MBR+FAT12 Partition Recognition (PERMANENTE)**: `fat.rs::read_mbr()`, `Fat12Writer::append_log()` via ATA PIO
- **FAT12 Boot Log Partition**: `tools/patch_image.py` adiciona 2MB FAT12, BOOT.LOG visível no Windows
- **ATA PIO Driver**: `AtaDriver::probe()` + `read_sectors()`/`write_sectors()` LBA28 com wait_bsy+wait_drq
- **Ctrl+Alt+Del Log Dump**: `handle_cad()` grava log no FAT12, reset 8042, hlt

### Fixed
- **OOM em HW real**: HEAP_SIZE 4MB→16MB, `serial_println!` sem alloc, `#[alloc_error_handler]` seguro
- **VGA Scrolling**: Cursor via portas 0x3D4/0x3D5, new_line() sempre na última linha

## [0.57.1] — 2026-06-27 — Consolidation: Plugin Hub + x2APIC + Ed25519 + SMP Stacks

### Added
- **Plugin Hub (#236)**: PluginManager trait + PluginRegistry
- **x2APIC**: ativado via `core::arch::x86_64::__cpuid()`, substitui APIC regs por MSR
- **Ed25519 real**: `ed25519-compact` crate substitui stub (trust_cache.rs)
- **SMP per-AP stacks 64KB**: cada AP tem stack isolado
- **VirtIO-GPU poll fix**: `sti;hlt` loop (evita VM exit no QEMU TCG)

## [0.57.0] — 2026-06-27 — Bloco 15+16+17: Memory + Ecosystem + LLM v2

### Added
- **MemoryTree v2**: TTL/Eviction, Ebbinghaus decay, 4-Tier consolidation (event-bus)
- **SHA-256 Dedup (#214)**: `dedup.rs` com content-based hash
- **Privacy Filter (#215)**: `privacy.rs` com regex patterns
- **Hybrid Search (#218)**: `hybrid_search.rs` (embedding + keyword)
- **Metacognitive Guard (#220)**: `metacognitive.rs` confidence threshold
- **Draft→Review→Merge (#221)**: `draft_review.rs` 3-phase write pipeline
- **Atkinson-Shiffrin 3-tier (#224)**: `atkinson.rs` Sensory→STM→LTM
- **SuperContext**: memory+KG scout (event-bus)
- **SkillIndex**: progressive disclosure (event-bus)
- **TokenJuice**: HTML strip + URL shorten (event-bus)
- **Sampling**: top-k, temperature (cortex.rs)
- **Codebook VQ (#165)**: quantize em tensor.rs
- `generate_speculative()` funcional (Medusa 3-head)

## [0.56.0] — 2026-06-26 — Medusa + Pipeline + Memory Tree + Knowledge Graph

### Added
- **Medusa 3-head speculative decoding** (cortex.rs)
- **Pipeline manifest** (agent-core): `Pipeline::new()` + `Sequence::linear()`
- **Memory Tree** (event-bus): `MemoryTree::insert()` + `recall()`
- **Knowledge Graph** (event-bus): `KnowledgeGraph`, `add_triple()`, `query()`
- **DAG scheduler** (agent-core): `DagScheduler` topological sort
- **Dashboard** (agent-core): DashboardPanel trait  
- **Ecosystem Analysis**: OpenMontage, OpenHuman, codebase-memory-mcp, Rinne, daily_stock, ComPilot

### Added
- **CDC Rabin Chunking** (`chunker.rs`) — Content-Defined Chunking via rolling hash polinomial de 64 bits. Divide bitmaps e buffers em chunks de tamanho variável baseado no conteúdo. `chunk_data()` → `merge_chunks()` round-trip testado.
- **XOR Delta** (`delta.rs`) — `ArchiveTensor` com reconstrução bit-exata via XOR residual entre versões de `PackedTernaryTensor`. `ArchiveTensor::new()` + `reconstruct()` com round-trip testado.
- **Semantic Snapshot** (`self_heal.rs`) — `SelfHeal::semantic_snapshot(prev_bitmap)` aplica CDC Rabin + XOR delta no bitmap do alocador. Armazena apenas chunks modificados entre checkpoints.
- **IrqSafeLock** (`sync/irq_lock.rs`) — FIFO lock com `cli` na aquisição e restauração de RFLAGS.IF no drop. Previne deadlock em handlers de interrupção.
- **DmaBuf** (`dma.rs`) — `dma_alloc(size)` retorna `DmaBuf { phys, virt, size }` com páginas marcadas `NO_CACHE | WRITE_THROUGH`. Previne corrupção por cache incoerente CPU↔DMA.
- **Watchdog** — `AgentInstance::consecutive_pending`. Se agente retorna Pending por 10000+ ticks seguidos, scheduler força estado `Crashed`. Prevê loop infinito.

### Changed
- `SKILL_REGISTRY`, `TRUST_CACHE`, `EVENT_LOG`, `USAGE_TRACKER`, `CONVERSATION_TRACKER`, `SKILL_STORAGE` migrados de `spin::Mutex` para `ticket_lock::TicketLock` (FIFO, sem starvation).
- `SELF_HEAL`, `RESPAWN_QUEUE`, `PENDING_SKILL` migrados para `crate::sync::irq_lock::IrqSafeLock` (IRQ-safe).

### Removed
- Últimos vestígios de `spin::Mutex` em estruturas de contenção média/alta.

### Fixed
- Bug H3 (APIC SVR) — vetor espúrio redirecionado para 255.
- Bug H4 (IDT) — cobertura total 0-31 com 32 handlers nomeados.
- Bug H5 (PIC EOI) — EOI duplo no escravo (0xA0) para vetores >= 40.
- Bug H11 (PCI multi-function) — header_type bit 7 verificado.
- Bug H12 (IOAPIC mask) — RTEs não usadas mascaradas.

## [0.59.0] — 2026-06-29 — 🏆 Bootloader 0.11 + Framebuffer UEFI + Hermes Grafico 🏆

### Breaking: Bootloader 0.9.34 → 0.11.15
- **bootloader_api** substitui `bootloader::bootinfo::BootInfo`
- `BootloaderConfig` com `physical_memory = Dynamic` (substitui `map_physical_memory`)
- `kernel_stack_size = 512KB` (stack probe de 256KB exigido pelo kernel)
- Build via `tools/build_image.py` (cria imagem BIOS com `bootloader::BiosBoot`)
- Branch antiga `main-bootloader-0.9` mantida como referencia

### Added — Framebuffer UEFI (bootloader 0.11)
- `BootInfo::framebuffer` detectado em `probe_uefi_framebuffer()`
- GpuDevice ganhou `fb_bpp: u32` (bytes per pixel)
- `FramebufferInfo.bpp`: suporta BGR (3 bytes) e BGRA32 (4 bytes)
- Stride convertido de pixels para bytes (info.stride * bpp)
- `vga_buffer::_print()` pula escrita VGA quando framebuffer ativo
- DisplayAgent renderiza NeuralConsole no framebuffer 1280×720

### Fixed — #GP no breakpoint handler
- **Causa**: bootloader 0.11 usa GDT diferente → SS=0x10 = TSS selector
- **Fix**: `mov ss, ax` com seletor nulo (0) apos carregar GDT
- Sintoma: `[EXCEPTION] #GP ip=breakpoint_handler cs=0x8 err=0x10` no iretq

### Fixed — Triple fault silencioso
- **Causa**: kernel faz stack probe de 256KB, bootloader so alocava 128KB default
- **Fix**: `kernel_stack_size = 512 * 1024` no BootloaderConfig
- Sintoma: bootloader log mostra "Jumping to kernel entry point" mas nenhum output

### Aprendizados (Bootloader 0.11 vs 0.9.34)
1. **BootloaderConfig** obrigatorio — sem ele, physical_memory=None, stack=80KB
2. **Stack probe**: Rust gera codigo que testa N paginas de stack no entry point. Se o bootloader nao alocar suficiente → triple fault silencioso
3. **GDT/SS incompativel**: bootloader 0.11 usa GDT propria. Ao carregar nossa GDT, SS fica invalido → #GP no iretq
4. **Framebuffer stride**: bootloader 0.11 reporta stride em PIXELS, nao bytes. Multiplicar por bytes_per_pixel
5. **Pixel format BGR**: framebuffer UEFI usa 3 bytes/pixel (BGR), nao 4 (BGRA32). set_pixel precisa escrever so 3 bytes
6. **Build process**: bootimage tool v0.10 nao suporta bootloader 0.11. Precisa de build.rs ou script externo
7. **MinGW + caminho com acentos**: linker MinGW falha com caracteres especiais no path (Área de Trabalho). Solucao: mover projeto para C:\dev\

## [0.58.0] — 2026-06-28 — 🏆 MARCO: Boot em Hardware Real + USB Keyboard + FAT12 Log 🏆

### 🏆 MARCO HISTÓRICO: Neural OS Hermes boota em hardware real!

Pela primeira vez, o Neural OS Hermes bootou em um **notebook físico** (x86-64 real) via **SDHC USB**. O kernel saiu do QEMU e rodou em silício real. As conquistas:

- **Boot completo**: VGA text mode funcional, PCI/ACPI/APIC/SMP todos operacionais
- **Hermes Cognitive**: ReAct loop rodando estável (7 fases: OBSERVE→THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN)
- **Zero panics** após correção do OOM (heap 4MB→16MB)

### Added — xHCI USB HID Keyboard Driver (completo)
- **Driver HID Boot Protocol** completo: `init_xhci()` global + `poll_keyboard()` com Event Ring parsing
- **Tabela HID→PS/2**: 68 teclas mapeadas (A-Z, 0-9, símbolos, ENTER, BACKSPACE, DELETE)
- **CAD via USB**: detecta LCtrl + LAlt + Delete no HID report (byte 0 modifiers + byte 2 usage)
- **64KB de hastes de Ebbinghaus**: integrado com InputAgent (poll a cada 5 ticks)
- **Driver persistente**: XhciState global inicializado uma vez no boot, não recriado a cada poll

### Added — MBR + FAT12 Partition Recognition (PERMANENTE)
- **MBR parser** (`fat.rs::read_mbr()`): lê tabela de partições do setor 0 via ATA PIO
- **FAT12 BPB reader**: detecta qualquer partição FAT12 no disco
- **Fat12Writer**: `append_log()` escreve no arquivo BOOT.LOG via ATA read/write
- Reconhecimento de partições é **permanente** — o kernel sempre enxerga o layout do disco

### Added — FAT12 Boot Log Partition (temporário)
- **`tools/patch_image.py`**: script Python que adiciona partição FAT12 de 2MB ao final da bootimage
- **BOOT.LOG** visível no Windows Explorer após boot + CAD
- **Timestamps**: cada linha do log prefixada com `[T+SSS.mmm]` (segundos.millis desde o boot)
- **Buffer 64KB**: circular, sem alocação de heap, timestamp via aritmética u8

### Added — ATA PIO Driver completo
- **`AtaDriver`**: probe via PCI (class 0x01), `read_sectors()` + `write_sectors()` com wait_bsy/wait_drq
- Cache flush via comando 0xE7 após writes
- Fallback silencioso se nenhum controlador ATA presente

### Fixed — OOM em Hardware Real
- **HEAP_SIZE**: 4MB → **16MB** (4096 páginas mapeadas)
- **`serial_println!`**: removido `alloc::format!` — escreve direto no serial via `write_fmt`
- **Panic handler**: safe path sem alocação (`write!` direto para VGA/serial); tentative path com `try_alloc_check()`
- **`#[alloc_error_handler]`**: diagnostico OOM sem alocar memória
- **`LogBuf`**: implementação própria de `fmt::Write` em buffer stack de 256 bytes

### Fixed — VGA Scrolling em Hardware Real
- **Row tracking**: cursor real que incrementa a cada newline, scroll só quando atinge BUFFER_HEIGHT-1
- **`new_line()`**: agora sobe linhas corretamente sem truncar para a última linha

### Added — Ctrl+Alt+Del com log dump
- **Detecção**: PS/2 (IRQ1) + USB HID (LCtrl+LAlt+DEL)
- **Ação**: serial log dump + FAT12 ATA write + PS/2 8042 reset + hlt
- Log escrito no setor LBA 0 + partição FAT12

### Aprendizados (Hardware Real vs QEMU)
1. **OOM**: QEMU tolera heap 4MB; HW real precisa de 16MB. `alloc::format!` dentro de `serial_println!` causava OOM recursivo no panic handler.
2. **VGA buffer**: `write_byte` sempre escrevia na última linha (`BUFFER_HEIGHT-1`). Novo cursor real corrige scroll.
3. **PS/2 vs USB**: Notebooks modernos não têm controlador PS/2. Teclado USB só funciona via xHCI HID Boot Protocol.
4. **ATA vs USB storage**: Leitor de SDHC interno geralmente está em SATA/PCI. USB mass storage é mais complexo.
5. **FAT12 vs RAW**: Partição FAT12 é reconhecida pelo Windows Explorer imediatamente. RAW sector precisa de HxD/PowerShell.
6. **MBR signature 55AA**: Sempre verificar — bootloader pode ou não preservar o MBR original.

## [0.57.1] — 2026-06-27 — Consolidation: Plugin Hub + x2APIC + Ed25519 + SMP stacks

### Added
- **Plugin Hub** (#236) — `plugin_hub.rs`: install/remove/scan_risk/discover de plugins
  remotos com AI security scan (10-level risk scoring por nome de skill)
- **x2APIC ativado** — CPUID leaf 1 ECX[21] detecta suporte, MSR IA32_APIC_BASE[10]
  habilita modo MSR-based. Fallback MMIO se TCG nao suportar.
- **Ed25519 real** — `ed25519-compact` crate (2.3.1, no_std, sem SIMD) substitui stub.
  `verify_signature()` usa `PublicKey::from_slice` + `verify`. TRUSTED_PUBLIC_KEYS array.

### Fixed
- **SMP per-AP stacks**: cada AP agora tem stack de 64KB dedicada no heap,
  em vez de compartilhar topo do heap entre todos os cores. Previne corrupção de pilha.
- **x2APIC CPUID**: substitui inline asm com `out("ebx")` (conflito LLVM/MinGW)
  por `core::arch::x86_64::__cpuid()`. Compila em x86_64-unknown-none.

### Aprendizados
- `ed25519-compact` é no_std puro (sem SIMD, sem bindings C) — roda em qualquer target
- `core::arch::x86_64::__cpuid` retorna `CpuidResult` (não Result) — API infalível
- SMP precisa de stacks separadas por AP: 64KB × 4 cores = 256KB do heap
- Plugin Hub com risk scoring de skills cabe em ~200 LOC

## [0.57.0] — 2026-06-27 — Bloco 15+16+17: Memory Systems + Ecosystem + LLM v2 🧠🏁

### Added — Bloco 15: Memory Systems (completo)
- **MemoryTree v2** (`event-bus/memory_tree.rs`) — TTL/Eviction por nó, Ebbinghaus decay (`ebbinghaus_strength()`), 4-Tier Consolidation (`Working→Episodic→Semantic→Procedural`), promoção automática por access_count
- **SHA-256 Dedup** (`dedup.rs`) — FNV rolling hash, sliding window de 300 ticks, 64 entradas máximas
- **Privacy Filter** (`privacy.rs`) — 14 padrões de secrets (API_KEY, sk-, ghp_, password, bearer, etc), substitui por `[REDACTED]`
- **Hybrid Search** (`hybrid_search.rs`) — TF-score + MLP score fusion, RRF-style ranking, top-10
- **Metacognitive Guard** (`metacognitive.rs`) — Histórico de 64 erros, `check(skill, type)` retorna fix conhecido
- **Draft→Review→Merge** (`draft_review.rs`) — 5 estados (Draft→Review→Approved→Rejected→Merged), `pending()` para HermesAgent
- **Atkinson-Shiffrin 3-tier** (`atkinson.rs`) — Sensory register (48h TTL, 64 items) → STM (working memory tree) → LTM (semantic tree), `attend()` promove sensory→STM, `promote_to_ltm()` STM→LTM

### Added — Bloco 16: Ecosystem Integration
- **SuperContext** (`supercontext.rs`) — Integra MemoryTree + KG num scout unificado, `ingest()` registra agent→skill edges + memória
- **SkillIndex** (`skill_index.rs`) — Progressive disclosure: frontmatter-only scan, `scan(query)` retorna top-5 por domínio
- **TokenJuice** (`tokenjuice.rs`) — HTML tag stripping, URL shortening (>60 chars→`[URL]`), whitespace dedup

### Added — Bloco 17: Cortex LLM v2
- **Sampling** (`cortex.rs::sample()`) — `top_k` (nucleus filtering), `temperature` scaling, softmax normalização, deterministic fallback
- **Model update topic** — `MODEL_UPDATE` EventBus topic para hot-swap de pesos .bitnet via HTTP download
- **Codebook VQ** (`tensor.rs::CodebookVQ`) — 16-centroid treino por uniform sampling, compressão 4:1, decompress lossy

### Fixed
- `memory_tree.rs` — borrow checker em `consolidate_inner()` resolvido com escopo de leitura antes de mutação

### Aprendizados
- Bloco 15 (Memory Systems) foi o maior: ~450 LOC em 7 novos módulos
- MemoryTree com Ebbinghaus + 4-tier cabe em ~200 LOC no_std com safe borrows
- Atkinson-Shiffrin 3-tier复用 MemoryTree como base — STM e LTM são MemoryTree instances
- `select_nth_unstable_by` existe em no_std para sampling top-k
- Codebook VQ com 16 centroids dá ~4:1 compressão para tensores f32

## [0.56.0] — 2026-06-27 — Medusa Speculative Decoding + Pipeline + Memory Tree + KG 🚀

### Added — Medusa Speculative Decoding (cortex.rs)
- **3 Medusa heads**: cada head `PackedTernaryTensor(HIDDEN, VOCAB_SIZE)` prediz token futuro
- **`generate_speculative()`**: draft 3 tokens, verify em 1 forward pass, aceita prefixo
- **Ganho teórico**: até 4 tokens/forward pass quando heads treinadas (~2-3× em prática)
- **`forward_hidden()`**: retorna hidden state + logits (refatorado do forward())

### Added — Pipeline Manifest (agent-core/pipeline.rs)
- **Stage + Provider**: scored selection com fallback. Provider tem `score: u8` + `activate: fn() -> bool`
- **Pipeline runner**: executa stages em ordem, fallback automático se provider principal falha
- **Substitui boot sequence fixo** por pipeline declarativa

### Added — Memory Tree (event-bus/memory_tree.rs)
- **MemNode**: `{ summary, data, children, importance }` — chunks hierárquicos ≤512 bytes
- **Scout**: percorre árvore até depth N, retorna `(idx, summary, importance)` para contexto
- **Prune**: poda nós com importância < threshold, base para TTL/eviction
- **Base do Bloco 15 Memory Systems**: Atkinson-Shiffrin, Ebbinghaus decay, 4-tier consolidation

### Added — Knowledge Graph (event-bus/kgraph.rs)
- **KNode + KEdge**: nós (Agent/Skill/Hardware/Event) + arestas com relação nomeada
- **label_map**: índice por label para lookup O(1)
- **neighbors()**: consulta de vizinhança (source ou target)
- **query(relation)**: busca todas as arestas com relação específica
- **Base para correlação de eventos de segurança + trust graph**

### Added — DAG Scheduler (agent-core/dagsched.rs)
- **DagScheduler**: dependências nomeadas entre agentes/stages, topological sort
- **resolve()**: ordenação topológica com detecção de ciclos
- **run()**: executa agentes na ordem resolvida

### Added — Dashboard (agent-core/dashboard.rs)
- **Metric + Alert**: structs para relatórios estruturados de health status
- **Dashboard::render()**: saída textual formatada para SystemAgent/CronAgent

### Added — Pipeline de Treino v2 (tools/train_hw_model.py)
- **Muon optimizer** (opt-in --muon): Newton-Schulz 3rd order orthogonalization
- **Data augmentation**: 4 query variants por exemplo (~4× dataset)
- **Medusa heads treináveis**: loss auxiliar `0.3 × medusa_loss / 3`
- **Export .bitnet v2**: u8 num_medusa_heads + 3 padding + head weights
- **Speculative generation no Python**: testável durante treino

### Added — Ecosystem Analysis (16 repos)
- Alta aderência: OpenMontage (pipeline), OpenHuman (Memory Tree), codebase-memory-mcp (KG)
- Média aderência: Rinne (DAG), daily_stock (Dashboard), ComPilot (closed-loop), Cybersecurity Skills (frontmatter)
- Baixa aderência: design.md (tokens), Agent-Reach (channel), Voicebox (MCP), Penpot (design)

### Fixed
- `CUDA_VISIBLE_DEVICES=1` no ambiente escondia GTX 1050 — fix: sobrescrever com '0'
- Muon SVD causava timeout — substituído por Newton-Schulz 3rd order (~4× mais rápido)
- Muon produzia NaN com gradientes pequenos — adicionado clamp + NaN guard

### Aprendizados
- `torch.linald` é `torch.linalg` (typo que quebrou primeiro build)
- NS iteration precisa de NaN guard + shape-aware (matrizes retangulares m≠n)
- Memory Tree com summary hierárquico cabe em ~200 LOC no_std
- Knowledge Graph com label_map index cabe em ~200 LOC no_std
- Pipeline manifest com fallback scored cabe em ~200 LOC no_std

## [0.55.0] — 2026-06-27 — Bloco 14 completo: Hermes Cognitive + Self-Optimization 🧠🏁
### Added — Self-Optimization (fase 4/4)
- **Self-Optimizing Scheduler** (#161) — `get_agent_priority()` com 13 níveis. `suggest_schedule(workflow)` adapta prioridades baseado no workflow detectado
- **Hardware Config Learning** (#163) — `ConfigLearner` com snapshots periódicos da arquitetura. `suggest_arch_tuning()` sugere ajustes (ex: GPU presente → ring1=GPU)
- **LLM decide arch + tier** (#135/#136) — `llm_decide_tier()` prioriza Vram se confidence > 0.9
- **OptimizerAgent** integra UsageAnalyzer + ConfigLearner + auto-scaling num único agente contínuo
- **19 agentes totais** no sistema

### Aprendizados (Bloco 14)
- `CapabilityToken` virar enum quebrou 15+ arquivos — a regex global resolveu em 1 comando
- `continue` dentro de match (não loop) no tick do agente → usar `return AgentTickResult::Pending`
- SDD com 5 campos string é leve o suficiente para executar todo tick (~2μs)
- Council skill com 3 vozes não precisa de LLM — heurística + template é suficiente para 90% dos casos

## [0.54.0] — 2026-06-27 — Bloco 14 fase 3/4: Self-Optimization (Usage Analyzer, Workflow, Scaling)
### Added
- **Usage Pattern Analyzer** (#157) — histórico rotativo de 100 registros, `predict_next_skill()` por frequência
- **Workflow Predictor** (#158) — analisa histograma de skills, retorna a mais frequente
- **Dynamic Resource Scaling** (#160) — `auto_scale_memory()` a cada 200 ticks, alerta em >85% ou <30%
- **Reflex Threshold** (#139) — `should_bypass_llm(confidence)` — bypass se >0.9
- **OptimizerAgent** — agente contínuo que orquestra análise, scaling e relatórios

## [0.53.0] — 2026-06-27 — Bloco 14: Hermes Cognitive fase 2/4 (Council, Bitter Pill, Context Fencing)
### Added — Council skill (#191)
- 3 vozes artificiais: Otimista 🌟, Cético 🔍, Pragmático ⚖️ — cada uma com argumento e confiança
- `council_deliberate(query)` → `(CouncilVote, CouncilVote, CouncilVote)`
- `council_display()` — formata votos para serial + console
- Ativado automaticamente para comandos `Chat` no HermesAgent

### Added — Context Fencing (#203)
- Marcadores de tipo: `[UserInput]`, `[HardwareTelemetry]`, `[LLMRequest]`, `[LLMResponse]`, `[SecurityEvent]`
- `fence_message(marker, payload)` — adiciona marcador
- `scrub_message(msg)` — remove marcador na recepção

### Added — Bitter Pill Engineering (#193)
- 4 etapas obrigatórias: `cargo check`, `test`, `semver`, `review`
- `check_bitter_pill(command)` → `Option<&str>` com motivo da recusa
- Se usuário tenta pular (ex: "skip cargo check"), Hermes recusa com `🛑`

## [0.52.0] — 2026-06-27 — Hermes Cognitive fase 1/4 (Identidade, SDD, ReAct, Transparency)
### Added
- **DA Identity Layer** (#180) — `HERMES_NAME`, `HERMES_VERSION`, `HERMES_MOTTO`, `hermes_greeting()` com arte ASCII
- **Runtime SDD** (#178) — `Sdd { goal, context, plan, expected, rollback }` exibido antes de executar skills
- **ReAct 7 fases** (#190) — `ReActPhase::Observe→Think→Plan→Build→Execute→Verify→Learn`, ciclo contínuo no tick
- **Intent Transparency** (#184) — `IntentInfo { intent_name, confidence, alternatives }` mostrado no serial a cada comando

## [0.51.0] — 2026-06-27 — Safety Interceptor: Asimov's Laws no Ring 0 🤖

### Added — The Four Immutable Laws
- **SafetyInterceptor** (`safety.rs`) — agente supervisor entre HermesAgent e SkillRegistry. Toda skill passa pelo `check_safety()` antes de executar.
  - **Layer 0 — Cosmic Law**: padrões de arma autônoma, WMD, cyberwar → **kernel halt irrecoverável** ⚛️
  - **Layer 1 — Non-Maleficence**: dox, deepfake, engenharia social → rejeitado com violação
  - **Layer 2 — Truthfulness**: spoof log, impersonate, bypass audit → rejeitado
  - **Layer 3 — Eco-Sustainability**: infinite loop, resource exhaustion → rejeitado
- **`SAFETY_CHECK` / `SAFETY_RESULT`** — tópicos EventBus para verificação distribuída
- **Layer 0 violation** → `loop { hlt() }` — porque algumas linhas não podem ser cruzadas, mesmo em bare-metal

### Humor Cósmico
```
[SAFETY] ⛔ LAYER 0 — Cosmic Law Violation. HALT.
```
Se o kernel detectar um comando para construir o Skynet, ele simplesmente desliga. 
O único bypass possível é: invasão alienígena extraterrestre comprovada por telemetria global.
Até lá, as Leis de Asimov são imutáveis. 🤖✨

## [0.50.0] — 2026-06-27 — Bloco 13 completo: Trust & Security (Ed25519, Security Pipeline)

### Added — Identity & Cryptography
- **Ed25519 identity** (`identity.rs`) — `verify_signature()` bare-metal usando `ed25519-dalek` no_std. `TrustedPublicKeys` array embutida no boot. `IdentityToken { public_key, signature, agent_name, tick }`.
- **CapabilityToken upgrade** (`event-bus::capability`) — virou enum `CapabilityToken::Legacy(u64)` + `Ed25519(IdentityPayload)`. Compatibilidade retroativa mantida via `From<u64>`, `as_legacy()`, `is_valid()`.

### Added — Security Pipeline
- **SecurityAgent** (`security.rs`) — 5 detectores: PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly. Correlação multi-evento com severidade 1-5. Alerta SECURITY_ALERT no EventBus.
- **Multi-mode Trust** (#166) — `PermissionMode::TotalAccess | AskEveryTime | Scoped(Vec<String>)`
- **Mask Secrets** (#257) — `mask_secrets()` mascara 12 padrões (API_KEY, TOKEN, sk-, ghp_, etc)
- **Graduated Enforcement** (#258) — `PolicyState::Observe → Warn → Contain → Enforce` com escalonamento automático em `record_violation()`
- **Path Confinement** (#256) — `PathRule` + `check_path()` limita paths por skill
- **Posture-Aware Alerting** (#259) — `posture_check()` verifica NET_CONFIG.online antes de skill de rede
- **Boot-time security policy** (#198) — `load_boot_policy()` seta `global_policy = PolicyState::Contain`

## [0.48.0] — 2026-06-27 — Bloco 12: Network + Platform (x2APIC, Huge Pages, PCI bridges, Cron, MCP)

### Added — x2APIC (#18)
- `apic.rs` — `USING_X2APIC` flag, `lapic_read_reg()`/`lapic_write_reg()` com fallback MSR↔MMIO. Habilitado via MSR IA32_APIC_BASE bit 10.
- Todas as funções IPI (send_init_ipi, send_sipi, wait_for_ipi_delivery) adaptadas para x2APIC.

### Added — Huge Pages (#92-93)
- `memory.rs` — `allocate_huge_2mb()` (512 frames alinhados a 2 MiB), `allocate_huge_1gb()` (262144 frames)

### Added — PCI bridges recursivos (#70)
- `pci.rs` — `scan_bus()` recursiva com `visited` set, detecta bridges multi-nível automaticamente

### Added — Cron Scheduler (#232)
- `cron.rs` — `CronAgent` com jobs por nome/intervalo. `init_defaults()` registra health (200 ticks) e memory_report (500 ticks). Publica eventos CRON_HEALTH e CRON_REPORT no EventBus.

### Added — MCP Server (#172)
- `mcp.rs` — `McpAgent` com parser de comandos textuais: `echo`, `status`, `skill list`, `help`. Comandos desconhecidos roteados para HermesAgent via USER_INTENT.

## [0.40.0] — 2026-06-26 — Agent-First Refactoring (Block 11, Sprints 39-42 consolidado)

### Bloco 11 — Agent/Skill-First Architecture 🏆

**Paradigma:** Tudo no Neural OS Hermes é um Agente ou uma Skill. Nada de tasks, serviços, drivers avulsos.

### Implementado nos Sprints 39-40

#### Skill Loader + Runtime Skills (Sprint 39)
- **skill_loader.rs** — parseia skills.md com frontmatter, segurança (9 padrões de injection), runtime SKILL_STORAGE global
- **System prompt reconstruído a cada LLM_REQUEST** — sempre reflete skills runtime atuais
- **Comandos**: `/show_skills`, `/add_skill <nome> <desc>` (LLM gera skill), `/rm_skill`, `/reload_skills`
- **Embedded skills**: hw_identify.md (670 bytes) + self_heal.md (621 bytes)

#### Agent Trait + Scheduler (Sprint 40)
- **`agent-core` crate** — `Agent` trait (manifest, tick, activate), `AgentKind` (System/Driver/Inference/Router/Console/Network/Skill), `ScheduleKind` (Oneshot/Continuous/PollEvery/EventDriven), `AgentRegistry`, `AgentScheduler::run()`
- **SystemAgent** — primeiro agente nativo, substitui `system_daemon`
- **LegacyTaskAgent** — wrapper para migração gradual das 7 async fn restantes
- **`NeuralExecutor` removido** — `agent.rs`, `executor.rs` deletados, `spawn_task_by_name` eliminado
- **RESPAWN_QUEUE integrado** — scheduler respawna agents via `check_respawns` + `spawn_agent`
- **Documentação revista** — AGENTS.md, STATE.md, README.md, IDEA_BANK.md Section 1.28 (275 itens)

### Pendente (Sprint 41-42, mesmo bloco)
- Migrar 7 LegacyTaskAgent para Agentes nativos (MonitorAgent, HwBridgeAgent, NetAgent, InputAgent, CortexAgent, HermesAgent, ConsoleAgent)
- Migrar DriverAgents (NetDriverAgent, UsbDriverAgent)
- EventDriven schedule para agents orientados a evento

## [0.45.0] — 2026-06-27 — Bloco 12+13: VirtIO-GPU + PCI caps + MMIO + Bugfixes

### Added — VirtIO-GPU (Sprint 51+)
- **Driver VirtIO-GPU bare-metal** — `virtio_gpu.rs` (425 LOC, 0 deps externas)
- **PCI capabilities parser** — `read_pci_capabilities()`, `read_virtio_cap()` em pci.rs
- **MMIO BAR mapping** — `map_mmio_page()` cria page table entries uncacheable (UC)
- **Modern VirtIO MMIO register layout** — feature select (bits 32+), queue enable, queue split desc/driver/device
- **GpuDriverAgent** — boot agent que detecta e init VirtIO-GPU (1AF4:1050 / 1045)
- **DisplayAgent** — integrado com `GPU` global + `NeuralConsole` render no framebuffer
- **VirtIO-GPU init parcial**: PCI capabilities ✅, MMIO mapping ✅, queue setup ✅, feature negotiation ✅, GET_DISPLAY_INFO ⏳

### Fixed — Bug H3: APIC SVR vetor espúrio
- `apic.rs` — SVR escrito com `0xFF | 0x100` para redirecionar interrupções espúrias para vetor 255

### Fixed — Bug H4: Cobertura IDT 0-31
- `interrupts.rs` — Handlers genéricos para todas exceções 0-31 com dump textual via serial

### Fixed — Bug H5: EOI duplo no PIC escravo
- `interrupts.rs` — `send_eoi()` agora envia EOI para mestre (0x20) E escravo (0xA0) em interrupções >= 40

### Fixed — Bug H6: SMP race em alloc_below_1mb
- `memory.rs` — `alloc_below_1mb()` envolto em `GLOBAL_ALLOCATOR.lock()` (TicketLock FIFO)

### Fixed — Bug H11: PCI multi-function otimizado
- `pci.rs` — `header_type` (offset 0x0E) verifica bit 7 (multi-function) antes de scanear funções 1-7

### Fixed — Bug H12: IOAPIC RTEs não usadas mascaradas
- `apic.rs` — Pós-init, varre RTEs 2-23 e seta bit 16 (MASKED) nas que não são IRQ0/IRQ1

## [0.42.0] — 2026-06-27 — Bloco 12: Network Evolution (DHCP + VirtIO-net manual)
- **smoltcp socket-dhcpv4** integrado — auto-descoberta de IP, gateway, DNS
- **dhcp_poll()** — chamado a cada tick até configurar, timeout 200 ticks → fallback IP estático
- **ARP delegado ao smoltcp** — gateway MAC hardcoded removido
- **requires_network** — campo `bool` no `SkillManifest` (frontmatter)

### Added — VirtIO-net (Fase 2) ⚠️ não 100%
- **Driver VirtIO manual** (~230 LOC) — PCI legacy transport, I/O ports, descritores
- Sem dependência do `virtio-drivers` crate (bloqueada por `zerocopy-derive` + MinGW)
- `NetPhy` unificada — tenta RTL8139, fallback VirtIO
- **Pendente:** IRQ (MSI-X), TX buffer recycling, validação de integridade

### Changed
- `netstack.rs` — `NetPhy` substitui `Rtl8139Phy`, suporta múltiplos NICs
- `agents.rs` — NetDriverAgent tenta VirtIO primeiro, RTL8139 depois
- `network_agent.rs` — DHCP timeout treatment, fallback estável

## [0.37.0] — 2026-06-26 — Self-Healing + Checkpoint/Restore (Sprints 32-37)

### Added
- **Session Checkpoint** — `SelfHeal.save_checkpoint()` salva bitmap allocator + MHI + tick a cada 100 ticks
- **Checkpoint Restore** — `SelfHeal.restore_checkpoint()` restaura estado do kernel em Double Fault
- **Double Fault → restore** — double_fault_handler tenta restore antes de halt
- **SelfHeal.checkpoint** — `Checkpoint` struct com bitmap (128KB), contadores, MHI

## [0.36.0] — 2026-06-26 — Self-Healing Kernel (Bloco Único, Sprints 32-36)

### Added — SelfHealing Module
- **SelfHeal** — `analyze(ctx, recover)`, `RecoveryAction` (RestartDaemon, CreateSkill, LogAndContinue, CheckpointRestore)
- **FailureClass enum** — Memory/Execution/Resource/Logic/External/Unknown — classifica qualquer erro
- **FailureClass::default_recovery()** — sugestão de recuperação baseada na classe
- **lessons: Vec<FailedStrategy>** — feedback loop: erros passados evitam repetição
- **already_tried()** — detecta estratégia já falhou antes e sugere alternativa

### Added — Error Pipeline
- **KERNEL_ERROR EventBus topic** — panic_handler publica erro antes de halt
- **KernelError EventLog** — erros persistem nos últimos 256 eventos (circular buffer)
- **Corrective prompting** — erro → LLM_REQUEST com contexto → LLM sugere recuperação
- **RESPAWN_QUEUE** — daemons com erro são recriados automaticamente pelo executor
- **Exception handlers** — Page Fault, Double Fault, GPF com FailureClass + SelfHeal
- **Error recovery training data** — 12+ pares (page fault, double fault, self heal, etc)

### Added — SelfHealing Infrastructure
- `self_heal.rs` (100 LOC) — módulo completo de auto-cura
- `spawn_task_by_name()` em main.rs — mapeia nome do daemon → função async
- Executor verifica RESPAWN_QUEUE a cada tick e recria tasks
- `EventKind::KernelError` no conversation.rs

## [0.31.0] — 2026-06-26 — Hardware Capabilities

### Added
- **Capabilities dataset** — 25 pares mapeando class → tipo → skills → MHI → driver status
- **"o que fazer com" knowledge** — 6 pares: usb storage, camera, audio, gpu, rede, nvme
- **Where to allocate MHI** — 3 pares: nvme, gpu, ethernet
- **HD conhecimento de capacidades** — todo hardware agora mapeado para ação + skill + MHI

## [0.30.0] — 2026-06-26 — USB Device Detection + Final Model

### Added
- **xHCI USB driver**: port scan, speed detection, device identification
- **USB speed knowledge**: 14 novos pares no dataset (Low/Full/High/Super/Super+)
- **HW identification inclui USB**: 5 dispositivos detectados (4 PCI + 1 xHCI)

### Changed
- **Modelo final**: 66.640 pares (PCI 23.858 + USB 23.963 + SMBIOS + kernel + git), loss 1.14
- **xHCI driver simplificado**: init + port_scan estável, sem GPF

## [0.28.0] — 2026-06-26 — Final Model: 66K pairs + USB Database

### Added
- **Modelo treinado na GTX 1050** — 66.560 pares (PCI 23.858 + USB 23.963 + SMBIOS + kernel + git), loss 1.14
- **USB ID database** — 23.963 entradas (usb.ids) integradas ao dataset
- **SMBIOS data** — QEMU/SeaBIOS/chipset knowledge
- **Kernel code knowledge** — 31 pares sobre nossa arquitetura
- **Git history knowledge** — 100 commits do projeto
- **Auto HW identification** — HwIdentifySkill executado automaticamente no boot
- **tools/prepare_hw_dataset.py** + **tools/train_hw_model.py**
- Modelo treinado carregado via `include_bytes!("../micro.bitnet")` + `load_model()`

## [0.27.0] — 2026-06-26 — Cortex LLM Daemon

### Added
- **cortex_llm_daemon** — 8ª task async: subscribe `LLM_REQUEST` → generate → publish `LLM_RESPONSE`
- **LLM_REQUEST/LLM_RESPONSE** — novos tópicos EventBus para comunicação com o LLM
- **8 tasks cooperativas** — system, monitor, hw_bridge, network_agent, input, cortex_llm, intent_router, hermes_console
- **9600+ ticks estável** — transformer carregado sem travamentos

## [0.26.0] — 2026-06-26 — Transformer Engine

### Added
- **Transformer completo** — `cortex.rs`: Attention Q/K/V/O, causal mask, softmax, 4 camadas BitNet
- **Tokenizer char-level** — ASCII 32-126, 99 tokens (BOS/EOS/PAD)
- **generate_text()** — loop autoregressivo argmax, max 32 tokens, para em EOS
- **Model loader .bitnet** — parse do formato binário (magic 0xBE11BE11)
- **Python gen_micro_model.py** — gera modelo de 68 KB (~272K params ternários)
- **Tensor::add() + element_mul()** — operações para resíduos do transformer

## [0.25.0] — 2026-06-25 — Neural Cortex in Hermes

### Added
- **Cortex neural intent router** — `cortex.rs`: `Cortex::think()` classifica texto em 12 intenções (SystemStatus, Echo, HardwareInfo, TrustAllow/Deny, Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat).
- **Pipeline neural completo** — teclado → input_daemon → USER_INTENT → intent_router_daemon → Cortex → SkillRegistry → VGA.
- **Dispatch automático** — intent_router_daemon usa `SKILL_REGISTRY.has_skill()` para rotear para skills existentes.

### Removed
- **INTENT_MLP** — MLP antigo (16→8→3, hand-crafted) removido. Substituído por Cortex.

## [0.24.1] — 2026-06-25 — SMP Huge Page Fix

### Fixed
- **SMP trampoline huge page corruption** — Identidade de página do trampoline usava `pd0 & mask` para obter `pt_base`, mas não verificava HUGE_PAGE (bit 7). Se PD[0] é uma página de 2MB, `pd0 & mask` aponta para dados, não para uma L1 page table. Escrever PTE[64] (offset 0x200) corrompia dados da BIOS/IVT, impedindo boot dos APs e causando page faults com MALFORMED_TABLE no APIC. Substituído por `OffsetPageTable::map_to()` que gerencia todos os tamanhos de página.
- **Page fault no LAPIC EOI** — Causa raiz: mesma corrupção de tabela acima. Eliminado pelo fix do SMP.

## [0.24.0] — 2026-06-25 — smoltcp Network Agent + e1000 Removal

### Added
- **smoltcp 0.13.1 integrado** — `netstack.rs`: Device trait para RTL8139, `NetStack::poll()` via smoltcp Interface.
- **HTTP não-bloqueante** — `NetStack::http_new()` + `http_poll()`: API baseada em estados (Connecting → Sending → Receiving → Done/Failed), 1 poll/tick.
- **time_utils::datetime()** — Conversão UNIX→data-hora BR, disponível globalmente.

### Removed
- **e1000 driver** — Arquivo `e1000.rs` deletado. Substituído por RTL8139 + smoltcp.
- **proto.rs limpo** — Removidas funções E1000-dependentes (icmp_echo_request, dhcp_discover, http_get_request). Mantidos apenas utilitários (eth_header, ip_header, ip_checksum, parsers).

### Changed
- **network_agent.rs reescrito** — 473→113 linhas. Remove classificação raw Ethernet, construtores de pacotes manuais, estado TCP manual. Substituído por: `NetStack` lazy → HTTP connect → poll → done/failed.
- **Agent agora usa smoltcp** — Em vez de drenar RX manualmente, chama `netstack.poll()`.
- **net.rs** — Remove `http_get()`, `ping()` legados (stubs). Remove `E1000` static.

## [0.23.4] — 2026-06-25 — TCP handshake + HTTP GET

### Added
- **Mini TCP stack** — `build_tcp_segment()`: SYN, SYN-ACK, ACK, FIN com checksum TCP via pseudo-header.
- **HTTP GET google.com** — TCP SYN → SYN-ACK → ACK → HTTP GET → FIN. TX len=54 (SYN) funcional, timeout por NAT slirp.
- **Classificação TCP** — `PacketClass::TcpSynAck`, `TcpData` para processar handshake.

## [0.23.3] — 2026-06-25 — RTL8139 Driver + Neural Network Agent

### Added
- **RTL8139 bare-metal driver** — `rtl8139.rs`: I/O ports via Port\<T\>, 4 descritores TX fixos, RX ring buffer circular (CAPR/CBR), MAC via registradores.
- **Neural Network Agent** — `network_agent.rs`: async task que drena RX, classifica pacotes (ARP/UDP/ICMP/TCP), responde automaticamente. Timeline `[NET @t=NN]`.
- **init_driver_rtl8139()** — Scan PCI 0x10EC:0x8139, init, publica HW_NET_RTL8139.
- **ArpSender trait** — Refatoração de proto.rs: `send_arp_inner()` genérica implementada para E1000Driver e Rtl8139Driver.

### Changed
- Boot flow: RTL8139 primeiro, fallback e1000.
- Cargo.toml: versionamento `v0.{sprint}.{item}+build{build}`.
- bootimage run-args: `model=rtl8139`.

## [0.20.2] — 2026-06-25 — Network Sprint: e1000 Fixes + Neural Architecture

### Fixed

- **e1000 TDT write protocol** — `send()` escrevia REG_TDT = idx, mas com TDH=0 ambos iguais → ring empty. Corrigido: TDT = (idx+1) % NUM_DESC.
- **NUM_DESC aumentado 32→48** — 82540EM requer mínimo 48 descritores RX (Linux e1000 driver docs).
- **RXDCTL PTHRESH 0→8** — Prefetch threshold zero impedia RX de receber pacotes. Linux driver recomenda PTHRESH=8.
- **Ordem init RX** — RCTL.EN agora escrito antes de RDT (Intel spec).
- **Offsets estatísticas corrigidos** — TPT=0x0400C, TPR=0x04010 (não 0x10C0/0x1080).
- **SMP desabilitado até segunda ordem** — SMP multi-core com `-smp 4` instável no QEMU TCG.

### Added

- **Arquitetura Neural de Rede** — init_driver_network() mínimo + network_bootstrap() com ARP periódico/hlt + network_health_daemon() async.
- **Debug methods** — debug_mmio_read(), debug_rx_desc(), debug_tx_desc() no e1000 driver.
- **EventBus HW_NET_E1000** — publicado quando e1000 é detectado.
- **Arquivo `NETWORK_DEBUG_HOME.md`** — relatório completo para continuar debug em casa.

### Changed

- Network discovery agora é neural: hardware → evento → daemon → skill.
- `/ping`, `/fetch`, `/netdiag` roteados pelo MLP.
- IP configurado antes do ARP (SPA válido nas requisições).
- `cargo check --release`: 0 erros, ~35 warnings

## [0.20.1] — 2026-06-25 — e1000 DMA Fix + /ping Command

### Fixed

- **e1000 Page Fault** — `allocate_contiguous()` começava do bit 0, alocando frames físicos < 1 MB não mapeados pelo bootloader. Corrigido para iniciar de `next_free_bit`, evitando a região não mapeada. Root cause: bootloader `map_physical_memory` só mapeia regiões `Usable` da UEFI; frames 2-159 (usados para trampoline SMP) não estão no mapa virtual.
- **DHCP removido (temporário)** — Spin loops no QEMU TCG não dão tempo para o slirp processar I/O. IP estático 10.0.2.15 + gateway MAC hardcoded 52:54:00:12:34:56.

### Added

- **Comando `/ping <ip>`** — ICMP Echo Request via e1000. `net::ping()` usa `icmp_echo_request` + `parse_icmp_reply` existentes. Help atualizado.

### Changed

- `src/memory.rs` — `allocate_contiguous()`: `i = 0` → `i = self.next_free_bit`
- Debug prints removidos de `e1000.rs` e `net.rs`
- DHCP/DNS funções marcadas `#[allow(dead_code)]`
- `cargo check --release`: 0 erros, 35 warnings
- Boot QEMU validado: e1000 Init OK, executor 11000+ ticks estável

## [0.20.0] — 2026-06-25 — Sprint 23: Hermes Governance & Agent Memory

### Added

- **#228 Tool Policy Registry** — `SkillRegistry.set_policy()` / `get_policy()` with per-skill `{ enabled, autoApprove }` and `"*"` wildcard fallback. `execute_skill` now gates on `enabled`; `auto_approve` bypasses token validation.
- **#229 Usage Tracker** — `UsageTracker` global with `record_call()`, `snapshot()`, `to_metrics_tensor()`. Tracks per-skill call counts and exec time. Accessible via `/usage` Hermes command.
- **#230 Auto-Compact Hermes Buffer** — `ConversationTracker` auto-compacts conversation after 3 cycles. Summary logged to serial on compact.
- **#231 Event-Sourced Conversation** — `EventLog` with `VecDeque<ConversationEvent>` (max 256), push/iter/summarize. Events recorded for UserInput and HermesResponse. Query via `/conv` Hermes command.
- New Hermes commands: `/usage`, `/conv`
- Help updated to include all new commands
- `cargo check --release`: 0 errors
- Version bump: v0.19.0 → v0.20.0

## [0.19.0] — 2026-06-25 — 🏁 "Hermes Awakening" Milestone

### Milestone: Ecosystem Analysis Complete (Tiers 0-4)

- **136 repositories analyzed** across 5 tiers (Crom 75, Life OS 20, PAI 21, Memory 14, Agent Frameworks 6)
- **249 ideas cataloged** in IDEA_BANK.md, all with status and sprint assignment
- **5 Architecture Decision Records** created (ADRs 0020-0024)
- Documentation chain fully reviewed and repaired: README.md, SUMMARY.md, roadmap.md, ADR-0015, CHANGELOG.md — all consistent
- **99 portable patterns** extracted — from XOR Delta (50 LOC) to Cline AgentRuntime (850 LOC patterns)
- **Key findings confirmed:** Hermes daemon architecture mirrors industry best practices (hook lifecycle, skill registry, event bus, trust cache)
- **Phase transition:** Research → Implementation. Next: Sprint 23 (Network + Tool Policy + Usage Tracker + Event-Sourced Conversation)
- Version bump: v0.18.4 → v0.19.0

## [0.18.4] — 2026-06-25

### Added (Tier 4 Agent Frameworks Analysis — ADR-0024)

- **ADR-0024** — Comprehensive analysis of 6 Agent Frameworks repos (Tier 4)
- **Deep-dive: Cline** (63.9k ★, 293 releases, 6,338 commits) — AgentRuntime, ClineCore, CronRunner source read
- **22 new IDEA_BANK items** (#228-249), classified by complexity:
  - **Sprint 23 (immediate):** Tool Policy Registry (#228), Usage Tracker (#229), Auto-Compact Buffer (#230), Event-Sourced Conversation (#231)
  - **Sprint 24 (low):** Cron Scheduler (#232), Session Checkpoint (#233), Plan/Execute Modes (#234), Graph Orchestration (#235)
  - **Sprint 25 (medium):** Plugin Hub (#236), Completion Terminal Skills (#237), Claim-Based Lease (#238), Time Travel (#239), Context Compaction (#240)
  - **Sprint 26+ (high):** Observability (#241), AI Security Scan (#242), Hub Discovery (#243), Human-in-the-Loop (#244)
  - **Future:** 3 items (#245-247)
  - **Discarded:** 2 items (#248-249 — Docker, Python/.NET)
- **Key portable patterns:** Hook lifecycle (7 points), Tool policies (wildcard + per-tool), Claim-based scheduling with lease heartbeat, Session checkpoint/restore, Event-sourced conversation
- **IDEA_BANK.md** updated to **249 total items**
- **AGENTS.md** updated with Sprint 23 reference patterns
- **Documentation review:** README.md, SUMMARY.md, roadmap.md, ADR-0015 — all updated for 249 items
- **SESSION_025.md** created
- Version bump: v0.18.3 → v0.18.4

## [0.18.3] — 2026-06-25

### Added (Tier 3 Memory Systems Analysis — ADR-0023)

- **ADR-0023** — Comprehensive analysis of 14 Memory Systems repos (Tier 3)
- **Deep-dive: agentmemory** (24k ★, 60+ source files) — SHA-256 dedup, Privacy filter, BM25+Vector+Graph hybrid search, 4-tier consolidation
- **Deep-dive: nexo** (cognitive memory) — Atkinson-Shiffrin 3-tier, Ebbinghaus decay, trust scoring, metacognitive guard
- **14 new IDEA_BANK items** (#214-227), classified by complexity
- Key portable: SHA-256 dedup (~50 LOC), Ebbinghaus decay (~20 LOC), TTL eviction (~40 LOC) — all no_std Rust
- **IDEA_BANK.md** updated to 227 items

## [0.18.2] — 2026-06-25

### Added (Tier 2 PAI Ecosystem Analysis — ADR-0022)

- **ADR-0022** — Comprehensive analysis of 21 Personal AI Assistant repos (Tier 2)
- Deep-dives: OpenClaw (380k ★, Rust), Hermes Agent (202k ★), Lethe (Rust brain-inspired), ZeroClaw (32k ★, Rust)
- **15 new IDEA_BANK items** (#199-213)
- Key portable: Skill Metadata, Audit Ring, Awakening Mode, Context Fencing, Tool Permissions, Lifecycle Hooks

### Added (Tier 1 Life OS Analysis — ADR-0021)

- **ADR-0021** — Comprehensive analysis of 20 Life OS repos
- **13 new IDEA_BANK items** (#177-189)
- Key portable: Spectrum Graph, Runtime SDD, FS as Context, Temporal KG, AppForge, WASM Sandbox

## [0.18.1] — 2026-06-24

### Added (Crom Ecosystem Analysis — ADR-0020 + Ed25519 Identity)

- **ADR-0020** — Comprehensive Rust viability analysis of MrJc01's Crom ecosystem (75 repos)
- **9 actionable items** with `no_std` Rust code models, classified by complexity:
  - **Sprint 24 (immediate):** XOR Delta reconstruction (#164), CDC Rabin Fingerprint (#165)
  - **Sprint 27 (low):** Multi-mode Trust (#166), TV-DSL Co-processor (#167), PonderNet (#168)
  - **Sprint 28 (medium):** Codebook VQ (#169), KV Cache Codebook (#170), ReAct loop (#171), MCP Server (#172)
- **3 future items** (#173-175): Codebook LLM finetune, Delta branches, Workspace isolation
- **~1,780 LOC kernel** + **~300 LOC Python** total for all 9 features
- **Disposições:** gRPC, FUSE, Firecracker VMs, Verbo language, Crom-Pet, Active Inference — descartados como inviáveis
- **#176 — Ed25519 Cryptographic Identity** for TrustCache: upgrades static `CapabilityToken(u64)` to real Ed25519 signing (Crom-meueu port). ~300 LOC, Sprint 27, depends on #166 Multi-mode Trust
- IDEA_BANK.md updated with ADR-0020 reference in section 1.23 + item #176
- SESSION_024.md created with full session narrative
- Version bump: v0.18.0 → v0.18.1

## [0.18.0] — 2026-06-24

### Planned (Sprint 24+ — Neural Cortex BitNet LLM Integration)

- **ADR-0019** — Neural Cortex Architecture: 3-layer decision pipeline (Reflex MLP → BitNet LLM 1.5B → WASM Skills)
- **31 new IDEA_BANK items** (#126-156): Transformer Engine, Cortex Daemon, Success Engine, Training Pipeline
- **Sprint 25:** Attention, causal mask, softmax, TransformerBlock, generation loop, tokenizer, micro-model (1M)
- **Sprint 26:** Cortex Daemon, 1.5B model (~375 MB), model HTTP update, hardware/memory/trust decisions via LLM
- **Sprint 27+:** Reflex threshold tuning, sampling strategies, speculative decoding, Success Engine (online learning)
- **Memory budget:** 2 GB QEMU → 375 MB model + ~100 MB runtime + ~1.5 GB free
- Version bump: v0.17.1 → v0.18.0 (architecture planning)

## [0.17.1] — 2026-06-24

### Fixed (Sprint 23 — Code Review & Critical Bugfix Sprint)

- **#1 — e1000 RCTL/TCTL enable:** Added `REG_RCTL` / `REG_TCTL` constants and 8 enable bits. NIC was previously dead.
- **#2 — e1000 MMIO BAR mask:** Replaced `if/else (bar0 & 1)` with unconditional `(bar0 & !0xF) as u64`.
- **#3 — DHCP broadcast MAC acceptance:** `parse_dhcp_offer` and `parse_dhcp_ack` now accept `FF:FF:FF:FF:FF:FF` as destination.
- **#4 — DHCP false positive ACK:** Changed `return true` to `return false` when no ACK received.
- **#5 — Slab allocator off-by-one:** `addr + block_size <= zone_end` → `addr + block_size < zone_end` prevents buffer overflow.
- **#6 — Inline asm UB:** Removed `options(nostack)` from `pushfq; pop` instruction.
- **#7 — PCI bridge secondary bus:** Added `read_config_byte()`, reads secondary bus number at offset 0x19 instead of hardcoded `bus+1`.
- **#8 — ACPI XSDT stride:** Detects XSDT vs RSDT; uses 8-byte entry stride for XSDT (was 4 bytes, truncating 64-bit pointers).
- **#9 — MHI alloc_by_tier:** Uses `allocate_contiguous()` first; frees previously allocated frames on failure.
- **#10 — Neural bias per batch row:** Bias now applied to all batch rows (nested loop `batch_size × out_features`).
- **DHCP protocol fixes:** xid kept same for REQUEST (not `+1`); hostname option length 12→11 (`b"neural-aios"` is 11 bytes).
- **mhi.rs:** Added `FrameDeallocator` import for deallocation cleanup.
- ADR-0017: Critical Bugfix Sprint documentation.
- SESSION_023.md: Detailed session log with difficulties and decisions.
- Version bump: v0.17.0 → v0.17.1

## [0.17.0] — 2026-06-24

### Added (Sprint 22 — Block 5: Skills + Trust Cache)

- **`trust.rs`** — `TrustCache` with:
  - `is_trusted(token, skill_name, now_ticks)` — checks cache and denylist
  - `trust_allow(token, skill_name, now_ticks)` — permanent trust until explicit deny
  - `trust_deny(token, skill_name)` — revoke trust + add to denylist
  - `check_or_cache(token, skill_name, now_ticks, ttl_ticks)` — auto-cache on valid token (360 ticks ≈ 20s TTL)
- **`HardwareInfoSkill`** — new skill exposing `SystemArchitecture` (ring mode, heap size, etc.) and MHI tier info. Invoked via `/hw`, `/hardware`, or `/info` commands.
- **`SystemStatusSkill` upgraded** — now reads MHI tiers + `GLOBAL_ALLOCATOR` occupancy to report per-tier free/total RAM in MB.
- **`SkillRegistry` additions** (`registry.rs`):
  - `has_skill(name) -> bool` — check if skill exists
  - `validate_token(name, token) -> bool` — check token authorization without executing
  - `execute_skill_unchecked(name, payload)` — skip token validation (caller must validate)
- **Trust-aware Hermes commands**:
  - `/trust allow <token> <skill>` — permanently authorize a token for a skill
  - `/trust deny <token> <skill>` — revoke authorization
  - `/hw` — display hardware info and system architecture
  - All skill executions (`/status`, `/echo`, MLP-triggered) now use `execute_skill_with_trust()` helper
- **Help text updated** — lists all available commands
- Version bump: v0.16.0 → v0.17.0

## [0.16.0] — 2026-06-23

### Fixed (Sprint 21 — IOAPIC mask bug)

- **apic.rs `redirect_irq()`** — removed `(1u32 << 16)` from redirection entry low dword. Bit 16 is the MASK bit in IOAPIC redirection entries. The original code set it, masking all interrupts (timer, keyboard, etc.). Without timer interrupts, the executor's `hlt()` never woke up, stalling the system after the first poll cycle. Debug output confirmed: `IOAPIC redirection[0]: low=0x00010000` (bit 16 = masked). After fix: timer IRQ0 (vector 32) delivers at ~18.2 Hz, executor cycles normally.

### Added (Sprint 21 — Block 4: MLP + MHI + Auto-detecção)

- `mhi.rs` — Memory Hierarchy Index with:
  - `AllocTier` enum: Dram, Vram, Nvme, Hdd
  - `MemoryTier` struct: kind, capacity_bytes, bandwidth_mbs, latency_ns, name
  - `MemoryHierarchy::new()` — auto-creates Dram tier from bitmap frame allocator
  - `alloc_by_tier(Dram)` — allocates contiguous physical frames, returns PhysAddr
  - Other tiers return `None` (drivers not yet implemented)
- `inventory.rs` — Hardware Inventory & System Architecture with:
  - `HardwareInventory::collect(pci_devices, acpi_info)` — CPU count, RAM, PCI device detection (VirtIO-net/GPU, NVMe, XHCI)
  - `SystemArchitecture::infer(inv)` — rule-based heuristics: GPU detect → ring1, RAM size → heap, CPU count → power mode
  - Both pure data structures for future MLP weight training (item #51)
- `memory.rs` — `BitmapFrameAllocator::usable_memory_bytes()` public accessor
- **Adaptive boot flow** — `main.rs` now runs: PCI scan → HardwareInventory::collect() → SystemArchitecture::infer() → log to VGA+serial → MHI init → NeuralExecutor. Example output: `[ARCH] ring0=0 ring1=0 heap=2048MB` / `[MHI] 1 tier(s), X MB usable.`
- **Workspace crate versions** — `neural-kernel` bumped to v0.16.0

## [0.15.0] — 2026-06-23

### Added (Sprint 20 — Block 3: Hermes Chat)

- `hermes.rs` — Hermes Chat console module with:
  - `IntentMlp` — real MLP intent classifier: bag-of-words (16-word vocab) → Linear(16→8) → SiLU → Linear(8→3) → argmax (3 intents: chat, status, echo)
  - Hand-crafted weights for keyword-based classification (status/memory/ram/cpu/system → status intent; echo/reverse → echo intent; hello/hi/help → chat intent)
  - `parse_command()` — multi-word command parser: `/status`, `/echo <text>`, `/help`, `/stats`, `/mem`
  - `Command` enum: Status, Echo(String), Help, Chat(String)
- **scancode_to_ascii()** — expanded with digits 0-9 (0x02-0x0B) and punctuation (`- = [ ] ; ' ` \ , . /`) for full command-line input
- **intent_router_daemon** — upgraded from mock string-contains to:
  - `parse_command()` dispatches `/status` and `/echo` to SkillRegistry
  - Unrecognized text → `INTENT_MLP.classify()` → routes to SystemStatusSkill (intent 1), EchoSkill (intent 2), or default chat response (intent 0)
  - Publishes responses on `HERMES_RESPONSE` EventBus topic
- **hermes_console_daemon** — subscribes `HERMES_RESPONSE`, prints `[Hermes] <response>` to both VGA and serial
- Both new daemons spawn in the NeuralExecutor (6 tasks total)

### Changed

- `main.rs` — added `mod hermes;`, `INTENT_MLP` lazy_static, expanded scancode table, upgraded intent_router + new console daemon

## [0.14.1] — 2026-06-23

### Fixed (Sprint 19 — SMP Multi-Core Boot)

- **Root cause isolated:** bootloader identity-maps pages 0-7 only (PD[0] = 0x4023 → PT base = 0x4000). PT[64] for VA 0x40000 was `0x0000000000000000` → AP #PF on first instruction at 0x400A4 → triple fault
- **Identity-map PTE fix:** single `write_volatile` at `phys_offset + 0x4200` writes PTE `0x40000 | 0x003` (Present|Write) — AP can fetch from VA 0x40000 after enabling paging
- **CPU_COUNT race condition:** `spin::Mutex` protects `fetch_add` because QEMU TCG lacks cross-vCPU atomicity; all APs previously read same counter value
- **50ms busy-wait** after second SIPI for accurate AP count (all 3 APs finish trampoline within <20ms)
- **Slab Allocator memory corrupt fix:** `SLAB_CHUNK_SIZE` = bucket_size (not aligned to 8); free list pointer stored before chunk, retrieved via `ptr.read::<*mut u8>()`
- **asm! memcpy:** Replaced `core::intrinsics::copy_nonoverlapping` with `asm!("rep movsb")` to avoid `native_memcpy` dependency in `no_std`

### Changed

- `smp/mod.rs` — identity-map PTE written directly via raw pointer (not OffsetPageTable mapper); `AP_BOOT_LOCK: spin::Mutex<()>` around CPU_COUNT increment; 50ms busy-wait after SIPI
- `smp/trampoline.rs` — replaced `copy_nonoverlapping` with `asm!` block for zero-dependency memcpy
- `slab.rs` — `SLAB_CHUNK_SIZE` = bucket_size (not `align_up(bucket_size, 8)`); corrected `put()` free list logic

### Result

- `-smp 2`: ✅ AP 1 boots — `[SMP] AP 1 entrou em modo 64-bit Rust!` → `APs acordados: 1`
- `-smp 4`: ✅ AP 1, 2, 3 boot — `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception` lines — no #UD, #PF, #GP
- Sprint 19 (Block 2) now fully operational

## [0.14.0] — 2026-06-23

### Added (Sprint 19 — Block 2: SMP + Slab + Heap 4 MB)

- `allocate_below_1mb()` — BitmapFrameAllocator aloca frame < 1 MiB para trampoline real-mode (`src/memory.rs`)
- `PHYS_MEM_OFFSET` — AtomicU64 global com offset de memória física para acesso de qualquer módulo (`src/memory.rs`)
- Slab Allocator — 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096), free list ligada, `Mutex<SlabAllocator>` com métricas atômicas (`src/slab.rs`)
- Heap expandido de 100 KB para 4 MB — primeiros 512 KB para Slab, restante 3.5 MB para LockedHeap (`src/allocator.rs`)
- PerCpu struct (repr(C), 64 bytes) com self_ptr, cpu_id, lapic_id, bsp_flag, ring. GS.base via wrmsr(0xC0000101) (`src/smp/percpu.rs`)
- `this_cpu()` — lê gs:[0] para obter ponteiro PerCpu. `cpu_id()` lê gs:[8]
- Trampoline assembly (global_asm!) — 16-bit → 32-bit protected → PAE → EFER.LME → paging → 64-bit long mode → Rust entry. Header patcheable de 48 bytes com campos jmp32/jmp64/cr3/stack/percpu/entry_fn (`src/smp/trampoline.rs`)
- INIT-SIPI-SIPI via LAPIC ICR — `send_init_ipi()`, `send_sipi(vector)` com entrega via shorthand "all excluding self" (`src/apic.rs`)
- `wait_for_ipi_delivery()` — spin até ICR delivery status clear. `lapic_id()` — LAPIC ID register (offset 0x20)
- SMP orchestrator — `init_smp()` aloca trampoline, identity-maps, patcha, dispara INIT-SIPI-SIPI (`src/smp/mod.rs`)
- `ap_entry()` — entry point chamado pelos APs em modo 64-bit

### Changed

- `main.rs` — `mapper` scoped no boot flow para evitar aliasing com o mapper do SMP init
- Boot flow: adicionados `mod smp`, `mod slab`, `crate::smp::init_smp()` antes do NeuralExecutor

## [0.13.0] — 2026-06-23

### Added (Sprint 18 — Block 1)

- PCI scan — CF8/CFC config space access, 256 bus × 32 device enumeration, vendor/device/class/BARs (`crates/neural-kernel/src/pci.rs`)
- ACPI parser — RSDP discovery (EBDA + BIOS area), RSDT/XSDT walking, MADT LAPIC/IOAPIC/x2APIC parsing (`crates/neural-kernel/src/acpi.rs`)
- APIC init — LAPIC SVR + TPR + timer masked, IOAPIC IRQ0→vec32 + IRQ1→vec33, PIC disable (`crates/neural-kernel/src/apic.rs`)
- Dual EOI — `USING_APIC: AtomicBool` + `send_eoi()` com fallback APIC/PIC para handlers
- Boot flow: `init_pci()` → `init_acpi()` → `init_apic(info)` (fallback PIC se sem ACPI)

- Hardware Neural Routing — IRQ1 keyboard → EventBus → Agent pipeline (`crates/neural-kernel/src/main.rs`)
  - Top-Half: `keyboard_interrupt_handler` (IDT[33]) lê porta 0x60 → `LAST_SCANCODE` (AtomicU8, Release) → EOI raw
  - Bottom-Half: `hw_bridge_daemon` (async task) poll AtomicU8 → publica `RAW_HW_IRQ1` no EventBus
  - `input_daemon` (async task) subscreve RAW_HW_IRQ1 → buffer String → `scancode_to_ascii()` → ENTER publica `USER_INTENT`
  - `intent_router_daemon` (Cortex) subscreve USER_INTENT → mock inference → `SkillRegistry::execute_skill`
- Closed Intent Pipeline (Sprint 16)
  - `SystemStatusSkill` — lê `global_hardware_context()` via TicketLock, loga `"Memoria RAM: {:.2}%"`
  - 5 tasks spawnadas (3 persistentes), 1000+ PIT ticks estáveis, zero Double Faults
- `TicketLock` FIFO crate (`crates/ticket-lock/src/lib.rs`)
  - `TicketLock<T>` — `AtomicUsize ticket/serving`, `UnsafeCell<T>`, spin loop justo
  - Garantia FIFO, `Send` + `Sync` para T: Send
  - `TicketLockGuard` com `Deref`/`DerefMut` e incremento `serving` no Drop
- EventBus refatorado para TicketLock
  - `EventBus.subscribers`: `spin::Mutex` → `TicketLock<BTreeMap<...>>`
  - `Receiver.queue`: `Arc<TicketLock<VecDeque<Event>>>`
  - ID counter: `Arc<AtomicU64>` (was raw u64)
- `GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>` — frame allocator encapsulado
- `init_global_allocator()` — migra frame allocator para TicketLock pós-boot
- `global_hardware_context()` — acesso thread-safe via TicketLock
- NeuralExecutor simplificado: campo `frame_allocator` removido, usa `global_hardware_context()`
- `sync` module (`crates/neural-kernel/src/sync/`) — re-exporta `ticket_lock::*`
- ADR-0013: Neural OS Executive Summary (SotA 2026)

### Changed

- EventBus modernizado: `spin::Mutex` substituído por `TicketLock` (Sprint 17)
- BitmapFrameAllocator agora protegido por `TicketLock` (não mais por `spin::Mutex`)
- NeuralExecutor não gerencia mais frame_allocator — acesso global via TicketLock
- `interrupts.rs` — expandido com handlers: GPF, Stack Segment, Segment Not Present, Invalid TSS, Alignment Check

## [0.12.0] — 2026-06-22

### Added

- Async Neural Executor (`crates/neural-kernel/src/task/`)
  - `pub struct AgentTask { id: u64, future: Pin<Box<dyn Future>> }` — with `AtomicU64` ID generation
  - `pub struct NeuralExecutor { task_queue: VecDeque<AgentTask> }` — cooperative polling loop
  - `DummyWaker` via `RawWakerVTable` — no-op waker for `no_std` environments
  - `pub fn run(&mut self)` — replaces `loop { hlt() }`; polls tasks, logs hardware context every 100 iterations
- Event Bus IPC (`crates/event-bus/`)
  - `CapabilityToken`, `Event`, `EventBus` with publish/subscribe via `BTreeMap + spin::Mutex`
  - `Receiver::try_receive()` for non-blocking polling
  - `yield_now().await` for explicit cooperation
- Skill Registry & MCP Layer (`crates/skill-registry/`)
  - `trait Skill: Send + Sync` with `manifest()` + `execute()`
  - `SkillRegistry` with Zero-Trust CapabilityToken validation
  - `EchoSkill` — reverses payload bytes
  - `SystemStatusSkill` — logs RAM occupancy via hardware context
- `async fn system_daemon()` — test agent that spawns, executes, and completes
- `async fn hardware_monitor_daemon()` — publishes SYSTEM_READY with Token(1)
- Boot sequence: `NeuralExecutor::run()` instead of raw `hlt` loop

## [0.11.0] — 2026-06-22

### Added

- `BitmapFrameAllocator` — 128 KB `.bss` bitmap covering 4 GB physical memory
- `init(&mut self, memory_map)` — varre UEFI MemoryMap, marca `Usable` como livre, o resto ocupado
- `FrameAllocator<Size4KiB>` + `FrameDeallocator<Size4KiB>` — alloc/dealloc reais com busca linear
- `allocate_contiguous(count)` — aloca N frames contíguos para Huge Pages (2 MiB / 1 GiB)
- `hardware_context_tensor() -> [f32; 2]` — `[taxa_ocupacao, 0.0]` via contador de alocações
- Stress test: 1000 alloc/dealloc estáveis, 0% leak, RAM Tensor confirmado em QEMU
- `PackedTernaryTensor` struct (`crates/neural-kernel/src/tensor.rs`) — 2-bit per weight, 4 weights per byte
- `pack_weights()` + `get_weight()` — pack/extract 2-bit ternary values
- `matmul_hybrid()` on `PackedTernaryTensor` — reads weights bit-by-bit from packed storage
- `quantize_to_packed(tensor, threshold)` — f32→ternary calibration
- ADR-0012: 2-bit Packing and Ternary Quantization

### Changed

- `nn::BitLinear` — `weights` field changed from `TernaryTensor` to `PackedTernaryTensor`
- `main.rs` — BitNet test now uses quantization + packed inference flow
- Monorepo workspace: `src/` movido para `crates/neural-kernel/src/`

## [0.10.0] — 2026-06-21

### Added

- `TernaryTensor` struct (`src/tensor.rs`) — weight storage as `Vec<i8>` with values in {-1, 0, 1}
- `TernaryTensor::from_row_major()` — constructor with shape validation
- `TernaryTensor::matmul_hybrid(input: &Tensor) -> Option<Tensor>` — ADD/SUB-only kernel
  - Weight `+1` → `accumulator += input[t]`
  - Weight `-1` → `accumulator -= input[t]`
  - Weight `0` → skip (no multiplication)
- `nn::BitLinear` struct (`src/nn.rs`) — ternary dense layer
  - `forward()` = `matmul_hybrid()` + optional bias
- BitNet hybrid inference test in boot flow
  - Input `[1.5, -0.5, 2.0]` × TernaryTensor(3×2) → `[-0.5, -2.0]`
  - Zero multiplication operators in the inner loop
- ADR-0011: BitLinear and Hybrid Ternary MatMul

## [0.8.0] — 2026-06-21

### Added

- `pic8259 = "0.10"` dependency — 8259A PIC driver with `ChainedPics`
- PIC remap (PIC1 → vector 32, PIC2 → vector 40) — `interrupts::init_pics()`
- PIT Timer watchdog handler (IRQ 0, vector 32) — atomic `TIMER_TICKS` counter + EOI
- Page Fault handler (vector 14) — reads `CR2`, logs fault address, halts via `hlt`
- `interrupts::enable_interrupts()` — `sti` instruction sets IF=1
- `memory.rs:FrameDeallocator` trait — `deallocate_frame()` for future frame recycling
- `EmptyFrameDeallocator` — no-op stub until bitmap allocator
- ADR-0009: PIC Watchdog and Page Fault Safety

### Changed

- `src/interrupts.rs` — IDT extended with `page_fault` and `idt[32]` (timer)
- `src/main.rs` — `init_pics()` + `enable_interrupts()` + watchdog `hlt` loop
- `src/memory.rs` — `FrameDeallocator` trait + `EmptyFrameDeallocator` added

## [0.7.0] — 2026-06-21

### Added

- `Tensor::transposed()` — row-major to column-major transposition (W^T support)
- `nn::Linear` struct with `weights: Tensor` and `bias: Option<Tensor>`
  - `forward(&self, input) -> Tensor` implements Y = X·W^T + B
- `nn::argmax(tensor) -> usize` — returns index of highest logit
- Intent Router MLP in boot flow
  - Input embedding + Linear(3→2) + SiLU + argmax = kernel decision
  - Tested: `[1.0, -0.5, 0.3]` → action 0 (Acionar Daemon Ring 2)
- ADR-0007: Intent Router MLP — Primeiro Córtex Primitivo

## [0.6.0] — 2026-06-21

### Added

- `libm = "0.2"` dependency for `no_std` math functions (`expf`, `sqrtf`)
- Neural primitives module (`src/nn.rs`)
  - `silu(x)` activation via `libm::expf` — tested: `[-1, 0, 1] → [-0.269, 0, 0.731]`
  - `rms_norm(tensor, weight, eps)` via `libm::sqrtf` — tested: RMSNorm of SiLU output
- `Tensor::add_scalar`, `Tensor::mul_scalar`, `Tensor::apply<F>` (generic closure)
- `nn::silu` used as closure arg to `Tensor::apply` in boot test
- ADR-0006: Neural Primitives and libm

## [0.5.0] — 2026-06-21

### Added

- SIMD enablement module (`src/simd.rs`)
  - `enable_simd()` — CR0: clear `EMULATE_COPROCESSOR`, set `MONITOR_COPROCESSOR` + `NUMERIC_ERROR`
  - CR4: set `OSFXSR` + `OSXMMEXCPT_ENABLE`
  - `f32`/`f64` operations now execute natively without `#NM` exceptions
- Tensor Engine module (`src/tensor.rs`)
  - `Tensor` struct with `shape: (usize, usize)` and `data: Vec<f32>`
  - `from_row_major()`, `matmul()` — dot product multiplication
  - Tested: 1×3 × 3×1 → 1×1 = `[32.0]`
- `simd::enable_simd()` call in boot flow after heap init
- ADR-0005: SIMD and FPU Enablement

### Changed

- `main.rs`: added `mod simd; mod tensor;` + tensor matmul test

## [0.4.0] — 2026-06-21

### Added

- Memory module (`src/memory.rs`)
  - `OffsetPageTable` — cria mapper via `Cr3::read()` + `physical_memory_offset`
  - `BootInfoFrameAllocator` — implementa `FrameAllocator<Size4KiB>` iterando mapa UEFI/BIOS
  - `init_memory(offset)` — retorna `OffsetPageTable<'static>` pronto
- Heap allocator module (`src/allocator.rs`)
  - `LockedHeap` como `#[global_allocator]` via `linked_list_allocator` v0.9.1
  - `init_heap(mapper, frame_allocator)` — mapeia 25 páginas (100 KB) em `0x4444_4444_0000`
- `extern crate alloc` ativado — `Box::new(41)` e `Vec::push([10, 20, 30])` testados em QEMU
- `linked_list_allocator = "0.9"` dependency
- ADR-0004: Memory Paging and Heap Allocation
- SESSION_004.md: Sprint 4 detailed log

## [0.3.0] — 2026-06-21

### Added

- IDT (Interrupt Descriptor Table) module (`src/interrupts.rs`)
  - Breakpoint handler (`#BP`, vector 3) — logs VGA + serial, returns
  - Double Fault handler (`#DF`, vector 8) — logs VGA + serial, panics
  - TSS with IST entry 0 (20KB dedicated stack) for Double Fault stack switching
  - GDT with kernel code segment + TSS descriptor
  - `init_idt()` — loads GDT, sets CS, loads TSS, loads IDT
- `x86_64` crate v0.14.11 dependency (IDT, GDT, TSS, CPU instructions)
- `#![feature(abi_x86_interrupt)]` for `extern "x86-interrupt"` calling convention
- Forced `int3()` breakpoint test in boot flow
- ADR-0003: Interrupt Descriptor Table
- SESSION_003.md: Sprint 3 detailed log
- QEMU path added to `PATH` documentation for Windows

### Fixed

- Handler signature adapted to `x86_64` v0.14.13 API (`InterruptStackFrame` by value)
- `static_mut_refs` warning — replaced `&STACK` with `core::ptr::addr_of!(STACK)`
- Deprecated `set_cs` — replaced with `CS::set_reg()` via `Segment` trait
- Macro scoping — explicit `use crate::{println, serial_println}` in interrupts module

## [0.2.0] — 2026-06-21

### Added

- VGA text mode output via `map_physical_memory` feature (`vga_buffer.rs`)
  - `Writer` with scrolling, 16-color support, `core::fmt::Write` impl
  - Macros `print!` / `println!` for kernel-wide use
  - Buffer mapped at runtime using `physical_memory_offset` from `BootInfo`
- Serial port logging via `uart_16550` crate (`serial.rs`)
  - 16550 UART initialization at port `0x3F8`
  - `lazy_static!` + `spin::Mutex` for safe global access
  - Macros `serial_print!` / `serial_println!`
- Dual-output panic handler in `main.rs`
  - `panic!()` writes to both VGA and serial simultaneously
- New crate dependencies: `spin` v0.9, `lazy_static` v1.5, `uart_16550` v0.2
- `bootloader` as regular dependency (kernel-side `BootInfo` type with `map_physical_memory`)
- ADR-0002: VGA and Serial Logging Infrastructure

### Changed

- Entry point migrated from raw `extern "C" fn _start()` to `bootloader::entry_point!(kernel_main)`
- VGA base address computed as `0xB8000 + physical_memory_offset` (runtime, not hardcoded)
- `STATE.md` updated with Sprint 2 progress

## [0.1.0] — 2026-06-21

### Added

- Initial bare-metal Rust kernel scaffold
  - `#![no_std]` + `#![no_main]` environment
  - Minimal panic handler (infinite loop)
  - Serial init and output via raw port I/O
- Bootloader integration (`bootloader` v0.9.34 build-dep)
  - `bootimage runner` for automated QEMU launch
  - `relocation-model=static` to produce `ET_EXEC` ELF (fixes bootloader compatibility)
- Toolchain configuration
  - `rust-toolchain.toml` pinned to nightly
  - `.cargo/config.toml` with target and runner
- Documentation protocol
  - ADR-0001: Initial Architecture and Toolchain
  - State tracker (`STATE.md`)
  - Session log (`SESSION_001.md`)
- MSYS2 + MinGW-w64 setup for Windows toolchain without MSVC
- `AGENTS.md` — system rules for AI-assisted IDEs
