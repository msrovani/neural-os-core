# 📊 Hermes Crate — Auditoria Completa: Neural OS vs Nous Research Hermes Agent

**32,166 LOC** | 119 arquivos | 817 pub fns | 138 testes | 402 locks | 7,446 LOC dead code (23%)

---

## 🏗️ Comparativo Arquitetural

| Feature | Nous Research Hermes Agent | Neural OS Hermes |
|---------|--------------------------|------------------|
| **Language** | Python (TypeScript desktop) | Rust `#![no_std]` bare-metal |
| **Memory** | 4 camadas: MEMORY.md + USER.md + SQLite FTS5 + Skills | memory_store + SGDB + AFFECT + cognitive_bridge |
| **Learning Loop** | Nudge periódico → auto-save → skill creation | evolve + self_evolve + skill_opt + auto_learn |
| **Skills** | Markdown files + agentskills.io standard + patch updates | WASM + op-IR + skill_manifest + skill_observer |
| **Gateway** | Cross-platform (Telegram/Discord/Slack/WhatsApp) | EventBus + P2P Mesh |
| **Session** | SQLite com FTS5 full-text search | BTreeMap in-memory (volátil) |
| **Compression** | LLM summarization + lineage preservation | AUTO_COMPACT_THRESHOLD=3 (mínimo) |
| **Prompt Caching** | Frozen snapshot + prefix cache aware | N/A (bare-metal, sem API calls) |
| **Self-Improvement** | Background review + memory writes | self_evolve + evolve + skill_opt |
| **User Modeling** | Honcho dialectic (12 identity layers) | SoulProfile + AffectVector (simplificado) |
| **Security** | Memory injection scanning + zero telemetry | CapGate + PermissionGate + Quarantine |

---

## 🔴 Gaps Críticos (P0)

### 1. **7,446 LOC dead code (23% da crate)** — 35 módulos sem callers

| Módulo | LOC | Status |
|--------|-----|--------|
| graph_engine | 884 | ❌ DEAD |
| trinity_inject | 608 | ❌ DEAD |
| adaptation | 487 | ❌ DEAD |
| skill_sync | 480 | ❌ DEAD |
| wifi_agent | 474 | ❌ DEAD |
| wasi_host | 410 | ❌ DEAD |
| sgdb_agent | 340 | ❌ DEAD |
| skill_marketplace | 321 | ❌ DEAD |
| optimizer | 302 | ❌ DEAD |
| safety | 278 | ❌ DEAD |
| app_factory | 243 | ❌ DEAD |
| chat_tree | 234 | ❌ DEAD |
| git_thin | 222 | ❌ DEAD |
| quarantine | 219 | ❌ DEAD |
| native_agents | 212 | ❌ DEAD |
| + 20 mais | 1,430 | ❌ DEAD |

**Impacto:** 23% do código nunca é executado. Compilação mais lenta, binário maior, manutenção desnecessária.

### 2. **Zero testes nos módulos ativos mais importantes**

| Módulo | LOC | Testes | Callers | Veredicto |
|--------|-----|--------|---------|-----------|
| net | 684 | 0 | 119 | ❌ CRÍTICO |
| cognitive_bridge | 835 | 0 | 30 | ❌ CRÍTICO |
| hermes | 751 | 0 | 33 | ❌ |
| netstack | 1,199 | 0 | 46 | ❌ |
| package_hub | 1,226 | 0 | 39 | ❌ |
| self_evolve | 454 | 0 | 25 | ❌ |
| wasmi_rt | 580 | 0 | 22 | ❌ |
| memory_store | 301 | 0 | 11 | ❌ |

**Impacto:** Qualquer refactor nos módulos ativos quebra silenciosamente.

### 3. **402 lock() calls no hotpath**

Os módulos com mais locks:
- net: 22 locks
- cognitive_bridge: 22 locks
- self_evolve: 10 locks
- globals: 5 locks
- memory_store: 5 locks
- skill_opt: 6 locks

**Impacto:** Priority inversion, deadlocks potenciais em IRQ context.

### 4. **Sessão volátil — sem persistência**

O Nous Research Hermes usa SQLite com FTS5 para persistir sessões. O Neural OS usa `BTreeMap` in-memory — todas as sessões são perdidas no reboot.

**Impacto:** Impossível fazer "session search" (feature core do Hermes Agent).

---

## 🟡 Gaps Médios (P1)

### 5. **Sem "Memory Nudge" periódico**

O Hermes Agent tem um mecanismo de "nudge" que periódicamente pede ao LLM para revisar o que aconteceu e salvar o que for útil. O Neural OS tem `AFFECT_SNAPSHOT` + `cognitive_bridge` mas não tem um mecanismo equivalente de "auto-save" periódico.

### 6. **Skills sem "patch" incremental**

O Hermes Agent usa `skill_manage(action="patch")` para atualizar skills com mudanças incrementais. O Neural OS usa `promote_skill_to_wasm()` que recria a skill inteira.

### 7. **Sem "frozen snapshot" para prompt cache**

O Hermes Agent mantém MEMORY.md/USER.md como frozen snapshots para preservar o prefix cache do LLM. O Neural OS reconstrói o prompt a cada tick.

### 8. **AUTO_COMPACT_THRESHOLD=3 é mínimo**

O threshold de compactação é muito baixo. O Hermes Agent usa LLM summarization com lineage preservation.

### 9. **Sem "write_approval" para memory**

O Hermes Agent tem um gate de aprovação para writes de memória. O Neural OS escreve livremente.

---

## 🟢 O que Funciona Bem (alinhado com Nous Research)

| Feature | Hermes Agent | Neural OS | Estado |
|---------|-------------|-----------|--------|
| Learning Loop | ✅ | evolve + self_evolve + skill_opt | ✅ |
| Skill Creation | ✅ Markdown | ✅ WASM + op-IR | ✅ |
| Self-Improvement | ✅ Background review | ✅ self_evolve | ✅ |
| Affect/Emotion | ✅ Honcho (12 layers) | ✅ AffectVector + SoulProfile | ✅ |
| HITL | ✅ Approval flow | ✅ approval + hitl_ui | ✅ |
| P2P Mesh | N/A | ✅ ADR-0081 | ✅ |
| CapGate Security | ✅ Container hardening | ✅ FE grant/revoke | ✅ |

---

## 📐 Plano de Correção

### Fase 1: Limpeza (dead code)

| # | Ação | LOC | Impacto |
|---|------|-----|---------|
| **P0-1** | Remover 35 dead modules (7,446 LOC) | -7,446 | Build 23% mais rápido, binário menor |
| **P0-2** | Atualizar lib.rs (remover pub mod) | -35 | Limpeza |

### Fase 2: Testes nos módulos ativos

| # | Ação | Testes | Impacto |
|---|------|--------|---------|
| **P1-1** | Testes para cognitive_bridge (emotion_hint, prompt_slice) | +15 | Valida pipeline LLM |
| **P1-2** | Testes para memory_store (get/set/list/cap) | +10 | Valida persistência |
| **P1-3** | Testes para self_evolve (skill promotion, evolution) | +10 | Valida self-improvement |
| **P1-4** | Testes para wasmi_rt (sandbox, fuel, cap gate) | +10 | Valida WASM runtime |
| **P1-5** | Testes para executive (LoopPhase, EgoLayer, Supervisor) | +10 | Valida orquestração |

### Fase 3: Performance (locks)

| # | Ação | Impacto |
|---|------|---------|
| **P2-1** | Substituir locks em cognitive_bridge por atomics | -20 locks/frame |
| **P2-2** | Substituir locks em net por atomics | -15 locks/frame |
| **P2-3** | Adicionar memory nudge periódico | Feature parity com Hermes Agent |

### Fase 4: Funcionalidade (parity com Nous Research)

| # | Ação | Impacto |
|---|------|---------|
| **P3-1** | Session persistence (NeuralFS em vez de SQLite) | Sessões sobrevivem reboot |
| **P3-2** | Memory nudge periódico (auto-save) | Learning loop completo |
| **P3-3** | Skill patch incremental | Updates eficientes |
| **P3-4** | Frozen snapshot para prompt | Preserva prefix cache |

---

## 📊 Resumo

| Métrica | Valor | Meta |
|---------|-------|------|
| LOC total | 32,166 | — |
| Dead code | **7,446 (23%)** | 0 |
| Testes | 138 (64 em wasm_build) | ≥200 |
| Módulos sem callers | **35** | 0 |
| Locks no hotpath | **402** | <100 |
| Sessão persistente | ❌ | ✅ |
| Memory nudge | ❌ | ✅ |
| Skill patch | ❌ | ✅ |

**Prioridade imediata:** P0-1 (dead code removal) + P1-1 (testes cognitive_bridge)
