# SESSION_215 — Emagrecer neural-kernel: análise profunda + plano cirúrgico ADR-0075

**Data:** 2026-07-23
**Release:** v1.9.10-emagrecer-plan (checkpoint 4d8f0d5)
**Base:** v1.9.9 TEST (a216ca8)

---

## 1. Contexto

O neural-kernel bin carrega ~29.431 LOC em ~150 arquivos. Destes, **~12.000 LOC (41%) são bin_ahead** — módulos com versão canônica em crates K³CHJ que não estão wired. O produto cognitivo já vive nos crates (~68k LOC K³CHJ). O bin duplica caminhos críticos (LLM, fleet, net, audio, FS).

## 2. Distribuição real (dados do recon, não do dashboard)

| Classe | LOC | % | Módulos | Destino |
|--------|----:|---|---------|---------|
| **bin_ahead** | ~12.000 | 41% | cortex, agents, bpe, gguf, gguf_streaming, neural_fs, boot_logger, virtio_net, model_hub, memory_agent, memory_systems, hnsw, vfs, fs, work_stealing, mouse_agent | Promover bin→crate, depois stub |
| **role_diff** | ~6.500 | 22% | net, netstack, network_agent, tls_trust, tls_client, shutdown, jarbas_fb, address_space, user_mode, cortex_mmap, k_ia_dma, syscall, virtio_vring, gguf_mmap, usb_trust, usb_msc, exec_arena, demand_page, capability_gate | Ficam no bin |
| **glue** | ~5.000 | 17% | main.rs (3.102), interrupts, allocator, vga_buffer, shell, limine_boot, smp/mod, serial, boot_log_agent, labor_smokes, jarbas_bridge, block_dev | Permanente no bin |
| **audio** | ~2.900 | 10% | audio/* (21 arquivos) | ADR-0045 deferido; pode ir p/ crate K³CHJ |
| **stubs** | ~100 | 0,3% | ~40 arquivos pub use | Já cutover (ondas 0-6) |

### 2.1 Correções vs dashboard

| Dashboard diz | Código diz | Impacto |
|---------------|------------|---------|
| main.rs 2.745 LOC | 3.102 LOC | **+357 LOC** — alvo "3-5k magro" inviável |
| net.rs + netstack 681+ LOC | 701 + 1.073 = 1.774 LOC | **+1.093 LOC** — role_diff não stubável |
| audio/* 3.311 LOC | ~2.900 LOC | ~400 LOC de comentários |
| "38 stubs" | 40 stubs | próximo |
| Alvo magro 3-5k LOC | Mínimo real: ~11.000 LOC (glue 5k + role_diff 6.5k - audio 2.9k após E4) | Alvo realista: **~11.000 LOC** |

## 3. Sequência E0–E4

### E0 — Freeze (imediato)
- PR policy: nenhum módulo >200 LOC novo no bin sem ADR
- diff_bin_crate.py --strict em CI
- Risco: 🟢

### E1a — Wave 7a: promover cortex/bpe/gguf/gguf_streaming/model_hub → cortex crate (P0)
- Mover bpe.rs (990 LOC) → cortex crate (sobrescrever stub 313 LOC)
- Mover gguf.rs (895 LOC) → cortex crate (crate não tem)
- Mover gguf_streaming.rs (757 LOC) → cortex crate (crate não tem)
- Stub cortex.rs (2.300 LOC) → pub use cortex_crate::cortex
- Stub model_hub.rs (264 LOC) → pub use cortex_crate::model_hub
- **LOC removido:** ~5.200
- **Risco:** 🔴 (LLM path crítico)

### E1b — Wave 7b: promover agents/neural_fs/vfs/fs → hermes crate (P0)
- Verificar lista de agentes (bin 25 vs crate 25) — se idêntica, stub
- Promover neural_fs/agent (657 LOC) → hermes crate
- Stub vfs/* (256 LOC) + fs/* (149 LOC)
- **LOC removido:** ~3.100
- **Risco:** 🔴 (AgentFleet crítico)

### E1c — Wave 7c: promover boot_logger/virtio_net/usb_msc → k_nano crate (P1)
- Promover boot_logger.rs (507 LOC) → k_nano crate
- Stub virtio_net.rs (408 LOC) → pub use k_nano::virtio_net
- Stub usb_msc.rs (205 LOC) → pub use k_nano::usb_msc
- **LOC removido:** ~1.120
- **Risco:** 🟡 médio

### E2 — ADR-0062 P4 Limine (P1)
- Criar handoff trait (BootHandoff) + Bootloader011Handoff + LimineHandoff
- Refatorar kernel_boot() para usar dyn BootHandoff
- **Pré-requisito:** E1 completo (bin sem dual-source crítico)
- **Risco:** 🟡 médio

### E3 — ADR-0062 infra P2/P3/P5/P6 nos crates
- P2 VFS, P3 AHCI/NVMe, P5 GPU, P6 WiFi — todos já nos crates
- Só wire pub use no bin (~10 LOC)
- **Risco:** 🟢 baixo

### E4 — ADR-0045 audio cutover (P2)
- Mover audio/* (21 arquivos, ~2.900 LOC) → jarbas crate
- Atualizar imports, verificar contract sync
- **Pré-requisito:** ADR-0045 revisado
- **Risco:** 🟡 médio

## 4. Alvo final pós-E4

**~11.000 LOC** (redução de 62%). Não 3-5k como o dashboard sugere — main.rs (3.102) + glue (2.000) + role_diff (6.500) = 11.600 LOC mínimo.

## 5. Lições críticas desta sessão

1. **Cursor auto-checkpoint (SESSION_176):** Checkpoint de segurança antes de começar E0 preveniu que 139 arquivos de sprints 178-214 fossem engolidos. Ritual mantido: `git status` limpo → commit nomeado → tag.
2. **diff_bin_crate.py** está em `docs/archive/migration/`, não em `tools/`. Mover para `tools/` seria mais canônico.
3. **Audio pode ir para jarbas crate** (ADR-0045 deferido) — incluído como E4.
4. **LEGACY já existe** em `LEGACY/v1.5-neural-kernel-src/` com snapshot baseline. Código migrado vai para lá.

## 6. Próximo passo

E0: rodar diff_bin_crate.py --strict, documentar policy, iniciar E1a.

## 7. Referências

- ADR-0075 (emagrecer neural-kernel)
- IDEA #467 / #511
- SESSION_163 (emagrecer ondas 0-6)
- BIN_CRATE_DIFF.md
- AGENTS.md (plano diretor)
- .cursor/rules/neural-emagrecer-bin.mdc
