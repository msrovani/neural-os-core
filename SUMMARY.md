# Neural OS Hermes — AI Summary

**O que é:** Um sistema operacional bare-metal (no_std Rust, sem Linux) onde o kernel É uma rede neural. Uma "viagem no reino da maionese" — um SO que roda IA, mas mais importante, que **é** IA.

**Visão:** Substituir Windows/Linux/macOS por um SO que trata hardware como um problema de inferência neural. A hierarquia de memória (VRAM, DRAM, NVMe, HDD) é um pool único roteado por MLP. O boot é um pipeline de IA: detecta hardware → decide configuração → conversa com usuário → executa skills.

**Estado atual:** MVP em construção via chain de 6 blocos (ADR-0015). Blocos 0-5 concluídos — kernel bootável com Hermes Chat, Trust Cache, HardwareInfoSkill, MHI, SMP, PCI/ACPI/APIC, EventBus IPC, Skill Registry, 6 agentes cooperativos. Próximo: Sprint 23 (Network Sprint — VirtIO-net + smoltcp). Análise de ecossistema completa (Tiers 0-4, 136 repos, 249 ideias catalogadas — ADRs 0020-0024).

**Arquitetura chave:**
- Ring 0: NPU (Intent routing, contexto)
- Ring 1: GPU (tensor execution)
- Ring 2: CPU (agents, skills)
- TicketLock FIFO para SMP
- Memory Hierarchy Index (MHI): alloc_by_tier(Dram|Vram|Nvme|Hdd)
- Zero-trust: nenhum dispositivo roda sem permissão explícita

**Para AI que vai me editar:**
1. Leia `docs/memory/IDEA_BANK.md` antes de sugerir qualquer mudança — lá estão **249 ideias** catalogadas com status e dependências (Tiers 0-4, 136 repos analisados). Toda ideia já discutida tem destino.
2. Leia `docs/architecture/0015-curso-correcao-mvp.md` para o plano diretor.
3. Leia `docs/memory/STATE.md` para o estado atual detalhado.
4. Leia `AGENTS.md` para regras operacionais (no_std, sem POSIX, QEMU first).

**Stack técnica:** Rust nightly, x86_64-unknown-none, bootloader 0.9.34, sem std, sem libc, Windows toolchain MinGW-w64, QEMU para teste.

**Premissa:** Estamos inovando em caminhos pouco trilhados (neural OS bare-metal, MHI, intent routing em Ring 0). Muitas ideias não são implementáveis hoje — mas amanhã a tecnologia melhora. O IDEA_BANK.md existe para que nada se perca. **Nunca descarte uma ideia sem registrá-la.**

> "We don't need an OS that runs AI. We need an OS that IS AI."
