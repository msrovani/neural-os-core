# Session 023 — Code Review & Critical Bugfix Sprint (v0.17.1)

**Date:** 2026-06-24  
**Goal:** Install Context7 MCP, review all code, fix Critical bugs, plan Sprint 24

## Context

Sprint 23 (Network Sprint, pós-MVP) was implemented but never reviewed. Before shipping, we decided to do a comprehensive code review using Context7 (fresh documentation) and 5 parallel agent reviewers.

## What Happened

### Context7 MCP Setup
- Installed via `npx ctx7 setup --opencode` (remote MCP server)
- OAuth login as msrovani@gmail.com
- Configuration at `~/.config/opencode/opencode.jsonc`
- Skill installed at `~/.agents/skills/context7-mcp/SKILL.md`
- AGENTS.md rules added at 3 levels (global opencode, project, skill)
- crates.io search capability documented for Rust crates not in Context7

### Code Review (5 Parallel Agents)
1. **Kernel Core Agent** — main.rs, memory.rs, allocator.rs, interrupts.rs → found nostack UB, slab off-by-one
2. **Hardware Agent** — pci.rs, acpi.rs, apic.rs, e1000.rs → found PCI bridge bus, XSDT stride, e1000 dead
3. **Neural Agent** — nn.rs, tensor.rs, hermes.rs → found batch bias bug
4. **Rede Agent** — net.rs, proto.rs, e1000.rs → found DHCP broadcast reject, missing ACK check
5. **Workspace Agent** — Cargo.toml, event-bus, skill-registry, ticket-lock → found 0 criticals

### 10 Critical Bug Fixes Applied
All 10 CRITICAL items fixed and verified with `cargo check --release` (0 errors).

### QEMU Boot Validation
Boot sequence starts, tests pass, e1000 initializes and detects Link UP. DHCP discover triggers PageFault (`VirtAddr(0x2103b0)`) in `e1000::send()` — pre-existing DMA buffer mapping bug exposed after enabling RCTL/TCTL.

## Dificuldades e Correções

1. **e1000 estava morto desde o Sprint 23** — RCTL/TCTL nunca escritos. O `send()` e `recv()` nunca foram testados porque nenhum frame era transmitido.
2. **PHYS_MEM_OFFSET usado corretamente** no `new()` e `init()` (MMIO funciona), mas o `send()` acessa TX buffer físico via `buf_paddr + pmoff`. A Page Fault em `0x2103b0` sugere que o offset não está sendo aplicado — possível shadowing ou race do AtomicU64.
3. **XSDT stride bug** teria quebrado ACPI em hardware real com >4 GB RAM — só não quebrou no QEMU porque a tabela RSDT (4-byte entries) foi usada (revision 0).
4. **Bridge PCI bug** não afeta QEMU (sem bridges PCI-PCI) mas quebra em hardware real.
5. **DHCP nunca funcionou** — xid+1 viola RFC, broadcast MAC rejeitado, `return true` sem ACK.

## Decisões Tomadas

1. Context7 via remote MCP (OAuth) — API key ativa para documentação
2. crates.io + docs.rs como fallback para crates não indexados no Context7
3. Bugfixes aplicados antes de planejar Sprint 24 — não acumular dívida técnica
4. e1000 DMA buffer bug deixado para Sprint 24 (requer refatoração do alloc_page ou mapping)

## IDEAS Moduladas

- `💡 PRECISA SPONSOR: e1000 DMA buffer mapping fix` — mover TX buffers para memória identity-mapped ou criar pool de páginas reservadas
- `💡 Adicionado ao IDEA_BANK: "Verificar AtomicU64 PHYS_MEM_OFFSET em send()/recv()"` — possível race ou shadowing

## Versão

v0.17.0 → v0.17.1 (bugfix release)
