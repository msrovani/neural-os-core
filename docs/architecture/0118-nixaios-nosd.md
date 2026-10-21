# ADR-0118: NixAIOS — Daemon `nosd` em Linux/NixOS (produto paralelo)

**Status:** Accepted (scaffold)  
**Lifecycle:** `fazendo`  
**Data:** 2026-10-21  
**Relaciona:** ADR-0057, ADR-0059, ADR-0060, ADR-0078, ADR-0081, ADR-0088, ADR-0091, ADR-0113  
**Não substitui:** bare-metal n-os-c; ADRs históricas permanecem canônicas para Ring0.

## Contexto

O neural-os-core opera bare-metal `no_std`. A CAMADA 4 de host apps (visão ADR-0089 whitepaper) e a premissa AIOS-First (0088) pedem um path Linux com a mesma ontologia Agent/Skill, CapGate e memória nsgdb — sem cutover nem fork de repositório.

## Decisão

1. **Produto paralelo** no monorepo: diretório `nixaios/` (não é fork git).
2. **`nosd`**: daemon Rust `std` + Tokio, gerido por **systemd `Type=notify`** (não PID 1 no MVP).
3. **Bancada primária:** WSL2; aceite ISO/metal são provas separadas (0113).
4. **Memória canônica:** neural-sgdb (ADR-0091) via `nos-memory` — FileStorage sob `/var/lib/nosd/nsgdb`.
5. **Frugal WASM (política NixAIOS):** heap máx. **256 KiB** e fuel **100_000** por ciclo de agente (diverge dos defaults bare-metal atuais de propósito).
6. **Eixos PORT do tree:** App Factory A (0059), Mesh (0081), dispatch/distrib (0057 + mesh_distrib), BEI tick (0060), ModelHub (0078).
7. **GPU split:** iGPU `card0` → Cage/JARBAS; dGPU `renderD*` → inferência `nosd`.
8. **HotSwap multi-vendor:** trait + 4 fases; MVP = llama.cpp/GGUF; EXL2/vLLM/IPEX = `EngineAbsent` até libs no flake.

## Consequências

- Scaffolding em `nixaios/packages/nosd` e módulos NixOS.
- Matriz: `nixaios/docs/MIGRATION_MATRIX.md`.
- FHS/CUDA: `autoPatchelfHook` + RPATH + `NOSD_ENGINE_DIR`.
- `ai-managed.nix`: staging → `nixos-rebuild build-vm` → HITL `switch` → rollback.

## Não-objetivos (MVP)

PID1; cutover bare-metal; AppForge (#186); Factory B/C sem isolamento; Mem0/Supabase como motor primário; BitFuzzy no hub.
