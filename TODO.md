# 📋 TODO MASTER — neural-os-core

**Versão release:** v1.9.5 TEST / BEI planejado
**Data:** 2026-07-21
**Propósito:** Checklist mestre do roadmap v1.5.x → v2.0.
**Documento oficial:** AGENTS.md (seção roadmap)
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado
**Pista ativa:** **ADR-0086 ✅ concluída (SESSION_252, 10 gaps)** — próximo: fila ADR item 5 (Market fetch v3) ou U3 (assinatura/TPM) quando update for público.
**PreFlight:** `python tools/preflight_wave.py --wave N` · `--idea 418` · `--anti-fake-ready` · cache `.preflight_cache/`
**Tags:** `depends_on: lan` (✅ L3.5–L5) / `depends_on: wifi` ▶️ · ▶️ **AWAITING_HW** · **BEI** (BitNet Ecosystem Intelligence)
**Gate v2.0.0:** `por_fazer` zerado **ou** residual replanejado + OK maintainer. AWAITING_HW bloqueia salvo defer explícito.
**Residuals por onda:** 0–7 ✅ · Pós-LAN ✅ (NetFs PASS) · **ADR-0086 ✅ (Instalação + Update OTA completos, SESSION_252)** · WiFi AWAITING · TLS BLOCKED · R soft-float defer.
**Fora do gate (não atracar):** SmileyOS 279a–b/e, Cube 283a, XDNA 💰, SKYNET 315.26–27, Mach-O/APK, wasmi-USB #8/#11.

---

## ▶️ FILA ADR — pendências ordenadas por complexidade (mais simples → mais complexo)

Fonte: `docs/architecture/INDEX.md` (lifecycle). Sequência de execução recomendada: 1→2→3→4 (aquecimento), depois 5→6→8→9 (padrão já dominado). 🔴 = blocker · ▶️ = AWAITING_HW.

| # | Item | ADR | Tier | Esforço | Status |
|---|------|-----|------|---------|--------|
| 0 | **Instalação + Update OTA (processo unificado)** — 10 gaps fechados (U1/U2/U4/U6 + I3–I12): slot→kernel.elf, rollback, GPT, boot_mode, SELF.STATE, ModelProvisioner, NeuralFS boot, telemetria, imagem mini | 0086 | 0 | ~1.200 | ✅ SESSION_252 (U3 Ed25519/TPM = defer p/ update público) |
| 1 | Aceite QEMU slog (`NotifySent` + Cap/AS non-fatal como evidência) | 0041 | 0 | ~0 (evidência) | ✅ `docs/evidence/boot-whpx-20260805.txt` (WHPX; fix raiz: GDT usa `&*TSS` p/ ISTs) |
| 2 | Log honesto no fallback LCG + assets FAT (`ROUTER.BITNET`) | 0083 | 1 | ~50 | ✅ SESSION_251 (warn honesto + ROUTER.BITNET no FAT) |
| 3 | Cutover jarbas pleno (espelho áudio → crate; soft-float/VITS defer honesto) | 0045 | 1 | ~150 | ✅ SESSION_251 (cutover já feito e51a48b; docs reconciliados) |
| 4 | HardwareInfo expansão em ondas (MVP já em `platform_probe.rs`; snapshot WASM futuro) | 0082 | 1 | ~200 | ✅ Onda CPU (SESSION_251) — expansão restante 🟡 |
| 5 | Market fetch v3 | 0056 | 2 | ~200 | ✅ SESSION_252: `search_remote` usa base do UPDATE.CFG (não hardcoded 10.0.2.2) + `/api/search` no serve_update.py (testado: ROUTER.BITNET listado) |
| 6 | SGDB DoD 10M chaves / 100k docs (benchmark + tuning ART) | 0063 | 2 | ~300 | ✅ SESSION_252: `bench_dod` (ART 10M chaves hex sem alloc + BQ 100k×1024-dim), host-only, teste PASS (75s); P50 via TSC sem claim P99 (honestidade ADR-0063); D-series 100k mantido |
| 7 | F4 W2A8 kernel (gated WHPX/HW real) | 0084 | 2 | ~400 | ⏳ ▶️ |
| 8 | AirLLM residuals (prefetch DMA / stream-to-disk / K-quants / e2e GGUF grande) | 0046 | 2 | ~500 | 🟡 SESSION_252: hot-swap ATA+Net reais (stubs → `cortex::gguf` header-only + stream-to-disk Range 4MB chunks + append FAT); DMA prefetch / K-quants avançados = AWAITING |
| 9 | FS residuals (NTFS/EXT **read** ✅ · NTFS/EXT **write** defer · **SysInstaller UI** · Storage Manager UI · Cloud mounts S3/WebDAV; MHI DMA + GPU DS ▶️ AWAITING_HW) | 0040 | 2 | ~800 (vários) | 🟡 **SysInstaller núcleo ✅ ADR-0086 (I3/I6/I8/I12)** — resta seleção disco UI (A5). **NTFS/EXT READ ✅ conectado**: `storage_bus::detect_ext/detect_ntfs` montam `/mnt/ext`+`/mnt/ntfs` via BlockDevice genérico (cobre **USB-MSC/pendrives** e ATA/AHCI/NVMe; lado a lado com NeuralFS p/ dual boot). **NTFS/EXT WRITE = defer honesto** (SESSION_252): OS escreve FAT32/exFAT/NeuralFS nativos; risco de corromper disco alheio; reabrir no dual boot quando for escrever partições alheias. Cloud S3/WebDAV = defer (NetFs HTTP cobre — §3.6B). SMART = AWAITING_HW. Storage Manager CLI ✅; card = cauda 0058 |
| 10 | Cards S5 (widgets ricos/tema/TTF) + A/V real (HDA/UVC) | 0058 | 3 | ~800 | ⏳ |
| 11 | Backprop real + router treinado (`ROUTER.BITNET`) | 0083 | 3 | ~1.000 | ✅ SESSION_252: `train_router.py` 93.5% (gate 0.80) exporta ROUTER.BITNET v6 (25.818B); **loader Rust corrigido v3→v6** (antes nunca carregava → fallback LCG); teste host load_router_v6_roundtrip PASS |
| 12 | Cross-OS Ecosystem F1–F5 (Skill Manifest, Membrane, …) | 0076 | 3 | ~1.500 | ⏳ |
| 13 | SemanticRouter / merge CRDT / merkle piece (BitTorrent ❌) | 0081 | 3 | ~1.500 | ⏳ |
| 14 | Layer S/HW: APs workers vivos (IDT/IPI reschedule) + GPU W2A8 + driver NPU (pós-v2.0) | 0057 | 3 | ~2.000 + HW | ⏳ |
| 15 | Ring3 isolation — 🔴 triple-fault blocker (`TRY_ENTER_RING3=false`; sessão debug dedicada) | 0077 | 4 | ~1.500 | 🔴 |
| 16 | Multi-slot multimodal (GGUF→ternário, 6 slots, visão SigLIP, learning contínuo) | 0078 | 4 | ~2.500 | ⏳ |
| 17 | Golden silício GPU multi-geração (NVIDIA ACR/GSP · AMD PSP/KIQ/MES · Intel GuC/walkers) | 0048–50 | 4 | ~3.000 + HW | ⏳ ▶️ |

**Regra:** itens ▶️ AWAITING_HW não bloqueiam o gate v2.0.0 se tiverem defer explícito; 🔴 0077 resolve ANTES de re-habilitar Ring3 (ver `interrupts_ext.rs` review HIGH, SESSION_245).

---

## ▶️ ADR-0086 — Instalação + Update OTA (visão completa, SESSION_252)

Processo canônico em `docs/architecture/0086-instalacao-e-update-ota.md`. ✅ 10 gaps fechados;
**restam os itens de evolução** (U3 = hardening deferido; refinamentos documentados).

| # | Item | Estado | Detalhe |
|---|------|--------|---------|
| A1 | **U3 — Assinatura Ed25519 + TPM PCR[8] no update** | ⏳ defer (reabrir p/ update público/mesh) | FNV-1a cobre integridade; Ed25519 = anti-tamper — custo real é o server assinar (quebraria fluxo dev). Server assina KERNEL.BIN → `.SIG`; kernel verifica contra pk embutida (`identity::verify_signature` já existe) |
| A2 | **Smoke QEMU do ciclo completo** (Ato 1–3: instalar → boot target → provision) | ⏳ | `serve_update.py` → guest 10.0.2.2:8080 → `install` → `provision` → `update` → `telemetry`; evidência de aceite |
| A3 | **Auto-disparo do ModelProvisioner no 1º boot Residente** | ⏳ | hoje via shell `provision`; hook NET_READY no NetAgent (1º boot, first_boot=true do SELF.STATE) |
| A4 | **Menu live/install no boot do pendrive** (I9 dá o modo, falta a UI) | ⏳ | `[L]ive` default timeout ~5s / `[I]nstall`; `CONFIG.TXT BOOT_MODE=install/live/auto`; `set_boot_mode()` já existe |
| A5 | **Comando `install` com seleção de disco** | ⏳ | hoje target = 1º AHCI/NVMe/USB; menu `scan_disks()` → lista → escolha (validação target ≠ source) |
| A6 | **Update a quente de fw/skills/modelos** (sem reboot) | ⏳ | `register_bytes()` + hot-swap existem; falta o roteamento fw/skills pelo update_check |
| A7 | **Loop de telemetria com auto-push periódico** | ⏳ | hoje via shell `telemetry`; LogAgent + cron diário (alinhado ao update_check) |
| A8 | **Rollback: tries > 1 (hoje 1 tentativa)** | ⏳ | BOOTCFG `tries` já estrutura; ampliar p/ 3 com last_good (padrão ChromeOS/Android, ADR-0031 §1.4) |
| A9 | **Imagem mini como default do fluxo instalável** | ⏳ | `--mini` existe; elevar p/ default do `--hw --unified` (MODELS_SOURCE=network) |

**Relação:** A1 = U3 · A2–A9 = refinamentos da §3.4/§3.5/§3.6 da ADR-0086. A2 desbloqueia a evidência de aceite do ciclo completo.

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

**Truth** = `jarbas/src/audio/*` (ADR-0045; cutover **✅ e51a48b** — bin re-exporta `jarbas_crate::audio`, antigos truth de `neural-kernel/src/audio/*` deletados).
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
| jarbas/audio wire | ✅ | cutover **e51a48b**: bin `pub use jarbas_crate::audio::*`; espelhos bin deletados |
| VAD refinements | ✅ | noise-floor EMA + ZCR + histerese |
| SER refinements | ✅ | confidence gate + thresholds calibrados |
| Wake ML polish | ✅ | Continuous + sensitivity + telemetria throttled |
| Unify truth↔espelho | ✅ | cutover pleno **e51a48b**; bridge topics+settings agora tautológico (mantido como contrato documental) |

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

- [x] **P001** Unificar globals (`EVENT_BUS` / `GLOBAL_ALLOCATOR` / `SKILL_REGISTRY`) em `k_nano` como singleton único — SKILL_REGISTRY shadow removido; `register_builtin_skills()` em k_nano
- [ ] Migrar `neural-kernel` para depender de `k_nano` e eliminar ~66 módulos duplicados (maior drift)
  - [x] `env.rs` drift fix — `is_online()` movido para k_nano; bin é `pub use k_nano::env::*`
  - [x] `block_dev.rs` — bin mantém `impl BlockDevice for UsbMassStorage` local (tipo difere de k_nano)
  - [ ] Demais drifts (net, interrupts, boot_logger, virtio_net, vfs, smp, serial, vga_buffer, usb_msc, hnsw, ipc) — futuras ondas
- [ ] **P08** Um só `SELF_HEAL` / `TRUST_CACHE` no path boot (hoje: monólito × hermes/k_ai)
- [x] **Checkpoint SelfHeal** — `restore_checkpoint` expandido: heap_start/size, PML4/CR3 addr, driver_state_hash FNV-1a, checkpoint_version=2
- [x] **Boot path — Agency fallback** — `register_agency_agents` cria 2 AgentSpecs (SystemDiagnostics, HwMonitor) quando PACKAGE_HUB vazio
- [x] **Safety I4 Merkle verify** — `verify_counter` + `AUDIT_TRAIL.lock().verify()` a cada 100 ticks
- [x] **AuditTrail::entry_count()** — adicionado em k_ai/audit.rs (pré-requisito do I4 verify)
- [ ] Validar AHCI em QEMU (`-device ahci`) e/ou HW real — read/write + detecção TFES
- [ ] Validar `dma_va_to_pa` com buffers fora do heap (stack/.bss) vs identity map
- [ ] Validar FAT32 com falha de I/O real (erro visível no chamador)
- [ ] Validar IrqSafeLock::try_lock sob contenção SMP
- [ ] Validar xHCI com heap pressionado (boot continua sem panic)
- [ ] Validar PCI multi-function BARs em device real multi-função
- [ ] Wire-up / ownership único VirtIO-GPU (stub k_nano vs impl jarbas/neural-kernel)
- [ ] Implementar `disk_power` real ou mover para agente (hoje stub)
- [x] Driver RTC (mencionado em AGENTS.md, sem módulo) ou atualizar docs
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
- [x] Mover **safety / security / optimizer / SleepCycle / AutoLearn** para `k_ai` **ou** manter em hermes e congelar docs (decidir ownership Ring 1) — **decidido: manter em hermes (R3)** por dependerem de EVENT_BUS, agent tick model, net_bridge, self_evolve e globals do hermes. Documentado via header comments em cada módulo. ADR-0060 A.4.
- [x] Arquivar `crates/k_ia` em `LEGACY/k_ia` (legado pós-rename; 2026-07-16)
- [ ] Arquivar `hermes/src/monolith_stubs.rs` residual

### Checkpoint / SelfHeal (P09)

- [ ] Expandir `restore_checkpoint` além do bitmap (page tables / heap talc / estado drivers) ou documentar como “best-effort” e nunca chamar em produção sem validação
- [ ] Validar em QEMU: BootSelfHeal lê boot log real (FAT32/`/logs`) após unexpected shutdown

### Trust / Security (validação runtime)

- [ ] Validar Contain/Enforce: skill sem `trust_allow` é negada pós-boot
- [ ] Validar skills de sistema com `Legacy(1)` após exempt explícito (EventBus interno não quebra)
- [x] Wire `check_or_cache` em todos os execute_skill paths (hermes + neural-kernel)

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
- [x] Boot path hermes (jarbas): Agency registra >0 agentes (não stub vazio)
- [x] Boot path hermes: `HwRegistry::detect_all` lista PCI reais no serial
- [x] Safety I4 escreve trilha Merkle verificável (`AuditTrail::verify()`)
- [x] Atualizar TECNOLOGIAS.md se ownership Ring 1 mudar p�s-migra��o Ring 1 mudar pós-migração

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

**Relação com ADR-0086:** o update OTA **não depende** destes bloqueadores no cenário cabo/ICS
(HTTP puro sobre ethernet — §3.4B). Mas: **TLS** desbloqueia o A1 (U3 assinatura) em server público;
**WiFi** estende o cenário note1→note2 ao sem-fio (hoje o update trafega no cabo). Ver seção ADR-0086 acima.

### ✅ Desbloqueados (histórico)

| Item | Status |
|------|--------|
| **B-01** DHCP/RX + internet QEMU | ✅ L3.5–L5 (SESSION_149/150) + Pós-LAN (SESSION_152) |
| WWW Agents (HTTP) | ✅ Search/RSS/Browser via net_bridge; Email SMTP residual |
| **Self-Update HTTP #308a** | ✅ **ADR-0086 completo (SESSION_252)**: A/B + reboot + rollback + GPT — ver seção ADR-0086 acima; U3 (Ed25519/TPM) = A1 defer |
| NetFs #418 peer | ✅ `[NETFS] VERDICT=PASS` (S3/WebDAV backends residual — **canal HTTP dev↔neural cobre o caso hoje**; S3/WebDAV = evolução de transporte, mesma lógica do mesh §3.6B) |

---

## ⏳ Pós-MVP

| Item | Esforço |
|------|---------|
| GGUF v3 loader (modelos 9B+) | ~500 LOC |
| ADR-0057 WS-B speedup matmul multicore (HW real; AVX2 off no TCG) | validação HW |
| ADR-0057 WS-D GPU BitLinearW2A8 kernel + KernelPack assinado (Layer S/HW) | ~800 LOC + HW |
| ADR-0057 WS-E NPU AMD XDNA driver (💰 sponsor, firmware/overlay Vitis) | ~2.000 LOC |
| ADR-0057 WS-E NPU Intel (Meteor/Lunar Lake, NCE) (💰 sponsor, firmware) | ~1.500 LOC |
| ADR-0057 WS-F scheduler heterogêneo (IPI reschedule, run-queues, per-CPU slab) | ~1.000 LOC |
| ADR-0058 S1 `DrawTarget` adapter (embedded-graphics sobre DoubleBuffer) | ~150 LOC |
| ADR-0058 S2 `UiDeclaration`+`UiRenderer` (cards) sobre toolkit no_std | ~600 LOC |
| ADR-0058 S3 árvore de janelas retida + `UI_SPEC` spawn/close/focus (remove `AppId`) | ~400 LOC |
| ADR-0058 S4 card-answer Hermes (#412 grammar) + skill WASM `weather` + Cron | ~400 LOC |
| ADR-0059 F1–F2 runtime wasmi + CapGate | ✅ (rodada anterior) |
| ADR-0059 F3 bridges → wasmi_rt + DynamicSkill | ✅ SESSION_165 — wasm.rs reescrito; WasmExecutor removido |
| ADR-0059 F4 decode harness (PONYTAIL — sem full assembler) | ✅ SESSION_165 — decode_harness.rs (Add/Echo/Default); upgrade qdo `wat` no_std |
| ADR-0059 F5 promote (DynamicSkill::with_wasm + SkillOpt) | ✅ SESSION_165 — chain completo |
| ADR-0059 F6 MicroPython.wasm → wasmi_rt + fallback dev | ✅ SESSION_165 — micropython_wasm.rs reescrita |
| ADR-0059 F7 ring gate (isolation_ring_available=false) | ✅ (já existia) |
| ADR-0059 aposentar `Op` VM (`wasm_exec.rs`) + limpeza | ✅ SESSION_165 — headers deprecação; `wasm.rs` ativo migrado |
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

## 🧹 Higiene de Repositório (2026-08-03, SESSION_245)

**Feito (commits `8d478bd`..`f41aa03`):**
- Binários versionados: `firmware/**` + `models/tokenizer/*.BIN` untracked + gitignore (download via `tools/download_firmware.py`/`download_models.py`); 157 arquivos removidos do tracking.
- `LEGACY/` deletado (2,3 MB, 322 arquivos) — coberto por 188 tags git.
- Dedup fonte-única: `ntfs_reader.rs`, `load_status.rs`, `k_ai::memory_agent` (cópias mortas) deletados; `neural-kernel/src/interrupts.rs` virou facade de `k_nano::interrupts` + `interrupts_ext.rs` (residuais Ring3: TssCell/TSS_ARRAY, seletor user, syscall 0x90, hooks demand-page/allocator, `init_pic_fallback_and_sti`). IDT/GDT com fonte única em k_nano.
- Evidência de boot: `docs/evidence/boot-whpx-20260802.txt` commitada (logs/ continua gitignored).
- Política de idiomas: EN para código/comentários/logs; PT para docs/sessões (README §Language Policy).
- Histórico git NÃO reescrito (612 MiB pack) — decisão do maintainer (editor paralelo ADR-0083 ativo; reescrever exige pausar outras sessões + force-push). Revisitar quando `main` estiver quiescente.

**Deferrals (duplicação, fonte-única) — exigem refactor multi-sprint (emagrecer), NÃO facades cegos:**
- `agents.rs` (hermes 139 KB ↔ bin 128 KB): **NÃO é facade-safe hoje** — `SKILL_STORAGE`/`TRUST_CACHE`/`USAGE_TRACKER`/`EVENT_LOG` são statics DUPLICADOS (bin `main.rs:622` vs hermes `globals.rs:39`); `EVENT_BUS`/`SKILL_REGISTRY` são compartilhados (k_nano). Facade cego faria agentes hermes gravarem no SKILL_STORAGE do hermes, invisível para o bin → quebra skill path. Passo 1: consolidar os 4 statics; passo 2: facade + mover residuais (sysinfo_agent, SelfEvolveAgent, PLATFORM_READY, dispatch_pnp_action_nk).
- `boot_log_agent.rs` (k_ai 7,2 KB ↔ bin 13,4 KB): AMBOS vivos (bin registrado + hermes via BootSelfHealAgent). Bin tem fixes de produção (budget FAT, SelfHeal hook) com deps bin-locais; portar budget p/ k_ai + seam ErrorContext, depois facade.
- `agents/mouse_agent.rs`, `agents/log_analyst_agent.rs` (hermes ↔ bin): idem agents.rs.
- Espelhos `net/` (netstack, network_agent, net.rs), `fs/` (ata_agent, proc_fs_agent, …), `cortex.rs`/`k_ai` (hnsw, chunker, …): listados pelo guarda `tools/check_duplication.py` (50 DUPs restantes pós-higiene). Programa emagrecer — ver `.cursor/rules/neural-emagrecer-bin.mdc`.
- `interrupts_ext.rs` (bin): residuais Ring3 referenciam GDT/TSS próprios — o review (oracle, f41aa03) marcou HIGH mascarado por `TRY_ENTER_RING3=false`: GDT user segments não é carregado (k_nano carrega o dele), TSS per-proc `set_rsp0` muta TSS que não é o do LTR, AP RSP0 nunca setado. Resolver ANTES de re-habilitar Ring3 (ADR-0060), não antes.

---

**Detalhes completos:** `TODO.md`
**Catálogo de tecnologias:** `TECNOLOGIAS.md`
**Roadmap completo:** `ROADMAP.md`

