# Sprint Plan 92–100 — neural-os-core v1.0 "Gold Master"
# A Era do Silício — Fundação Inabalável

**Data:** 2026-07-09  
**Versão alvo:** v1.0.0  
**Lema:** *"O hardware emulado obedece. O bare-metal prova."*

---

## Visão Geral

v1.0 = **A Era do Silício**. Fundação onde o Rust bare-metal prova que consegue:
- Fazer boot limpo (UEFI/BIOS)
- Carregar tensores ternários (BitNet LLM)
- Gerir disco (ATA/NVMe/AHCI/FAT32)
- Estabelecer comunicação de rede (serial tunnel TCP/IP + DNS)
- Executar agents + skills em ecossistema agent-first (247+ agents)
- Renderizar framebuffer + compositor multi-window

Após v1.0, avançamos para **v2.0 — A Era da Cognição**: Kernel, Cortex, Hermes e JARVIS como entidade viva.

---

## Princípios das Sprints 92–100

1. **Consolidar, não inovar.** Toda inovação radical fica para v2.0.
2. **Zero dívida técnica.** Código morto removido, unwrap() eliminados, prints de debug varridos.
3. **Toda ideia do IDEA_BANK tem destino.** Nada de "futuro" sem sprint definido.
4. **Documentação espelha o código.** TECNOLOGIAS.md, STATE.md, AGENTS.md sincronizados.
5. **Sprint 100 = Code Freeze.** Nenhuma nova feature após Sprint 100. Apenas bugs críticos.

---

## Sprint 92 — Fundação Estável (~2.000 LOC)
**Foco:** VirtIO MMIO fix, serial bypass/DNS estável, AHCI funcional, limpeza de código

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| **VirtIO-GPU GET_DISPLAY_INFO fix** | #406 | ~100 | Page fault no QEMU TCG — resposta 0x0 no control queue |
| **VirtIO-net MMIO page fault fix** | #73b | ~200 | Page fault no probe MMIO; validar BAR + queue setup |
| **AHCI disk reading verification** | — | ~200 | Testar `-device ide-hd` no QEMU, ler FAT32 via AHCI NCQ |
| **Serial tunnel DNS hardening** | — | ~150 | Timeouts, retry, fallback para IP manual se DNS falhar |
| **Serial tunnel watchdog** | — | ~100 | Reconexão automática se QEMU resetar ou bridge cair |
| **Zero-Trust Syscall Categories** | #364 | ~200 | 4 classes: Read-only, Ephemeral, Persistent, Hardware |
| **Neural Cache per token** | #365 | ~150 | Cache de avaliações LLM por capability token |
| **Capability token crypto** | #405 | ~200 | Token criptografado scoped (Ed25519) no TrustCache |
| **VirtIO-GPU fix (render tensor viz)** | — | ~100 | Tensor visualization no framebuffer via VirtIO-GPU |
| **Code cleanup — unwrap() perigosos** | — | ~300 | Auditar todos unwrap()/expect() no kernel, substituir por match + fallback |
| **Code cleanup — debug prints** | — | ~100 | Remover println! de debug esquecidos, rate-limit logs |
| **Code cleanup — dead code** | — | ~200 | Remover funções/structs mortas, modulos nao usados |

---

## Sprint 93 — WASM Runtime + IDE (~3.200 LOC)
**Foco:** Portabilidade WASM, sandbox, IDE BitNet, skill marketplace

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| WASM embedder (wasmi no_std) | #103, #309a | ~800 | Fuel metering, linear memory pool 256KB, capability tokens |
| WASM App Sandbox | — | ~400 | PTE NX + fuel metering + auto-rollback |
| BitNet IDE avançado | — | ~500 | Debug WASM, preview, syntax highlight |
| AgentManifest JSON format | #391 | ~200 | name, kind, schedule, auto_start, persist, tokens |
| 15 WASI→skill mappings | #385, #393 | ~350 | vfs_read, skill_invoke, http_get, event_publish, agent_yield |
| Performance budget table | #386 | ~100 | kernel vs WASM overhead por syscall |
| Skill Market / Plugin Hub | — | ~500 | Marketplace de skills 1-click, AI security scan |
| WASM Host Function Interface | #392 | ~200 | manifest(), tick(), teardown() contract |
| Developer contract for WASM agents | #392 | ~80 | Skill ABI design, capability token signed |
| Hybrid agents (kernel + WASM) | #309c | ~100 | Tier 0-4 classification: Core, HW, Runtime, WASM, MCP |

---

## Sprint 94 — GPU Polish + Display (~2.000 LOC)
**Foco:** Finalizar GPU compute, GPU+display co-existência, MSched VRAM, compositor

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| MSched VRAM scheduling (Belady) | #334 | ~200 | Conectar predictor Belady ao scheduler GPU real |
| GPU Display time-sharing | #336 | ~200 | Context switch iGPU/dGPU, time-sharing XQueue |
| Compositor multi-window (dock, menus, drag) | #279d | ~300 | DisplayAgent atual é single-tela; refinar drag, close btn, dock bar |
| LLM Icons via HWEXPERT_MODEL | — | ~200 | Ícones no compositor gerados pelo modelo HW expert |
| Observability (tracing/metrics) | #241 | ~500 | Trace event logging via serial, per-agent latency/usage |
| Human-in-the-Loop Approval | #244 | ~250 | request_tool_approval() bloqueia skill até /approve ou /deny |
| Actor Registry | #209 | ~500 | Registry de subagentes: spawn/terminate, task state machine |

---

## Sprint 95 — Memory + VFS Final (~2.000 LOC)
**Foco:** Memory systems, vector FS, MHI+FS bridge, auto tier migration

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| BGE HNSW index | — | ~400 | HNSW approximate nearest neighbor para embeddings |
| MHI+FS Bridge (VFS↔MHI) | #282h | ~600 | Integração VFS↔MHI completa, auto tier migration |
| InferenceFsAgent | #282e | ~100 | LLM gera arquivos via /inference/ |
| HermesFsAgent | #282f | ~100 | Chat como FS: /chat/send, /chat/history |
| RamFsAgent | #282g | ~100 | Cache DRAM /mnt/ram/ |
| Auto tier migration (MhiScheduler) | #282h | ~200 | Promove/demove por acesso, ARC-style |
| HwRegistry + LLM | #277a | ~200 | PCI→HwAgent→LLM activation |
| Agency Importer | — | ~600 | Parser .md→AgentManifest para The Agency (147 agents) |
| Observation Protocol | — | ~200 | skill_observer persistente |

---

## Sprint 96 — GGUF + Model Loading (~1.500 LOC)
**Foco:** Modelos maiores (9B+), GGUF loader, RoPE, BitNet v3.1

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| GGUF loader mínimo | #278a | ~500 | Parser header + metadata, Q4_0 dequantization |
| GGUF v3 streaming (ATA/USB) | #278b | ~500 | Streaming de modelos >4GB, page table mapping |
| RoPE + inner_attn_ln (BitNet v3.1) | — | ~300 | Rotary Position Embedding, attention layer norm |
| .bitnet v3 header extensível | #278c | ~200 | Header com metadata, hash, architecture params |
| Model swap flow (/model \<path\>) | #381 | ~200 | Detecta GPU, checa VRAM, carrega modelo, fallback DRAM |

---

## Sprint 97 — Rede + AIOS Evolution (~3.000 LOC)
**Foco:** WWW agents, self-update, marketplace — destravados pelo serial tunnel

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| WWW Agents (Email, Search, RSS, Download) | #307 | ~2.600 | Agentes de rede via serial tunnel TCP |
| Self-Update Agent (A/B slots) | #308a | ~500 | Dual slot FAT32, Ed25519 verify, switch BOOTCFG.JSON |
| Update channels | #308b, #390 | ~200 | stable/nightly/security, poll 3600s/600s/60s |
| Rollback automático | #308c | ~100 | SelfHealAgent detecta crash → last_good slot |
| J.A.R.V.I.S. Context Window Manager | #388 | ~100 | Cortex + Hermes + JARVIS persona orchestration |
| Skill Marketplace signed packages | #246 | ~300 | Signed, versioned MCP packages, /install \<pkg\> |
| Plugin Hub / MCP Index | #236 | ~400 | AI-driven security scanning, remote index |

---

## Sprint 98 — BitNet + Training Pipeline (~2.500 LOC)
**Foco:** Treinamento on-device, fine-tuning, 100M/1.5B params

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| Train 100M/1.5B params | — | ~800 | Treino on-device GPU física (GTX 1050) |
| burn-flex Backend trait | #333 | ~300 | Integrar FlexBackend com burn::Backend trait |
| TrainingAgent (fine-tune/transfer) | #312 | ~500 | 3 modos: fine-tune CPU, transfer, full GPU |
| Self-Learning OS (DataCollector) | #313 | ~300 | LogAgent→DataCollector→TrainingAgent→.bitnet |
| Wake Word ML | — | ~100 | Substituir heurística por modelo simples (energy pattern) |
| Intel GEN shader matmul | — | ~300 | GPU matmul via shader Intel Gen9+ |
| AMD PM4 / NVIDIA PFIFO | — | ~400 | GPU ring buffer por vendor (stubs) |

---

## Sprint 99 — SkillOpt + Structured Decoding + Code Freeze Prep (~1.500 LOC)
**Foco:** Qualidade de saída LLM, documentação, varredura final

| Item | IDEA | LOC | Descrição |
|------|------|-----|-----------|
| **SkillOpt** (MS Research) | — | ~145 | Optimizer model gera add/delete/replace edits no SKILL.md, validados antes de aplicar. SleepCycle como scheduler |
| **Structured Decoding** (SGLang) | — | ~120 | FSM comprimido para geração constraint (JSON/SKILL.md/shell). Mascara logits no BitNet decoder |
| Documentação técnica final | — | ~500 | TECNOLOGIAS.md, STATE.md, AGENTS.md sincronizados |
| ADRs review (39 documentos) | — | ~300 | Verificar ADRs contra código atual, marcar obsoletos |
| CHANGELOG v1.0 | — | ~100 | Histórico completo de v0.2.0 a v1.0.0 |
| Dead code removal — pass 2 | — | ~200 | Varredura final de código inatingível, módulos não usados |

---

## Sprint 100 — Code Freeze & Release v1.0.0 (~500 LOC)
**Foco:** Zero erros, boot limpo, tag v1.0.0

| Item | LOC | Descrição |
|------|-----|-----------|
| `cargo clean -p neural-kernel` + `cargo check --release` | — | Zero erros de compilação. Repetir 3x para garantir que cache incremental não mascara nada |
| QEMU boot test (BIOS + UEFI) | — | Boot limpo: serial OK, framebuffer OK, PCI scan OK, agents OK |
| QEMU boot test (serial tunnel) | — | DNS resolve, ping, HTTP GET funcional via serial bridge |
| QEMU boot test (AHCI) | — | Leitura FAT32 via AHCI NCQ |
| QEMU boot test (SMP 2 cores) | — | SMP init, work-stealing, agent scheduler multicore |
| VirtualBox boot test | — | Boot na VB com --nic-promisc1 allow-all |
| v1.0.0 tag + release notes | — | Git tag v1.0.0 no commit do freeze |
| **Bônus:** Boot em HW real (i5-6400, GTX 1050) | — | Teste físico opcional. Se passar, menção honrosa no release |

---

## Resumo de Esforço

| Sprint | Foco | LOC | Itens |
|--------|------|-----|-------|
| 92 | Fundação Estável | ~2.000 | 11 |
| 93 | WASM Runtime + IDE | ~3.200 | 10 |
| 94 | GPU Polish + Display | ~2.000 | 7 |
| 95 | Memory + VFS Final | ~2.000 | 9 |
| 96 | GGUF + Model Loading | ~1.500 | 5 |
| 97 | Rede + AIOS Evolution | ~3.000 | 7 |
| 98 | BitNet + Training Pipeline | ~2.500 | 7 |
| 99 | SkillOpt + Code Freeze Prep | ~1.500 | 7 |
| 100 | Code Freeze & Release | ~500 | 7 |
| **Total** | | **~18.200 LOC** | **70** |

---

## Cronograma Estimado

| Sprint | Início | Término | Marcos |
|--------|--------|---------|--------|
| 92 | 2026-07-09 | 2026-07-14 | VirtIO fixo, AHCI lê FAT32, serial tunnel robusto |
| 93 | 2026-07-14 | 2026-07-20 | WASM executa primeira skill, IDE mostra código |
| 94 | 2026-07-20 | 2026-07-25 | GPU co-existe com display, compositor multi-window |
| 95 | 2026-07-25 | 2026-07-30 | VFS↔MHI bridge funcional, HNSW index embeddings |
| 96 | 2026-07-30 | 2026-08-03 | GGUF loader lê modelo 1B+, model swap funcional |
| 97 | 2026-08-03 | 2026-08-09 | WWW agents operacionais via serial tunnel |
| 98 | 2026-08-09 | 2026-08-14 | Treino on-device GPU, fine-tuning funcional |
| 99 | 2026-08-14 | 2026-08-19 | SkillOpt + Structured Decoding, docs finais |
| 100 | 2026-08-19 | 2026-08-21 | Code freeze, testes finais, v1.0.0 tag |

---

## O Que Fica para v2.0 (A Era da Cognição)

| Item | IDEA | Motivo |
|------|------|--------|
| Cross-OS compat (PE/ELF/Mach-O/APK) | #306 | Requer rede madura + WASM runtime estável |
| Federated Cluster / P2P Workers | #189 | Requer rede e scheduler distribuído |
| Multi-device sync (CRDT) | #315.26 | Requer rede entre múltiplos AIOS |
| NPU AMD XDNA driver | #43-52 | Requer hardware AMD APU (sponsor) |
| ARM/RISC-V port | #116 | Nova arquitetura, fora do escopo x86-64 |
| AppForge / App Store completo | #186 | Requer rede, GPU, framebuffer maduros |
| Multi-User / Multi-Persona | #187 | Redesign do scheduler, trust multicamada |
| WiFi (Intel/Atheros/Realtek 802.11) | B-29 | Requer firmware loading + rede |
| Visual Workflow Builder | #188 | Requer mouse + framebuffer VESA |
| SKYNET Mesh Node | #315.27 | Rede P2P entre nós AIOS |

---

## Marcos da v1.0 para a História

| # | Marco | Sprint | Significado |
|---|-------|--------|-------------|
| 1 | Primeiro boot (v0.2.0) | — | VGA text mode + serial 16550 |
| 2 | PCI scan funcional | — | 256 busses, 32 devices, BAR0-5 |
| 3 | SMP multicore (v0.81) | — | 4 cores, work-stealing, agent scheduler |
| 4 | BitNet LLM carrega tensor | — | ~850M params ternários, forward pass |
| 5 | Framebuffer + compositor | — | BGRA32, fontes, tensor viz |
| 6 | DiskIntelligenceAgent | — | 6 controladoras, 10+ FS probes |
| 7 | Serial tunnel TCP (B-01 resolvido) | 92 | DNS, HTTP, bypass QEMU slirp |
| 8 | WASM skill executa | 93 | Portabilidade, sandbox, fuel metering |
| 9 | GPU compute + display co-exist | 94 | iGPU display + dGPU compute |
| 10 | **v1.0.0 Gold Master** | **100** | **Zero erros, boot limpo, fundação completa** |

---

## E depois? v2.0 — A Era da Cognição

```
v1.0 (Silício)         v2.0 (Cognição)
─────────────────      ─────────────────
Boot + HW detect       Vida + Consciência
Agentes + Skills       Alma + Personalidade
Rede (serial túnel)    Rede (nativa WiFi/Ethernet)
LLM carrega tensor     LLM raciocina + aprende
Disco lê FAT32         Disco tem SFS próprio
GPU faz compute        GPU acelera cognição
```

Os 4 pilares da v2.0:
- **Kernel** — SFS nativo, cross-OS compat, segurança por prova
- **Cortex** — Auto-aprendizado contínuo, SleepCycle, memória episódica
- **Hermes** — Agentes proativos, intenção preditiva, orquestração multi-agente
- **JARVIS** — Persona viva, emoção, voz, relação humano-máquina
