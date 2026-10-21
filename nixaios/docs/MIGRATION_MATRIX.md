# NixAIOS — Migration Matrix (PORT lines first)

Taxonomia: `PORT` | `ADAPT` | `DELEGATE_LINUX` | `BAREMETAL_ONLY` | `SUPERSEDE_NIX` | `RESEARCH` | `DEAD_WEIGHT`

Produto paralelo em `nixaios/` — bare-metal permanece canônico. Evidência: ADR-0113.

## Eixos viáveis (prioridade)

| ID | Nome | Disposicao | Alvo NixAIOS | Fase | Evidencia |
|----|------|------------|--------------|------|-----------|
| A-0059 | App Factory path A | PORT | `nos-factory` | F3 | wasmi self-test + CapGate |
| A-0081 | Mesh P2P | PORT | `nos-mesh` | F5 | TOFU/FRAG/SkillSync frames |
| A-0057 | Compute dispatch | PORT/ADAPT | `nos-cortex` dispatch | F2 | fail-closed local |
| T-mesh_distrib | Distributed assign | PORT | `nos-cortex::distrib` | F5 | capacity_weighted_assign |
| A-0060 | BEI | PORT | `nos-bei` (tick fino) | pós-MVP | budget/affect hooks |
| A-0078 | ModelHub multi-slot | PORT/ADAPT | `nos-cortex` hub | F2 | validate_for_slot + LOO |
| A-0091 | neural-sgdb | PORT | `nos-memory` | F4 | FileStorage remember/recall |
| A-0088 | Premissa AIOS | PORT | doutrina nosd | F0 | ADR-0118 |
| A-0113 | Missão sistêmica | PORT | honesty nosd | F0 | NOSD_SCORE |

## Tecnologias (amostra PORT/ADAPT)

| ID | Nome | Disposicao | Alvo | Fase |
|----|------|------------|------|------|
| T-1.1 | Agent ontológico | PORT | `nos-hermes` | F5 |
| T-1.6 | Trinity MoE | PORT | `nos-cortex` | F2 |
| T-2.9 | EventBus | PORT | `nos-bus` | F1 |
| T-2.10a | CapGate WASM | PORT | `nos-wasm` + ipc | F1/F3 |
| T-2.10f2 | AirLLM/GGUF | ADAPT | HeaderScanner + HotSwap | F2 |
| T-1.7 | UI Orb/HUD | ADAPT | `nos-jarbas` + cage | F6 |
| T-4.18+ | Mesh lab | PORT | `nos-mesh` | F5 |
| T-7.x | BitNet/ModelHub | PORT/ADAPT | hub + llama.cpp | F2 |
| T-2.1 | Limine | BAREMETAL_ONLY | — | — |
| T-2.2 | IDT | BAREMETAL_ONLY | — | — |
| T-2.3 | PMM | DELEGATE_LINUX | kernel | — |
| T-2.6 | SMP trampoline | BAREMETAL_ONLY | — | — |
| T-2.8 | CFS próprio | DELEGATE_LINUX | CFS Linux | — |
| T-2.10g | Ring3 bare | SUPERSEDE_NIX | namespaces/seccomp | defer |
| T-3.x | BAR Mapped físico | ADAPT | CUDA/HIP Mapped honesty | F2+ |
| T-5.x | HDA bare | DELEGATE_LINUX | PipeWire | F7 |
| T-6.x | ATA/FAT drivers | DELEGATE_LINUX | ext4/btrfs | — |

## ADRs (amostra)

| ID | Disposicao | Nota NixAIOS |
|----|------------|--------------|
| 0001–0005, 0009 | BAREMETAL_ONLY / DELEGATE | boot/IRQ/paging |
| 0041 CapGate P3 | PORT | in-process + Landlock |
| 0052 Artifact | PORT | promote assinado |
| 0056 Device LEGO | ADAPT | udev/nixos hardware |
| 0058 Cards UI | ADAPT | Slint widgets |
| 0077/0102 Ring3 | SUPERSEDE_NIX | cgroup/seccomp |
| 0079→0086 Install | SUPERSEDE_NIX | ISO + nixos-rebuild |
| 0086 OTA | ADAPT | geracoes Nix |
| 0089 malha whitepaper | RESEARCH | Fase E host apps ≈ este produto |
| 0089 smp-runqueue | BAREMETAL_ONLY | conflito ID — nao fundir |
| 0090 Jarbas v2 | ADAPT | nos-jarbas |
| 0092 Boot obs | ADAPT | NOSD_SCORE / journald |
| 0101 Falcon3 lab | PORT contratos | hub slots |
| 0112 BAR | ADAPT | CUDA malloc honesty |
| 0117 BitFuzzy | DEFER hub | host-only |

Ondas futuras preenchem as ~100 linhas restantes de TECNOLOGIAS §1–15 e INDEX completo.
