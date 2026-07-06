# SESSION_081 — Documentação Reestruturada: HW Real First + Multi-Vendor + Sprint Plan 84-95

**Data:** 2026-07-05 | **Sprint:** 84 — Bloco 21c | **v0.84.0-design**

## Objective
Revisão completa da documentação do projeto para:
1. Remover foco exclusivo em QEMU/VirtualBox como targets primários (são dev/debug apenas)
2. Remover foco exclusivo em GTX 1050 como GPU de referência (é um dos notes de teste)
3. Estabelecer **HW Real First** como princípio: hardware real, geral e moderno (NVIDIA, AMD, Intel; CPU, GPU, NPU)
4. Estabelecer **Busca Ativa na Internet** como política para itens bloqueados
5. Alinhar sprints 84-95 com TODAS as 354 ideias do IDEA_BANK, garantindo que nenhuma fique sem destino
6. Criar navegação clara para AI assistants: SESSION_INDEX, onboarding, cross-references

## Modified/Created Files

### Criados
- **`docs/sprint-plan-84-95.md`** (+820 LOC) — Plano mestre de 9 sprints. Todos os 354+ items do IDEA_BANK assignados a sprints/blocos. Sprint 84 (GPU Foundations NVIDIA/AMD/Intel), Sprint 85 (GPU Decode), Sprints 86-90 (JARVIS Cognitive), Sprint 91 (Polimento+Ecosystem), Sprint 92+ (AIOS Evolution 🔴).
- **`docs/memory/SESSION_INDEX.md`** (+120 LOC) — Catálogo de 42 sessões com títulos, sprints, e principais descobertas. Inclui seção "Lições Críticas (NÃO REPETIR)" com 10 dead-ends documentados.
- **`docs/ONBOARDING.md`** — (não criado, incorporado em STATE.md)

### Modificados (corpo)
- **`docs/TODO.md`** — Reescrito como checklist multissprint. Cada sprint seção com checkboxes, goals, sub-itens, dificuldades, dependências, fontes. Status flags: ✅ 🟡 ⏳ 🔴 💰 ❌.
- **`docs/memory/STATE.md`** — Roadmap expandido para 84-95 (9 sprints). Seção "Navegação Rápida para AI DEVs" com árvore de diretórios. Pendentes reorganizados por sprint com #ID do IDEA_BANK.
- **`docs/memory/IDEA_BANK.md`** — 9 items orphan (🟡 Futuro sem sprint) atualizados com sprints específicos (91 ou 92+). Seção 6 (Sprint Planning) expandida: Bloco 28→desmembrado em 21c+21d, tabela resumo de 8 para 10 blocos.
- **`docs/sprint-plan-84-90.md`** — Marcado como 🔴 ARQUIVO LEGADO com redirect para 84-95.
- **`docs/architecture/0037-smp-gpu-architecture.md`** — GTX 1050→genérico NVIDIA/AMD/Intel. QEMU/VBox→HW Real. 4.3 renomeado.
- **`docs/architecture/0029-gpu-architecture.md`** — Tabela HW expandida (Intel+AMD, Intel Arc), firmware ACR/PSP/GuC, hardware layer genérico.
- **`docs/architecture/0016-network-strategy.md`** — RTL8139 dev + e1000/r8169 HW real + busca por NIC não suportada. Fases N1-N3 sem QEMU references.
- **`docs/roadmap.md`** — Bloco 21c: NVIDIA/AMD/Intel, firmware FECS/PSP/GuC. Nota HW Real: multi-vendor. QEMU loader removido. Bloqueios: 🔴 Buscar na internet.

### Modificados (estrutura)
- **`AGENTS.md`** — NAVEGAÇÃO RÁPIDA adicionada (linhas 5-15). "Emulation First" → "HW Real First". Premissa de benchmark reformulada. **Nova:** "Soluções Bloqueadas Exigem Busca Ativa na Internet". **Nova:** "Dev/Teste em QEMU/VBox" com validação final em HW real. ~200 linhas de sessões históricas inline removidas (apontam para SESSION_*.md). Network Strategy: drivers por PCI ID + busca. Current Sprint: Sprint 84 (GPU Foundations). MemPalace integration (checkpoint/search/diary_write).
- **`docs/architecture/0001-initial-architecture-and-toolchain.md`** — "Emulator: QEMU primary test harness" → "QEMU: desenvolvimento e debug. HW real é o alvo primário."

## Key Decisions

1. **HW Real First é agora o princípio arquitetural #4** (substitui "Emulation First"). QEMU/VBox são estritamente dev/debug. Nenhum benchmark é válido em emulação.

2. **Busca Ativa na Internet** é política para TODO item 🔴 bloqueado. Context7, WebFetch, crates.io, arXiv, GitHub são as ferramentas. Nada de ficar bloqueado eternamente.

3. **Multi-vendor GPU**: NVIDIA (primeiro, firmware + docs mais acessíveis) → AMD (segundo, GPUOpen) → Intel (terceiro, i915 público) → NPU (futuro). GTX 1050 é APENAS um dos hardwares de teste.

4. **Sprint Plan 84-95**: Expansão de 7 para 9 sprints (84-91 ativos + 92+ bloqueado). Todos os 354+ items do IDEA_BANK assignados a sprints. Nenhum item 🟡 Futuro sem destino.

5. **Navegação AI-first**: AGENTS.md agora é um hub que aponta para docs/ específicos. SESSION_INDEX.md cataloga aprendizado. STATE.md tem árvore de navegação.

## Aprendizados

1. **AGENTS.md estava inchado** com 461 linhas misturando política, história, sessões e instruções. A separação clara (política em AGENTS.md, história em SESSION_*.md, plano em sprint-plan, estado em STATE.md) é muito mais sustentável.

2. **IDEA_BANK.md tinha 9 items orphan** (🟡 Futuro sem sprint). Todos foram assignados para Sprint 91 ou 92+. O critério: itens que dependem de HW ou rede vão para 92+; itens independentes vão para 91.

3. **Cross-referência ADR×IDEA×Sprint×TODO** é complexa mas necessária. A matriz de rastreabilidade em integration-adrs-idea-bank-sprints-todo.md ajuda, mas precisa ser mantida viva.

4. **Sessões históricas** em AGENTS.md duplicavam SESSION_*.md. Removê-las economizou ~200 linhas sem perda de informação.

## Bloqueios Atuais

- **B-01** (DHCP/DNS/HTTP): 🔴 Nenhum progresso. A política agora exige busca ativa na internet antes de qualquer trabalho.
- **Sprint 84 (GPU Foundations)** e seguintes: 🟡 Nada bloqueia. Podem começar.

## Próximos Passos

1. Sprint 84: GPU BAR0/BAR1 mapping UC (NVIDIA/AMD/Intel) — ~300 LOC
2. Sprint 84: Secure boot ACR/PSP/GuC — ~600 LOC
3. Sprint 85+: GPU Decode (BitNet offload)

## Files Changed (this session)
- AGENTS.md (major restructure)
- docs/TODO.md (full rewrite)
- docs/memory/STATE.md (roadmap + navigation)
- docs/memory/IDEA_BANK.md (9 orphan items + Seção 6)
- docs/memory/SESSION_INDEX.md (created)
- docs/sprint-plan-84-95.md (created)
- docs/sprint-plan-84-90.md (legacy notice)
- docs/roadmap.md (multi-vendor + HW Real)
- docs/architecture/0037-smp-gpu-architecture.md (generic GPU)
- docs/architecture/0029-gpu-architecture.md (multi-vendor table)
- docs/architecture/0016-network-strategy.md (HW real NICs + search)
- docs/architecture/0001-initial-architecture-and-toolchain.md (QEMU→dev)
- docs/integration-adrs-idea-bank-sprints-todo.md (multi-vendor + search)
