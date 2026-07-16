# ═════════════════════════════════════════════════════════
#   STATE — neural-os-core v1.7.1
#   SPRINT 107 — Adequação ADR-0042 + Voice I/O (marco 2B LOADED)
#   Cadeia: k-nano → k-ai → cortex → hermes → jarbas
#   Áudio/voz: ADR-0045 (Sound Voice Stack)
# ═════════════════════════════════════════════════════════

## Roadmap Atual
**Versão:** **v1.7.1** (2026-07-16) — ADR-0045 Sound (docs); funcional permanece marco **v1.7.0** (N1 ✅ + 2B LOADED).  
**Gate `v2.0.0`:** ainda N1–N5 completos (ADR-0042) — **não** declarar v2.0.  
**Cadeia canônica:** `k-nano → k-ai → cortex → hermes → jarbas`.  
**Nota:** 1.6.0-dev absorvida por 1.7.0 (sem tag `v1.6.0`).

### Sound / Voice (ADR-0045)
| Item | Estado |
|------|--------|
| Truth path | `neural-kernel/src/audio/*` (boot) |
| Espelho | `jarbas/src/audio/*` — **não wired** ao bin |
| Stack | HDA + Piper (+formant) + STT CTC + VAD + mixer + FB TTS paint |
| WakeWord | código existe; **não registrado** |
| UAC | stub (#84 futuro) |
| Obsoleto | sherpa / Pocket / Kokoro-primário / Vosk / Wyoming / Rustpotter |

### Adequação N0–N5 (ADR-0042)
| Fase | Status |
|------|--------|
| **N0** Baseline boot Runtime | ✅ |
| **N1** k-nano legível | ✅ N1.1+N1.2+N1.3 |
| **N2** k-ai HW-AI / SelfHeal | ⏳ |
| **N3** cortex cérebro | ▶️ **parcial** — **2B LOADED** + FWD + BPE HF + **chat frame Llama** (`prompt_len=6`) + soft_stride=3 + constrained clima (`' tempo rain'`) |
| **N4** hermes orquestra | ✅ STT-sim → Hermes → generate_via_model (path clima) |
| **N5** jarbas ego/UI | ▶️ TTS `pcm_samples>0` + **FB paint** (`[JARBAS-TTS-FB] painted`); Piper LOADED / formant fallback |

### Evidência clima e2e (2026-07-16 — `logs/boot_whpx_20260716_012934.txt`) — Sprint 107 cont.
| Critério | Resultado |
|----------|-----------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` `-Window -Smp 2`; soft_stride FWD ok |
| Chat template | ✅ `prompt_len=6` `first=128000 last=128007` (BOS+cue+eot+assistant hdr) |
| soft_stride | ✅ `[FWD] soft_stride=3 layers≈10/30` |
| GEN | ✅ `decoded_len=11 text=' tempo rain'` — **weatherish** (tempo+rain), não mash EN / não `"6666"` |
| Constrained | ✅ `argmax_row_weather_only` no lexicon (logits reais; sem string canned) |
| TTS | ✅ `pcm_samples=13920` (formant; emb invalid) |
| FB | ✅ `[JARBAS-TTS-FB] painted len=11 1280x800` |
| Experts | ✅ RUSTCODER LOADED · STT CTC LOADED · BGE LOADED · ❌ HWEXPERT parse FAILED |
| HIT e2e | ✅ `readable + weatherish=True + pcm + fb` |
| **Veredito clima** | **PARTIAL** — melhor que mash EN; ainda não frase PT climática plena (`O tempo está bom…`) |

**Ops rebuild:** `CARGO_TARGET_DIR` sandbox mascara `cargo nk` → forçar `$env:CARGO_TARGET_DIR=…\target` + `bootloader_linker -u` (crate `boot` trava em `cargo install bootloader-x86_64-uefi`).

### Evidência clima e2e REAL (2026-07-15 — `logs/boot_whpx_20260715_185914.txt`)
| Critério | Resultado |
|----------|-----------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` sem `-NoSerialBridge`; kill 18 min |
| BPE | ✅ `[BPE] BPB1 LOADED vocab_n=128256` @0x150000000 |
| GEN | ✅ IDs HF reais `bpe=1 first=128000 last=24108` (`Ġtempo`); `decoded_len=50` com letras (não `"6666"`) |
| Texto PT clima | ❌ saída mash EN (` importantlyabil-worker…`) — não frase climática |
| TTS | ✅ `pcm_samples=59040` (formant; `emb invalid`) |
| FB | ✅ `[JARBAS-TTS-FB] painted len=50 1280x800` |
| Canned | ✅ texto vem do decode do modelo |
| **Veredito clima** | **FAIL fechar pleno** / **PASS mínimo anti-`6666`** (letras+len+TTS+FB) — **superseded** pela evidência 2026-07-16 |

**Root `"6666"`:** `.bitnet` só embute stub `CHAR:32-126`; sem BPE, encode CHAR aponta embeddings HF errados e `argmax_row_char_vocab` travava em token 25 = `'6'`.

**Fix parcial:** `tools/export_bpe_bin.py` → `target/bpe_vocab.bin` (BPB1); loader QEMU; tokens `u32`; chat frame Llama 6 toks; soft_stride=3; constrained weather lexicon; e2e exige letras + weatherish≥2 hits.

**Blocker restante:** soft-float 2B — logits HF ainda fracos p/ frase PT gramatical; Piper `emb.weight`; HWEXPERT parse FAILED (magic OK); float/AVX path ou merges BPE encode pleno.

### Evidência multi-token smoke antigo (2026-07-15 — `logs/boot_whpx_20260715_155416.txt`)
| Item | Resultado |
|------|-----------|
| Bridge | via `run_weather_e2e.ps1` + `-Window` (GUI); kill 15 min |
| GEN | `max_gen=4` → `decoded_len=4 text='6666'` (CHAR argmax; qualidade baixa = dígito repetido) |
| TTS | `[JARBAS-TTS] 6666` + `pcm_samples=1600` (formant fallback; `emb invalid len=0`) |
| FB | `[JARBAS-TTS-FB] painted len=4 1280x800` — splash Orb + texto na janela QEMU |
| HIT e2e | `HIT multi-token+TTS … decoded>=4 pcm ok fb=True` (**obsoleto** — critério endurecido) |

**Antes (smoke 1-token):** `logs/boot_whpx_20260715_145128.txt` — `decoded_len=1` `"6"`; FB só DIAG/AIOS.

### Evidência clima e2e + bridge (2026-07-15 — `logs/boot_whpx_20260715_145128.txt`)
| Item | Resultado |
|------|-----------|
| Bridge | `[BRIDGE] started` → QEMU → `[BRIDGE] killed` (PS1 finally; **sem** `-NoSerialBridge`) |
| Status | `llm=LOADED bge=LOADED piper=LOADED` |
| GEN | `prompt_len=2 (raw=39)` (nao mais EOS-sozinho) → `next=25` → `decoded_len=1` → texto `"6"` |
| TTS | `[TTS] Piper: "6" (1600 samples)` + `[JARBAS-TTS] piper=LOADED pcm_samples=1600` |
| Clima e2e | **PASS** smoke antigo (generate nao-vazio + samples>0) — **não** fecha qualidade |

**Root cause empty generate:** soft-float 2B truncava prompt ao **ultimo token = EOS** (`prompt_len=1`) → argmax→EOS → string vazia; vocab HF 128k + decode CHAR sem BPE tambem devolvia vazio. Fix: slim BOS+1 char + argmax no range CHAR.

**Loop:** attempt1 `prompt_len=5` timeout FWD; attempt2 generate OK + Piper panic `%0`; attempt3 PASS 1-token; attempt4 multi-token `max_gen=8` timeout 10m (3 steps); attempt5 `max_gen=4` + 15m kill → PASS smoke; attempt6 BPE HF → letras mas não PT clima.

**Ops:** soft-float + `cargo nk`; QEMU 6G/SMP; timeout ~18 min BPE; helper `tools/run_weather_e2e.ps1`. Ver `SESSION_108.md`.

### Micro-experts QEMU loaders (2026-07-16)
Mapa phys (após BPE `@0x150000000`): **HW Expert** `@0x160000000` (`hw_expert_v3.bitnet` ~260KB), **RustCoder** `@0x161000000` (`rust_coder.bitnet` ~270KB), **BGE** `@0x162000000`, **STT** `@0x163000000`. Kernel consome via QEMU-loader + fallback FAT (`HWEXPRT.BIN` / `RUSTCDR.BITNET`). Alias legado `hw_expert.bitnet` ausente no disco — usar v3/tf. Regenerar: `python tools/train_hw_expert_v3.py`, `python tools/download_and_train.py --rustcoder`.

**Boot 2026-07-16 (`012934`):** `[RUSTCODER] LOADED` · `[STT] CTC LOADED` · `[BGE] …LOADED` · `[HWEXPERT] parse FAILED` (magic OK @0x160000000 — layout/size hint).

### Serial SLIP bridge (bypass NIC)
- Script: `tools/serial_bridge.py` — TCP **server** `127.0.0.1:4444`; QEMU COM2 = **cliente** (`-serial tcp:127.0.0.1:4444`, sem `server=on`).
- Lifecycle: `run-qemu-whpx.ps1` sobe bridge antes do QEMU e mata no `finally`. PS1 = **ASCII-only** (em-dash UTF-8 partia parser PS5/CP1252). `Test-PortListening` cruza netstat (Get-NetTCPConnection pode mentir vazio).
- Skip: `-NoSerialBridge`. `-Bridge` = WinTAP (distinto do SLIP).

### Piper + BGE (2026-07-15)
| Item | Antes | Agora |
|------|-------|-------|
| **Piper** | skip / ausente | LOADED via QEMU-loader; `emb.weight` lookup ainda fraco (`0M params` / formant fallback sem panic) |
| **Weather TTS** | FAILED empty generate | generate real + `pcm_samples>0` (texto ainda pobre) |
| **BGE** | FAILED | **LOADED** stub |

### Próximo
- **Clima pleno:** frase PT gramatical (ex. `O tempo esta bom`) — float/AVX FWD ou lexicon+ordem melhor; hoje constrained → `' tempo rain'`
- Fix HWEXPERT `load_model` parse FAILED; Piper neural (`emb.weight`); merges BPE encode pleno
- **Voice (ADR-0045):** registrar WakeWordAgent; fechar TTS→STT→LLM→TTS; depois UAC (#84) / wiring `jarbas/audio`
- N2 SelfHeal gated; Gate `v2.0.0` = N1–N5 only
- Ops: sempre `CARGO_TARGET_DIR=repo\target` + `bootloader_linker` (evitar hang `cargo build -p boot`)

### Identidade funcional K²CHJ (ADR-0042)
| Anel | Função |
|------|--------|
| **k-nano** | Sistema **legível** (HW bruto, Caps, CR3, log honesto) |
| **k-ai** | AI **para hardware** + SelfHeal + HMI de máquina |
| **cortex** | **Cérebro** — MoE, learn, busca, mmap pesos |
| **hermes** | **Orquestrador** agentic — intent, skills, criação |
| **jarbas** | **Ego / persona / +10%** — UI, humor, frontend |

**Nota ops:** Builds isolados sob `target/` (`target/agent-*`, `target/check-*`, `target/n16-*`). Rebuild `uefi.img` via `cargo build --release -p boot` (pode travar em `cargo install` bootloader — liberar lock `.cargo`).

### Boot endurecido + Capability Rings (2026-07-14)
| Pacote | Conteúdo | Status |
|--------|----------|--------|
| **A** | STI+PIC, stack heap ≥2MB, `init_phase` RR, `BOOT_PHASE`+consumer, DiagnosticSkill | ✅ |
| **B** | `init_platform_sync` antes dos drivers; Platform/NetDriver idempotente; Agency → EventDriven | ✅ |
| **MVP C→P9** | ADR-0041: AS/CR3/SPSC/Cap/`int 0x90` + CapGate + FB + DMA/mmap + Ring3 + #PF + vring + GGUF | ✅ PoC |

### Real vs stub (pós P0–P9)
| Peça | Real | Stub / limite |
|------|------|----------------|
| 2 AS + CR3 + SPSC + Cap + `int 0x90` | ✅ | Shallow L4 (PTs kernel compartilhadas) |
| CapGate Hermes (`aios_*`) | ✅ | Sem AS separado / SFI pleno (#426 🟡) |
| JARBAS FB map + present | ✅ | VSync stub; path = bootloader FB |
| DMA pin + weight mmap | ✅ | Pesos simulados no eager path |
| Ring3 `iretq` + stub | ✅ código | **Untested QEMU estável**; `TRY_ENTER_RING3`; sem ELF/preempt |
| Demand-page #PF | ✅ | Frames pré-alocados; **sem I/O no fault** |
| VirtIO vring | ✅ layout+pin | **Sem QUEUE_NOTIFY**; NIC live observe-only |
| GGUF/FAT mmap | ✅ pré-fill 1–4 pág. | Prefixo só; fallback `NFIL`; sem streaming 8GB |

### K²CHJ Capability Rings — P0–P9 (ADR-0041) — todos ✅ PoC
P0 gap · P1 ADR · P2 MVP C · P3 CapGate · P4 FB · P5 DMA/mmap · P6 Ring3 · P7 #PF · P8 vring · P9 GGUF/FAT.  
**Módulos:** `address_space`, `syscall`, `ipc/*`, `capability_gate`, `jarbas_fb`, `k_ia_dma`, `cortex_mmap`, `user_mode`, `demand_page`, `virtio_vring`, `gguf_mmap` + demos non-fatal em `main.rs`.

**Riscos / follow-ups:** Ring3 default `TRY_ENTER_RING3=false` (PoC); VirtIO sem QUEUE_NOTIFY; #PF sem I/O; telemetria modelo ainda inconsistente (alvo N1); Agency EventDriven ociosa sem eventos; crates K²CHJ ≠ bin até wiring; **Boot OK ≠ visão completa** (ADR-0042).

## Marcos Acumulados
- **🏆 v1.7.0 (2026-07-15):** N1 ✅ + BitNet 2B LOADED (~590MB, 30L, FWD); soft-float/`cargo nk`; TTS empty known. Ver `SESSION_108.md`.
- **🏆 v1.5.7 (2026-07-14):** Boot A/B + ADR-0041 capability ladder P0–P9 (PoC non-fatal). Ver `SESSION_107.md`.
- **🏆 v2.0.0 (2026-07-14):** Sprint 106 completa (10/10). Workspace estrito com 5 crates K²CHJ. SOUL.md via VFS (`neural_kernel::fs::read_vfs`). MicroPython/WASM sandbox (`micropython_wasm.rs`). SkillOpt + AIOS API. Heap em `0x4000_0000_0000` para HW real. **0 erros.**
- **🏆 v1.5.3 (2026-07-13):** Ponytail audit 100% implementado. 6 dead files → LEGACY/v1.5-dead-k2chj/.
- **🏆 v1.5.2 (2026-07-13):** 0 erros. RingBufStore extraído em fs/mod.rs (ram_fs + log_fs delegam para tipo genérico com evicção FIFO). LEGACY/v1.5-neural-kernel-src/ snapshot criado — baseline para migração v2.0.
- **🏆 v1.5.1 (2026-07-13):** 0 erros. ~600 LOC removidos, 11 dep entries eliminados. 6 dead files movidos do neural-kernel para K²CHJ crates. pic8259 eliminado. #[cfg(not(x86_64))] branches removidos. Architecture trait removido.
- **🏆 v1.5.0 (2026-07-13):** 0 erros. K²CHJ workspace migration: monólito → 5 crates (k_nano, cortex, k_ia, hermes, jarvis). Dep chain linear. k_nano compila independentemente. migrate_k2chj.py (193 files, 79 refs).
- **🏆 v1.2.0 (2026-07-12):** ATA PIO bug fix crítico — READ_SECTORS e IDENTIFY usavam `in al, dx+1` para byte alto (lendo FEATURES/ERROR). Fix: `in ax, dx`. TODO acesso a disco desde o início do projeto era lixo.
- **🏆 v1.1.5 (2026-07-12):** 0 erros, ~26.000 LOC, 116 firmwares. HW Expert v3 (61.453 VID/DID), SelfHealing I3/I4, WiFi Intel AX200 ucode loading, 3 camadas visuais (Orb + Hermes CLI + WM), HDA playback, BrowserAgent real, FFT audio.
- **🏆 B-01 MORTO (v0.109.3 — 2026-07-09):** O bloqueador de 18 sprints caiu. Serial tunnel TCP bridge resolveu o RX=0 que perseguia o projeto desde o início. Primeiro RX: 304 bytes.
- **v0.109.1** — Correção em massa: 32 erros de compilação mascarados pelo cache incremental. `cargo clean -p neural-kernel` revelou imports faltando, APIs trocadas, format string.
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 log + Xuvisco
- **v0.73.0-0.73.1** — Consciousness (10 métricas), Self-Improvement Loop, Shutdown tracking
- **v0.74.0-0.74.2** — TPM TIS driver, Ed25519 kernel signing, Partition mask 0x1C
- **v0.75.0-0.75.6** — FAT32-only, DiskIntelligenceAgent (680 LOC, 6 controllers, 10+ FS probes)
- **v0.76.0-0.76.1** — NVMe driver, S.M.A.R.T., Adaptive heap, Dynamic tick, Event-driven Hermes
- **v0.80.0-0.80.1** — AVX2 Debug, WHPX Detection, KV Cache (200x+ speedup)
- **v0.84.0-0.84.1** — GPU Foundations (BAR UC, SPSC job ring, VRAM alloc, secure boot)
- **v0.85.0** — GPU Decode (BitNet offload, CPU↔GPU KV cache DMA)
- **v0.86.0** — JARVIS Persona (SoulProfile, EmotionAnalysis, EgoLayer, Heartbeat)
- **v0.87.0** — Security + AHCI (TPM extend, Audit Trail, SATA 6G NCQ)
- **v0.88.0** — Emotion + Cache (EmotionEngine, SleepCycle, NeuralCache)
- **v0.89.0** — JARVIS Deep Cognitive (DreamEngine, BabelIndex, AutoSkillGen)
- **v0.90.0** — Desktop UI (JarvisDesktop compositor, Hermes Chat, Settings, Power)
- **v0.91.0** — LAN + Dependencies (DHCP, ARP cache, smoltcp upgrade)
- **v0.92.0** — WASM Runtime + IDE (MemoryPool, HybridRegistry, BitNet IDE F4)
- **v0.93.0-wasm** — WASM Skill Runtime refinado (+WASI mappings)
- **v0.94.0-0.94.1** — Vision + Display + TTF (TrueType font engine, tensor heatmap viz)
- **2026-07-06** — **v0.95.0-cog+v0.96.0-heal:** Sprint 95 (Cognitive Engine) + Sprint 96 (Self-Healing Avançado). cognitive.rs reescrito com 25+ itens (510+ LOC): IntentPlanner, SuccessEngine, NeuralCache, MatMulFreeLM, FeedbackLoop, TernaryUpdate, ReplayBuffer, WorkflowPredictor, AutoSkillGen, DynamicScaler, SelfOptScheduler, CodebookVQ (com KV Codebook e Finetune), ReActLoop, McpServer, DeltaBranches, WorkspaceIsolation, EpisodicMemory, BitNetTrainer, CandleSidecar, TaskSpawner, SleepCycleGuard. Sprint 96 completo com M1-M29 (350 LOC): ZeroCopySfs, SkillModule, FailureTaxonomy, ExceptionSelfHeal, CorrectivePrompting, Verifier, EventLog, BudgetedRecovery, SilentDetection, MultiLevelFailure, FailurePrediction, NotificationGate. +860 LOC totais. 0 erros.

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
247+ agentes: 20 nativos + 147 The Agency + ~80 importados + ~6 HW + ~6 FS.
Bootloader 0.11.15 com `bootloader_api`. Boot sequence agent-centric.

### Activation on Demand
Agentes só congestionam o tick-tock quando necessário.
- Apenas Hermes, Display, HwBridge usam `Continuous`
- Todo agente importado declara `on_demand: true` no manifesto
- AgentScheduler não polla sem evento pendente
- Penalidade: Continuous não-essencial >5% ticks → rebaixado para EventDriven

### DiskIntelligenceAgent (v0.75.x)
StorageController trait com 6 implementações (ATA, USB-MSC, NVMe, stubs AHCI/SCSI/VirtIO).
FilesystemProbe registry com 10+ probes (FAT32, NTFS, EXT4, XFS, ISO9660, exFAT, Btrfs, HFS+, EROFS, ReFS).
VolumeManagerProbe (LVM2, LUKS). GPT partition table. SED/OPAL detection.
S.M.A.R.T. monitoring (ATA READ DATA 0xB0+0xD0, health alerts).
ARC cache 1MB DRAM + tier migration MHI. I/O scheduler (batched writes). Read-ahead (32KB).

### MemoryAgent (v0.76.1)
Adaptive heap: `resize_heap_to_mb()` dinâmico via frame allocator + map_page_uc.
Orçamento calculado do modelo AI: `heap = clamp(128, params/10MB, 2048)`, `kv = params/40`.
CPU measurement via rdtsc. Dynamic tick calibration via LAPIC init_count.

### Security Stack
TPM 2.0 TIS driver (SHA256 embedded, PCR[8] extend, fallback silencioso).
Ed25519 kernel signing + auto-verification. Partition mask 0x1C (Hidden FAT32 LBA).

### Tick System (v0.76.1)
LAPIC timer com init_count dinâmico: 12-192 ticks/s baseado em agentes ativos.
Hermes event-driven: ReAct cycle só avança com entrada real (silêncio sem trabalho).
EventDriven scheduler fix: `has_event=true` + `has_pending()` early-return pattern.

### Agent Tier Classification
| Tier | Schedule | Exemplos |
|---|---|---|
| Permanent | Continuous | Hermes, Display, HwBridge |
| SystemDemand | EventDriven | DiskAgent, Cortex, Net |
| UserDemand | EventDriven | Skills, Apps, Plugins |
| Periodic | PollEvery(N) | Cron, Observer, Optimizer |
| Learning | PollEvery(2000) | Novos agentes → analisados 5000 ticks → promovidos |

## Roadmap v1.0 — Sprints 92-100 (Plano completo em docs/sprint-plan-92-100.md)

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| **92** | Fundação Estável (VirtIO, AHCI, serial, cleanup) | ~2.000 | ✅ Completa |
| **93** | WASM Runtime + IDE (wasmi, sandbox, marketplace) | ~3.200 | ✅ Completa |
| **94** | GPU Polish + Display (MSched, compositor, co-exist) | ~2.000 | ✅ Completa |
| **95** | Memory + VFS Final (BGE HNSW, MHI bridge, agents) | ~2.000 | ✅ Completa |
| **96** | GGUF + Model Loading (loader, streaming, RoPE) | ~1.500 | ✅ Completa |
| **97** | Rede + AIOS Evolution (WWW, self-update, marketplace) | ~3.000 | ✅ Completa |
| **98** | BitNet + Training Pipeline (100M params, fine-tune) | ~2.500 | ✅ Completa |
| **99** | SkillOpt + Structured Decoding + Code Freeze Prep | ~1.500 | ✅ Completa |
| **100** | **Code Freeze & Release v1.0.0** | ~500 | ✅ Completa |
| **Total v1.0** | | **~18.200 LOC** | |

**v2.0 "Cognição"** começa na Sprint 101: Kernel, Cortex, Hermes, JARVIS como entidade viva.

**Ver também:** `docs/sprint-plan-92-100.md` para detalhes de cada sprint.

## Aprendizados Chave
1. **Roadmap readequado 2026-07-04:** Reorganização completa por dependências. Itens independentes primeiro (Foundation → Agentic → LLM → JARVIS → GPU). B-01 e dependentes no final.
2. **Activation on Demand:** Hermes/Display/HwBridge (+ nativos Net/Input/Cortex…) Continuous; Agency SpecialistAgent → EventDriven (Pacote B).
3. **VGA CRTC + UEFI GOP = incompatível** (Sprint 71)
4. **Cortex acorda antes do HW** — LLM deve participar das decisões de hardware
5. **FAT12 removido** — FAT32-only, 102 LOC eliminados
6. **Partition mask 0x1C** — mbr_nostd aceita Hidden FAT32, bootloader OK, SO não monta
7. **TPM fallback** — silencioso se ausente (0xFFFF FFFF), Ed25519 como enforcement primário
8. **RX=0 persistente** — QEMU slirp + VirtualBox bridge, pre-existente (B-01)
9. **Hermes event-driven** — 84 linhas/seg → 0 quando ocioso
10. **Tick dinâmico** — calibrado por workload (12-192 t/s)
11. **Sprint 77** — 7 Foundation Quick Wins: Prompt `>`, Pre-Flight, FanOut, TaskSchema, SkillIndex, CompletionContracts, DynamicSkill. ~380 LOC, 0 erros.
12. **Sprint 78** — 8 Agentic Evolution items: IntentCache wiring, OutputCache wiring, WorkflowEngine wiring, SelfCritique, GgufBackedModel, AgentTier+migrate_to_tier, FsBridgeAgent, WasmExecutor+WasmSkill. ~400 LOC, 0 erros.
12. **VirtualBox SMP fix** — AP_COUNT static from MADT lapic_count. 2 vCPUs now boot reliably on VB.
13. **Sprint 79** — LLM Infrastructure: BitNet-b1.58 850M downloaded + .bitnet v2 conversion (1.5GB). AVX2 ternary matmul kernel. BPE tokenizer. Trinity MoE stub. QEMU loader boot pipeline at phys 4GB. Ramdisk via bootloader impossível (FAT limit). Forward pass blocked by GQA + BitFFN grouped projections.
14. **BitNet b1.58 real arch** — Microsoft's model is 850M params (not 2B). GQA (20 heads Q, 5 KV heads). BitFFN with grouped down_proj (640→6912). `tie_word_embeddings=true`. vocab_size=128256 (requires u32).
15. **QEMU loader strategy** — `-device loader,file=.bitnet,addr=0x100000000` com `-m 6G` + WHPX. Model in high memory avoids frame allocator conflicts. ~30s boot overhead acceptable for dev.
16. **Build_image.py UEFI issue** — bootloader 0.11.15 default features include UEFI. `default-features=false, features=["bios"]` avoids serde compile panic.
17. **VGA buffer clear fix (v0.79.1):** `[BOOT] FB ativo — VGA text mode desligado` agora é verdade. 0xB8000 limpo via `write_bytes` sem CRTC I/O. Framebuffer limpo para preto imediatamente no probe.
18. **VGA sequencer fix (v0.79.2):** `clear_physical_buffer()` write a 0xB8000 causa page fault pre-IDT. UEFI/OVMF não mapeia legacy VGA hole. Solução: VGA sequencer I/O (0x3C4/0x3C5) Screen Off bit — zero acesso a memória desmapeada.
19. **WHPX emula AVX2/VEX lentamente (v0.80.0):** CPUID mostra AVX2=disponível, mas cada instrução VEX causa VM exit (~10k+ ciclos). Scalar GP instructions rodam nativos. `has_avx2()` deve detectar WHPX via CPUID 0x40000000 e retornar false. AVX2 sob WHPX = 4443 ticks/layer vs scalar = 2218 ticks/layer (~2.2s/layer, ~60s/forward pass).
20. **`unpack_all()` não é o gargalo (v0.80.0):** Substituir alocação de 17.7 MB por row buffer de 6.9 KB não acelerou o forward pass — o gargalo real é a emulação VEX + WHPX memory virtualization. Operações aritméticas dominam, não alocação.
21. **Forward pass BitNet b1.58 sob WHPX:** ~60s para 64 tokens × 30 layers. Generate_speculative de 8 tokens levaria ~6h. Inviável sem KV cache ou bare metal.
22. **Build incremental mascara erros de compilação (v0.109.1):** `cargo clean -p neural-kernel` revelou 32 erros que o cache incremental escondia por meses. Causas comuns: imports faltando (`alloc::vec`, `Vec`, `String`, `ToString`), APIs que mudaram de nome (slab, VFS, jarvis), format string não escapada, `.sqrt()` sem trait `F32Ext`.
23. **RTL8139 RX=0 root cause (v0.109.2):** Bit CR_RE (0x01) nunca era escrito no Command Register (offset 0x37). `cr=0x0c` no log confirmava — só RXE+TXE ativos, RE=0. MAC da Realtek descartava pacotes antes do DMA. E1000 não tem esse bit. **Lições**: dumps de registrador na telemetria salvam dias; sempre verificar enable bits individuais vs combinados.
24. **AHCI funciona, mas sem disco SATA no QEMU:** `scan_pci_cb()` zero-alloc encontrou o controlador AHCI (00:1f.2 class=01/06), driver init OK (Porta 0 com SATA sig=0x101). Mas `-drive if=ide` não anexa disco ao barramento SATA — precisa `-device ide-hd` explícito para testar FAT32 via AHCI.
25. **SkillOpt viability (Microsoft Research, maio/2026):** Primeiro otimizador sistemático de skills em espaço textual. Viável para neural-os-core (~145 LOC) usando CortexAgent como optimizer + SleepCycle como scheduler de épocas. Recomendado para Sprint 99.
26. **SGLang Compressed FSM (Stanford/Berkeley, 2023):** Decodificação constraint via FSM comprimido. RadixAttention inviável em bare-metal (memória), Compressed FSM viável. Mascara logits no BitNet decoder para tokens válidos (JSON/SKILL.md/shell). ~120 LOC, impacto imediato na confiabilidade da saída LLM. Sprint 99.
27. **FlashAttention (Stanford, NeurIPS 2022):** IO-aware tiling para atenção. Aplica-se ao BitNet CPU: processar atenção em blocos de 16 tokens no cache L1 (32 KB). ~3-5× speedup para sequências >256 tokens. Sprint 100+.
28. **🏆 B-01 MORTO (v0.109.3 — 2026-07-09):** O bloqueador de 18 sprints caiu. Causa real: incompatibilidade Windows 11 × QEMU TCG × NIC emulada. Solução: bypass serial TCP. Kernel `slip.rs` (82 LOC) + `serial_bridge.py` (Python como servidor TCP) + `-serial tcp:127.0.0.1:4444` (QEMU como cliente). Primeiro RX: 304 bytes. O kernel sempre esteve correto — era o ambiente que isolava fisicamente o RX.

## Pendente Técnico (Roadmap v1.5.x → v2.0)

### ✅ COMPLETO (Sprints 84-91 + Sound + 95-97)
Todos os sprints de infraestrutura (GPU, JARVIS, SleepCycle, Cognitive, Self-Healing, Trinity MoE) estão implementados e verificados.

### ✅ Sprint 92-100 — Todos completos
- **Code cleanup**: 94 warnings → 0 em todos os crates
- **Zero-Trust Syscall (#364)**: `check_syscall()` + `exempt_tokens` + wireado no WASM runtime
- **Neural Cache (#365)**: Verificado em `cognitive.rs`
- **Serial bridge**: Watchdog + DNS healthcheck + reconexão automática
- **Human-in-the-Loop (#244)**: `/approve`, `/deny`, `/pending` + bloqueio de skills
- **LLM Icons**: `generate_llm_icon()` integrado no compositor com cache
- **GGUF streaming**: `load_gguf_header_from_disk()` + `load_gguf_streaming()`
- **Frame allocator**: Bitmap estendido para 8GB
- **FAT32 streaming**: `read_file_range()` — leitura chunked
- **RssAgent + EmailAgent**: Agentes WWW via HTTP + SMTP
- **HW Expert GPU**: 43.339 dispositivos, loss 0.097, acurácia 95.4%
- **Tag v1.0.0**: Criada e pushada

### ✅ Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` 0 erros ✅
- QEMU UEFI boot (OVMF + TCG) — kernel init até runtime/scheduler ✅
- Bootloader v0.11: BIOS image não funciona (triple fault), UEFI funciona ✅
- Ponytail Audit: -19 arquivos, -500 LOC, -3 deps, -32 transitive crates ✅
- #PF no scheduler resolvido via heap stack switch (Pacote A: stack ≥2MB via Vec heap) ✅
- **Pacote B (boot):** `init_platform_sync` (PCI+ACPI+APIC+SMP) antes dos drivers; PlatformAgent/NetDriverAgent idempotentes; Agency SpecialistAgent Continuous→EventDriven ✅
- **🔴 Conhecido**: WHPX crasha com SMP ("Unexpected VP exit code 4") — usar TCG.
- VirtualBox boot test — manual

### ✅ Sprint 101-105 — v2.0 Fundação
- Piper TTS, STT, HDA capture, NVIDIA GPU compute ✅
- K²CHJ workspace migration (5 crates, dep chain) ✅
- Ponytail audit (600 LOC, 11 deps) ✅
- RingBufStore refactor + LEGACY snapshot ✅

### ✅ Sprint 106 — v2.0 Ecossistema de Anéis Lógicos (10/10)
- Cargo workspace estrito (k_nano, k_ai, cortex, hermes, jarbas) ✅
- Rename k_ia→k_ai, jarvis→jarbas ✅
- SOUL.md parser via VFS (4 arquivos jarbas corrigidos) ✅
- Trinity MoE router (classifica intents, não roteia hardware) ✅
- MicroPython/WASM sandbox + WASI→Skill bridge (20+ mapeamentos) ✅
- Page faults fix (allocator → events → agents) ✅
- AIOS API (aios_net, aios_fs) + SkillOpt (Python→Rust no_std) ✅
- Heap address HW real (`0x4000_0000_0000`) ✅

### ⏳ Sprint 107+ — v2.0 Cognição Plena
- Voice I/O pipeline (TTS→STT→LLM→TTS) ⏳
- Self-evolving agents (auto-skill generation) ⏳
- LLM Agent 24/7 multi-turn conversation ⏳
- DHCP/Rede nativa (e1000 sem serial tunnel) 🔴

### ✅ Scheduler performance fix (Sprint 95/96 runtime)
- RTL8139 RX debug rate-limited (1/100 chamadas) — serial flood eliminado
- Scheduler skipa agentes passivos (>50 consecutive Pending → 80% skip)
- `has_event` agora depende de `ScheduleKind` real, não hardcoded `true`

## Arquivos Chave
| Arquivo | Função |
|---|---|
| `disk_agent/mod.rs` | DiskIntelligenceAgent (198 LOC) |
| `disk_agent/controller.rs` | StorageController trait + AtaCtrl + UsbMscCtrl + NvmeCtrl |
| `disk_agent/fs_probe.rs` | FilesystemProbe + 10 probes (260 LOC) |
| `disk_agent/nvme.rs` | NVMe driver (239 LOC) |
| `memory_agent.rs` | Adaptive budget + CPU calibration + dynamic tick |
| `allocator.rs` | resize_heap_to_mb() + CURRENT_HEAP_MB |
| `tpm.rs` | TPM 2.0 TIS + SHA256 embedded (279 LOC) |
| `identity.rs` | Ed25519 kernel verification |
| `agents.rs` | HermesAgent event-driven + Cortex fallback |

---

## Navegação Rápida para AI DEVs

```
📁 docs/                         → Toda a documentação
├── 📄 SPRINT-106.md             → Detalhes Sprint 106 (10 sub-sprints)
├── 📄 SPRINT-106-STATUS.md      → Status consolidado v2.0
├── 📄 sprint-plan-92-100.md     → Plano v1.0 Gold Master
├── 📁 architecture/             → ADRs: decisões arquiteturais (40 documentos)
│   └── 📄 0039-boot-flow.md     → Boot sequence agent-centric
├── 📁 memory/                   → Estado, ideias, sessões
│   ├── 📄 STATE.md              → ⭐ COMEÇE AQUI: estado atual do kernel
│   ├── 📄 IDEA_BANK.md          → 416+ ideias catalogadas
│   ├── 📄 SESSION_INDEX.md      → Índice de sessões + lições críticas
│   └── 📄 SESSION_NNN.md        → Sessões individuais com debug e descobertas
📄 AGENTS.md                     → ⭐ POLÍTICAS: regras de engenharia, premissas
📄 ROADMAP.md                    → Roadmap v1.0 → v2.0
📄 TODO.md                       → Checklist mestre
📄 crates/k_nano/ … jarbas/      → 5 crates K²CHJ (v2.0)
📄 crates/neural-kernel/         → Bin de integração
```
