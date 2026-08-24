# 📋 TODO MASTER — neural-os-core

**Versão release:** v1.9.99 TEST
**Data:** 2026-08-22
**Propósito:** Checklist mestre; execução = ADR-0100 T-001–T-075.
**Documento oficial:** AGENTS.md + `docs/architecture/0100-k3chj-backlog-custo-anel.md`
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado | `[ ]` TODO 0100
**Pista ativa:** **ADR-0100** (SESSION_282) — ondas 0–10. Premissa 0088. SMP s281 código ✅; metal K23 = T-017+.
**PreFlight:** `python tools/preflight_wave.py --wave N` · `--idea 418` · `--anti-fake-ready` · cache `.preflight_cache/`
**Tags:** `depends_on: lan` (✅ L3.5–L5) / `depends_on: wifi` ▶️ · ▶️ **AWAITING_HW** · **BEI** (BitNet Ecosystem Intelligence)
**Gate v2.0.0:** `por_fazer` zerado **ou** residual replanejado + OK maintainer. AWAITING_HW bloqueia salvo defer explícito.
**Residuals por onda:** 0–7 ✅ · Pós-LAN ✅ (NetFs PASS) · **ADR-0086 núcleo ✅ (U1/U2/U4/U6 + I3–I12 SESSION_252; A1–A9 residual)** · WiFi AWAITING · **TLS parcial ✅ (s156 smoke PASS; PKI real pendente)** · R soft-float defer.
**Fora do gate (não atracar):** SmileyOS 279a–b/e, Cube 283a, XDNA 💰, SKYNET 315.26–27, Mach-O/APK, wasmi-USB #8/#11.

**Feito recente (SESSION_274):** GPU compute honesto — vendor matmuls → `None`; `boot_report.gpu_ok` real; MHI tier0 CE wired; msched/sasos acessos. SESSION_273: Trinity único + `runtime_observe` + HUD no render. SESSION_272: storage NVMe-first + Trust/HITL no boot.

---

## ▶️ FILA ADR-0100 — custo × anel (simples → complexo)

Fonte canônica: `docs/architecture/0100-k3chj-backlog-custo-anel.md`. Fila antiga 0–18 mapeada nas ondas (itens ✅ permanecem histórico).

| Onda | Custo | Anel | Tema | TODOs | Status |
|------|-------|------|------|-------|--------|
| 0 | S | R2 `k_ai` + bin | Honesty `BOOT_AI` + freeze HardwareInfo | T-001–T-006 | `[x]` s283 |
| 1 | S–M | R0 `k_nano` | `measure_bandwidth` + `/hw/storage|gpu|net` | T-007–T-016 | `[x]` s283 (T-010 UNSUPPORTED) |
| 2 | S evidência | R0 (já s281) | Metal K23 `online==madt-1` | T-017–T-021 | `[~]` T-017 img; T-018+ metal |
| 3 | S–M | R3 hermes/jarbas | 0086 A2–A8; A1/A9 HITL | T-022–T-032 | `[~]` T-022/023/031 abertos |
| 4 | M–L | R0 + cortex + agent-core | `ap_pollable` + runqueue 0089 | T-033–T-044 | `[~]` T-037 heap; feature OFF |
| 5 | M | R0/R3 | Mesh 2c + CRDT; SemanticRouter defer | T-045–T-050 | `[~]` T-048/049; T-045 2c |
| 6 | L | R0 + bin + hermes | Ring3 HW + `register_native_ring` + PIN_DMA | T-051–T-057 | `[ ]` |
| 7 | M–L | R2 cortex | W2A8 gated; 0078 **só Fase 1** | T-058–T-065 | `[ ]` |
| 8 | XL ▶️ | R1 `k_hal` | Golden GPU / SDMA / NPU 💰 | T-066–T-069 | ▶️ |
| 9 | M | R3 jarbas | 0058 S5 um widget; A/V | T-070–T-072 | `[ ]` |
| 10 | L ▶️ | R2 | AirLLM DMA/e2e | T-073–T-075 | ▶️ |

## ▶️ ADR-0092 — Observabilidade de boot (#539)

Fonte: `docs/architecture/0092-boot-observability.md`. **Não** entra nas ondas T-xxx do 0100.

| Onda | Custo | Anel | Tema | Aceite | Status |
|------|-------|------|------|--------|--------|
| O0 | S | R0 slog/serial | `sev` ok\|warn\|fail\|trace + filtro consola | teste host + check 0 erros | `[x]` |
| O1 | S | bin + k_nano | Banner `=== PHASE n= ===` + fase 8 PostRuntime | 9 banners no log QEMU | `[x]` código |
| O2 | S–M | k_nano + bin + jarbas | Mudos BPB/INIT1/e1000/SIPI/scan/PnP | serial curto até Runtime | `[x]` código |
| O3 | S | `boot_report` | `BOOT SCORE` + `tools/parse_boot_score.py` | parser honesto | `[x]` |
| O4 | S | jarbas | Sem K* no ecrã; HUD produto | screendump | `[x]` código |
| O5 | S | evidência | Profile qemu vs hw no placar | mesmo parser | `[x]` `qemu=` no SCORE |

**Próximo:** T-018 metal K23. T-022 guest OTA. T-037 BSS 511. T-044 HUD. T-045 mesh 2c. **O0 slog sev (0092).**

**Fora:** BitTorrent/merkle; NTFS write; 0078 Fase 2–4; 0076 F1–F17 (já ✅); WireGuard; Vulkan.

**Regra:** ▶️ AWAITING_HW não bloqueia v2.0.0 com defer. Ring3 TCG ≠ `register_native_ring`.

### Checklist T-001–T-075 (marcar aqui; detalhe na ADR-0100)

- [x] T-001 boot_report Observe/Plan/Act/Verify
- [x] T-002 Auto vs Escalate
- [x] T-003 linha serial `BOOT_AI`
- [x] T-004 teste host parse
- [x] T-005 congelar `HardwareInfo`
- [x] T-006 wrappers `hw_cpu_*`
- [x] T-007 API `measure_bandwidth` (TSC, 16 setores)
- [x] T-008 medir NVMe (default StorageController)
- [x] T-009 medir AHCI (idem)
- [x] T-010 BMIDE 0xC8 = UNSUPPORTED (ATA PIO amostra / skip TCG)
- [x] T-011 TCG: skip medida + ATA fora do plano
- [x] T-012 plano `k_ai` não inclui ATA em TCG
- [x] T-013 `/hw/storage`
- [x] T-014 `/hw/gpu`
- [x] T-015 `/hw/net`
- [x] T-016 wifi só se device
- [x] T-017 imagem USB unified (`target/usb_hw.img` 3199 MB, 2026-08-22)
- [ ] T-018 i5 K23 + online
- [ ] T-019 240H K23 + online
- [ ] T-020 log ICR canônico
- [ ] T-021 SESSION se falhar (não BSP-only destino)
- [ ] T-022 smoke QEMU A2
- [x] T-023 evidência host `docs/evidence/t022-ota-host-2026-08-22.txt` (guest A2 aberto)
- [x] T-024 provision NET_READY
- [x] T-025 HITL download (Active só via shell)
- [x] T-026 telemetria cron backoff
- [x] T-027 verificar A5 s253 (card 7902 + DISK_SELECTION)
- [x] T-028 menu Live/Install (I/L ~5s)
- [x] T-029 default Live (`boot_mode()` sem CONFIG)
- [x] T-030 tries 3 + last_good
- [ ] T-031 A9 mini — só HITL
- [x] T-032 A1 Ed25519 defer público (ADR-0100 §3.8)
- [ ] T-033 barreira AP_IDT_READY (código s281; aceite = metal T-018)
- [x] T-034 sti só com IST (`ap_load_idt_and_tss`)
- [x] T-035 parallel_* gated `ap_pollable()` (BSP até T-033 metal)
- [ ] T-036 self-test matmul smp2
- [x] T-037 PerCpu/TSS heap = MADT (sem BSS 511)
- [x] T-038 boot 1c: GDT early 7 u64; ap_slots()=0
- [ ] T-039 feature runqueue pós T-033 (OFF no bin)
- [x] T-040 dispatch; ring0 não migra (`resolve_target_core`)
- [x] T-041 steal min-1
- [x] T-042 IPI reschedule (`wake_core_if_needed`, gated pollable)
- [x] T-043 testes host CPU_COUNT + TEST_LOCK
- [x] T-044 HUD pending/core no render()
- [ ] T-045 mesh 2c vs s281
- [x] T-046 WHPX OVMF não é sprint kernel (scripts TCG)
- [x] T-047 teto 4G script (mesh default 4; ota_launch/qemu_ota_loop)
- [x] T-048 CRDT merge LWW + conflito visível
- [x] T-049 teste host merge
- [x] T-050 SemanticRouter não nesta pista (lexical/intent_bus)
- [ ] T-051 WHPX Ring3 vs OVMF
- [ ] T-052 metal iretq
- [ ] T-053 0077 §6 HW
- [ ] T-054 register_native_ring + HITL
- [ ] T-055 isolation_ring_available
- [ ] T-056 JIT sem SSE default
- [ ] T-057 SYS_PIN_DMA pós Ring3
- [ ] T-058 W2A8 gated ISA
- [ ] T-059 golden GTX 1050
- [ ] T-060 paridade quantizada
- [ ] T-061 threshold adaptativo
- [ ] T-062 convert GGUF script
- [ ] T-063 1 modelo pequeno QEMU
- [ ] T-064 PPL vs fixo
- [ ] T-065 0078 F2–F4 fora
- [ ] T-066 NVIDIA ACR ▶️
- [ ] T-067 AMD SDMA ▶️
- [ ] T-068 Intel GuC ▶️
- [ ] T-069 XDNA 💰
- [ ] T-070 um widget S5
- [ ] T-071 A/V HDA; UVC ▶️
- [ ] T-072 sem draw no tick()
- [ ] T-073 prefetch DMA ▶️
- [ ] T-074 e2e GGUF hash
- [ ] T-075 K-quants residual documentar

**Mapa fila antiga:** #7→Onda7 · #8→Onda10 · #10→Onda9 · #12 0076✅ · #13→Onda5 · #14→Onda2+4 · #15→Onda6 · #16→Onda7 F1 só · #17→Onda8 · #18→Onda1.

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
| **TLS real #123** | ~1.500 | **Parcial ✅**: compile PASS (s155) + wire `https_get` (s156) + smoke PASS google 80KB (s156, trust=unsecure). **Resta: PKI real** (RootPin/TOFU sem `unsecure`; blueprint absorvido de `TLS_INTEGRATION_BLUEPRINT.md` → legacy) |
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
- `interrupts_ext.rs` (bin): residuais Ring3 vs GDT/TSS do k_nano (oracle f41aa03). SESSION_278 moveu user segments para a GDT carregada. Residual: produção HW/`register_native_ring` (ADR-0077), não o demo TCG.

---

**Detalhes completos:** `TODO.md`
**Catálogo de tecnologias:** `TECNOLOGIAS.md`
**Roadmap completo:** `ROADMAP.md`

