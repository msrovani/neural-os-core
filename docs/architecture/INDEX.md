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
| `0031-aios-self-update-wasm-jarvis.md` | Accepted | `modernizada` | #306–310, #383–390 | Implementada com desvios documentados |
| `0032-wasm-agent-apps.md` | Proposed | `modernizada` | #391–396 | Runtime/ABI implementados |
| `0033-on-device-micro-learning.md` | Proposed | `modernizada` | #397–401 | Base de micro-learning implementada |
| `0034-jarvis-conscious-interaction-layer.md` | Superseded | `substituida` | #310/#315 | Substituída pela ADR-0036 |
| `0035-jarvis-deep-research-ecosystem-convergence.md` | Superseded | `substituida` | #315 | Substituída pela ADR-0036 |
| `0036-jarvis-unified-interaction-layer.md` | Accepted | `modernizada` | #315.* | Canônica para interação/persona |
| `0037-smp-gpu-architecture.md` | Superseded | `substituida` | #319–354 → **0055** / GPU **0048–50** | SMP→0055; GPU→0048–0050 |
| `0038-ecosystem-optimization.md` | Accepted | `pesquisa` | #355–360 | Auditoria e substituições |
| `0039-boot-flow.md` | Accepted | `modernizada` | Boot 8 fases | Alinhada a Pacotes A/B |
| `0040-filesystem-architecture.md` | Accepted | `completa` | #417–423 | MVP aceite 2026-07-16; **residuals `por_fazer`** (não reabrem ADR) |
| `0041-k2chj-capability-rings.md` | Accepted | `fazendo` | #424–432, **#459–461** | **K³CHJ** capability; P0–P9 ✅; planos H1–H5+HalOffer+H4+/AS ✅ v1.8.6; path file `k2chj` estável |
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
| `0051-hermes-ecosystem-packages.md` | Accepted (MVP) | `fazendo` | PackageHub + VFS | SESSION_135: stubs Agency removidos; contrato em ADR-0052 |
| `0052-neural-artifact-contract.md` | Accepted (MVP) | `fazendo` | schema/hash/sig/acionaveis | Deny unsigned; Hermes create; import sandbox |
| `0053-hanr-parity-marketplace-trust.md` | Accepted (MVP) | `fazendo` | HANR parity | SESSION_136: session Ed25519, market, memory, MCP mínimo |
| `0054-perci-bitwork-integration.md` | Proposed | `pesquisa` | Perci/Bitwork | Ex-0045 conflito; **adiada** — CognitiveRouter Bitwork→Trinity; sem wire |
| `0055-smp-revision.md` | Accepted | `fazendo` | #16–42, #20–33, #35–41, #317, #319–324, #347 | **Canônica SMP** — FeatureGate; TCG APs=1; WHPX BSP-only (SESSION_141); residuals HW hybrid/matmul |
| `0056-neural-device-lego.md` | Accepted (MVP) | `fazendo` | #464 | L0/L1/L2 DeviceRecipe; UnlockDAG; trust Ed25519; community hub; H1 bind table |
| `0057-compute-dispatch-smp-gpu-npu.md` | Accepted | `fazendo` | #20–42, #211, #329–331, #345–346, #454–456 | **Dispatch de compute** LLM: WS-A wake multi-AP ✅ (QEMU -smp 4 APs=3); WS-B/C dispatcher wired; WS-D GPU hook (Ready gate) + WS-E NPU XDNA/Intel = **Layer S/HW** honesto |
| `0058-generative-card-desktop.md` | Proposed | `fazendo` | #79/#80/#82/#279d/#283/#448/#452/#465 + #468–470 | **UI/Desktop Jarbas:** embedded-graphics `DrawTarget` + matrix-gui/embedded-gui + `UiDeclaration`/`UiRenderer` (cards gerados por LLM #412 / skill WASM). Supersede parcial 0047-HMI (H3 ❌). Aguarda confirmação |
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

## Substituições explícitas

- ADR-0034 → ADR-0036.
- ADR-0035 → ADR-0036.
- Rotas sherpa/Vosk/Kokoro/Wyoming/Rustpotter → ADR-0045 Sound, sem apagar o histórico nas ADRs antigas.
- ADR-0037 → ADR-0055 (SMP); GPU → ADR-0048–0050.
- ADR-0005 → ADR-0055 (autoridade ISA).
- ADR-0014 §SMP/CorePools → ADR-0055.

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

**Próximo aceite operacional (0041):** boot QEMU com slog `NotifySent` + Cap/AS non-fatal — não reabre plano; só evidencia lifecycle.

### Nome do produto

| Nome | Era | Cadeia |
|------|-----|--------|
| **K²CHJ** | ≤ v1.8.5 (histórico) | k-nano → k-ai → cortex → hermes → jarbas |
| **K³CHJ** | **canônico desde 2026-07-18** | k-nano → **k-hal** → k-ai → cortex → hermes → jarbas |

Arquivos ADR `*k2chj*` não são renomeados (links). Glossário: ADR-0042 §0.
