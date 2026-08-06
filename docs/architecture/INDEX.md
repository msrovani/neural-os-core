# Índice de ADRs e lifecycle

Inventário canônico dos documentos em `docs/architecture/`. O **Status** registra a decisão no corpo da ADR; o **lifecycle** registra sua situação operacional no tree atual.

## Lifecycle

| Valor | Uso |
|---|---|
| `por_fazer` | Proposta aceita ou registrada, ainda não iniciada |
| `fazendo` | Implementação ativa |
| `completa` | Critérios atendidos, sem necessidade de modernização registrada |
| `modernizada` | Completa e alinhada ao tree atual após evolução posterior |
| `substituida` | Preservada historicamente, mas superseded por outra ADR |
| `obsoleta` | Não seguir; mantida apenas como registro |
| `pesquisa` | Análise/research note, não decisão de implementação |
| `conflito_id` | ID compartilhado; o arquivo canônico é explicitado abaixo |
| `plano_sprint` | Plano/checklist histórico armazenado entre ADRs |

Status canônico no corpo: `Proposed | Accepted | Rejected | Superseded`. Variações históricas em português permanecem preservadas e são normalizadas apenas neste índice.

## Inventário

| ID / arquivo | Status | Lifecycle | Ideias / relação | Nota |
|---|---|---|---|---|
| `0001-initial-architecture-and-toolchain.md` | Accepted | `completa` | Fundação | Toolchain inicial |
| `0002-vga-and-serial-logging.md` | Accepted | `completa` | Console/serial | Base operacional |
| `0003-interrupt-descriptor-table.md` | Accepted | `completa` | IDT | Implementada |
| `0004-memory-paging-and-heap.md` | Accepted | `modernizada` | Memória | Heap e paging evoluíram |
| `0005-simd-and-fpu-enablement.md` | Superseded | `substituida` | SIMD/FPU → **0055** | Histórico SSE; autoridade ISA em ADR-0055 |
| `0006-neural-primitives-and-libm.md` | Accepted | `completa` | Primitivas neurais | Implementada |
| `0007-intent-router-mlp.md` | Accepted | `modernizada` | Cortex/Hermes | Evoluiu para Trinity/Hermes |
| `0009-pic-watchdog-and-page-fault.md` | Accepted | `modernizada` | IRQ/#PF | Endurecida em sprints posteriores |
| `0010-strategic-roadmap-and-innovations.md` | Accepted | `modernizada` | #361–405 e temas | Roadmap absorvido por ADRs temáticas |
| `0011-bitlinear-and-hybrid-matmul.md` | Accepted | `completa` | BitLinear | Base do tensor engine |
| `0012-2bit-packing-quantization.md` | Accepted | `completa` | Packing ternário | Formato implementado |
| `0013-neural-os-executive-summary.md` | Accepted | `pesquisa` | Visão executiva | Snapshot histórico |
| `0014-ideias-hardware.md` | Proposed | `pesquisa` | Seed #1–116; **SMP→0055** | §SMP/CorePools deprecated; USB/NPU seeds intactos |
| `0015-curso-correcao-mvp.md` | Proposed | `plano_sprint` | MVP Hermes | Rota histórica |
| `0016-network-strategy.md` | Proposed | `modernizada` | #117–125, #415 | Rede evoluiu para SLIP + nativa |
| `0017-critical-bugfix-sprint.md` | Accepted | `completa` | Bugfix crítico | Critérios atendidos |
| `0018-sprint-24-plan.md` | Proposed | `plano_sprint` | Sprint 24 | Checklist histórico |
| `0019-neural-cortex-bitnet-llm.md` | Accepted | `modernizada` | #126–156 | Formato e Cortex evoluídos |
| `0020-crom-ecosystem-analysis.md` | Accepted | `pesquisa` | #164–176 | Análise de ecossistema |
| `0021-life-os-ecosystem-analysis.md` | Proposed | `pesquisa` | #177–198 | Análise de ecossistema |
| `0022-personal-ai-assistant-ecosystem-analysis.md` | Proposed | `pesquisa` | #199–213 | Análise de ecossistema |
| `0023-memory-systems-second-brain-analysis.md` | Proposed | `pesquisa` | #214–227 | Análise de memória |
| `0024-agent-frameworks-analysis.md` | Accepted | `pesquisa` | #228–249 | Análise de frameworks |
| `0025-tier3-sandbox-security-analysis.md` | Accepted | `pesquisa` | #256–267 | Análise de segurança |
| `0026-ecosystem-batch3-analysis.md` | Accepted | `conflito_id` | #280 | **Canônico ADR-0026**; pesquisa/ports |
| `0026-sprint-29-xhci-usb-driver.md` | — | `conflito_id` | xHCI Sprint 29 | Documento de sprint; equivalente a `plano_sprint` |
| `0027-self-healing-research.md` | Proposed | `modernizada` | #366–374 | Pesquisa absorvida pelo SelfHeal |
| `0028-gguf-format-research.md` | Proposed | `pesquisa` | #375–377 | GGUF research |
| `0029-gpu-architecture.md` | Proposed | `modernizada` | #378–382, #406 | Base GPU evoluída |
| `0030-disk-intelligence-agent.md` | Proposed | `modernizada` | #303a–f | Agente implementado e expandido |
| `0031-aios-self-update-wasm-jarvis.md` | **Superseded (parcial)** | `substituida` | #306–310, #383–390 → **0059**; §1 Self-Update → **0086** | Desvio wasmi→Op VM revertido por ADR-0059; **§1 Self-Update consolidado na ADR-0086 §3 (deprecado 2026-08-05)**; JARVIS mantidos |
| `0032-wasm-agent-apps.md` | **Superseded** | `substituida` | #391–396 → **0059** | Visão absorvida; Op VM + wasm.rs aposentados por wasmi (ADR-0059) |
| `0033-on-device-micro-learning.md` | Proposed | `modernizada` | #397–401 | Base de micro-learning implementada |
| `0034-jarvis-conscious-interaction-layer.md` | Superseded | `substituida` | #310/#315 | Substituída pela ADR-0036 |
| `0035-jarvis-deep-research-ecosystem-convergence.md` | Superseded | `substituida` | #315 | Substituída pela ADR-0036 |
| `0036-jarvis-unified-interaction-layer.md` | Accepted | `modernizada` | #315.* | Canônica para interação/persona |
| `0037-smp-gpu-architecture.md` | Superseded | `substituida` | #319–354 → **0055** / GPU **0048–50** | SMP→0055; GPU→0048–0050 |
| `0038-ecosystem-optimization.md` | Accepted | `pesquisa` | #355–360 | Auditoria e substituições |
| `0039-boot-flow.md` | Accepted | `modernizada` | Boot 8 fases | Alinhada a Pacotes A/B |
| `0040-filesystem-architecture.md` | Accepted | `completa` | #417–423 | MVP aceite 2026-07-16; **residuals `por_fazer`** (não reabrem ADR) |
| `0041-k2chj-capability-rings.md` | Accepted | `completa` | #424–432, **#459–461** | **K³CHJ** capability; P0–P9 ✅; planos H1–H5+HalOffer+H4+/AS ✅ v1.8.6; aceite QEMU slog ✅ (SESSION_251, `docs/evidence/boot-whpx-20260805.txt`) |
| `0042-k2chj-adequacao-boot.md` | Accepted | `modernizada` | #433–440, #457, **#461** | N1–N5 + wire v1.8.0; produto **K³CHJ** (§0); tree teste v1.8.6 |
| `0043-cubecl-patterns-and-technologies.md` | Accepted | `pesquisa` | GPU patterns | Análise tecnológica |
| `0044-edge-python-patterns.md` | Accepted | `pesquisa` | VM/SSA patterns | Análise tecnológica |
| `0045-sound-voice-stack.md` | Accepted | `completa` | #75, #83, #84, #315.21–25, #360, #438, #442 | Canônico Sound; residuals VITS/soft-float/jarbas cutover |
| `0046-airllm-gguf-streaming.md` | Accepted (MVP) | `completa` | #377, #449 | MVP AirLLM ✅; hot-swap ATA+Net code ✅ SESSION_128 (RX runtime gate); residuals: DMA prefetch / stream-to-disk / K-quants / e2e GGUF grande |
| `0047-latent-space-ai-os.md` | Accepted (MVP parcial) | `completa` | LatentBus/Evolve/Probe PoC | **Canônico base**; SESSION_126; defer cross-modelo/Genesis/ISA plena |
| `0047-gpu-compute-pipeline.md` | Accepted (MVP parcial) | `completa` | Extensão GPU | G1+G2 PoC; G3–G5 + DP4A defer |
| `0047-hmi-neural-desktop.md` | **Superseded (parcial)** | `substituida` | Extensão HMI → **0058** | H1/H2/H4/H5 absorvidos por ADR-0058; H3 ❌; MVP PoC histórico |
| `0048-nvidia-compute-multigeracao.md` | Proposed | `fazendo` | #454; NVIDIA ACR/GSP/Kernel Pack | P0 PMC+DID; P1 NKP+session; ACR dual-shadow; canário HW aberto |
| `0049-amd-compute-multigeracao.md` | Proposed | `fazendo` | #455; AMD PSP/KIQ/MES/Kernel Pack | Discovery parse+PSP Degrau+KIQ/MES+pack gfx1030; golden HW aberto |
| `0050-intel-compute-multigeracao.md` | Proposed | `fazendo` | #456; Intel GuC/walkers/Kernel Pack | GMD+DID; GuC/Gen9 Degrau; dual iGPU/dGPU; golden HW aberto |
| `0051-hermes-ecosystem-packages.md` | Accepted (MVP) | `completa` | PackageHub + VFS | SESSION_135: stubs Agency removidos; PackageHub CRUD + 9 PackageKinds; Hermes commands wired ✅ |
| `0052-neural-artifact-contract.md` | Accepted (MVP) | `completa` | schema/hash/sig/acionaveis | Deny unsigned; schema 1; Ed25519 sign/verify; stage_create/update validated ✅ |
| `0053-hanr-parity-marketplace-trust.md` | Accepted (MVP) | `completa` | HANR parity | SESSION_136: session Ed25519, marketplace (list/search/install/remove), content_hash, trust chain ✅ |
| `0054-perci-bitwork-integration.md` | Proposed | `pesquisa` | Perci/Bitwork | Ex-0045 conflito; **adiada** — CognitiveRouter Bitwork→Trinity; sem wire |
| `0055-smp-revision.md` | Accepted | `completa` | #16–42, #20–33, #35–41, #317, #319–324, #347 | **Canônica SMP** — FeatureGate; TCG APs=1; WHPX BSP-only (SESSION_141); APs-IDT FASE 3.1 ✅; ISA dispatch AVX-512→AVX2→SSE4.2→scalar ✅ |
| `0056-neural-device-lego.md` | Accepted (MVP) | `completa` | #464 | L0/L1/L2 DeviceRecipe; UnlockDAG; trust Ed25519; community hub; H1 bind table; 4 goldens; 6/7 criteria ✅ (residual market fetch v3) |
| `0057-compute-dispatch-smp-gpu-npu.md` | Accepted | `completa` | #20–42, #211, #329–331, #345–346, #454–456 | **Dispatch completo:** WS-A wake multi-AP ✅ (-smp 4 APs=3); WS-B/C wired; WS-D GPU hook gated; WS-E NPU detection; WS-G decode self-test ✅. WS-F (AP IDT/IPI) também ✅ — código completo em k_nano/smp/mod.rs + interrupts.rs (ap_load_idt_and_tss faz IDT.load + load_tss + sti). Apenas gateado por `allow_smp()` que é false no WHXP (dev default) mas true em TCG/bare-metal. Layer S/HW residual pós-v2.0. |
| `0058-generative-card-desktop.md` | Accepted | `completa` | #79/#80/#82/#279d/#283/#448/#452/#465 + #468 | **UI/Desktop Jarbas:** embedded-graphics `DrawTarget` + `UiDeclaration`/`UiRenderer` (cards). **S1–S4 ✅** (QEMU: 3 cards + orb + HUD; self-tests PASS). S5 + A/V real residual |
| `0059-runtime-app-factory.md` | Accepted | `completa` | #103/#309a/#385–396/#402/#411/#8/#11/#306 + #469 | **App feita por IA:** wasmi real `add(2,3)=5` PASS; seletor A/B/C ✅ + F7 W^X arena ✅ (nativo `mov eax,42`→42). B/C (Cranelift) gated ring+HITL. F6 → ADR-0077 |
| `0060-bitnet-cognitivo-bei.md` | Proposed | `completa` | #470–#478 | **BEI:** 7/7 Ondas ✅ (MPMC, Economia, Cellular, MoE, Memória, Afeto, Executive). ~2900 LOC, 24 arquivos. |
| `0061-cpu-first-bitnet.md` | Proposed | `completa` | #479–#490 | **CPU-First BitNet:** bitnet_sse.rs com SSE4.2 dispatch; dispatch dinâmico AVX-512→AVX2→SSE4.2→scalar ✅. soft_stride=1, max_gen=32, BPE encode merge-order ✅ |
| `0062-claudioos-vs-neural-aios.md` | Proposed | `pesquisa` | #479–#494 | **ClaudioOS vs Neural-OS.** **P1 TLS ✅** · **P2 StorageBus ✅** · **P3 NVMe/AHCI ✅** · **P11 USB-MSC ✅** (SESSION_170/171). Abertos: Limine P4, P24 HID, GPU/WiFi… |
| `0063-tickv-noproto-ai-index-sgdb.md` | Proposed | `completa` | #491–#505 | **SGDB:** MVP + D-series + Memory Quality (SESSION_176). ART+BQ+engine+layers+store; 11 arquivos; e2e smoke ✅. DoD 10M/100k residual |
| `0064-rag-db-in-kernel.md` | Rejected | `descartada` | #486–#487 | **RAG DB in-kernel:** ❌ **REJEITADA.** Crate `vector-db` criada mas nunca integrada. SGDB real = ADR-0063 `k_ai::sgdb` (MemoryDoc/ART/BQ/TickvLite). BGE embedding existe em `k_ai::memory_systems`. |
| `0065-cosmic-like-wm-gpu-render-adr.md` | Proposed | `completa` | #495–#510 | **Cosmic-like WM + GPU:** FASES 1.1/1.2/2.1/2.2/3.1/3.2 ✅ completas (commits 289339c + 0fdf20e + 7a5e0a7). Tags adr0065-fase1-3-complete, adr0065-fase2.2-3.2-complete, ui-wm-fixes-v1 |
| `0075-emagrecer-neural-kernel.md` | Parcial | `completa (parcial)` | #467/#511 | **Emagrecer neural-kernel (2026-07-30):** E0 parcial, E1a/E1c/E2/E3/E4 ✅, aios_api stub ✅ (-72 LOC). Bin 20.403 LOC (−9.028 do original). +Fase 2: hardware/ + adaptation/ → LEGACY (−4.261 LOC speculativa). +main.rs simplificado (−71 linhas). agents/fs/vfs mantidos como role_diff (drift estrutural profundo). **Emagrecimento cirúrgico encerrado — irreducible floor ~115K LOC (workspace).** |
| `0076-cross-os-ecosystem.md` | Proposed | `fazendo` | #512+ | **Cross-OS Ecosystem:** ADR-0076 + CrossOsAgent + padrões de FYY/Wetware/WeftOS |
| `0082-hardware-info-registry.md` | Proposed | `por_fazer` | #520–#525 | **HardwareInfo:** Fonte única de HW — CPU/GPU/storage/memória. MVP em `platform_probe.rs`. SGDB já conectado. Expansão em ondas. Snapshot WASM futuro. |
| `0083-ai-layer-gap-auditoria.md` | Accepted | `fazendo` | Auditoria 7.x | **Gap camada IA (2026-08-03):** infra de inferência real; inteligência pendente. Correções: router MoE carregável de arquivo (`ROUTER.BITNET`), esqueleto backprop `TransformerTrainer` (honesto — warn no log), saudação sem pool canado (argmax real). Dívida: log honesto no fallback LCG, backprop real, router treinado, assets FAT. |
| `0084-bitnet-engine-fidelidade-e-kernels.md` | Proposed | `fazendo` | #126–156, #375–377, #479–490 | **Engine BitNet (2026-08-04):** fidelidade 2B4T (relu2/SubNorms/theta/embed Q6_K) + kernels CPU (decode branchless, activation-parallel, tiling) + receita treino 1-bit (speedrun/Hestia). Ordem: F1 decode → F2 prefill → F3 fidelity+Q6_K → F4 W2A8 gated. Sem retreino. Revisão §11 (2026-08-04): F1/F5 executáveis já; F3 bloqueada até auditoria de layout. **Implementação (SESSÃO 2026-08-05):** F1 decode branchless ✅ (`bitnet_avx2.rs` `(pair&1)-(pair>>1)`), F5 tiling consts ✅, F2 activation-parallel gated `m≥8` ✅, F3 fidelity ✅ (M1 act_type relu2/silu nos 4 forwards, M2 eps 1e-5 + `rms_ffn_norm` intermediate, M3 theta header, M4 embed Q6_K encoder+loader+lookup+unembed). **Bug latente corrigido:** `f16_to_f32` (gguf.rs) tinha `0.0*-1.0=-0.0` — todo f16 positivo virava -0.0, quebrando todos os dequants GGUF. F4 W2A8: gated (WHPX/HW real). |
| `0085-bitnet-v6-formato-canonico.md` | Proposed | `fazendo` | #491 | **Formato canônico `.bitnet v6` (2026-08-04):** padronização do pipeline inteiro (codificação→escrita→leitura→inferência→propagação K³CHJ). Header autodescritivo (act_type, embed_type, feat computado, num_params u64, tie_flag), body por model_type (llm/hwexpert/router), writer canônico `bitnet_writer.py` + paridade byte-exact Rust, loader v6 estrito com fallback legado WARN, registro `cortex::model` + ModelHub. Formaliza a auditoria de layout da ADR-0084 §11.3.1. **Implementação (SESSÃO 2026-08-05):** F0 ✅ (writer + golden + `v6_writer_parity` byte-exact PASS), F1 ✅ (8 conversores → v6, incl. silu no forward de treino train_models_gpu), F2 ✅ (`load_model_v6` estrito + fallback WARN), F3 ✅ (Q6_K cross-check Rust↔Python PASS), F4 ✅ (`cortex::model` ModelView + `ModelHub::register_bytes` + call sites LLM do main.rs roteados). Fases F0-F4; decisões do dono em §10. |
| `0086-instalacao-e-update-ota.md` | Accepted | `fazendo` | #308, #421, ADR-0079/0031/0074 | **Processo UNIFICADO Instalação + Update OTA (2026-08-05):** consolida ADR-0079 (AutoInstaller, Fases 0–3, marcos M0–M4) + ADR-0031 §1 (A/B dual-slot, canais, tries/rollback) + ADR-0074 (git thin, referência de código) + IDEA_BANK #176/#306–310/#417–423. Instalação: SysInstaller M1 ✅, Fases 1–3 pendentes (gaps I1–I9, incl. AutoInstallerAgent órfão I6, VRAM hardcoded I7, kernel não lê CONFIG.TXT I9). Update: mecanismo A/B pré-existente agora **disparado** — skill `update_check` diária (CronAgent 86400×HZ ticks) lê `UPDATE.CFG` (FAT32, env `UPDATE_URL` na build) → GET `UPDATE.MANIFEST` → semver → slot inativo. Servidor OTA on-demand `tools/serve_update.py`. ✅ cargo check 0 erros + 2 testes host. **Cenário-alvo dev: note 1 dev serve, note 2 real se auto-atualiza (§3.4).** **Princípio AIOS (§2.7/2.8): ciclo de vida auto-consciente** — visitante/mensageiro/residente + adaptação ao silício no 1º boot Residente; **autobiografia do OS via SGDB** (SELF.STATE + episódica L2/L3 + HANR L7 + audit — §2.8); **loop de telemetria dev↔neural (§3.5)** — neural POSTa BOOT.LOG, opencode analisa a quente e serve update; **imagem instalável fixa (§3.6A)** — `MODELS_SOURCE=network` default, alvo decide RAM em runtime (fim do "cabe/não cabe"); **transporte HTTP→mesh (§3.6B)** — contrato estável, mesh = otimização futura 1→nós, BitTorrent ❌; detecção de modo por GPT (menu live/install §2.6). **Implementado 2026-08-05: U1 ✅ (switch_slot promove slot→kernel.elf), U2 ✅ (comando shell `update`), U4 ✅ (rollback com guarda tries + BootSelfHeal dispara em PANIC), U6 ✅ (filtro ESP 0xEF + UPDATE.CFG na ESP)**. Gaps restantes U3 (assinatura/TPM) e I1–I12. |
| `0081-malha-cognitiva-distribuida-p2p.md` | Accepted | `completa (parcial)` | #189/#312f/#315.26/#315.27 | **Malha P2P/mesh ADR-0081:** Fase A TOFU/fail-closed ✅ (s238), FRAG\0 MTU ✅ (s238), Fase C experts/DSD/tier/FL/CRDT ✅ (s239), Fase B tiers HMAC/Ed25519 ✅ (s240), AEAD Tier F + anti-replay dados + calibração ed25519 (69-114µs) ✅ (s241). BitTorrent ❌ (merkle piece p/ Fase C futura). Abertos: SemanticRouter, merge CRDT, merkle piece. |
| `0077-ring3-isolation-ring.md` | Proposed | `fazendo` | ADR-0059 F6; #426 | **Ring3 Isolation Ring (ex-ADR-0060):** seams + boot wire; porto seguro (triple-fault blocker). Conectores no código, ring gated. Sessão debug dedicada |
| `0078-multi-slot-multimodal-learner.md` | Proposed | `por_fazer` | GGUF→ternário, 6 slots, visão, learner | **Multi-slot multimodal:** conversão GGUF→ternário com threshold adaptativo, 6 slots reais (Active/Fast/Pro/Coder/HwExp/Learner), visão SigLIP, aprendizado contínuo on-device. Cabe em 8GB RAM total ~5.8GB. Fases 1-4 sequenciais |
| `0079-neural-auto-installer.md` | **Superseded (processo) → 0086** | `substituida` | #421 | **AutoInstaller Neural (2026-07-27):** migração pendrive→HD/SSD/NVMe com seleção por HW (modelo por RAM, firmware por PCI, WASM por CPU), MHI no lugar de swap, Limine, particionamento, `MODELS_SOURCE=network`. **Deprecada 2026-08-05 — processo consolidado na ADR-0086 §2** (canônica); mantida como referência de design/riscos |
| `0079-neural-auto-installer-plan.md` | **Superseded (processo) → 0086** | `substituida` | #421 | **Plano de implementação AutoInstaller:** Fases 0–3 (~3.550 LOC), marcos M0–M4, topologia, checklists por fase. **Deprecado 2026-08-05 — consolidado na ADR-0086 §2**; Fases/Marcos continuam válidos como plano de trabalho detalhado |
| `NeuralFS.md` | Proposed | `fazendo` | #422 | SESSION_133: USB lock + GPT + unified exFAT; residual power-loss/stress |

### Follow-up ADR-0040 (residuals `por_fazer`, MVP intacto)

Triagem Onda 0 (2026-07-18) + Pós-LAN SESSION_152: `depends_on: lan` liberado (L3.5–L5 + NetFs peer). WiFi → `depends_on: wifi`. SESSION_154: TLS N4 opções A–D (pesquisa); WiFi inventário API77 + plano S0–S5 (FW-MAC, não SoftMAC clássico).

| Residual | IDEA | Destino | Bloqueio / tag |
|----------|------|---------|----------------|
| exFAT/NTFS/EXT **write** | #417 w | Onda 3 | exFAT ✅ opt-in; NTFS/EXT ⏳ |
| MHI DMA NVMe↔DRAM/VRAM | #420 DMA | Onda 5 | ▶️ AWAITING_HW típico |
| SysInstaller | #421 | defer | UI/LLM + write HD |
| NeuralFS disco fisico | #422 / NeuralFS.md | mount/GPT ✅; evidência Onda 1 | USB power-loss ▶️ AWAITING_HW |
| GPU Direct Storage | #423 | Onda 5 após Ready | GPU compute + NVMe DMA |
| Cloud mounts plenos | #418 | peer TCP ✅ SESSION_152 | S3/WebDAV backends residual (não RX) |
| Storage Manager App UI | #419 UI | Onda 3 cauda opcional | CLI report já existe |

## Conflitos de ID

Os conflitos são preservados; nenhum arquivo deve ser renomeado sem migração aprovada.

- **0026:** `0026-ecosystem-batch3-analysis.md` é a ADR canônica. `0026-sprint-29-xhci-usb-driver.md` é um plano de sprint histórico.
- **0045:** `0045-sound-voice-stack.md` é a decisão canônica Accepted. Perci/Bitwork migrou para `0054-perci-bitwork-integration.md` (pesquisa adiada).
- **0047:** `0047-latent-space-ai-os.md` é o documento-base. GPU e HMI são extensões nomeadas da família, não novas decisões numeradas.
- **Lacuna 0008:** não há arquivo ADR-0008 no repositório; o índice não infere conteúdo ausente.
- **ADR-0074 (lacuna):** sem arquivo próprio; referenciada apenas no código (`hermes/src/git_thin.rs` "git-over-HTTPS thin client (ADR-0074)") e SESSION_241. Conteúdo consolidado na ADR-0086 §3.3.

## Substituições explícitas

- ADR-0034 → ADR-0036.
- ADR-0035 → ADR-0036.
- Rotas sherpa/Vosk/Kokoro/Wyoming/Rustpotter → ADR-0045 Sound, sem apagar o histórico nas ADRs antigas.
- ADR-0037 → ADR-0055 (SMP); GPU → ADR-0048–0050.
- ADR-0005 → ADR-0055 (autoridade ISA).
- ADR-0014 §SMP/CorePools → ADR-0055.
- **#136** (LLM decide memory tier) → **ADR-0060** (política determinística BudgetManager).
- **ADR-0047-HMI §7 Soul Mirror** → **ADR-0060 Onda 7** (absorvido com AffectVector como fonte).
- **ADR-0036 EmotionState** → **ADR-0060 A.7** (classifier mantido como frontend do AffectVector).
- **#314 SleepCycle** → **ADR-0060 A.4** (fundido: SleepCycle batch + BEI contínuo).
- **ADR-0040 NeuralFS (persistência IA)** → **ADR-0063** (TicKV + NoProto + Índices IA como SGDB primário); RAG lexical → **ADR-0064**.

## Manutenção

Ao fechar uma sprint, aplicar o checklist de `docs/GOVERNANCE.md`: atualizar IDEA_BANK, lifecycle deste índice, TODO, STATE e SESSION na mesma passagem.

## Planos Cursor → ADR (implementados)

Registro dos planos de implementação (Cursor Plans) já refletidos no corpo das ADRs. Fonte dos planos: histórico maintainer / SESSION; **não** versionar `.cursor/plans` no repo.

| Plano (nome) | ADR(s) | Status implementação | Sessão / tag |
|--------------|--------|----------------------|--------------|
| `k-HAL H1-H5` | **0041** §11.1 | ✅ H1–H5 | pré-140 → v1.8.6 |
| `HalOffer API 1.8.x` | **0041** §9.4 / §11.2 | ✅ MVP HalOffer | v1.8.6 |
| `ADR41 H4 H5 full` | **0041** §11.3 | ✅ H4+/MMIO/Cap/AS | SESSION_140 / **v1.8.6** |
| `GPU Multivendor Unlock` | **0048–0050** | ✅ Degrau A–D (golden HW aberto) | SESSION_138 |
| `Pascal ACR Degrau` | **0048** §13.2 | ✅ Degrau P2 honesto | SESSION_138 |
| `ADR-0047 MVP PoC` | **0047** (+ gpu/hmi) | ✅ Accepted parcial | SESSION_126–127 |
| `Sprint Sound completa` | **0045** §9 | ✅ parcial honesto | SESSION_122 |
| `Ecosystem Package Hub` | **0051** (+ NeuralFS §12) | ✅ MVP | SESSION_134–135 |
| `Migrar agentes NeuralFS` | **0051** / **0052** | ✅ → stubs **corrigidos** por 0052 | SESSION_134–135 |
| `HANR Hermes Port` | **0053** | ✅ Waves 0–4 MVP | SESSION_136–137 |
| `Sandbox gates SMP` | **0055** | ✅ Fases 0–C wired; evidência TCG/WHPX | SESSION_141 |
| `Neural Device LEGOs` | **0056** (+ 0051–53, NeuralFS §12) | ✅ docs hub+specs+H1 bind; goldens VirtIO/ath10k | community + `device_recipe.rs` |
| `Sanitizar pasta docs` | INDEX + GOVERNANCE | ✅ ciclo IDEA→ADR | archive + INDEX |

**Próximo aceite operacional (0041):** ✅ CONCLUÍDO (SESSION_251) — boot WHPX com slog `NotifySent` + Cap/AS non-fatal evidenciado em `docs/evidence/boot-whpx-20260805.txt`. Fix raiz do reboot loop (commit 2662d50): GDT passa a usar `&*TSS` (lazy_static TSS com ISTs zerados nunca era dereferenciado → entrega #PF/#GP/timer fazia push para 0 → triple fault).

### Nome do produto

| Nome | Era | Cadeia |
|------|-----|--------|
| **K²CHJ** | ≤ v1.8.5 (histórico) | k-nano → k-ai → cortex → hermes → jarbas |
| **K³CHJ** | **canônico desde 2026-07-18** | k-nano → **k-hal** → k-ai → cortex → hermes → jarbas |

Arquivos ADR `*k2chj*` não são renomeados (links). Glossário: ADR-0042 §0.
