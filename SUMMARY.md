# Neural OS Hermes — AI Summary

**O que e:** Um sistema operacional bare-metal (no_std Rust, sem Linux) onde o kernel e 19 agentes cooperativos. Um SO que roda IA, mas que **e** IA.

**Visao:** Substituir Windows/Linux/macOS por um SO que trata hardware como um problema de inferencia neural. O LLM identifica hardware, decide alocacao no MHI, e se recupera de erros automaticamente.

**Estado atual (v0.56.0):** 20 agentes rodando. Transformer 4 layers BitNet (~272K params) + Medusa 3-head speculative decoding. Pipeline manifest, Memory Tree (hierarchical chunks), Knowledge Graph (agent/skill/hardware nodes). Hermes Cognitive com SDD + ReAct 7 fases + Council skill. Self-Healing com FailureClass taxonomy + checkpoint. Trust & Security com Ed25519 identity + Security Pipeline + Safety Interceptor (Asimov 4 Laws). Self-Optimization com Usage Analyzer + Workflow Predictor + Config Learner.

**Arquitetura chave:**
- 20 agentes nativos (Agent trait, AgentRegistry, AgentScheduler)
- Ring 0: NPU (Intent routing), Ring 1: GPU, Ring 2: CPU (agents/skills)
- Pipeline manifest com scored provider selection + fallback
- Memory Tree: hierarchical summaries + importance pruning + scouting
- Knowledge Graph: Node+Edge indexado, query por relação/vizinhança
- TicketLock FIFO + IrqSafeLock para SMP sem deadlock
- Memory Hierarchy Index: alloc_by_tier(Dram|Vram|Nvme|Hdd)
- Zero-trust: CapabilityToken com suporte Ed25519
- Safety Interceptor: Asimov 4 Laws no Ring 0

**Blocos completos (15 blocos, 56 sprints):**
1. Chassi — VGA, heap, EventBus, SMP, APIC
2. Discovery — PCI, ACPI, MHI, Trust
3. Rede — RTL8139, smoltcp, DHCP, VirtIO-net
4. Transformer — Attention 4 layers BitNet
5. HW-Aware LLM — PCI+USB training (66K pairs)
6. Capabilities — HW -> skill mapping
7. Self-Healing — Failure taxonomy, checkpoint
8. Agent/Skill-First — 18 agentes nativos
9. Network Evolution — DHCP, ARP, VirtIO-net
10. Display + Bugfix — Framebuffer, VirtIO-GPU PCI caps
11. CDC + Delta + Locks — Rabin chunking, IrqSafeLock, DmaBuf
12. Network+Platform — x2APIC, Huge Pages, PCI bridges, Cron, MCP
13. Trust & Security — Ed25519, Security Pipeline, Mask Secrets
14. Hermes Cognitive — SDD, ReAct, Council, Self-Optimization
15. **Medusa+Ecosystem** — **Spec decode, Pipeline, MemTree, KG, DAG, Dashboard**

**Para IA que vai me editar:**
1. Leia `docs/memory/IDEA_BANK.md` — 336+ ideias catalogadas com status
2. Leia `AGENTS.md` para regras operacionais (no_std, QEMU first)
3. Leia `docs/memory/STATE.md` para estado atual detalhado

**Stack:** Rust nightly, x86_64-unknown-none, bootloader 0.9.34, smoltcp, ed25519-dalek, embedded-graphics. Windows MinGW-w64, QEMU para teste.

> "We don't need an OS that runs AI. We need an OS that IS AI."
