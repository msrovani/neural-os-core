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
| `0005-simd-and-fpu-enablement.md` | Accepted | `completa` | SIMD/FPU | Implementada |
| `0006-neural-primitives-and-libm.md` | Accepted | `completa` | Primitivas neurais | Implementada |
| `0007-intent-router-mlp.md` | Accepted | `modernizada` | Cortex/Hermes | Evoluiu para Trinity/Hermes |
| `0009-pic-watchdog-and-page-fault.md` | Accepted | `modernizada` | IRQ/#PF | Endurecida em sprints posteriores |
| `0010-strategic-roadmap-and-innovations.md` | Accepted | `modernizada` | #361–405 e temas | Roadmap absorvido por ADRs temáticas |
| `0011-bitlinear-and-hybrid-matmul.md` | Accepted | `completa` | BitLinear | Base do tensor engine |
| `0012-2bit-packing-quantization.md` | Accepted | `completa` | Packing ternário | Formato implementado |
| `0013-neural-os-executive-summary.md` | Accepted | `pesquisa` | Visão executiva | Snapshot histórico |
| `0014-ideias-hardware.md` | Proposed | `pesquisa` | Seed #1–116 | Ideias migradas ao IDEA_BANK |
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
| `0037-smp-gpu-architecture.md` | Accepted | `modernizada` | #319–354 | Implementação multi-vendor evolutiva |
| `0038-ecosystem-optimization.md` | Accepted | `pesquisa` | #355–360 | Auditoria e substituições |
| `0039-boot-flow.md` | Accepted | `modernizada` | Boot 8 fases | Alinhada a Pacotes A/B |
| `0040-filesystem-architecture.md` | Proposed | `por_fazer` | #417–423 | Arquitetura FS futura |
| `0041-k2chj-capability-rings.md` | Accepted | `completa` | #424–432 | PoC P0–P9, não produção plena |
| `0042-k2chj-adequacao-boot.md` | Accepted | `modernizada` | #433–440 | N1–N5 + wire completos em v1.8.0 |
| `0043-cubecl-patterns-and-technologies.md` | Accepted | `pesquisa` | GPU patterns | Análise tecnológica |
| `0044-edge-python-patterns.md` | Accepted | `pesquisa` | VM/SSA patterns | Análise tecnológica |
| `0045-sound-voice-stack.md` | Accepted | `conflito_id` | #75, #83, #84, #315.21–25, #360, #438, #442 | **Canônico ADR-0045**; Sound ativa |
| `0045-perci-bitwork-integration.md` | Proposed | `conflito_id` | Perci/Bitwork | Pesquisa, não canônica para o ID |
| `0046-airllm-gguf-streaming.md` | Proposed | `por_fazer` | GGUF streaming | Não implementada |
| `0047-latent-space-ai-os.md` | Proposed | `conflito_id` | LatentBus/EvolveAgent/Probe | **Canônico base da família ADR-0047** |
| `0047-gpu-compute-pipeline.md` | Proposed | `conflito_id` | Extensão GPU | Extensão temática `0047-GPU` |
| `0047-hmi-neural-desktop.md` | Proposed | `conflito_id` | Extensão HMI | Extensão temática `0047-HMI` |
| `NeuralFS.md` | Proposed | `por_fazer` | #422 | Documento arquitetural sem ID ADR |

## Conflitos de ID

Os conflitos são preservados; nenhum arquivo deve ser renomeado sem migração aprovada.

- **0026:** `0026-ecosystem-batch3-analysis.md` é a ADR canônica. `0026-sprint-29-xhci-usb-driver.md` é um plano de sprint histórico.
- **0045:** `0045-sound-voice-stack.md` é a decisão canônica Accepted. `0045-perci-bitwork-integration.md` é pesquisa/proposta independente.
- **0047:** `0047-latent-space-ai-os.md` é o documento-base. GPU e HMI são extensões nomeadas da família, não novas decisões numeradas.
- **Lacuna 0008:** não há arquivo ADR-0008 no repositório; o índice não infere conteúdo ausente.

## Substituições explícitas

- ADR-0034 → ADR-0036.
- ADR-0035 → ADR-0036.
- Rotas sherpa/Vosk/Kokoro/Wyoming/Rustpotter → ADR-0045 Sound, sem apagar o histórico nas ADRs antigas.

## Manutenção

Ao fechar uma sprint, aplicar o checklist de `docs/GOVERNANCE.md`: atualizar IDEA_BANK, lifecycle deste índice, TODO, STATE e SESSION na mesma passagem.
