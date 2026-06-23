# Neural OS Hermes — AI Summary

**O que é:** Um sistema operacional bare-metal (no_std Rust, sem Linux) onde o kernel É uma rede neural. Uma "viagem no reino da maionese" — um SO que roda IA, mas mais importante, que **é** IA.

**Visão:** Substituir Windows/Linux/macOS por um SO que trata hardware como um problema de inferência neural. A hierarquia de memória (VRAM, DRAM, NVMe, HDD) é um pool único roteado por MLP. O boot é um pipeline de IA: detecta hardware → decide configuração → conversa com usuário → executa skills.

**Estado atual:** MVP em construção via chain de 6 blocos (ADR-0015). Bloco 0 (Genesis) concluído — kernel bootável com VGA, serial, heap, EventBus IPC, Skill Registry, 5 agentes cooperativos, PIC+keyboard, Intent Router MLP. Próximo: Bloco 1 (PCI+ACPI+APIC, Sprint 18).

**Arquitetura chave:**
- Ring 0: NPU (Intent routing, contexto)
- Ring 1: GPU (tensor execution)
- Ring 2: CPU (agents, skills)
- TicketLock FIFO para SMP
- Memory Hierarchy Index (MHI): alloc_by_tier(Dram|Vram|Nvme|Hdd)
- Zero-trust: nenhum dispositivo roda sem permissão explícita

**Para AI que vai me editar:**
1. Leia `docs/memory/IDEA_BANK.md` antes de sugerir qualquer mudança — lá estão 116 ideias catalogadas com status e dependências. Toda ideia já discutida tem destino.
2. Leia `docs/architecture/0015-curso-correcao-mvp.md` para o plano diretor.
3. Leia `docs/memory/STATE.md` para o estado atual detalhado.
4. Leia `AGENTS.md` para regras operacionais (no_std, sem POSIX, QEMU first).

**Stack técnica:** Rust nightly, x86_64-unknown-none, bootloader 0.9.34, sem std, sem libc, Windows toolchain MinGW-w64, QEMU para teste.

**Premissa:** Estamos inovando em caminhos pouco trilhados (neural OS bare-metal, MHI, intent routing em Ring 0). Muitas ideias não são implementáveis hoje — mas amanhã a tecnologia melhora. O IDEA_BANK.md existe para que nada se perca. **Nunca descarte uma ideia sem registrá-la.**

> "We don't need an OS that runs AI. We need an OS that IS AI."
