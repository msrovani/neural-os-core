# Sprints Legadas — Conhecimento Consolidado (Sprints 60-91)

**Data:** 2026-07-08
**Propósito:** Conhecimento dos planos de sprint originais, consolidado quando os sprints foram concluídos. Itens não implementados foram movidos para Sprint 92.

---

## Sprint 60 — Plano de Correção Estrutural (16/17 ✅)
**Arquivo original:** `sprint-060-correction-plan.md`
**7 sub-sprints (~3.140 LOC):** UX double buffering, Security Pipeline + Path Confinement, e1000 + USB-MSC BOT, WHPX + AVX2 BitNet, model training pipeline (Python), GGUF loader + heap 5GB, WASM sandbox + TV-DSL, MHI LLM-optimized, UserProfile (6 perfis).
**Item pendente → Sprint 92:** 60.5a/b Train 100M/1.5B params (bloqueado GPU física).

## Sprint 61 — Desktop SmileyOS (~3.430 LOC planejados)
**Arquivo original:** `sprint-061-desktop.md`
**Implementado:** MouseAgent PS/2 (IRQ12, 3-byte), Compositor (multi-window + dock), DisplayAgent, Theme engine (5 temas), HermesApp/SettingsApp/PowerApp, 55+ comandos shell, TTF engine.
**Itens pendentes → Sprint 92:** LLM-Generated Icons (61.5), WASM App Sandbox (61.6).

## Sprint 62 — InferenceFS + MHI + Storage Agents (~2.400 LOC planejados)
**Arquivo original:** `sprint-062-fs.md`
**Implementado:** VFS layer (mount/resolve/lookup/list_dir), AtaAgent (/mnt/hdd/), DevFsAgent (/dev/), ProcFsAgent (/proc/), HermesFsAgent (/chat/), InferenceFsAgent (/inference/), RamFsAgent (/mnt/ram/).
**Itens pendentes → Sprint 92:** MHI+FS Bridge completo, integração com `main.rs`.

## Sprint 63 — Cortex Evolution (~580 LOC)
**Arquivo original:** `sprint-063-cortex-evolution.md`
**Implementado:** Model trait (swap), PTRM (gaussian + 3 trajetórias + Q-head), Kanerva (sparse distributed memory 256-bit), Hard blocklist (12 comandos), Curated memory budget (4KB).
**Itens pendentes → Sprint 92:** GGUF model swap (heap >5GB).

## Sprint 63 — WWW Agents (~2.600 LOC, 100% B-01)
**Arquivo original:** `sprint-063-www.md`
**Plano:** 7 agentes (ConnectionPool, Browser, Email, Search, RSS, Download, WebSocket). Tudo bloqueado por B-01.

## Sprint 66 — GPU Architecture (~550 LOC)
**Arquivo original:** `sprint-066-gpu.md`
**Implementado:** detect.rs (30+ GPUs), vram.rs (bump + DEADBEEF), intel.rs (Gen9+ ring), nvidia.rs (P8 mode), amd.rs (PM4 probe), backend.rs (GpuAccel enum), cube.rs (crossfade). 24 bugs resolvidos.
**Itens pendentes → Sprint 92:** AMD PM4 ring buffer completo, NVIDIA PFIFO + FALCON, Intel GEN shader matmul, BCS blitter, GTT setup.

## Sprint 67 — Auto-Skills + Agency (~1.400 LOC planejados)
**Arquivo original:** `sprint-067-auto-skills.md`
**Implementado (parcial, via Sprint 77):** Pre-Flight Principle, /learn command, Completion Contracts, Background Fan-out.
**Itens pendentes → Sprint 92:** Observation Protocol (skill_observer persistence), Trigger Integration (OptimizerAgent/SafetyAgent hooks), Comprehensive Review (CronAgent semanal), Agency Importer completo.

## Sprint 72 — Agentic Evolution (~1.300 LOC)
**Arquivo original:** `sprint-072-agentic-evolution.md`
**Completo via Sprints 77-78.** Crew+FlowTrigger, TaskSchema, IntentCache, WorkflowEngine, StateGraph, SkillIndex.

## Sprint 77 — Foundation Quick Wins (~376 LOC)
**Arquivo original:** `sprint-077-foundation-quick-wins.md`
**✅ Completo:** Prompt `>`, Pre-Flight, FanOut, TaskSchema, SkillIndex, /learn, CompletionContracts.

## Sprint 78 — Agentic Evolution (~400 LOC novos)
**Arquivo original:** `sprint-078-agentic-evolution.md`
**✅ Completo:** IntentCache, OutputCache, WorkflowEngine, SelfCritique, AgentTier, FsBridgeAgent, WasmExecutor.

## Sprint Plan 84-90 (legado, substituído por 84-95)
**Arquivo original:** `sprint-plan-84-90.md`
**Status:** Todos os itens foram implementados ou movidos para sprints posteriores.

## Sprint Plan 84-95 (parcialmente substituído por 92-99)
**Arquivo original:** `sprint-plan-84-95.md`
**Sprints 84-85 ✅ completos.** Sprints 86-91 foram implementados (verificar TECNOLOGIAS.md). Sprint 97 foi renomeado e reatribuído.
