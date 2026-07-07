# Sprint Plan 92-99 — neural-os-core v0.92.x-0.99.x
# TODAS AS 408+ IDEIAS DO IDEA_BANK ASSIGNADAS A SPRINTS

**Data:** 2026-07-06  
**Contexto:** Sprints 1-91 completos (~35.000 LOC). 46 novas ideias (#361-#406) extraídas de ADRs.  
**Premissa:** Toda ideia do IDEA_BANK tem sprint definido. Nada de "futuro" sem destino.  
**Nota:** Sprint Sound adicionado (audio drivers + voice pipeline) — #83 e #84 restaurados do descarte.

---

## Sprint 92 — Bloco LAN: Rede Funcional + Dependências (~3000 LOC)
**IDEA_BANK:** B-01, #117-124, #250-255, #186-189, #241-247, #306a-d  
**Depende de:** Nada (é o gatekeeper)  
**Foco:** Destravar a rede para todo o ecossistema AIOS

| IDEA | Item | LOC | Depende |
|---|---|---|---|
| **B-01** | RX fix (RTL8139 DHCP/RX) — buscar na internet | 500 | — |
| #117 | NIC driver genérico (detecta por PCI, busca driver online) | 400 | B-01 |
| #118-120 | smoltcp + DNS + HTTP stack | 500 | B-01 |
| #250 | /ping command | 50 | B-01 |
| #251-252 | DHCP/ARP com timeout + fallback | 200 | B-01 |
| #186 | AppForge / App Store (catalog, install one-click) | 1500 | B-01 |
| #187 | Multi-User / Multi-Persona (memória isolada) | 600 | B-01 |
| #188 | Visual Workflow Builder (DAG pipeline) | 500 | B-01, GPU |
| #189 | Federated Cluster / P2P Workers | 800 | B-01 |
| #241 | OpenTelemetry-Like Observability | 500 | B-01 |
| #242 | AI-Driven Security Scan for Skills | 350 | B-01 |
| #243 | Hub Discovery / Multi-Instance Board | 400 | B-01 |
| #244 | Human-in-the-Loop Approval | 250 | B-01 |
| #245 | Remote Agent Execution | 800 | B-01 |
| #246 | Skill Marketplace | 600 | B-01 |
| #247 | Automatic Context Compaction Agent | 300 | B-01 |
| #306a | Windows PE32+ loader | 600 | B-01 |
| #306b | Linux ELF loader | 500 | B-01 |
| #306c | macOS Mach-O loader | 400 | B-01 |
| #306d | Android APK compat | 500 | B-01 |
| **M4** | Zero-Trust Syscall Categories (4-class) | 100 | B-01 |
| **M5** | Neural Cache decisions per token | 150 | B-01 |

---

## Sprint 93 — Bloco WASM: Runtime + Skills (~4000 LOC)
**IDEA_BANK:** #103-104, #309a-c, M31-M36, M42-M43, M45  
**Depende de:** B-01 (para download de WASM modules)  
**Foco:** WASM embedder, skill runtime, IDE, marketplace

| IDEA | Item | LOC |
|---|---|---|
| #103 | WASM embedder (wasmi no_std) | 500 |
| #104 | Linear memory pool (256 KB/skill) | 100 |
| #309a | WASM Skill Runtime (wasmi v0.42+, fuel metering) | 800 |
| #309b | IDE Agent (BitNet IDE) | 2000 |
| #309c | Hybrid Agents (kernel + WASM) | 100 |
| **M31** | AgentManifest JSON format spec | — |
| **M32** | Developer contract for WASM agents | 80 |
| **M33** | 15 WASI→skill mappings | 350 |
| **M34** | BitNet IDE with HOWTO feature | 2000 |
| **M35** | Marketplace/App Store agent | 400 |
| **M36** | DiskMonitor example agent | 70 |
| **M42** | WASM linear memory pool (256KB) | 100 |
| **M43** | Skill ABI design | 100 |
| **M45** | Capability token cryptographically signed | 100 |

---

## Sprint Sound — Bloco Audio: JARVIS Ouvir + Falar (~3500 LOC)
**IDEA_BANK:** #83, #84, #315.21-25, #360  
**Depende de:** B-01 (download modelos TTS/STT), PCI (HDA detectado), Sprint 84 `map_bars_uc()`  
**Objetivo:** JARVIS escuta por wake word, transcreve fala, processa com Cortex, responde em voz. Tudo bare-metal Rust via EventBus.

### Arquitetura

Pipecat (13.2K★) é referência **arquitetural apenas** — pipeline composition pattern sobre EventBus nativo:
```
[HDA Mic] → WakeWord(Rustpotter) → STT(sherpa-onnx) 
                                         ↓  texto
                                    CortexAgent
                                         ↓  texto
                                   TTS(sherpa-onnx)
                                         ↓  áudio PCM
[HDA Speaker] ← AudioRingBuffer ← [VoicePipeline]
```

**Motores (Rust, adaptáveis para no_std via sherpa-onnx):**
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — faz TTS + STT num único crate Rust. Suporta PocketTTS, Kokoro, Whisper, silero-vad. Bindings para 12 linguagens (Rust nativo). Roda em Raspberry Pi.
- [Rustpotter](https://github.com/Priler/rustpotter) — wake word detection em Rust puro, no_std compatível.
- Pipecat descartado como dependência direta (Python). Patterns extraídos: Frame types, FrameProcessor trait, Pipeline composition via EventBus.

### Pipeline de Áudio (EventBus Frames)

```
TOPIC_AUDIO_IN       → [i16 PCM chunks do HDA/USB]
TOPIC_WAKEWORD       → ["jarvis" detectado]
TOPIC_STT_TEXT       → [texto transcrito]
 → reusa HermesAgent existente (USER_INTENT → cortex.think → response)
TOPIC_TTS_CMD        → [texto para falar]
TOPIC_AUDIO_OUT      → [i16 PCM chunks para HDA/USB]
```

### Arquivos

| Arquivo | Função |
|---|---|
| `audio/pipeline.rs` | VoicePipeline struct — orquestra frames entre módulos via EventBus |
| `audio/frame.rs` | Tipos AudioFrame, TranscriptionFrame, TTSCommandFrame |
| `audio/ringbuf.rs` | Circular buffer PCM para DMA audio (HDA/USB consomem daqui) |
| `audio/wakeword.rs` | Rustpotter wrapper — publica TOPIC_WAKEWORD |
| `audio/stt.rs` | sherpa-onnx STT — transcreve áudio → texto |
| `audio/tts.rs` | sherpa-onnx TTS — texto → áudio PCM |
| `audio/hda.rs` | Intel HDA audio driver (PCI 04.03) — DMA playback/capture |
| `audio/usb_uac.rs` | USB Audio Class driver — isochronous xHCI |
| `audio/mod.rs` | Re-exports + init_audio_pipeline() |

### Ordem de Implementação

| Passo | O quê | LOC | Depois de |
|---|---|---|---|
| 1 | `audio/ringbuf.rs` — Circular buffer PCM lockless | 100 | — |
| 2 | `audio/hda.rs` — Intel HDA driver: detect, DMA playback | 800 | PCI scan + map_bars_uc |
| 3 | `audio/frame.rs` + `audio/pipeline.rs` — Frame types + EventBus wiring | 300 | ringbuf |
| 4 | `audio/ser.rs` — SER (Speech Emotion Recognition): pitch/energy/ZCR → emoção + `loqa-voice-dsp` ou `sensevoice` como alternativa | 200 | ringbuf |
| 5 | `audio/tts.rs` — sherpa-onnx TTS (PocketTTS engine) | 400 | B-01 (download model) |
| 6 | `audio/wakeword.rs` — Rustpotter + TOPIC_WAKEWORD | 150 | ringbuf |
| 7 | `audio/stt.rs` — sherpa-onnx STT (Whisper engine) | 400 | B-01 (download model) |
| 8 | `audio/usb_uac.rs` — USB Audio Class (microfone) | 600 | xHCI |
| 9 | Integração: TOPIC_STT_TEXT → SER → emotion → USER_INTENT | 100 | ser + stt |
| 10 | TrinityRouter SpeechSynth → dispara TTS pipeline | 50 | já feito ✅ |
| **Total** | | **~3100** | |

### Speech Emotion Recognition (SER)

| Fonte | Uso | Tipo |
|---|---|---|
| `loqa-voice-dsp` (crates.io, v0.5.0) | Pitch, formants, spectral features | Crate Rust (DSP) |
| `sensevoice` (crates.io, v0.1.0) | SenseVoice bindings — ASR + emoção + diarização | Crate Rust (ggml) |
| `audeering/wav2vec2-large-robust-12-ft-emotion-msp-dim` (602k↓ HF) | SER estado-da-arte | Modelo ONNX |
| `onnx-community/wav2vec2-emotion-recognition-ONNX` (HF) | SER via sherpa-onnx | Modelo ONNX |
| `speechbrain/emotion-recognition-wav2vec2-IEMOCAP` (169k↓ HF) | SER SpeechBrain | Modelo PyTorch |

**Pipeline SER atual (heurístico):** PCM → autocorrelação(pitch) + RMS(energy) + ZCR → regras → Emotion
**Futuro:** PCM → loqa-voice-dsp(features) → sensevoice/wav2vec2-onnx → Emotion

### Referências

| Projeto | Estrelas | Uso |
|---|---|---|
| [pipecat-ai/pipecat](https://github.com/pipecat-ai/pipecat) | 13.2K★ | **Arquitetura apenas** — Frame types, FrameProcessor, pipeline composition |
| [kyutai-labs/pocket-tts](https://github.com/kyutai-labs/pocket-tts) | 5.4K★ | TTS CPU-native 100M params, 6× real-time, voice cloning. Acessível via sherpa-onnx |
| [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) | 15K★ | **Motor principal** — Rust bindings para TTS (PocketTTS/Kokoro) + STT (Whisper) + VAD (silero) |
| [Priler/rustpotter](https://github.com/Priler/rustpotter) | 300★ | Wake word detection Rust puro, no_std |
| **FunAudioLLM/SenseVoice** (HF) | — | ASR + emotion recognition + diarization. Rust bindings: `sensevoice` crate |

## Sprint 94 — Bloco Vision: Camera + Display (~1500 LOC)
**IDEA_BANK:** #79-82  
**Depende de:** Nada  
**Foco:** USB camera, framebuffer rendering, tensor viz

| IDEA | Item | LOC |
|---|---|---|
| #79 | UEFI framebuffer renderização de fontes | 200 |
| #80 | Font rendering para alta resolução | 200 |
| #81 | VirtIO-GPU 2D/3D acelerado | 400 |
| #82 | Tensor visualization no framebuffer | 300 |

---

## Sprint 95 — Bloco Cognitive: Aprendizado + Memória Avançada ✅✅✅
**IDEA_BANK:** #105-108, #149-152, #158-162, #169-175, M37-M41, M2  
**Depende de:** Nada (CPU-only)  
**Foco:** Success Engine, feedback loop, EWC, codebook VQ, on-device learning  
**Status:** ✅ Completo. cognitive.rs (510+ LOC, 25 structs/funcs) + lazy_static em main.rs. Todos os itens implementados.

| IDEA | Item | LOC | Status |
|---|---|---|---|
| **#105** | Intent Planner (SkillSteps com params) | 65 | ✅ |
| **#106** | Success Engine (win/loss streak, recent_rate) | 50 | ✅ |
| **#107** | Neural Cache (TTL, evicção LRU, max_entries) | 55 | ✅ |
| **#108** | MatMul-Free LM (RWKV-style WKV forward) | 55 | ✅ |
| **#149** | Feedback Loop (rating + comment) | 35 | ✅ |
| **#150** | Ternary Weight Update (gradient-based {-1,0,+1}) | 15 | ✅ |
| **#151** | Experience Replay Buffer (ring buffer, sample) | 40 | ✅ |
| **#152** | Weight Consolidation (export snapshot) | 25 | ✅ |
| **#158** | Workflow Predictor (confidence scoring) | 40 | ✅ |
| **#159** | Auto-Skill Generator (WASM templates) | 40 | ✅ |
| **#160** | Dynamic Resource Scaling (heap MHI) | 30 | ✅ |
| **#161** | Self-Optimizing Scheduler (timeslice adjust) | 40 | ✅ |
| **#162** | Workflow Profile (export JSON) | 20 | ✅ |
| **#169** | Codebook VQ (quantize com distância real) | 55 | ✅ |
| **#170** | KV Cache Codebook (compress/decompress) | 30 | ✅ |
| **#171** | ReAct Loop (Thought→Action→Observation) | 45 | ✅ |
| **#172** | MCP Server (tools/list, tools/call) | 35 | ✅ |
| **#173** | Codebook Finetune (centroid adjustment) | 15 | ✅ |
| **#174** | Delta Branches (speculative decode verify) | 35 | ✅ |
| **#175** | Workspace Isolation (sandbox per agent) | 40 | ✅ |
| **M2** | Episodic Memory (NVMe-backed ring) | 25 | ✅ |
| **M37** | SleepCycle Guard Rails (per phase) | 15 | ✅ |
| **M38** | BitNetTrainer (train_step com ternary) | 45 | ✅ |
| **M39** | Candle Trainer sidecar stub | 20 | ✅ |
| **M40** | Task Spawner (ELF wrapper) | 20 | ✅ |
| **M41** | Three Data Sources (replay/feedback/episodic) | 15 | ✅ |

---

## Sprint 96 — Bloco Self-Heal + Security: Resiliência ✅✅✅
**IDEA_BANK:** #226-227, #265-267, M6-M14, M1, M3, M29  
**Depende de:** Nada  
**Foco:** Self-healing avançado, audit trail, vector FS  
**Status:** ✅ Completo. Todos os itens M1-M29 implementados em self_heal.rs + vfs/mod.rs + memory_systems.rs.

| IDEA | Item | LOC | Status |
|---|---|---|---|
| **#226-227** | Team/Shared Memory + Git Snapshots | 55 | ✅ |
| **#265-266** | Vector FS + Vector API (dot product search) | 45 | ✅ |
| **#267** | OverlayFS Copy-on-Write | 15 | ✅ |
| **M1** | Zero-Copy SFS (slice references) | 40 | ✅ |
| **M3** | Skills-as-Modules (fn pointer import) | 15 | ✅ |
| **M6** | Failure Taxonomy (classify_by_code) | 15 | ✅ |
| **M7** | Exception Self-Heal (auto recovery) | 20 | ✅ |
| **M8** | Corrective Prompting (context-aware) | 10 | ✅ |
| **M9** | Verifier Pós-Recovery (fn check) | 10 | ✅ |
| **M10** | Erros no EventLog | 7 | ✅ |
| **M11** | Budgeted Recovery (attempt limits) | 30 | ✅ |
| **M12** | Silent Failure Detection (heartbeat) | 35 | ✅ |
| **M13** | Multi-level Failure Assessment | 7 | ✅ |
| **M14** | Failure Prediction (trend analysis) | 8 | ✅ |
| **M29** | Notification Gate (allow/block) | 35 | ✅ |

---

## Sprint 97 — Bloco AIOS: Cross-OS + Update + Mesh (~3000 LOC)
**IDEA_BANK:** #306-310, #307, #308a-c, M23-M30, M46  
**Depende de:** B-01, WASM (Sprint 93)  
**Foco:** Sistema de updates, compatibilidade cross-OS, mesh

| IDEA | Item | LOC |
|---|---|---|
| #307 | Syscall-to-Skill Translation Layer | 500 |
| #308a | Update/Upgrade Agent (A/B slots) | 500 |
| #308b | Update channels (stable/nightly/security) | 200 |
| #308c | Rollback automático | 100 |
| #310a | J.A.R.V.I.S. Layer (persona completa) | 500 |
| #310b | Stack final Boot→Kernel→Cortex→Hermes→JARVIS | 200 |
| **M23** | Detalhado WASI→Skill mapping (20 syscalls) | 200 |
| **M24** | Tier 0-4 Agent Classification | 100 |
| **M25** | WASM Host Function Interface signatures | 80 |
| **M26** | Performance budget table (kernel vs WASM) | — |
| **M27** | ChromeOS A/B update reference | — |
| **M28** | J.A.R.V.I.S. Context Window Manager | 500 |
| **M30** | Update channel strategy (detalhado) | 100 |
| **M46** | VirtIO-GPU GET_DISPLAY_INFO pending fix | 50 |

---

## Sprint 98 — Bloco NPU + GPU Polish (~3000 LOC)
**IDEA_BANK:** #43-52, #333-336, M18-M22  
**Depende de:** 💰 HW AMD APU (para NPU), GPU já funcional  
**Foco:** NPU AMD XDNA driver, GPU compute polimento

| IDEA | Item | LOC |
|---|---|---|
| #43-52 | NPU AMD XDNA driver completo | 3000 |
| #333 | burn-flex backend (SIMD gemm profissional) | 800 |
| #334 | MSched evicção VRAM (Belady/OPT) | 500 |
| #335 | CFS scheduler (vruntime-based fairness) | 500 |
| #336 | GPU + Display co-existência | 300 |
| **M18** | Per-vendor GPU driver LOC (NVIDIA PFIFO, AMD PM4) | 4600 |
| **M19** | NVIDIA Pascal Push Buffer channel layout | — |
| **M20** | AMD RDNA PM4 packet types | — |
| **M21** | Model swap flow (/model <path>) | 80 |
| **M22** | iGPU display + dGPU compute architecture | — |

---

## Sprint 99+ — Bloco Meta: Extras (~2000 LOC)
**IDEA_BANK:** #212-213, #278a-b, #315.26-28, #210-211  
**Depende de:** Infraestrutura madura  
**Foco:** GGUF loader, plugin system, gamification

| IDEA | Item | LOC |
|---|---|---|
| #210 | Subagent Crash-Recovery Persistence | 600 |
| #211 | ComputeBackend Trait | 800 |
| #212 | Plugin System via Loadable Page Ranges | 400 |
| #213 | WASM + Docker Sandbox for Skills | 500 |
| #278a | GGUF loader mínimo | 500 |
| #278b | .bitnet v3 header extensível | 200 |
| #315.26 | Multi-device sync (CRDT) | 300 |
| #315.27 | SKYNET Mesh Node | 300 |
| #315.28 | Gamification | 200 |
| **M44** | NVMe submission/completion queue | 800 |

---

## ❌ Descartados
| IDEA | Item | Motivo |
|---|---|---|
| #116 | Port ARM/RISC-V | Fora do escopo x86-64 atual |
| #248 | Docker Sandbox | Incompatível com bare-metal no_std |
| #249 | Python/.NET Runtime | Barreira de linguagem |
| #357 | khal-std | Requer wgpu (std-only) |

---

## Resumo de Esforço

| Sprint | Bloco | LOC | Depende de |
|---|---|---|---|
| 92 | LAN: Rede + Dependências | ~3000 | — |
| 93 | WASM: Runtime + Skills | ~4000 | B-01 |
| Sound | Audio: Drivers + Voice | ~3500 | B-01 + PCI |
| 94 | Vision: Camera + Display | ~1500 | — |
| 95 | Cognitive: Aprendizado | ~510 | — ✅ |
| 96 | Self-Heal + Security | ~350 | — ✅ |
| 97 | AIOS: Cross-OS + Update | ~3000 | B-01 + WASM |
| 98 | NPU + GPU Polish | ~3000 | HW AMD APU |
| 99+ | Meta: Extras | ~2000 | Infra madura |
| **Total** | | **~23.500 LOC** | |
