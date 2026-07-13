# Neural OS Hermes — AI Summary

**O que é:** Um sistema operacional bare-metal (`no_std` Rust, sem Linux) onde TUDO é um Agente ou uma Skill. 247+ agentes cooperativos, com MoE (Mixture of Experts) integrado ao LLM. O kernel roda IA, mas **é** IA — cada hardware é identificado por rede neural, cada decisão de alocação é inferida pelo modelo ternário BitNet.

**Estado atual (v1.2.0):** ~26.000 LOC, 180+ arquivos Rust, 0 erros. **ATA PIO bug corrigido** — disco (MBR, FAT32, modelos, firmware) lê corretamente pela primeira vez desde v0.1. Firmware NVIDIA + Intel + Realtek + WiFi (116 blobs, ~12MB). HW Expert v3: 61.453 VID/DID. SelfHealing I3/I4. WiFi Intel AX200. 3 camadas visuais. HDA capture+playback. BrowserAgent real HTTP GET.

**Arquitetura chave:**
- 247+ agentes (20 nativos + 147 The Agency + ~80 importados + HW + FS)
- Ring 0: NPU (Intent routing), Ring 1: GPU (tensor), Ring 2: CPU (agents/skills)
- Trinity MoE Router com 6 experts + router_weight treinado (sem keyword matching)
- BitNet-b1.58 850M (GQA, BitFFN) + Medusa speculative decoding + KV Cache
- Memory Tree v2 + Knowledge Graph bitemporal + Atkinson-Shiffrin 3-tier
- TicketLock FIFO + IrqSafeLock + SPSC lock-free rings
- Memory Hierarchy Index: alloc_by_tier(Dram|Vram|Nvme|Hdd)
- SDIO MoE: 95.812 entradas .inf/.sys reais com análise pefile
- Ed25519 identity + TPM 2.0 + Merkle Audit Trail (SHA-256 chain)
- Safety Interceptor: 4 invariantes SMT-proof (I1-I4)
- WiFi: Intel AX200, Realtek, Atheros, Broadcom com IA-generate register map
- Áudio: Pocket TTS GPU offload, Klatt formant, VAD, SER, Wake Word

**Sprints completos (109 sprints, ~19.000 LOC):**
1. Chassi — VGA, heap, EventBus, SMP, APIC, PCI, ACPI
2. Agent System — 20 agentes nativos + The Agency (147) + HW agents
3. Rede — RTL8139, E1000, smoltcp, DHCP, HTTP, ARP, DNS
4. Transformer — BitNet-b1.58 850M, Tensor Engine, PackedTernary
5. GPU — NVIDIA/AMD/Intel, VRAM buddy, SPSC ring, secure boot
6. Display — Compositor multi-window, JARVIS avatar, temas, TTF
7. JARVIS Persona — SOUL.md, IPW, Notification Gate, Session Compression
8. JARVIS Security — Fail-Closed I1-I4, Merkle Audit, Fluid Persona
9. JARVIS Emotion — ADE Pipeline, 16-stage Persona, Emotion Analysis
10. SleepCycle — 5 fases, MemoryTree, KG bitemporal, BGE embedding
11. WiFi — Generic driver, 802.11 scan, WPA2, MSI-X, AER, DMA
12. Trinity MoE — 6 experts, AutoLearn, RegMap IA, Boot Agent
13. SDIO MoE — 45 packs, 95.812 entradas, pefile analysis
14. Áudio — HDA, USB, TTS(VAD+SER+WakeWord+RingBuffer+Mixer)
15. WASM — Stack VM, 20+ opcodes, fuel metering, BitNet IDE
16. SmileyOS Nativo — 55+ comandos shell, compositor drag/resize

**Para IA que vai me editar:**
1. Leia `docs/TECNOLOGIAS.md` — catálogo completo de todas as tecnologias
2. Leia `AGENTS.md` — regras operacionais e plano diretor
3. Leia `docs/memory/STATE.md` — estado atual detalhado
4. Leia `docs/TODO.md` — itens pendentes reais

**Stack:** Rust nightly, x86_64-unknown-none, bootloader 0.11.15, smoltcp 0.13, ed25519-compact, embedded-graphics 0.8. QEMU + VirtualBox para dev, HW real para validação.

> "We don't need an OS that runs AI. We need an OS that IS AI."
