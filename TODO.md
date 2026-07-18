# 📋 TODO MASTER — neural-os-core

**Versão release:** v1.9.0 TESTE / NÃO ESTÁVEL
**Data:** 2026-07-18
**Propósito:** Checklist mestre do roadmap v1.5.x → v2.0.
**Documento oficial:** AGENTS.md (seção roadmap)
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado
**Pista ativa:** **Pós-LAN B-01 unlock ✅** (SESSION_152; NetFs PASS). Próximo: `/model-fetch` e2e · WiFi AWAITING · TLS real · gate v2.0.0 review.
**PreFlight:** `python tools/preflight_wave.py --wave N` · `--idea 418` · `--anti-fake-ready` · cache `.preflight_cache/`
**Tags:** `depends_on: lan` (✅ L3.5–L5) / `depends_on: wifi` ▶️ · ▶️ **AWAITING_HW**
**Gate v2.0.0:** `por_fazer` zerado **ou** residual replanejado + OK maintainer. AWAITING_HW bloqueia salvo defer explícito.
**Residuals por onda:** 0–7 ✅ · Pós-LAN ✅ (NetFs PASS) · WiFi AWAITING · TLS BLOCKED · R soft-float defer.
**Fora do gate (não atracar):** SmileyOS 279a–b/e, Cube 283a, XDNA 💰, SKYNET 315.26–27, Mach-O/APK, wasmi-USB #8/#11.

---

## ✅ SPRINTS 1-105 — COMPLETOS

| Sprint | v | Foco | LOC | Status |
|--------|---|------|-----|--------|
| 1-100 | v1.0.0 | Gold Master — Code Freeze + Release | ~26.000 | ✅ |
| 101 | v2.0 | Cognição: TTS, STT, HDA capture, ATA fix, NVIDIA GPU | ~2.000 | ✅ |
| 102 | v1.1.x | GPU Compute, HW Expert v3, Firmware Pipeline, WiFi | ~1.500 | ✅ |
| 103-104 | v1.5.0 | K³CHJ Workspace Migration (5 crates) | ~500 | ✅ |
| 105 | v1.5.1 | Ponytail Audit: ~600 LOC removidos, 11 deps eliminadas | ~100 | ✅ |
| 105b | v1.5.2 | RingBufStore refactor + LEGACY snapshot | ~50 | ✅ |
| 105c | v1.5.3 | K³CHJ crate dead code cleanup + PICS fix | ~50 | ✅ |

---

## ✅ SPRINT 106 — v2.0 Cognição: Refatoração para Ecossistema de Anéis Lógicos

| Sprint | Item | LOC | Status | Detalhes |
|--------|------|-----|--------|----------|
| 106-1 | Estruturar Cargo workspace estrito | ~100 | ✅ | k_nano, k_ai, cortex, hermes, jarbas membros |
| 106-2 | Renomear crates k_ia→k_ai e jarvis→jarbas | ~200 | ✅ | Copiados backups preservados, nomes atualizados |
| 106-3 | Corrigir SOUL.md parser (dependência ring2→ring0) | ~300 | ✅ | jarbas usar neural_kernel::fs::read_vfs(), não k_nano::ATA_DRIVER |
| 106-4 | Corrigir Trinity MoE Router | ~100 | ✅ | Trinity classifica intents via ML/keyword — não roteia para hardware |
| 106-5 | RustPython viabilidade | ~200 | ✅ | Documentado: RustPython não é no_std nativo — rota WASM (106-6) é principal |
| 106-6 | MicroPython via WASM | ~300 | ✅ | Compilado para .wasm, sandbox isolado |
| 106-7 | Corrigir page faults (ordem de inicialização) | ~200 | ✅ | allocator → events → agents |
| 106-8 | AIOS API para Python (RAG + System Prompt) | ~300 | ✅ | aios_net, aios_fs injetadas via RAG |
| 106-9 | Escalonamento Evolutivo de Código (JIT Cognitivo) | ~500 | ✅ | Python efêmero → WASM cravado em pedra |
| 106-10 | SkillOpt - Tradução Python→Rust no_std | ~400 | ✅ | Geração Rust no_std via Cortex LLM |

---

## ✅ SPRINT 107 — Voice I/O (FECHADA — PASS parcial forte+)

**Veredito:** fechada para voz — entregues clima e2e, HWEXPERT, Piper neural-lite, EventBus skinny, WakeWord registrado.
**ADR:** [0045-sound-voice-stack.md](docs/architecture/0045-sound-voice-stack.md).
**Pendências de voz ≠ 107** — migradas para **Sprint Sound (reaberta)** abaixo.
**Evidência:** `SESSION_110.md`, `SESSION_107_CLOSE.md`, log `logs/boot_whpx_20260716_110041.txt`.

| Item | Status | Nota |
|------|--------|------|
| Clima e2e GEN + TTS + FB | ✅ | `'O tempo esta'` + Piper neural-lite + paint |
| HWEXPERT LOADED | ✅ | header u32 + sim host |
| WakeWordAgent registrado | ✅ | Loop 5 / AgentFleet |
| EventBus STT→INTENT (skinny) | ✅ | boot path; runtime Mic→Wake ainda Sound |
| ~~sherpa / Vosk / Kokoro / Wyoming~~ | ❌ | Supersedido — ADR-0045 |

---

## ✅ SPRINT SOUND — voz production-path (2026-07-16)

**Truth** = `neural-kernel/src/audio/*` (ADR-0045). Espelho jarbas sincronizado; **sem cutover**.
**Check:** `cargo check --release -p neural-kernel` = 0 erros (`target/check-sound`).
**SESSION:** `docs/memory/SESSION_122.md`.

| Item | Status | Detalhes / deps |
|------|--------|-----------------|
| STT real / retrain PCM→MFCC | ✅ | `train_stt.py` PCM→MFCC kernel-aligned; `STT.BIN` regenerado; CTC tiny ainda fraco (WER) |
| Mic→Wake→STT→LLM→TTS runtime | ✅ | Wake Continuous; gate pós-WAKEWORD; MIC/PLAYBACK rings; pipeline barge-in; rota única HERMES→TTS |
| Piper neural-lite polish | ✅ | prosódia/duração/PT normalize; VITS/HiFi-GAN = **blocker soft-float** (não fakeado) |
| Soft-float voice latency | ⏳ | known blocker; defer honesto (sem fake fix) — Onda 4 |
| UAC (#84) | ▶️ AWAITING_HW | parse+probe+USB-TRUST; `[UAC-HW] VERDICT=AWAITING_REAL_HW` (iso TRB) |
| USB Trust #6/#12–15 | ✅ | `usb_trust.rs` + `usb.tbl` + enforce/disable_port (SESSION_145) |
| jarbas/audio wire | ▶️ | espelho sync (VAD/settings/wake Continuous); cutover re-export = futuro |
| VAD refinements | ✅ | noise-floor EMA + ZCR + histerese |
| SER refinements | ✅ | confidence gate + thresholds calibrados |
| Wake ML polish | ✅ | Continuous + sensitivity + telemetria throttled |
| Unify truth↔espelho | ▶️ | topics+settings contract no bridge; cutover pleno adiado (ADR-0045) |

---

## ✅ ADR-0042 — adequação K³CHJ (v1.8.0)

| Fase | Status | Versão |
|------|--------|--------|
| N1 k-nano legível | ✅ | v1.7.0 |
| N2 k-ai SelfHeal / Trust | ✅ CLOSED + N2.5 wired | v1.7.4 / v1.7.8 |
| N3 cortex cérebro | ✅ CLOSED + N3.5 wired | v1.7.5 / v1.7.9 |
| N4 hermes orquestra | ✅ CLOSED + N4.6 wired | v1.7.6 / v1.7.10 |
| N5 jarbas ego/UI | ✅ CLOSED + N5.7 wired | v1.7.7 / v1.7.11 |

**Marco v1.8.0:** N1–N5 funcionais + wire crates completo. Gate `v2.0.0` = review formal (qualidade voz → Sprint Sound).

---

## 🧪 RELEASE v1.8.5 — integração e testes

- [x] Consolidar aprendizados das sessões 121–128
- [x] Registrar Self-Evolve, Sound, NeuralFS, AirLLM e família ADR-0047
- [x] Manter ADRs 0048–0050 como propostas `por_fazer`
- [x] Marcar versão como não estável / em teste
- [ ] Validar residuals em HW real (WiFi RF · TLS real · GPU/UAC/DMA AWAITING; #418 peer já PASS QEMU)
- [ ] Liberar v2.0.0 somente após review formal e OK explícito do maintainer

---

## ▶️ ADR-0042 — histórico (arquivado)

## ✅ SPRINT 108 — v2.0 Self-Evolving Agents

| Item | LOC | Status | Detalhes |
|------|-----|--------|----------|
| Auto-skill generation via LLM | ~500 | ✅ | `self_evolve` + AddSkill/LLM + pattern≥3 |
| Runtime skill verification | ~300 | ✅ | `verify_skill_md` no loader + register path |
| Agent self-improvement loop | ~400 | ✅ | SIL Research→Create→Improve→Verify wired |
| Meta-cognition and reflection | ~400 | ✅ | SleepCycle REFLECT + SelfEvolveAgent |

**Engine:** `crates/hermes/src/self_evolve.rs` · **Agente:** `SelfEvolveAgent` (PollEvery 100) · Serial `[S108]` / `[S108-SIL]` / `[S108-REFLECT]`


---

## 🔍 Auditoria de erros Cursor — k_nano (Jul 2026)

> **Origem:** auditoria Cursor k_nano (Sprint/Jul 2026). Bugs de código já corrigidos (AHCI MMIO/TFES, VA→PA, FAT32 read_sectors, ATA PIO write, IrqSafeLock CAS, journal recover, BlockDevice len, xHCI init, PCI multi-function BARs, warnings/stubs). Resta dívida arquitetural + validação em runtime.

- [ ] Migrar `neural-kernel` para depender de `k_nano` e eliminar ~66 módulos duplicados (maior drift)
- [ ] Validar AHCI em QEMU (`-device ahci`) e/ou HW real — read/write + detecção TFES
- [ ] Validar `dma_va_to_pa` com buffers fora do heap (stack/.bss) vs identity map
- [ ] Validar FAT32 com falha de I/O real (erro visível no chamador)
- [ ] Validar IrqSafeLock::try_lock sob contenção SMP
- [ ] Validar xHCI com heap pressionado (boot continua sem panic)
- [ ] Validar PCI multi-function BARs em device real multi-função
- [ ] Wire-up / ownership único VirtIO-GPU (stub k_nano vs impl jarbas/neural-kernel)
- [ ] Implementar `disk_power` real ou mover para agente (hoje stub)
- [ ] Driver RTC (mencionado em AGENTS.md, sem módulo) ou atualizar docs
- [ ] Exportar ou deprecar claramente `debug_rl!` no ecossistema crate
- [ ] Adicionar `k-ia` ao workspace ou deprecar em favor de `k_ai`
- [ ] Reorganizar `hnsw` / `multi_user` fora do Ring 0 (candidatos cortex/k_ai)

---

## 🔍 Auditoria de erros Cursor — k_ai (Jul 2026)

> **Origem:** auditoria Cursor Ring 1 / k_ai (Jul 2026). Já corrigido: stubs hermes→k_ai real (agency/hw/audit/boot_log/inventory), Trust sem auto-grant + BootTrust `add_exempt_token(1)`, Audit Merkle ativo, MHI scheduler ligado, `mask_secrets` UTF-8, I3/I4 via `k_nano::EVENT_BUS` (sem jarvis), DataCollector sem dummy, docs AGENTS.md alinhados. Canvas: `k-ai-audit.canvas.tsx`.

### Dívida restante (bloqueio / arquitetura)

- [ ] **P01** Unificar globals (`EVENT_BUS` / `GLOBAL_ALLOCATOR` / `SKILL_REGISTRY`) em `k_nano` como singleton único
- [ ] **P01** Após singleton: `neural-kernel` depender de `k_ai` e eliminar mods locais (`self_heal`, `trust`, `agency`, `cognitive`, `audit`, …)
- [ ] **P08** Um só `SELF_HEAL` / `TRUST_CACHE` no path boot (hoje: monólito × hermes/k_ai)
- [ ] Mover **safety / security / optimizer / SleepCycle / AutoLearn** para `k_ai` **ou** manter em hermes e congelar docs (decidir ownership Ring 1)
- [x] Arquivar `crates/k_ia` em `LEGACY/k_ia` (legado pós-rename; 2026-07-16)
- [ ] Arquivar `hermes/src/monolith_stubs.rs` residual

### Checkpoint / SelfHeal (P09)

- [ ] Expandir `restore_checkpoint` além do bitmap (page tables / heap talc / estado drivers) ou documentar como “best-effort” e nunca chamar em produção sem validação
- [ ] Validar em QEMU: BootSelfHeal lê boot log real (FAT32/`/logs`) após unexpected shutdown

### Trust / Security (validação runtime)

- [ ] Validar Contain/Enforce: skill sem `trust_allow` é negada pós-boot
- [ ] Validar skills de sistema com `Legacy(1)` após exempt explícito (EventBus interno não quebra)
- [ ] Wire `check_or_cache` em todos os execute_skill paths (hermes + neural-kernel)

### Cognitive / treino (hollow → real)

- [ ] Substituir toys restantes (`CandleSidecar`, `TaskSpawner`, `ReActLoop` scripted, `McpServer` echo) por no-op documentado ou impl mínima
- [ ] Conectar AutoLearn/SleepCycle do **hermes** ao `update_with_replay` + cache R3 (hoje R3 está no neural-kernel)
- [ ] BGE `memory_systems`: alinhar `f32` load (alignment) + evitar `static mut` unsync em SMP

### Validação / polish

- [x] `wasm_rt::SkillMarket::top` — `total_cmp` NaN-safe (SESSION_130; alinhado a `skill_market`)
- [x] `cargo check --release` limpo — 0 erros, 0 warnings (SESSION_130; `target/check-zero-warn`)
- [x] Framebuffer bpp dinâmico — `GpuDevice::from_probe` + consumidores via `from_gpu`/helpers (SESSION_130; PR #3 + reforço)
- [x] HW PnP `HwCapabilityCard` — identify→use contract + EventBus; Expert v4 seed (SESSION_131)
- [x] Hermes agentico PnP — card→decide→efêmera→WASM (`hw_pnp` + SkillOpt + evolve)
- [x] ADR-0051 Package Hub — ecosystem folders + CRUD HITL + Cortex catalog (SESSION_133)
- [x] Agency/nativos → AGENT.md + seed embutido + VFS bridge (SESSION_134)
- [ ] Boot path hermes (jarbas): Agency registra >0 agentes (não stub vazio)
- [ ] Boot path hermes: `HwRegistry::detect_all` lista PCI reais no serial
- [ ] Safety I4 escreve trilha Merkle verificável (`AuditTrail::verify()`)
- [ ] Atualizar TECNOLOGIAS.md se ownership Ring 1 mudar pós-migração

---

## 🔴 BLOQUEADORES — Apenas leitura (pós SESSION_152)

| Item | Esforço | Descrição |
|------|---------|-----------|
| **TLS real #123** | ~1.500 | `[TLS] BLOCKED softfloat_or_crate` — sem fake HTTPS |
| **WiFi RF** | ~2.000 | `[WIFI-HW]` AWAITING; SoftMAC `#407`/`#408` → `depends_on: wifi` |
| Cross-OS compat | ~2.000 | PE/ELF/Mach-O/APK (fora gate) |
| Federated Cluster | ~300 | Mesh multi-máquina (fora gate) |
| Multi-device sync | ~300 | CRDT `#315.26` (fora gate) |
| AppForge | ~3.000 | Apps multi-usuário (fora gate) |

### ✅ Desbloqueados (histórico)

| Item | Status |
|------|--------|
| **B-01** DHCP/RX + internet QEMU | ✅ L3.5–L5 (SESSION_149/150) + Pós-LAN (SESSION_152) |
| WWW Agents (HTTP) | ✅ Search/RSS/Browser via net_bridge; Email SMTP residual |
| Self-Update HTTP #308a | ✅ `fetch_update` + FNV + slot A/B; reboot A/B residual |
| NetFs #418 peer | ✅ `[NETFS] VERDICT=PASS` (S3/WebDAV backends residual) |

---

## ⏳ Pós-MVP

| Item | Esforço |
|------|---------|
| GGUF v3 loader (modelos 9B+) | ~500 LOC |
| NPU AMD XDNA driver (💰 sponsor) | ~2.000 LOC |
| ARM/RISC-V port (💰 sponsor) | ~5.000 LOC |

---

## 📊 RESUMO v2.0 "Cognição"

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| 100 | Code Freeze v1.0.0 | ~500 | ✅ |
| 101 | TTS+STT+ATA fix+NVIDIA GPU | ~2.000 | ✅ |
| 102 | GPU Compute + HW Expert v3 + Firmware | ~1.500 | ✅ |
| 103-104 | K³CHJ Workspace Migration | ~500 | ✅ |
| 105 | Ponytail Audit + v1.5.1..v1.5.3 | ~200 | ✅ |
| 106 | v2.0 Ecossistema de Anéis Lógicos | ~3.000 | ✅ 10/10 concluídas |
| 107 | Voice I/O (clima e2e + skinny EventBus) | ~1.500 | ✅ fechada (PASS parcial forte+) |
| Sound | Voz pipeline + STT PCM + UAC parse + neural-lite | — | ✅ (soft-float/VITS + cutover abertos) |
| ADR-42 | Adequação N1–N5 + wire | — | ✅ v1.8.0 |
| 108 | Self-Evolving Agents | ~1.600 | ✅ |
| **Total v2.0** | | **~9.000 LOC** | |

---

## 📝 NOTAS TÉCNICAS

### Sprint 106-1: Workspace Estrito
- **Cargo.toml raiz:** `members = ["crates/k_nano", "crates/k_ai", "crates/cortex", "crates/hermes", "crates/jarbas"]`
- **Resolver:** `resolver = "2"` para dependências escalonadas
- **Isolamento:** Dependências não vazam entre camadas

### Sprint 106-2: Rename Crates
- **k_ia → k_ai:** Ring 1 Lógico (Sondagem, SelfHeal, Trust)
- **jarvis → jarbas:** Ring 2 HCI (Display, Audio, CLI)
- **Backups:** Pastas antigas preservadas (LEGACY/k_ia, LEGACY/jarvis)

### Sprint 106-5/106-6: Python no_std
- **Rota Nativa:** RustPython embed com `#![no_std]`
- **Rota Sandbox:** MicroPython compilado para .wasm
- **Bridge:** `abi_x86_interrupt` para rust→python

### Sprint 106-7: Page Faults
- **Ordem correta:** allocator → events → agents
- **lazy_init!():** Macro para agentes dependentes de heap

### Sprint 106-8: AIOS API
- **Bibliotecas:** aios_net, aios_fs
- **Injeção:** RAG/System Prompt no RustPython

### Sprint 106-9/106-10: Escalonamento Evolutivo
- **SkillOpt:** Optimizador de skills via LLM
- **Knowledge Graph:** Rastreamento de evolução
- **Python → WASM:** Código efêmero → persistente

---

**Detalhes completos:** `TODO.md`
**Catálogo de tecnologias:** `TECNOLOGIAS.md`
**Roadmap completo:** `ROADMAP.md`
