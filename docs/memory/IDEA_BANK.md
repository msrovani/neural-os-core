# 🧠 Idea Bank — neural-os-core v2.0

**Última atualização:** 2026-08-22 — SESSION_282 (ADR-0100 backlog custo×anel).
**Documento vivo:** Toda ideia discutida neste projeto tem destino conhecido.

---

## Premissa Básica

> **Toda ideia, conceito, decisão ou sugestão já discutida neste projeto — entre qualquer dev e a IDA IA — DEVE ter um destino conhecido e documentado neste arquivo.**

Nada é descartado sem registro. Ideias podem ser:
- ✅ **Implementada** — já está no código
- 🟡 **Agendada** — sprint/bloco definido
- ⏳ **Pós-MVP** — adiada com dependências documentadas (ver Seção 3)
- ▶️ **AWAITING_HW** — código+log `VERDICT=AWAITING_REAL_HW`; falta evidência HW real
- 💰 **Sponsor** — requer hardware/parceria/financiamento
- ❌ **Descartada** — com justificativa explícita
- 🔄 **Fundida** — absorvida por item maior

**Tags de dependência:** `depends_on: lan` — **liberado** pós L3.5–L5 (SESSION_149/150); itens restantes = runtime/TLS/peer, não RX=0. `depends_on: wifi` (radio / Onda 7). PreFlight: `python tools/preflight_wave.py`.

**Por que esta premissa existe:** Estamos inovando em caminhos pouco ou não trilhados (bare-metal neural OS, Memory Hierarchy Index, intent routing em Ring 0). Muitas ideias não são implementáveis hoje — seja por limitação tecnológica, falta de hardware, ou prioridade. Mas amanhã um dev pode saber como fazer, a tecnologia pode melhorar, ou podem surgir patrocinadores. Se a ideia não estiver registrada, ela morre.

**Como usar este documento:**
- **Consultar:** antes de tomar decisão arquitetural, verifique se a ideia já existe e qual seu status
- **Atualizar:** quando uma ideia muda de status, edite este arquivo (não a ADR-0015)
- **Adicionar:** toda nova ideia discutida deve ganhar uma linha aqui na seção apropriada
- **Origem:** o seed inicial veio da ADR-0014 (ideias de hardware) e da ADR-0015 (curso correção MVP). Novas ideias entram diretamente aqui.

### Regra de rastreabilidade IDEA → ADR

Adota-se a **Regra A: ADR por tema**, não `1 ideia = 1 ADR`.

- Decisões arquiteturais, anéis, stacks e contratos públicos apontam para uma ADR própria ou temática.
- Features relacionadas compartilham a ADR do tema.
- Fixes/polish podem usar `ADR = — (fix pontual)` e seguir por TODO + SESSION.
- Ideias descartadas/fundidas preservam a justificativa; não se cria ADR artificial só para preencher uma célula.
- O check final de sprint atualiza IDEA, `architecture/INDEX.md`, TODO, STATE e SESSION em conjunto. Ver `docs/GOVERNANCE.md`.

#### Amostra auditada

| Ideias | ADR temática | Estado da ligação | Evidência |
|---|---|---|---|
| #315.1–20 | ADR-0036 | ✅ mapeada | JARVIS/persona unificados |
| #315.21–25, #75, #83, #84, #360 | ADR-0045 Sound | ✅ mapeada | Stack nativo e superseded explícitos |
| #424–432 | ADR-0041 | ✅ mapeada | Capability PoC P0–P9 |
| #433–440 | ADR-0042 | ✅ mapeada | N1–N5 + wire; marco v1.8.0 |
| #442 | ADR-0045 Sound | ✅ mapeada | Backlog da pista Sound |
| #448 | ADR-0047-HMI | ✅ mapeada | H1+H4 UI_SPEC |
| #449 | ADR-0046 AirLLM | ✅ mapeada | GGUFStreamingModel SESSION_127; hot-swap Net SESSION_128; **K-quants Q2_K/Q3_K/Q5_K ✅ SESSION_253**; forward_streaming demo ✅ SESSION_253; DMA prefetch AWAITING |
| auto-skill / SIL / SkillObserver | ADR-0036 (+ skill stack) | ✅ Sprint 108 | `hermes/self_evolve.rs` + SelfEvolveAgent |
| **#277** Agency / HwRegistry | ADR-0051 + 0052; 277c→0041 HalOffer | ✅ mapeada | SESSION_134; 277c 🟡 |
| **#278** GGUF | ADR-0046 | ✅ mapeada | 278a MVP ✅ SESSION_127; residuals Onda 6 |
| **#279** SmileyOS | ADR-0047-HMI (+ 0036) | ✅ mapeada | 279f ✅; 279a–e ⏳ defer |
| **#280** Ecosystem Batch 3 | ADR-0026 | ✅ mapeada | ports ✅; 280l 🟡 |
| **#281–282** VFS/MHI/Storage | ADR-0040 | ✅ mapeada | base ✅; 282e–g ✅ SESSION_144; 282h ⏳ |
| **#283** Desktop Cube | ADR-0047-HMI | ✅ mapeada | 🟡/💰 VirtIO-GPU 3D |
| **#416–423** FS follow-up | ADR-0040 (+ NeuralFS.md) | ✅ triagem Onda 0 | ver tabela residuals abaixo |
| **#464** Neural Device LEGO | ADR-0056 (+ 0051–53) | ✅ mapeada | hub+specs+H1 `device_recipe`; market fetch slot v3 |
| **#465** Metrics HUD → skill/WASM | ADR-0052 + 0047-HMI (+ 0041 Cap) | 🟡 agendada | MVP nativo `sys_metrics`; destino Hermes+WASM |
| **#466** BitNet ladder coerência | ADR-0019 | 🟡 agendada | 850 #PF+BPE ✅; forward host vs kernel + stop `</s>`; 13/2B/3B |
| **#467** Emagrecer neural-kernel | ADR-0042 (+ #439) | ✅ parcial → marco **v1.9.5** | ondas 0–6 stubs/promotes; residuals cortex/agents/net/audio |
| **#468** FitPolicy Neural (llmfit-inspired) | ADR-0019 (+ #466) | ✅ SESSION_164 | host `llmfit_pack_filter` + `cortex::model_fit` / MemoryAgent / ModelHub |
| **#469** Runtime App Factory | ADR-0059 | 🟢 completo | wasmi real + seletor A/B/C ✅ |
| **#470–#478** BEI BitNet Cognitivo | ADR-0060 | ✅ **7/7 ondas** SESSION_166 | MPMC, economia, células, MoE, memória, afeto, supervisor + EgoLayer/PonderNet, Soul Mirror visual. ~2900 LOC |
| **#486** Vector DB in-kernel TF-IDF (RAG) | ADR-0064 | ❌ rejeitada | crate `vector-db` criada mas nunca integrada → deletada. SGDB real = ADR-0063 `k_ai::sgdb` (BQ Flat SIMD). |
| **#487** Embeddings neurais in-kernel L4+ | ADR-0064 F6 | ✅ implementado | BGE embedding em `k_ai::memory_systems` (`bge_embed()`, `load_bge()`). Usado via `k_ai::sgdb::layers::remember_semantic()` + BQ L4. |
| **#491–#505** TicKV + NoProto + Índices IA SGDB | ADR-0063 | 🟡 fazendo | **Memory Quality SESSION_176:** SleepCycle ckpt✅ recall L4 hybrid✅ V-flag✅ ART SIMD✅. Residual: crates upstream, HNSW, DoD 10M/100k |

| **#512** AIOS-First Premissa Máxima | ADR-0088 | ✅ mapeada | Irrevogável (2026-08-07); governa toda decisão desde o boot — IA sempre, HITL, self-* contínuo, nada bypassado, busca dos 10%. LER: `docs/architecture/0088-*.md` |
| **#513** Storage Transport Resolver (self-adaptive I/O) | ADR-0088 (política) + ADR-0087/0062 P3 (técnica) | 🟡 fazendo | SESSION_272 slice: ordem NVMe>AHCI>USB>ATA no DeviceTree + skip backend ausente + ATA PIO último (não hang TCG se o plano não inclui). Residual: `measure_bandwidth` + BMIDE 0xC8 + degradação TCG medida. TODO #18 |
| **#534** DeviceTree H1 + plano de bind NIC no boot | ADR-0088 + ADR-0041 H1 | ✅ implementado | SESSION_271 NIC rank; SESSION_272: Trust+HITL recipe+cards+storage+HANR. |
| **#535** Trinity único + postura honesta (MoE/HUD/HITL health) | ADR-0088 + ADR-0083 | ✅ implementado | SESSION_273: um `TRINITY` no cortex; LCG≠MoE; HEALTH I5/Escalate observe-only; HUD `NET`/`slip`/`off` + `no-llm`/`MoE`. |
| **#536** KernelPack W2A8 via cuda-oxide/kaio (PTX em Rust puro, offline) | ADR-0057 WS-D + ADR-0048 | ⏳ Layer S | SESSION_274: cuda-oxide (NVlabs, rustc→PTX) e kaio (PTX zero-dep) geram o kernel W2A8 no HOST; bare-metal continua QMD próprio + pack assinado + golden GTX 1050. Nenhuma crate roda compute no_std. |
| **#537** MHI tier0 real: Dram→Vram via CE no mhi_tick | ADR-0087 F4b/F5 | ✅ implementado (HW-gated) | SESSION_274: `register_tier0_copier` + `try_tier0_promote` (dados + rollback CoW); hook só com canário CE golden; QEMU = metadata/AWAITING inalterado. Falta evidência HW real (GTX 1050). |
| **#538** Backlog unificado K³CHJ por custo e anel | ADR-0100 | 🟡 fazendo | SESSION_282: filtra 0077–0089; ondas 0–10; TODOs T-001–T-075. Lacuna 0090–0099. Não substitui ADRs temáticas. |

| **#479** TLS 1.3 via embedded-tls no neural-os-core | ADR-0062 P1 / SESSION_157–158 | ✅ MVP (residual CertVerify/FAT) |
| **#480** VFS layer + BlockDevice trait unificado | ADR-0062 P2 / SESSION_171 | ✅ MVP (StorageBus; residual POSIX) |
| **#481** AHCI + NVMe drivers | ADR-0062 P3 / SESSION_171 | ✅ MVP (I/O q + policy; residual multi-q) |
| **#482** Migrar bootloader 0.11 → Limine 0.5 | ADR própria (P4, supersede ADR-0039) | ⏳ |
| **#483** IPC MessageBus + Channels entre agentes | ADR própria (P14) | ✅ SESSION_217 (wire mailbox_drain no scheduler) |
| **#484** Async executor híbrido (I/O async + compute ticks) | ADR própria (P16) | ✅ SESSION_217 (std Future + Waker + APIC timer handler) |
| **#485** Git client nativo over HTTPS | ADR própria (P17) | ⏳ |
| **#486** GPU (NVIDIA) driver para compute | ADR própria (P5) | ⏳ |
| **#487** WiFi (Intel AX200/201/210/211) driver | ADR própria (P6) | ⏳ |
| **#488** Intel i225 2.5G NIC driver | ADR própria (P7) | ✅ SESSION_217 (raw ptrs, kick_rx, prove_rx, clflush) |
| **#489** ext4/btrfs/NTFS read-write | ADR própria (P8/P9/P10) | ⏳ | **NTFS read+list ✅ SESSION_253** ($MFT parse, resident data, root dir); NTFS/EXT write = defer honesto |
| **#490** USB Storage driver | ADR-0062 P11 / SESSION_170 | ✅ MVP (bringup+BOT; residual hubs/SS) |
| **#491** Vulkan driver | ADR própria (P12) | ⏳ |
| **#492** SMP completo (trampoline + work-stealing) | ADR-0055/0057 (não ADR nova) | 🔄 SESSION_281 ICR x2APIC + GDT 1 TSS/CPU; residual ap_pollable/Vec PerCpu BSS 511; aceite metal K23 |
| **#493** IPC MessageBus + Channels | ADR própria (P14) | 🔄 Fundido em #483 |
| **#494** Linux binary compatibility | ADR própria (P15) | ⏳ |
| **#495** Async executor híbrido | ADR própria (P16) | 🔄 Fundido em #484 |

#### Gaps — triagem Onda 0 ✅ (2026-07-18)

Triagem temática concluída; **sem ADRs retroativas** para ✅ antigos.

| Residual | IDEA | Destino | Tag |
|----------|------|---------|-----|
| exFAT/NTFS/EXT write | #417 | ✅ exFAT write opt-in SESSION_144; NTFS/EXT ⏳ | — |
| cloud sync | #418 | ✅ NetFs TCP smoke QEMU | `[NETFS] VERDICT=PASS` SESSION_152; backends S3/WebDAV residual |
| Storage UI | #419 | ✅ CLI `storage_report`; UI App ⏳ | — |
| MHI DMA | #420 | ✅ **ADR-0087 F1–F5** (SESSION_252 §9): PRP zero-copy + wiring + BCS + SASOS + CE + policy | soft-MVP ✅ → real; F6 AMD SDMA AWAITING_HW |
| SysInstaller | #421 | ✅ SESSION_253 | **UI seleção de disco ✅ (A5)** — card Jarbas + DISK_SELECTION + install_on_disk; núcleo ✅ ADR-0086 |
| NeuralFS disco | #422 | mount/GPT ✅; evidência Onda 1 | USB power-loss ▶️ AWAITING_HW |
| GPU Direct | #423 | ❌ **SKIP** (hairpin/ACS bloqueia em notebook; GDS é NVLink-only — ADR-0087 §4) | caminho prático: NVMe→DRAM (PRP) → CE (DRAM→VRAM) |
| OTA e2e (A2 smoke) | #308/#417–423 | ✅ **comunicação validada** (SESSION_252 §10): Jarbas sobe + GET /UPDATE.MANIFEST 200 + GET /KERNEL.BIN 200 no serve_update.py | download 17MB com tamanho exato; **hash_mismatch = bug no sha256 do guest** (0x80 do padding fora do bloco para len%64≠0) — download estava íntegro; fix: padding inline correto + vetores FIPS (SESSION_252 §11) |
| LAN/RX/DHCP/VirtIO-net | #73, #117–120 | ✅ L3.5–L5 SESSION_149/150; #117 polish / #251–252 timer | base LAN |
| `/fetch` + Update HTTP | #121, #308a/b | ✅ HTTP path SESSION_152; reboot A/B residual | net_bridge |
| WiFi ath10k QCA6174 | Note1050 / #407b | A0–A3 código ✅ SESSION_161; runtime Note AWAITING | pista ativa; iwlwifi secondary |
| WiFi FW-MAC / iwlwifi | #407–409, WifiAgent | S0+prepS1 ✅ SESSION_159; ALIVE secondary | #408 ≠ SoftMAC ACK clássico |
| TLS / model-fetch e2e | #123, #134, AirLLM Net | smoke ✅ + PKI hybrid SESSION_158; CertVerify/FAT residual; `airllm-net` PARTIAL | lan ✅ |

---

## Legenda dos Status

| Marca | Significado | Exemplo |
|---|---|---|
| ✅ Block N | Implementado no bloco N da chain MVP | ✅ Block 2 |
| 🟡 Onda N / ADR | Agendado com destino explícito (não Sprint fantasma) | 🟡 Onda 7 |
| 🔴 Bloqueado | Bloqueado por dependência externa (ex: wifi RF / soft-float) | 🔴 `depends_on: wifi` |
| ⏳ Pós-MVP / defer gate | Adiado com motivo; fora do gate v2.0.0 | ⏳ defer |
| ▶️ AWAITING_HW | Código+log AWAITING; falta evidência HW real | ▶️ AWAITING_HW |
| 💰 Sponsor | Requer hardware/parceria | 💰 Sponsor |
| ❌ Descartado | Não será feito, com motivo | ❌ Descartado |
| 🔄 Fundido | Absorvido por item maior | 🔄 Fundido |
| 🟢 v2.0 Sprint N | Implementado no Sprint N da v2.0 | 🟢 v2.0 Sprint 106-5 |
| tag `depends_on: lan` | Liberado (L3.5–L5 + NetFs peer PASS) | TLS/e2e restantes ≠ RX=0 |
| tag `depends_on: wifi` | Radio iwlwifi FW-MAC — **não** “Pós B-01” | #407–409, WifiAgent; SESSION_154 |

---

## Seção 1 — Master Registry (Inventário Completo)

### 1.1. The Agency — HW Agents + User Agents (IDEA #277)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 277a | HwRegistry: cada PCI/USB vira HwAgent com capabilities | ✅ SESSION_134 | v1.8.5 | LLM pergunta "o que tem de HW" → ativa agentes |
| 277b | Agency: AGENT.md + seed → SKILL.md pipeline (ADR-0051) | ✅ SESSION_221 | v1.9.11 | AGENCY_SEEDS removido (dead code). skills/agents/*/SKILL.md é a fonte da verdade. 41 agents carregados via include_str! |
| 277c | LLM-aware hardware activation por intent | 🟡 ADR-0041 HalOffer / PnP | pós-PnP | SESSION_143: VIABLE; intent net → `depends_on: lan` |

### 1.2. GGUF Format Support (IDEA #278)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 278a | Loader GGUF mínimo para kernels no_std (~500 LOC) | ✅ MVP; ▶️ AIRLLM-DMA SESSION_148 | ADR-0046 | soft prefetch; DMA/stream/K-quant AWAITING |
| 278b | .bitnet v3: header extensível com metadata | ⏳ defer gate | cortex / ADR-0012 | Alternativa leve; não bloqueia gate |

### 1.3. SmileyOS Patterns (IDEA #279)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 279a | Shell com 40+ comandos (ls, cat, ps, uptime, theme) | ⏳ defer gate | ADR-0047-HMI | SESSION_143: fora do gate; shell mínima já existe |
| 279b | Sistema de temas (5+ cores, hot-swap) | ⏳ defer gate | ADR-0047-HMI | Escopo UX largo; ADR-0052 anti-massa |
| 279c | Filesystem proprio com permissoes | 🔄 → #422 NeuralFS | ADR-0040 | Fundido em NeuralFS |
| 279d | Compositor multi-window (dock, menus, drag) | 🟡 PARTIAL ADR-0047-HMI | Onda 4 cauda | jarbas compositor parcial N5 |
| 279e | v86 browser demo (WebAssembly x86 emulator) | ⏳ defer gate | — | Fora do gate v2.0.0 |
| 279f | App SDK via trait + registry (JA TEMOS!) | ✅ Confirmado | — | Nosso Agent trait + AgentRegistry validado |

### 1.4. Ecosystem Batch 3 — 12 Repos Portados (IDEA #280)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 280a | redox-os/redox: SchemeHandler trait (scheme.rs) | ✅ v0.59.2 | v0.59.2 | Namespace I/O: gpu://, usb:// |
| 280b | theseus-os/Theseus: TypedAgent<Boot\|Running\|Faulted> (state.rs) | ✅ v0.59.2 | v0.59.2 | Type-safe lifecycle transitions |
| 280c | embassy-rs/embassy: TimerWheel 64-slot (timer_wheel.rs) | ✅ v0.59.2 | v0.59.2 | Agendamento eficiente de wake-ups |
| 280d | openai/swarm: Handoff enum (SwitchTo/Escalate/Delegate) | ✅ v0.59.2 | v0.59.2 | Agent handoff protocol |
| 280e | tock/tock: Register<T> + RegisterField (mmio.rs) | ✅ v0.59.2 | v0.59.2 | Typed MMIO sem unsafe manual |
| 280f | raga-ai-hub/Catalyst: Span tracer 256-entry (tracer.rs) | ✅ v0.59.2 | v0.59.2 | Tracing de spans de agentes |
| 280g | kyegomez/swarms: Orchestrator decompose+assign | ✅ v0.59.2 | v0.59.2 | Task decomposition por keywords |
| 280h | TransformerOptimus/SuperAGI: SkillScore scoring | ✅ v0.59.2 | v0.59.2 | Skill performance ranking |
| 280i | VRSEN/agency-swarm: SpecialistAgent (214 data-driven) | ✅ SESSION_134 | v1.8.5 | AGENT.md + seed; não mais 147 hardcoded |
| 280j | browser-use: HwRegistry device tree (ja tinhamos!) | ✅ Confirmado | v0.59.1 | HW context para LLM |
| 280k | micro/go-micro: endpoints discovery (ja tinhamos!) | ✅ Confirmado | v0.55.0 | AgentManifest extensivel |
| 280l | pydantic-ai: SkillManifest derive macro (conceitual) | ⏳ defer gate | ADR-0052 | Manifests manuais bastam; nice-to-have |
| 280m | redox-os/redoxfs: Data/metadata checksums (verify) | ✅ v1.9.9 TEST | v1.9.9 | F4 NeuralFS: CRC32C dos dados no LeafValue do inode (bytes 22..26) + `verify_file` streaming (redoxfs verify-style); read_file recusa `data crc mismatch`. Legado (crc==0) skip. |
| 280n | redox-os/redoxfs: per-block checksum tree | ✅ v1.9.9 TEST | v1.9.9 | F4b NeuralFS: `ItemType::Checksum` (0x05) — `write_file` grava CRC32C da página 4096B (padding incluso) por bloco `(ino, Checksum, bloco)`; `read_range` verifica página a página (recusa `block crc mismatch`) — bit-flip detectado no streaming AirLLM sem reler o arquivo. Legado skip. `checksum_tree_root`=0 (items na mesma b-tree, não root separado) |

### 1.6. VFS + MHI Bridge (IDEA #281)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 281a | VfsRegistry: mount table + resolve + lookup | ✅ v0.62.0 | v0.62.0 | Foundation para todo FS |
| 281b | VfsNode: arvore de diretorios com FileMode | ✅ v0.62.0 | v0.62.0 | Mount points + virtual files |
| 281c | Path utils: canonicalize, split, join, parent | ✅ v0.62.0 | v0.62.0 | Processamento de paths |
| 281d | MHI ARC-style suggest_tier (ZFS-inspired) | ✅ v0.62.0 | v0.62.0 | MFU→Dram, MRU→Nvme, cold→Hdd |

### 1.7. Storage Agents (IDEA #282)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 282a | FilesystemAgent trait + VFS bridge | ✅ v0.62.1 | v0.62.1 | Interface padrao FS agents |
| 282b | AtaAgent: /mnt/hdd/ + block R/W | ✅ v0.62.1 | v0.62.1 | ATA via DriverAgent |
| 282c | DevFsAgent: /dev/pci/ + NIC + USB + mem | ✅ v0.62.1 | v0.62.1 | Hardware como arquivos |
| 282d | ProcFsAgent: /proc/agent/mem/uptime/cpu | ✅ v0.62.1 | v0.62.1 | Sistema como arquivos |
| 282e | InferenceFsAgent: /inference/ com LLM | ✅ SESSION_144 (já wired `fs/`) | ADR-0040 | register_fs_agent |
| 282f | HermesFsAgent: /chat/send + /chat/history | ✅ SESSION_144 (já wired `/chat/`) | ADR-0040 | hermes_fs_agent |
| 282g | RamFsAgent: /mnt/ram/ cache DRAM | ✅ SESSION_144 (já wired) | ADR-0040 | ram_fs_agent |
| 282h | Auto tier migration via MhiScheduler | ⏳ defer (sem auto-migrate MHI) | ADR-0040 | SESSION_144 |

### 1.8. Desktop Cube (IDEA #283)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 283a | Workspace Cube 3D com rotação via GPU (VirtIO-GPU) | ⏳ defer gate / 💰 | ADR-0047-HMI | 3D baixo vs GPU compute golden |
| 283b | Transição crossfade entre workspaces (fallback sem GPU) | 🟡 Onda 4/HMI cauda | ADR-0047-HMI | ~100 LOC se compositor estável |

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 1 | xHCI controller mínimo (<500 LOC, BAR0, port status) | ✅ k_nano xHCI | USB-MSC/UAC | SESSION_143 STALE→feito; residual trust/UAC |
| 2 | `identify_device()` → VID/PID/class | ✅ PCI/USB probe | — | SESSION_143; path parcial via scan |
| 3 | Neural Cortex classify (MLP 7→5: allow/deny/learn/no_intent/suspect) | ✅ Sprint 25 | Sprint 25 | Implementado como `cortex::Cortex::think()` com 12 intenções. Substitui INTENT_MLP antigo (16→8→3). |
| 4 | Trust Cache (TrustEntry, TrustTable, trust-once-use-always) | 🔄 Fundido no Block 5 | Sprint 22 | TrustCache do MVP (Block 5) é versão simplificada. |
| 5 | Trust Cache: regra de 5 situações (auto-ON, conhecido, novo, rejeitado, desconhecido) | 🔄 Fundido no Block 5 | Sprint 22 | Incorporado à TrustTable do MVP. |
| 6 | Trust Cache: persistência no SFS (`/system/trust/usb.tbl`) | ✅ SESSION_145 MVP | — | `usb_trust.rs` + NeuralFS `system/trust/usb.tbl` |
| 7 | Trust Cache: revogação ("não confio mais") | 🔄 Fundido no Block 5 | Sprint 22 | `trust deny <skill>` no MVP. |
| 8 | WASM skill dispatch para protocolos USB | ⏳ defer gate | pesquisa | WASM SFI existe; USB-via-WASM = pesquisa |
| 9 | Nível 1 — HW Detection (xHCI mínimo, sem IA) | ✅ xHCI | — | SESSION_143 STALE→feito |
| 10 | Nível 2 — Device Classification (MLP 7→5) | 🔄 → Cortex/Trust | — | Cobertura parcial por intents |
| 11 | Nível 3 — Dynamic Interface Creation (WASM) | ⏳ defer gate | pesquisa | Mesmo que #8 |
| 12 | USB flow: dispositivo desconhecido → porta desabilitada | ✅ SESSION_145 (`disable_port` + Deny) | — | `USB_TRUST_ENFORCE=1` |
| 13 | USB flow: trust-once → segunda conexão auto-ON | ✅ SESSION_145 (`allow` + usb.tbl) | — | reload no mount NeuralFS |
| 14 | USB flow: usuário precisa inferir intenção (nada automático) | ✅ princípio + Observe default | — | HITL = `allow()` / CONFIG |
| 15 | "Zero autorun, zero superfície de ataque USB" | ✅ parcial SESSION_145 | — | boot-once MSC; enforce Deny; EP0 VID ⏳ |

### 1.2. SMP / APIC / Multicore

> **ADR canônica: [ADR-0055](../architecture/0055-smp-revision.md)** (2026-07-18).  
> Itens abaixo com ✅ histórico que não estão wired no boot atual → **fazendo** até evidência.

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 16 | APIC Local (LAPIC) init no BSP | ✅ ADR-0055 / Block 1 | Sprint 18 | Implementado: SVR, TPR, timer masked. |
| 17 | IOAPIC init (roteamento IRQ externo) | ✅ ADR-0055 / Block 1 | Sprint 18 | Implementado: timer→vec32, keyboard→vec33. |
| 18 | x2APIC mode (MSR-based, sem MMIO) | ✅ ADR-0055 | Sprint 81 | x2APIC enable no path APIC. |
| 19 | MADT parsing (ACPI → LAPIC list) | ✅ ADR-0055 / Block 1 | Sprint 18 | type 0/1/2 MADT. |
| 20 | CPUID leaf 0x1A (P-core / E-core detection) | 🟡 ADR-0055 | SESSION_141 | CorePools detect; hybrid só HW Intel. |
| 21 | CPUID leaf 0x0B (Extended Topology) | 🟡 ADR-0055 | SESSION_141 | Usado em assign; HT refine residual. |
| 22 | CorePools / ComputePools (P→Ring0/1, E→Ring2) | ✅ ADR-0055 | SESSION_141 | Log TCG `r0=1 r1=1`; hybrid HW pending. |
| 23 | Algoritmo `assign_cores()` — P/E-aware + N+1 + fallback | ✅ ADR-0055 | SESSION_141 | Wired em `corepools.rs`. |
| 24 | PerCpu struct (core_id, lapic_id, core_type, ring, stack, queue) | ✅ ADR-0055 | SESSION_141 | BSP+AP path. |
| 25 | GS.base segment register per-core | 🟡 ADR-0055 | SESSION_141 | BSP; APs parcial. |
| 26 | INIT-SIPI-SIPI via LAPIC ICR | ✅ ADR-0055 | SESSION_141 | TCG APs=1; WHPX gated OFF. |
| 27 | Trampoline assembly (16→32→PAE→64→Rust) | ✅ ADR-0055 | SESSION_141 | k_nano global_asm; 286 bytes. |
| 28 | AP startup IPI (BSP → INIT → SIPI → SIPI) | ✅ ADR-0055 | SESSION_141 | FeatureGate.allow_smp. |
| 29 | Stack separada por core (64 KB cada) | ✅ ADR-0055 | SESSION_141 | Com PerCpu AP. |
| 30 | Regras de escalonamento por pool | ✅ ADR-0055 | SESSION_141 | affinity_ring + poll R0→R2. |
| 31 | "Se só E-cores, tudo roda em E-cores mais lentos" | ✅ ADR-0055 | SESSION_141 | assign_cores fallback. |
| 32 | "Se 1 core apenas (QEMU -smp 1), tudo no mesmo core" | ✅ ADR-0055 | SESSION_141 | FeatureGate + MADT. |
| 33 | "HT: 1 thread por core físico no Ring 0/1, restante no Ring 2" | 🟡 ADR-0055 | SESSION_141 | Política; refine leaf 0x0B. |
| 34 | `acpi` crate para parser MADT/PPTT | ✅ ADR-0055 | SESSION_141 | Parser mínimo + BootInfo.rsdp. |
| 35 | `raw-cpuid` crate para detecção de features | ✅ ADR-0055 | SESSION_141 | CPUID inline `platform_probe`. |
| 36 | SPSC ring lockless (bbqueue) | 🟡 ADR-0055 | SESSION_141 | ap_work queue + barrier. |
| 37 | `#[repr(align(line_size))]` cross-core | 🟡 ADR-0055 | SESSION_141 | CacheTopology.line_size exposto. |
| 38 | IPI handler registrável | ✅ ADR-0055 | SESSION_141 | IPI wake matmul/APs. |
| 39 | Work-stealing Chase-Lev | ✅ ADR-0055 | SESSION_141 | GLOBAL_POOL + AP steal. |
| 40 | Parallel-for AVX2 matmul | 🟡 ADR-0055 | SESSION_141 | Wired; speedup aceite = HW. |
| 41 | AgentScheduler multicore | 🟡 ADR-0055 | SESSION_141 | Affinity order; steal jobs AP. |
| 42 | Per-CPU slab allocator | 🟡 ADR-0055 | residual | Após APs vivos. |

### 1.3. NPU (AMD XDNA)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 43 | `Npu` struct + `try_init()` via PCI scan | 💰 Sponsor | Sprint 25+ | Requer AMD APU real (XDNA) ou QEMU com NPU virtual. |
| 44 | `Accelerator::XDNA(Npu)` / `Accelerator::Software` enum | 💰 Sponsor | Sprint 25+ | Depende de #43. |
| 45 | Command queue circular + doorbell write | 💰 Sponsor | Sprint 25+ | Requer documentação do XDNA. |
| 46 | Overlay loading via MMIO | 💰 Sponsor | Sprint 25+ | Vendor-specific. AMD Vitis AI compiler. |
| 47 | MSI-X interrupt registration | 💰 Sponsor | Sprint 25+ | Depende de #43 + IOAPIC/MSI. |
| 48 | Fallback automático: init_npu() → se falha → Software | ✅ Block 4 | Sprint 21 | Se NPU ausente, cai para software. |
| 49 | 3 cenários: QEMU / APU sem driver / APU com driver | 🟡 Block 4 | Sprint 21 | Lógica de fallback documentada. |
| 50 | Cadeia de programação: Modelo → Overlay → DRAM | 💰 Sponsor | Sprint 25+ | Requer toolchain AMD Vitis. |
| 51 | Ring 0 MLP NÃO precisa do NPU — 20 pesos rodam em 1 core | ✅ Block 4 | Sprint 21 | Premissa arquitetural adotada. |
| 52 | Caminho de migração: QEMU → APU f1 → f2 → f3 | 💰 Sponsor | Sprint 25+ | Depende de patrocínio/hardware. |

### 1.4. AI-Driven Hardware Detection

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 53 | `HardwareInventory::collect()` | ✅ Block 4 | Sprint 21 | Coração do Block 4. |
| 54 | `cortex::infer_architecture(&inventory)` | ✅ Block 4 | Sprint 21 | MLP 512→256→64→9 ternário. |
| 55 | MLP 512→256→64→9 ternário (~37 KB, pesos embutidos) | ✅ Block 4 | Sprint 21 | ~150k pesos ternários em .rodata. |
| 56 | `SystemArchitecture` struct (12 saídas categóricas) | ✅ Block 4 | Sprint 21 | ring0, ring1, ring2, heap, sfs, trust, power, tiers. |
| 57 | Boot flow adaptativo: collect → infer → init | ✅ Block 4 | Sprint 21 | Substitui boot sequence fixo atual. |
| 58 | Treinamento offline do MLP (10k hardware profiles) | ⏳ Pós-MVP | Sprint 21+ | Pesos iniciais heurísticos. Treinamento real depois. |
| 59 | Atualização do MLP via skill WASM | ⏳ Pós-MVP | Sprint 25+ | Requer WASM embedder. |
| 60 | Fallback seguro: MLP absurdo → valores default clamped | ✅ Block 4 | Sprint 21 | Heap mínimo 64 KB, ring0 sempre fallback software. |
| 61 | "MLP cabe no kernel — 37 KB no .rodata" | ✅ Block 4 | Sprint 21 | Premissa verificada. |
| 62 | "Inferência é rápida — µs" | ✅ Block 4 | Sprint 21 | MLP ternário em 1 core = microssegundos. |

### 1.5. Memory Hierarchy Index (MHI)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 63 | `struct MemoryTier { device, kind, capacity, bandwidth, latency }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP (cross-ref). |
| 64 | `struct MemoryHierarchy { tiers: Vec<MemoryTier> }` ordenado | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 65 | `enum AllocTier { Dram, Vram, Nvme, Hdd }` | ✅ Block 4 | Sprint 21 | Adicionado ao MVP. |
| 66 | `fn alloc_by_tier(tier, size) -> Option<PhysAddr>` | ✅ Block 4 | Sprint 21 | Dram implementado. Vram/Nvme → None com diagnóstico. |
| 67 | `AllocTier::Vram` → alocar no BAR da GPU | ✅ hook SESSION_146 | #420 AWAITING se init falha | `register_vram_allocator` → `vram_alloc` |
| 68 | `AllocTier::Nvme` → alocar no NVMe via SFS | ⏳ defer gate | após #420 | NVMe driver existe; tier pleno defer |
| 69 | `AllocTier::Hdd` → cold storage | ⏳ defer gate | após #420 | ATA ok; tier policy defer |
| 70 | MLP saídas: heap_tier, tensor_tier, kv_cache_tier, sfs_active_tier | ✅ Block 4 | Sprint 21 | 4 tiers de saída no MLP do MVP. |
| 71 | MLP saídas opcionais: sfs_cold_tier, tensor_swap_tier, skill_heap_tier | 🟡 Block 4 | Sprint 21 | Campos opcionais no SystemArchitecture. |
| 72 | Exemplo real: notebook i5 + GTX 1050 + NVMe + HDD | ✅ Doc | README | Caso de uso documentado. |
| 73 | Exemplo real: Xeon 6900 (1 TB RAM, NVMe RAID) | ✅ Doc | ADR-0015 | Caso de uso documentado. |
| 74 | Exemplo real: AMD APU Strix Point (unified memory) | ✅ Doc | ADR-0015 | Caso de uso documentado. |

### 1.6. Periféricos (PCI, NVMe, VirtIO)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 68 | PCI config space access (CF8/CFC) | ✅ Block 1 | Sprint 18 | Implementado: read_config_dword/word, BARs. |
| 69 | PCI scan: vendor, device, class, subclass, BARs | ✅ Block 1 | Sprint 18 | Implementado: 256 busses, 32 devices, BAR0-5. |
| 70 | PCI bridges (hierarquia de barramento) | 🟡 Block 1 | Sprint 18 | Suporte básico: multi-função em bridges PCI-PCI. |
| 71 | NVMe driver (PCI Class 01.08) | ✅ parcial `disk_agent/nvme` | Onda 5 DMA | Driver existe; DMA maduro = Onda 5 |
| 72 | VirtIO-blk (PCI 1AF4:1001) | ⏳ defer gate | — | ATA/NVMe cobrem necessidade atual |
| 73 | VirtIO-net (PCI 1AF4:1041) | 🟡 polish / e1000 L3.5 ✅ | LAN | SESSION_149: e1000 TX 0x3800 + RX PASS; VirtIO-net opcional |
| 73b | **VirtIO-GPU (PCI 1AF4:1050)** | 🟡 Onda 7 display quirk | — | Driver ~95%; GET_DISPLAY_INFO residual; #74 fundido aqui |
| 166 | **Multi-mode Trust** | ✅ v0.49.0 | Sprint 49 | PermissionMode enum. `trust_allow_with_mode()`. |
| 176 | **Ed25519 Cryptographic Identity** | ✅ v0.50.0 | Sprint 50 | `identity.rs`, `CapabilityToken` enum, `verify_signature()` bare-metal. |
| 198 | **Boot-time security policy** | ✅ v0.49.0 | Sprint 49 | `TrustCache::load_boot_policy()` seta `PolicyState::Contain` no boot. |
| 256 | **Path Confinement** | ✅ v0.49.0 | Sprint 49 | `PathRule` + `check_path()` no TrustCache. |
| 257 | **Mask Secrets** | ✅ v0.49.0 | Sprint 49 | `mask_secrets()` — 12 padrões, substitui por "*" |
| 258 | **Graduated Enforcement** | ✅ v0.49.0 | Sprint 49 | `PolicyState` máquina: Observe→Warn→Contain→Enforce. |
| 259 | **Posture-Aware Alerting** | ✅ v0.49.0 | Sprint 49 | `posture_check()` verifica hardware antes de executar skill. |
| 260 | **Event→Detector→Response Pipeline** | ✅ v0.50.0 | Sprint 50 | 5 detectores (PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly) + correlação. |
| 74 | VirtIO-gpu (PCI 1AF4:1050) | 🔄 → #73b | — | SESSION_143: duplicata; residual em #73b |
| 75 | Intel HDA audio | ✅ feito | Sprint Sound / 101 | ✅ `audio/hda.rs` no monólito. Ver ADR-0045. (histórico: “nenhuma skill no MVP”) |
| 76 | Sem kernel thread de hotplug | ✅ Princípio | — | Diretriz adotada. |
| 77 | Sem sysfs genérico | ✅ Princípio | — | Diretriz adotada. |
| 78 | Cada driver é módulo autocontido, sem trait Device universal | ✅ Princípio | — | Diretriz adotada. |

### 1.7. Áudio/Vídeo

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 79 | UEFI framebuffer (BGRA32 writer) | ✅ jarbas/fb N5 | SESSION_115 | SESSION_143 STALE: probe_uefi_framebuffer |
| 80 | Font rendering para alta resolução | ✅ console/fb | — | Depende #79; path atual OK |
| 81 | VirtIO-GPU 2D/3D acelerado | ⏳ defer gate | #73b | 2D parcial; 3D fora do gate |
| 82 | Tensor visualization no framebuffer | 🟡 ADR-0047-HMI PARTIAL | embed_viz | PoC H2/H5; não desktop 3D |
| 83 | Intel HDA audio driver — Áudio via PCI HDA controller. Essencial para TTS/STT do JARVIS sem depender de USB. | ✅ feito | Sprint Sound / 101 | ✅ SD0 capture + SD1 playback. ADR-0045. |
| 84 | Áudio via USB (UAC) — USB Audio Class para fones/microfone USB. Alternativa ao HDA quando não disponível. | ▶️ AWAITING_HW SESSION_145 | ADR-0045 | Parse+probe+USB-TRUST; iso DMA → `[UAC-HW] VERDICT=AWAITING_REAL_HW` |

### 1.8. Princípios Arquiteturais

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 85 | Mínimo viável: só implementar driver se requisito para skill WASM ou boot | ✅ Princípio | — | Guia todas as decisões do MVP. |
| 86 | VirtIO first: QEMU antes de hardware real | ✅ Princípio | — | Diretriz adotada. |
| 87 | Polling > Interrupção para dispositivos de baixa taxa | ✅ Princípio | — | Adotado. |
| 88 | Sem HAL genérica — cada driver é módulo autocontido | ✅ Princípio | — | Adotado. |
| 89 | "O usuário precisa inferir" — nenhum dispositivo tem autoridade implícita | ✅ Princípio | — | Fundamento do zero-trust. |
| 90 | Trust-once-use-always usabilidade | ✅ Block 5 | Sprint 22 | TrustCache implementa. |

### 1.9. Roadmap Original — Memória

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 91 | Bitmap Frame Allocator | ✅ Block 0 | Sprint 11 | Já implementado. |
| 92 | Huge Pages (2 MiB) | ⏳ defer gate (pós Onda 6) | — | Otimização modelos; fora do gate |
| 93 | Huge Pages (1 GiB) | ⏳ defer gate | — | Depende de #92 |
| 94 | Slab Allocator | ✅ Block 2 | Sprint 19 | Essencial para heap dinâmico. |

### 1.10. Roadmap Original — Kernel Abstraction

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 95 | Async Neural Executor | ✅ Block 0 | Sprint 12 | Já implementado. |
| 96 | Agent Scheduler (round-robin) | ✅ AgentScheduler boot | — | SESSION_143 STALE→feito |
| 97 | Budget de execução (tokens_consumed) | ⏳ defer gate | polish | Scheduler OK; budget = polish |
| 98 | MLP decide prioridade no scheduler | ⏳ defer gate | polish | Tipagem/MLP priority fora do gate |

### 1.11. Roadmap Original — EventBus

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 99 | EventBus + CapabilityToken | ✅ Block 0 | Sprint 13 | Já implementado. |
| 100 | Topic enum completo | ⏳ defer gate | polish | Strings funcionam |
| 101 | ML-based routing (EventBus consulta Intent Router) | ⏳ defer gate | polish | BTreeMap resolve; LatentBus parcial |

### 1.12. Roadmap Original — Skill Registry

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 102 | Skill trait + MCP + Registry | ✅ Block 0 | Sprint 14 | Já implementado. |
| 103 | WASM embedder (wasmi) | ✅ `hermes/wasmi_rt` | ADR-0059 | SESSION_165: **wasmi crate** (v0.47, no_std, fuel) — runtime próprio legado deprecado |
| 104 | Linear memory pool (256 KB por skill) | ✅ MemoryPool WASM | — | Path custom com #103 |

### 1.13. Roadmap Original — Cognitive Runtime

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 105 | Intent Planner (sequência de SkillCommands) | ✅ scaffold cognitive | Sprint 95 | SESSION_143: scaffold ≠ produção plena |
| 106 | Success Engine (feedback loop, ajuste online de pesos) | ⏳ defer gate | pesquisa | Scaffold; treino online real fora do gate |
| 107 | Neural Cache (lookup table 50 ns em Huge Pages) | ✅ scaffold / NeuralCache | Sprint 95 | Path parcial |
| 108 | MatMul-free LM (RWKV/Mamba/ternary pooling) | ⏳ defer gate | stub | `loaded: false`; não priorizar |

### 1.14. Roadmap Original — Timeline

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 109 | Sprint 16: Slab Allocator | 🔄 Remapeado | Block 2 S19 | Movido após PCI+APIC. |
| 110 | Sprint 17: Agent Scheduler | 🔄 Remapeado | Sprint 24+ | Executor cooperativo é suficiente. |
| 111 | Sprint 18+: Cognitive Runtime | 🔄 Remapeado | Fase 6 | MVP primeiro. |

### 1.15. Outras Ideias

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 112 | Bootável em hardware x86 real (UEFI) | ✅ MVP | Sprint 22 | Critério de aceite do MVP. |
| 113 | Nome "Hermes" como identidade do MVP | ✅ Adotado | — | README + ADR-0015 usam. |
| 114 | Chat loop estilo Hermes Agent (Nous Research) | ✅ Block 3 | Sprint 20 | Inspiração direta. |
| 115 | Sponsor: NPU AMD XDNA requer parceria | 💰 Sponsor | Sprint 25+ | Sem hardware, sem implementação. |
| 116 | Sponsor: port para ARM/RISC-V | 💰 Sponsor | Futuro | Fora do escopo x86-64. |

### 1.16. Rede/Network Stack

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 117 | VirtIO-net driver (PCI) sobre `virtio-drivers` crate | ✅ parcial / 🟡 polish | LAN ✅ e1000 gate | Driver manual existe; polish RX opcional |
| 118 | smoltcp TCP/IP stack integration | ✅ L5 HTTP | LAN ✅ | SESSION_150: TCP HTTP 301 via e1000 |
| 119 | DNS resolver (smoltcp `dns` feature) | ✅ raw e1000 | LAN ✅ | SESSION_150: DNS raw (bypass demux); skip_dns_name |
| 120 | HTTP GET/POST client minimal (~200 LOC) | ✅ L5 smoke | LAN ✅ | SESSION_150: GET / → 301 google |
| 121 | Hermes `/fetch` command | ✅ hostname DNS + HTTP | SESSION_152 | `resolve_and_http_get` / net_bridge |
| 122 | Skill manifest field `requires_network: bool` | ✅ / 🟡 polish enforce | — | Campo existe; enforce CapGate quando RX=0 |
| 123 | TLS 1.3 client (`embedded-tls` 0.19) | ✅ smoke + PKI hybrid | ADR-0016 N4 | SESSION_158: pins+TOFU; `root_learn`→`root_pin`; CertVerify/FAT residual |
| 124 | Wi-Fi / Ethernet (e1000/RTL8139 para HW real) | ✅ e1000 L3.5 / 🟡 wifi | `depends_on: wifi` | e1000 TX 0x3800 ✅ SESSION_149; WiFi aberto |
| 250 | **Comando `/ping <ip>`** — ICMP Echo Request via e1000 | ✅ Block 6 | Sprint 23 | `net::ping()` usa `icmp_echo_request` + `parse_icmp_reply`. |
| 251 | **DHCP timer-based wait** — refatorar spin loops para `hlt()` com timeout por timer ticks | 🟡 polish | LAN ✅ | Static user OK; timer polish opcional |
| 252 | **ARP não-bloqueante** — timeout com retry usando timer ticks em vez de spin loop | 🟡 polish | LAN ✅ | prove_rx multi-ARP OK; polish opcional |
| 253 | **e1000 TDT protocol fix** — `send()` escrevia REG_TDT = idx (== TDH), hardware via ring vazio. Corrigido: TDT = (idx+1) % NUM_DESC | ✅ Block 6 | Sprint 23 | Causa raiz TPT=0. Descritor lido mas pacote não enviado. |
| 254 | **e1000 NUM_DESC 32→48** — 82540EM requer mínimo 48 descritores RX | ✅ Block 6 | Sprint 23 | Linux e1000 driver docs: "48-256 for 82542 and 82543-based adapters". |
| 255 | **Arquitetura Neural de Rede** — init_driver_network() → HW_NET_E1000 EventBus → network_bootstrap() → network_health_daemon() → skill routing | ✅ Block 6 | Sprint 23 | Hardware detection first, IA decide routing. |

### 1.20. Tier 3 Security Patterns — InnerWarden, ai-jail, vexfs, Chisel

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 256 | **Path Confinement para Skills** — SkillRegistry verifica allowlist de paths por token antes de operar (Chisel + ai-jail) | 🟡 Sprint 24 | Sprint 24 | ~60 LOC. TrustCache já faz validação similar. |
| 257 | **Mask Secrets** — TrustCache/SkillRegistry mascara paths/env vars sensíveis antes de expor para skills (ai-jail `--mask`) | 🟡 Sprint 24 | Sprint 24 | ~50 LOC. Substitui padrões por "[REDACTED]". |
| 258 | **Graduated Enforcement** — PolicyState machine: Observe→Warn→Contain→Enforce (InnerWarden) | 🟡 Sprint 24 | Sprint 24 | ~80 LOC. Adiciona estado ao SkillRegistry. |
| 259 | **Posture-Aware Alerting** — Skills verificam estado do hardware antes de agir (InnerWarden) | 🟡 Sprint 24 | Sprint 24 | ~40 LOC. Se link down → não configura rede. |
| 260 | **Event→Detector→Response Pipeline** — EventBus → Detector stateful → Correlation → Response Skill (InnerWarden core) | 🟡 Sprint 25 | Sprint 25 | ~200 LOC. 5 detectores iniciais (PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly). Novo crate `security-pipeline`. |
| 261 | **Decision Review + Human Escalation** — Detector com baixa confiança publica NEEDS_REVIEW com timeout (InnerWarden) | 🟡 Sprint 25 | Sprint 25 | ~120 LOC. Timeout auto-resolve. High severity nunca auto-resolve. |
| 262 | **Hash Chain Audit Trail** — EventLog com SHA-256 chain: cada evento contém hash do anterior (InnerWarden) | 🟡 Sprint 25 | Sprint 25 | ~60 LOC. Extensão do #231. verify_chain() → bool. |
| 263 | **Knowledge Graph para Eventos de Segurança** — Grafo em memória: 6 node types, ~20 relations (InnerWarden knowledge graph) | 🟡 Sprint 26 | Sprint 26 | ~400 LOC. Node types: Process, NetworkEndpoint, File, Skill, Hardware, User. |
| 264 | **Cross-Layer Correlation Rules** — Regras multi-estágio: ARP Spoof→Port Scan→Data Exfil (InnerWarden 69 regras) | 🟡 Sprint 27 | Sprint 27 | ~300 LOC. 5 regras iniciais. Risco de falso positivo. |
| 265 | **Filesystem como Vector Search** — Operações de arquivo expõem vector search via xattr (vexfs) | ⏳ Pós-MVP | Sprint 28+ | Depende de SFS implementado. |
| 266 | **Multi-dialect Vector API** — API server compatível com ChromaDB/Qdrant (vexfs) | ⏳ Pós-MVP | Sprint 28+ | Depende de MemPalace ou SFS com embeddings. |
| 267 | **OverlayFS Copy-on-Write** — Writes de agentes vão para overlay separado (ai-jail) | ⏳ Pós-MVP | Sprint 28+ | Depende de VFS implementada. |

### 1.17. Documentação e ADRs

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 125 | ADR-0016: Network Strategy | ✅ Documentado | Sprint 20 | Decisão arquitetural sobre quando/como implementar rede. |

### 1.18. Neural Cortex — BitNet LLM (Novo Plano Diretor)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 126 | **Transformer Engine** — Attention (`QK^T/√d`), causal mask, softmax, FFN (SiLU), residual | ✅ Sprint 26 / N3 | — | cortex BitNet path |
| 127 | **Tokenizer character-level** — ASCII 32-126 + `<BOS>/<EOS>/<PAD>` | ✅ + BPE HF | — | CHAR + BPE loaders |
| 128 | **Autoregressive generation** — loop `tokenize → forward → sample → next` | ✅ N3 | — | generate path; soft-float fluency residual |
| 129 | **Model format `.bitnet`** — binary spec com magic, header, packed ternary weights | ✅ | — | Formato em uso |
| 130 | **Model loader** — `include_bytes!` + `allocate_contiguous()` → `PackedTernaryTensor` | ✅ | — | FAT/QEMU loader |
| 131 | **Micro-model TinyStories** (1M params, 4 layers, hidden=128) treinado em Python | ✅ / supersedido por 2B | — | Pipeline provado; 2B LOADED |
| 132 | **Cortex Daemon** — async task que recebe `LLM_REQUEST` → gera → publica resposta | ✅ Sprint 27 / N3 | — | CortexAgent Continuous |
| 133 | **Modelo 1.5B params** (distilado do Llama 3.2 1B → ternário 2-bit, ~375 MB) | ✅ 2B path | — | BitNet 2B LOADED (soft-float) |
| 134 | **Model update via HTTP** — download `.bitnet` → validar hash → hot-swap | 🟡 e2e / PreFlight PARTIAL | AirLLM Net ✅ path | DNS+Range SESSION_152; falta log `/model-fetch` aceite |
| 135 | **LLM decide hardware arch** — substitui `SystemArchitecture::infer()` heurístico | ✅ parcial Trinity/HWEXPERT | — | keyword+R3; não 100% LLM |
| 136 | **LLM decide memory tier** — roteia alocações Dram/Vram/Nvme/Hdd | ❌ deprecado | ADR-0060 | Substituído por política determinística BudgetManager (ADR-0060 A.6). LLM no hot path de alocação é anti-pattern |
| 137 | **LLM classifica USB devices** — Neural Cortex 7→5 allow/deny/learn/no_intent/suspect | 🔄 → #3 / Trust | — | Cobertura parcial |
| 138 | **LLM dispatch skills** — qual skill executar para cada intenção | ✅ Hermes/Trinity | — | Intent routing N4 |
| 139 | **Reflex MLP threshold tuning** — se confiança > 0.9, bypassa LLM | ✅ parcial | — | keyword bypass |
| 140 | **Speculative decoding** — Reflex MLP prediz próximo token, LLM verifica | ✅ n-gram ADR-0047 | SESSION_125 | Spec decode OK; MLP reflex residual |
| 141 | **1.5B model benchmark** — 5-15 tok/s on single x86-64 core (AVX2) | 🟡 soft-float residual | trilha R | Aceite qualidade ≠ tok/s só |

### 1.19. Transformer Engine (Detalhamento Técnico)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 142 | `Attention` struct — q_proj, k_proj, v_proj, o_proj (todos `Linear`) | ✅ | — | cortex |
| 143 | `causal_mask` — triângulo superior -inf, diag/abaixo 0 | ✅ | — | cortex |
| 144 | `softmax` row-wise em cima de `Tensor` | ✅ | — | cortex |
| 145 | `TransformerBlock` — RMSNorm → Attn → residual → RMSNorm → FFN(SiLU) → residual | ✅ | — | cortex |
| 146 | `Transformer` — embed → N×TransformerBlock → RMSNorm → unembed | ✅ | — | cortex |
| 147 | `generate()` — loop: forward → sample → next | ✅ | — | cortex |
| 148 | Sampling: argmax, top-k(3/5/10), temperature | ✅ parcial | — | argmax/constrained; top-k polish |

### 1.20. Success Engine (Ajuste Online)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 149 | Feedback loop — usuário avalia resposta (👍/👎) | ⏳ defer gate | pesquisa | Scaffold cognitive; fora do gate |
| 150 | Ternary weight update — {-1,0,+1} → {-1,0,+1} com probabilidade | ⏳ defer gate | pesquisa | Idem |
| 151 | Experience replay buffer (últimas N interações) | ✅ parcial SleepCycle | — | REPLAY phase; profundidade pesquisa defer |
| 152 | Weight consolidation — export modelo atualizado | ⏳ defer gate | pesquisa | Persistência online fora do gate |

### 1.21. Treinamento (Host-side, Python)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 153 | Train micro BitNet (1M params, TinyStories) → export `.bitnet` | ✅ tools Python | — | Pipeline treino existe |
| 154 | Distil Llama 3.2 1B → ternário → `.bitnet` 1.5B | ✅ 2B path | — | Modelo grande em uso |
| 155 | Pipeline `bitnet.cpp` quantization script | ✅ tools | — | Conversão/export em tools/ |
| 156 | Ferramenta de validação — forward match kernel vs Python | ⏳ defer gate | polish | Útil; fora do crítico |

### 1.22. Self-Optimization / Workflow Learning

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 157 | **Usage Pattern Analyzer** — LLM observa últimas N intenções, detecta workflow do usuário (hora, frequência, recursos) | ✅ scaffold SelfOpt | Sprint 95–96 | SESSION_143: scaffold; produção defer |
| 158 | **Workflow Predictor** — pré-carrega recursos (MHI tiers, scheduler priority) baseado em hora/dia/padrão detectado | ✅ scaffold | — | Idem |
| 159 | **Auto-Skill Generator** — cria skill WASM para tarefa repetitiva detectada (≥3 ocorrências no mesmo workflow) | ✅ Sprint 108 self_evolve | — | SelfEvolveAgent / SkillOpt path |
| 160 | **Dynamic Resource Scaling** — MHI ajusta tiers (Dram/Vram/Nvme) dinamicamente pelo uso real, não só por boot | ⏳ defer gate | Onda 3/5 | Soft-migrate; DMA tiers = #420 |
| 161 | **Self-Optimizing Scheduler** — prioriza agentes conforme workflow detectado (render → GPU agent high prio) | ✅ scaffold | — | OptimizerAgent Continuous |
| 162 | **Workflow Profile** — perfil salvo exportável ("arquiteto", "escritório", "dev") com recursos, skills, prioridades | ⏳ defer gate | — | Fora do gate |
| 163 | **Hardware Config Learning** — `SystemArchitecture` evolui com feedback do usuário (não só heurística de boot) | ✅ parcial HWEXPERT | — | Cards/PnP; evolução plena defer |

---

### 1.23. Crom Ecosystem — Ideas Ported from MrJc01 (75 repos)

| # | Item | Classificação | Sprint | Motivação |
|---|---|---|---|---|
| 164 | **XOR Delta reconstruction** — modo Archive lossless no PackedTernaryTensor; armazena resíduo XOR para round-trip bit-exact | ✅ Imediata | Sprint 24 | ~50 LOC sobre operações bitwise existentes. Permite verificação SHA-256 do output. |
| 165 | **CDC Rabin Fingerprint** — Content-Defined Chunking via rolling hash p/ dividir `.bitnet` models em chunks carregáveis | ✅ Imediata | Sprint 24 | ~80 LOC, rolling hash polinomial. Útil para carregamento sob demanda de modelos grandes. |
| 166 | **Multi-mode Trust** — PermissionMode enum (TotalAccess/AskEveryTime/Scoped) no TrustCache | ✅ v0.49 / parcial | — | PermissionMode existe; polish HITL defer |
| 167 | **TV-DSL Co-processor** — AST determinístico para expressões matemáticas; Hermes chama co-processador para cálculos exatos sem alucinação | 🟡 Baixa | Sprint 27 | ~200 LOC, parser de expr matemática em `no_std` (reusa `libm`). Zero alucinação aritmética — crítico p/ arquiteto (volumetria) e escritório (impostos). |
| 168 | **PonderNet dynamic stop** — Reflex MLP decide quantos ciclos de inferência executar (não fixo) baseado em confiança | 🟡 Baixa | Sprint 27 | ~150 LOC sobre executor existente. Adaptive compute = eficiência energética. |
| 169 | **Codebook Compression (VQ)** — Vector Quantization p/ PackedTernaryTensor; substitui `quantize_to_packed()` por `train_codebook()` + `lookup()` O(1) | 🟠 Média | Sprint 28 | ~300 LOC kernel + script Python treinamento. Crompressor-Neurônio: 97.56% acc com 40.8× compressão. |
| 170 | **KV Cache Codebook** — aplica VQ ao cache de atenção do Transformer Engine; 94.2% redução real (Crompressor-Neurônio Lab06) | 🟠 Média | Sprint 28 | Depende de #126-131 (Transformer Engine pronto). Reduz cache de 2 MB p/ ~120 KB por camada. |
| 171 | **ReAct loop com auto-correção** — NeuralExecutor evolui com fase de verificação: hash de ações recentes, detecção de loop infinito, re-tentativa em erro | 🟠 Média | Sprint 28 | ~300 LOC. Crom-Agente: 40 capacidades, loop ReAct com auto-verificação via lint/test. |
| 172 | **MCP Server support** — EventBus + SkillRegistry evoluem para suportar servidores MCP externos via JSON-RPC 2.0 | 🟠 Média | Sprint 28 | ~400 LOC. Requer parser JSON em `no_std` ou protocolo binário custom. Compatibilidade com ecossistema MCP. |
| 173 | **Codebook LLM finetune** — treinar APENAS o codebook (5.770 params) em vez dos pesos (235K), superando baseline 98.08% vs 97.53% | ⏳ Pós-MVP | Sprint 29+ | Pesquisa: Crompressor-Neurônio Tensor-Vivo Exp2. Success Engine pode usar codebook learning p/ ajuste online. |
| 174 | **Delta branches (speculative decoding)** — branches de inferência paralela com 99.9% economia de memória via XOR delta entre branches | ⏳ Pós-MVP | Sprint 29+ | Crompressor-Neurônio Lab07. Requer scheduler maduro + múltiplos cores. Viabilidade depende de benchmark real. |
| 175 | **Workspace isolation** — per-project config (skills/recursos/trust) isolados por workspace, estilo `.crom/config.json` | ⏳ Pós-MVP | Sprint 29+ | Crom-Agente workspace isolation. Requer SFS (Layer 2) para persistência. |
| 176 | **Ed25519 Cryptographic Identity for TrustCache** — substitui `CapabilityToken(u64)` estático por assinatura Ed25519; Token vira chave pública + assinatura da requisição; Zero-Trust real em nível de kernel | 🟡 Baixa | Sprint 27 | Crom-meueu: identidade criptográfica Ed25519 portada para bare-metal `no_std`. ~300 LOC usando `ed25519-dalek` (sem std) ou implementação custom. Depende de #166 (Multi-mode Trust) como camada de permissão sobre a identidade. |

**ADR-0020:** `docs/architecture/0020-crom-ecosystem-analysis.md` — Análise de viabilidade Rust com código modelo para 9 features (#164-175). Item #176 (Ed25519) adicionado posteriormente.

### 1.24. DiskIntelligenceAgent (IDEA #303 expandido)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 303a | DiskIntelligenceAgent com StorageController trait | ✅ v0.75.1 | v0.75.1 | "Mestre dos discos" — 6 controladoras, 10+ FS probes, MHI, VFS |
| 303b | S.M.A.R.T. monitoring com self-heal alert | ✅ v0.75.2 | v0.75.2 | Detecção precoce de falha de disco |
| 303c | GPT probe + SED/OPAL + EROFS/ReFS | ✅ v0.75.3 | v0.75.3 | Tabela GPT, Self-Encrypting Drives, mais FS |
| 303d | USB-MSC bulk fix + BOT protocol | ✅ v0.75.4 | v0.75.4 | xHCI IOC+ring+ERDP, SCSI INQUIRY/READ/WRITE |
| 303e | NVMe driver (Admin queue + Identify + I/O) | ✅ v0.75.5 | v0.75.5 | PCI 0x01/0x08, PRP1, SQ/CQ doorbell |
| 303f | ARC cache + tier migration | ✅ v0.75.6 | v0.75.6 | 1MB DRAM cache, write-back, MHI update |

### 1.25. AIOS Cross-OS Compatibility + WASM + J.A.R.V.I.S. (IDEA #306-#310)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 306a | **Windows binary compat** — PE32+ loader + syscall translation (NT→AIOS). DLL loader stub, SEH handler, PEB emulação | ✅ parcial PE / ⏳ defer | — | PE loader existe; NT pleno fora do gate |
| 306b | **Linux ELF compat** — ELF x86-64 loader + syscall translation (open/read/write/mmap/clone → agent skills) | ✅ parcial ELF / ⏳ defer | — | ELF loader existe; syscall layer defer |
| 306c | **macOS/iOS Mach-O compat** — Mach-O loader + XNU syscall translation. Desafio: APIs Cocoa/AppKit fechadas | ⏳ defer gate | — | Stub; fora do gate |
| 306d | **Android APK compat** — ART runtime como skill, Binder IPC → agentes. Desafio: framework Java → tradução | ⏳ defer gate | — | Fora do gate |
| 307 | **Syscall-to-Skill Translation Layer** — Camada única: syscall (NT/Linux/XNU) → skill request → agent.response. "abrir /etc/passwd" → DiskAgent.read() | ⏳ defer gate | — | PE/ELF cobrem necessidade atual |
| 308a | **Update/Upgrade Agent** — Dual Kernel Slot A/B no FAT32. Baixa novo kernel via smoltcp HTTP GET, verifica Ed25519 + SHA-256, escreve KERNEL~2, switch BOOTCFG.JSON, reboot | ✅ parcial HTTP | SESSION_152 | `fetch_update` + FNV + slot; Ed25519/reboot residual. **→ ADR-0086 §3 (processo canônico, deprecado 2026-08-05)** |
| 308b | **Update channels** — stable (Sprint), nightly (HEAD), security (hotfix). Channel manifesto via HTTP GET de update-server. Poll 3600s/600s/60s | 🟡 stub poll | SESSION_152 | `poll_channel` → `UPDATE.MANIFEST` host :8080. **→ ADR-0086 §3 (deprecado 2026-08-05)** |
| 308c | **Rollback automático** — BootSelfHealAgent detecta crash pós-update → restaura BOOTCFG.JSON → last_good slot. Três falhas seguidas → rollback forçado | ✅ parcial SelfHeal | — | Path heal existe; update full = lan. **→ ADR-0086 §3 gap U4 (deprecado 2026-08-05)** |
| 309a | **WASM Skill Runtime (wasmi)** — wasmi v0.47+ intérprete no_std. Cada .wasm vira agente com sandbox, fuel metering, capability tokens (CapGate) | ✅ `hermes/wasmi_rt` | ADR-0059 | SESSION_165: **wasmi crate real**; runtime próprio Sprint 93 legado deprecado |
| 309b | **IDE Agent (BitNet IDE)** — IDE no-navegador no AIOS, assistida por Cortex LLM BitNet. Escreve, debuga, compila para WASM | ⏳ defer gate | — | Fora do gate |
| 309c | **Agentes: kernel vs WASM (Hybrid)** — Tier 0-2 kernel (boot, HW, runtime crítico). Tier 3 WASM (user-extensible). Tier 4 external MCP. 20 agentes kernel, ∞ WASM | ✅ política ADR-0051/52 | — | Nativos no bin; WASM catalog |
| 310a | **J.A.R.V.I.S. Layer** — Camada de persona acima do Hermes: SOUL.md, contexto persistente (MemoryTree+KG), notificações proativas (NotificationGate), conversation engine (greetings, mood, task decomposition) | 🔄 → #315 / ADR-0036 | ✅ | Núcleo entregue; residuals Sound/Onda 4 |
| 310b | **Stack final:** Boot → Kernel → Cortex/LLM → Hermes → J.A.R.V.I.S. — Ver diagrama ADR-0036. Boot minimalista. Kernel acorda Cortex (BitNet). Cortex alimenta Hermes (intent). Hermes delega para J.A.R.V.I.S. (persona). Tudo agentes, tudo skills | ✅ ADR-0036 / N5 | — | Cadeia K³CHJ wired |

### 1.26. Trinity Model Hub — Mixture of Experts (IDEA #311)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 311a | **Router BitNet (68KB)** — classifica intenção e roteia para expert correto | 🟡 Sprint 77 | Sprint 77 | 5-10 classes de intenção |
| 311b | **hw_identify (68KB, ✅ existente)** — 66K pares PCI/USB, 99% precisão | ✅ v0.30 | Sprint 30 | Kernel mode |
| 311c | **rust_coder (444KB)** — gera código Rust no_std. "crie funcao rust para" → expert gera resposta direta sem LLM principal | ✅ Sprint 97 | Sprint 97 | Hidden=128, 6L, 1.6M params, loss 0.34 |
| 311d | **disk_diag (50KB)** — padrões SMART + erros de disco. Diagnóstico + ação sugerida | 🟡 Sprint 78 | Sprint 78 | Self-heal logs |
| 311e | **security (50KB)** — assinaturas de ataque + CVE patterns | 🟡 Sprint 78 | Sprint 78 | CVE + InnerWarden |
| 311f | **On-demand training** — "quero pilotar helicóptero" → manual → .bitnet → skill | ⏳ Pós-MVP | v0.80+ | Pipeline treino |
| 311g | **Generator 1.5B (375MB)** — tool-use: expert classifica, generator explica | 🟡 Sprint 77 | Sprint 77 | GGUF loader |

### 1.27. TrainingAgent — On-Device + GPU Learning (IDEA #312)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 312a | **GPU detection for training** — MemoryAgent already detects GPU + VRAM. If VRAM > 4GB → full training viable. If < 4GB → fine-tuning only | ✅ v0.75.1 | v0.75.1 | GPU detect exists |
| 312b | **Fine-tuning (CPU, ADD/SUB kernel)** — 100 exemplos, 50 iterações, ~2 segundos. BitNet ternário usa só ADD/SUB, sem FPU. On-device sempre disponível | 🟡 Sprint 77 | Sprint 77 | ~300 LOC kernel |
| 312c | **Transfer learning (CPU/GPU)** — 1000 exemplos, adapta modelo existente para novo domínio. Ex: disk_diag → net_diag (adapta padrões SMART para padrões de rede) | 🟡 Sprint 78 | Sprint 78 | ~200 LOC |
| 312d | **Full training (GPU, internet)** — 100K+ exemplos. Equivalente ao gen_micro_model.py rodando como agente. Pipeline: fetch data via HTTP → train PyTorch → export .bitnet → load no kernel. Só viável com B-01 + GPU ≥ 4GB VRAM | ⏳ Pós-MVP | v0.81+ | Depende de B-01 (rede) |
| 312e | **TrainingAgent** — agente que detecta GPU, escolhe modo (fine-tune/transfer/full), coleta dados do FS ou internet, executa treino, registra modelo no Trinity Hub | 🟡 Sprint 78 | Sprint 78 | ~500 LOC |
| 312f | **Federated learning** — múltiplos AIOS compartilham gradientes (não dados). Agregador central (Hermes Master) combina updates → modelo global melhor | ⏳ Pós-MVP → 🟡 ADR-0081 | v0.85+ | Absorvido pela ADR-0081 — GradientAggregator no Brain Mesh + Hermes Master. **Fase A (transporte P2P R0) + Fase B (skill sync) ✅ SESSION_234** — 2 QEMUs trocam heartbeats + skills via broadcast real. **Fase B cripto gate L/F ✅ SESSION_240** (HMAC Tier L mesmo range; Ed25519 Tier F externo) |

### 1.28. Self-Learning OS — Dados Gerados Pelo Próprio Sistema (IDEA #313)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 313a | **Dados gerados pelo AIOS** — EventBus (10K+ eventos/hora), boot logs, SMART data, self-heal logs, Hermes conversas, consciousness metrics, disk I/O patterns, network packets (c/ B-01), GPU errors. Tudo vira dataset de treino. | 🟡 Sprint 78 | Sprint 78 | ~300 LOC DataCollector |
| 313b | **Pipeline: LogAgent → DataCollector → TrainingAgent → .bitnet → Trinity Hub** — LogAgent coleta eventos, DataCollector estrutura como pares (input, output), TrainingAgent treina modelo, .bitnet vai pro Trinity Hub, Router classifica quando usar. | 🟡 Sprint 78 | Sprint 78 | ~200 LOC pipeline |
| 313c | **Melhoria contínua** — Cada boot gera dados. Cada treino melhora o sistema. O AIOS de hoje é melhor que o de ontem. Próximo boot: modelo treinado com dados do boot anterior. Sem internet. Sem humano. | 🟡 Sprint 78 | Sprint 78 | Integração boot sequence |

### 1.29. SleepCycle — Aprendizado Inspirado no Sono Humano (IDEA #314)
| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 314a | **Experience Replay** — Ring buffer dos últimos 1000 eventos do EventBus. Amostra 64 por ciclo de sono. Treina BitNet com replay. | 🟡 Sprint 77 | Sprint 77 | ~50 LOC |
| 314b | **Generative Replay (Dream)** — BitNet gera variações sintéticas dos exemplos reais (temperature sampling). "E se o erro fosse +20%? E se o usuário perguntasse X?" | 🟡 Sprint 77 | Sprint 77 | ~200 LOC BitNet VAE |
| 314c | **Elastic Weight Consolidation** — Protege skills existentes durante novo aprendizado. Pesos importantes para skill A não mudam quando treina skill B. | 🟡 Sprint 77 | Sprint 77 | ~150 LOC |
| 314d | **Synaptic Homeostasis (Pruning)** — Pesos com frequência de ativação < threshold viram 0. Elimina ruído, preserva sinal. ~18% de redução por ciclo (medido em cérebros reais). | 🟡 Sprint 77 | Sprint 77 | ~80 LOC |
| 314e | **Metacognitive Reflection** — Confidence tracking. Amostra interações com baixa confiança, gera micro-lessons corretivas, treina. | 🟡 Sprint 77 | Sprint 77 | ~250 LOC |
| 314f | **SleepCycle Agent** — CronAgent dispara a cada período de idle. 5 fases: REPLAY → DREAM → CONSOLIDATE → PRUNE → REFLECT. Ao final: modelo .bitnet atualizado no Trinity Hub. | 🟡 Sprint 77 | Sprint 77 | ~50 LOC scheduler |
| 314g | **Pioneirismo** — NENHUM sistema bare-metal implementa ciclo de sono/aprendizado. Neural AIOS seria o primeiro no mundo. | ✅ Conceito | — | Fato |

### 1.31. J.A.R.V.I.S. Unified Interaction Layer — Persona + Memória Cognitiva (IDEA #315)
**28 features, 5-layer architecture, Sprints 77-80+N+1+N+2. ADR-0036 substitui ADR-0034 + ADR-0035.**

| # | Item | Destino | Sprint | LOC | Fonte |
|---|---|---|---|---|---|
| 315.1 | **SOUL.md Personality Engine** — Parser markdown minimalista \| nome, tom, humor_level, formality, empathy, greetings, farewell, notification rules. Adapta tom por contexto (Fluid Personality paper) | 🟡 Sprint 77 | 77 | ~300 | JARVIS C# + BeFree JARBAS |
| 315.2 | **IPW Monitor (RAPL MSR 0x610)** — Intelligence Per Watt. Mede energia via PKG_ENERGY_STATUS, calcula tokens/watt, cache hit/miss ratio. Acoplado ao MemoryAgent | 🟡 Sprint 77 | 77 | ~150 | OpenJarvis Stanford |
| 315.3 | **Session Compression (4 strategies)** — Summarize (BitNet), DropLowest (importance), MergeSimilar (embedding), SegmentMeans (SKYNET). Mantém últimas N mensagens literais | 🟡 Sprint 77 | 77 | ~200 | OpenJarvis + SKYNET |
| 315.4 | **Notification Gate (4 urgency levels)** — Critical (imediato), High (30s), Medium (idle), Low (log). Rate limiting, dedup, startup grace period. Regras do SOUL.md | 🟡 Sprint 77 | 77 | ~200 | JARVIS C# + BeFree |
| 315.5 | **Sessionless Thread** — Modo de conversa contínua sem reset de contexto. JARVIS mantém thread ativa entre comandos, sem perder estado | 🟡 Sprint 77 | 77 | ~100 | Residuum |
| 315.6 | **Emotion Analysis (BitNet classifier)** — 7 emoções (Joy/Sadness/Anger/Fear/Surprise/Disgust/Neutral), intensidade 0-1, sarcasmo, urgência. Expert Trinity (~50KB). Ajusta tom da resposta | 🟡 Sprint 78 | 78 | ~250 | JARVIS C# |
| 315.7 | **Capability Contract + Consent Gates** — 3 níveis (Safe/Moderate/Dangerous). SkillRegistry + SafetyAgent validam antes de executar. Baseado em terminal-jarvis + Moltis | 🟡 Sprint 78 | 78 | ~200 | terminal-jarvis + Moltis |
| 315.8 | **Skill Discovery (DSPy/ACE)** — SkillObserver monitora padrões de uso, sugere novas skills. Pipeline: observe → analyze → propose → generate | 🟡 Sprint 78 | 78 | ~300 | OpenJarvis + SynkraAI |
| 315.9 | **ADE Pipeline (Spec→Execute→Review→Recover)** — 4 fases: Specification (SDD-style), Execution (AgentScheduler), Review (verification contracts), Recover (self-heal fallback) | 🟡 Sprint 78 | 78 | ~200 | SynkraAI |
| 315.10 | **Semantic Cache (5-tier routing)** — Tier 1: SHA-256 exact \| Tier 2: embedding similarity >0.95 \| Tier 3: pattern (intent+entities) \| Tier 4: fallback round-robin \| Tier 5: cold start. 97.5% reduction (NabaOS) | 🟡 Sprint 78 | 78 | ~150 | NabaOS |
| 315.11 | **Persona Pipeline (16 stages)** — SafetyCheck→StopHandler→Converse→SkillHigh→Persona→SkillMedium→CommonQA→FallbackLow→Reflexive→Dreaming→EgoUpdate→SessionCompress→NotificationGate→Heartbeat→BabelIndex→AuditLog. OVOS-inspired | 🟡 Sprint 78 | 78 | ~100 | OVOS |
| 315.12 | **Dreaming/Consolidation** — CronAgent noturno: agrupa memórias similares, gera insights sintéticos (BitNet), remove contradições, promove frequentes para LTM, Ebbinghaus decay | 🟡 Sprint 79 | 79 | ~200 | mem0-supabase Layer 6 |
| 315.13 | **Ego Layer (self-model)** — Meta-cognitive identity synthesis. JARVIS sabe o que sabe/não sabe. Confidence tracking por domínio. Atualiza auto-modelo por interação. Twin Agents paper | 🟡 Sprint 79 | 79 | ~250 | mem0-supabase Layer 12 |
| 315.14 | **Proactive Heartbeats** — JARVIS inicia conversa proativamente: "Disk 90% full, sir." Baseado em eventos do EventBus + regras do NotificationGate | 🟡 Sprint 79 | 79 | ~100 | mem0-supabase Layer 12 |
| 315.15 | **Tool-State Save Game** — Snapshot do estado das ferramentas/agentes antes de executar skill. Rollback automático se skill falhar | 🟡 Sprint 79 | 79 | ~100 | mem0-supabase Layer 9 |
| 315.16 | **Auto-Skill Generation** — Cratos-inspired: observa interações, gera SKILL.md com padrões detectados. Pipeline: watch → pattern → propose → generate → register | 🟡 Sprint 79 | 79 | ~150 | Cratos |
| 315.17 | **Babel-Index (entropy monitoring)** — Monitora entropia, contradiction rate, staleness index da memória. Prevê colapso de coerência → dispara consolidação automática | 🟡 Sprint 79 | 79 | ~100 | NEOTH |
| 315.18 | **Fail-Closed Safety Invariant** — SafetyAgent sempre nega por padrão. Toda skill precisa autorização explícita. SMT-proof (Z3-style): 4 invariants: process separation, pre-action, fail-closed, signed evidence | 🟡 Sprint 80 | 80 | ~200 | Unfireable Safety Kernel paper |
| 315.19 | **Merkle Audit Trail (Ed25519 signed)** — Chain de audit entries: tick, agent, action, payload_hash, prev_hash, Ed25519 signature. Verificação de integridade a cada entry. Ring buffer 4096 | ✅ SESSION_136 | ADR-0053 | ~200 | Session key assina entry_hash |
| 315.20 | **Cognitive Bridge (HANR UX / K³CHJ stack)** — SOUL≠PERSONA; BGE+Trinity no prompt; IterationBudget; session search; CapGate L0; SleepCycle REFLECT→MEMORY_NUDGE Jarbas | ✅ SESSION_137 | ADR-0053+ | ~400 | `cognitive_bridge.rs` |
| 315.20 | **Fluid Persona (context-adaptive)** — Persona muda por contexto: urgente→preciso, triste→empático, irritado→formal. 3 eixos: persona metafórica (coach/tutor/tool) + intensidade (low/med/high) + traits do usuário | 🟡 Sprint 80 | 80 | ~100 | Fluid Personality paper |
| 315.21 | **Pocket TTS Integration** (TTS) — Via sherpa-onnx (Rust bindings). PocketTTS engine 100M params, CPU-native, ~200ms latência, voice cloning, 6 idiomas. Alternativa: Kokoro via sherpa-onnx. Pós B-01 | ❌ supersedido | — | Histórico. Primário = Piper VITS + formant. ADR-0045. | k2-fsa/sherpa-onnx |
| 315.22 | **STT (sherpa-onnx Whisper)** — Speech-to-text via sherpa-onnx Rust bindings. Whisper engine, CPU offline. Alternativa: Vosk. Pós B-01 | ❌ supersedido | — | Histórico. Primário = STT CTC nativo (`audio/stt.rs`). ADR-0045. | k2-fsa/sherpa-onnx |
| 315.23 | **Wake Word (Rustpotter)** — Detecção de "Jarvis" via Rustpotter crate. Publica WAKEWORD_DETECTED no EventBus. Pós B-01 | ❌ supersedido | — | Histórico. Substituído por MLP nativo `wakeword.rs` (**registrado** no boot Loop 5); path Mic→WAKEWORD e2e ainda aberto. ADR-0045. | Priler/jarvis |
| 315.24 | **Audio Ring Buffer** — Circular buffer PCM lockless para DMA audio entre HDA/USB e voice pipeline. Produtor/consumidor SPSC via EventBus. | ✅ feito | Sprint Sound | `audio/ringbuf.rs` no truth. Não bloqueado por B-01. ADR-0045. | — |
| 315.25 | **Voice Pipeline** — Pipeline de áudio nativo Rust sobre EventBus: Mic→WakeWord→STT→Cortex→TTS→Speaker. Frame types (AudioFrame, TranscriptionFrame, TTSCommandFrame) + PipelineAgent que orquestra. Ref arquitetural: pipecat pipeline composition pattern. | ❌ supersedido (spec sherpa) / ▶️ Sprint Sound | Sprint Sound (reaberta) | Spec original (sherpa+rustpotter) ❌. Pipeline nativo parcial (107 skinny ✅); fechar loop runtime = Sound. ADR-0045. | EventBus nativo |
| 315.26 | **Multi-device sync (CRDT)** — Sincronização de memória/contexto entre dispositivos via CRDT (Automerge-style). Pós B-01 | ⏳ defer → 🟡 ADR-0081 | fora gate | Absorvido pela ADR-0081 — VectorClock + SGDB CRDT + Fase C. **Fases A-D ✅ SESSION_234/235** (descobrem-se + skill sync + compute distribuído) |
| 315.27 | **SKYNET Mesh Node** — Participa da malha SKYNET como nó L1 (PC) ou L2 (workstation). Speculative decoding distribuído. Pós B-01 | ⏳ defer → 🟡 ADR-0081 | fora gate | Absorvido pela ADR-0081 — NodeTier L1-L4 + NoProto + Brain Mesh. **Fases A-D ✅ SESSION_234/235** (descoberta + eleição + skill sync + matmul distribuído reais) |
| 315.28 | **Gamification** — Recompensas, streaks, achievements para interação com JARVIS. OptimizerAgent + CronAgent | 🟢 Futuro | N+1 | ~200 | Jotape |

**Total:** ~5650 LOC (Sprints 77-80: ~3550, N+1: ~1600, N+2: ~500). ADR-0036.

### 1.30. Bugfix Estrutural (Sprint 45) — H3 a H12

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 268 | **H3 — APIC SVR vetor espúrio** — SVR escrito com vetor 0, causa #DE falso em interrupções espúrias | ✅ v0.45.0 | Sprint 45 | Fix: SVR = `(svr & 0xFFFFFF00) \| 0xFF \| 0x100` (vetor 255 + APIC enable) |
| 269 | **H4 — IDT sem cobertura total 0-31** — Exceções #DE, #UD, #NM, #MC, #XM, #VE, #CP sem handlers, causam Triple Fault silencioso | ✅ v0.45.0 | Sprint 45 | Fix: 32 handlers nomeados com dump textual de InterruptStackFrame |
| 270 | **H5 — PIC EOI sem duplo para escravo** — Interrupções do PIC escravo (vetores 40-47) não recebem EQI no 0xA0 | ✅ v0.45.0 | Sprint 45 | Fix: `send_eoi(vector)` envia para 0x20 e 0xA0 se vector >= 40 |
| 271 | **H11 — PCI multi-function sem verificação** — Scanner força funções 1-7 em dispositivos single-function, desperdiçando ciclos | ✅ v0.45.0 | Sprint 45 | Fix: `header_type` (offset 0x0E) bit 7 verificado antes de escanear |
| 272 | **H12 — IOAPIC RTEs desmascaradas** — Ruído elétrico em linhas não usadas gera interrupções fantasmas | ✅ v0.45.0 | Sprint 45 | Fix: Todas RTEs inicializadas com bit 16 = MASK |
| 273 | **VirtIO-GPU driver manual** — PCI capabilities + MMIO mapping + control queue. GET_DISPLAY_INFO pendente (resposta 0x0) | 🟡 Sprint 45 (⚠️ 95%) | Sprint 45 | Sem zerocopy-derive. Feature negotiation, queue enable, ring layout corrigidos. Falta response. |



### 1.24. Life OS / Personal OS Ecosystem (20 repos Tier 1)

| # | Item | Classificação | Sprint | Motivação |
|---|---|---|---|---|
| 177 | **7D Spectrum Graph Leve** — Grafo de conhecimento 7-dimensional para EventBus; Edge Prophecy (Jaccard similarity) p/ predizer conexões. Substitui BTreeMap por `Vec<(u64,u64,u8,u64)>`. Dá memória associativa ao Hermes | ✅ Imediata | Sprint 24 | ~200 LOC sobre EventBus existente. Spectrum Graph do PrismOS-AI portado para no_std. |
| 178 | **Runtime SDD (Structured Decision Document)** — Antes de executar skill, Hermes mostra goal/context/plan/expected outcome/rollback. Reasoning visível no VGA | ✅ Imediata | Sprint 24 | ~80 LOC no intent_router_daemon. Alinha com chain-of-thought determinístico. |
| 179 | **File System as Context** — Em vez de RAG embedding, usa filesystem como índice de conhecimento. CDC Rabin chunking + grep-like scan sobre arquivos `.bitmem` | 🟡 Baixa | Sprint 27 | ~300 LOC sobre CDC Rabin (#165). Requer VirtIO-blk (Sprint 24+). Bridge p/ SFS (Layer 2). |
| 180 | **DA Identity Layer** — Persona persistente do Hermes (SOUL.md/IDENTITY.md/TELOS.md). Hermes atual é stateless; identidade dá voz/personalidade consistentes | 🟡 Baixa | Sprint 27 | ~100 LOC. Parser markdown mínimo. Identidade hardcoded como fallback sem disco. |
| 181 | **Temporal Knowledge Graph** — Grafo temporal com validity windows e contradiction detection 97% LongMemEval-S. Extensão do Spectrum Graph (#177) com `(t_start, t_end)` por aresta | 🟠 Média | Sprint 28 | ~500 LOC sobre #177. taOSmd port: archive append-only, detecção de sobreposição temporal. |
| 182 | **Proactive Push / Heartbeat Scheduler** — Tentáculos autônomos que monitoram fontes e fazem push proativo. NeuralExecutor ganha agendamento push baseado em LAPIC timer ticks | 🟠 Média | Sprint 28 | ~400 LOC. Dedup hash + priority queue. Push externo requer Network Sprint. |
| 183 | **WASM Sandbox para Skills** — Sandbox via paging (não wasmtime): skill executa em página separada com PTE NX + shared memory controlado. Fuel metering + auto-rollback | 🟠 Média | Sprint 28 | ~800 LOC. Alternativa no_std ao wasmtime. Depende de #172 MCP Server. |
| 184 | **Intent Transparency** — Após cada resposta Hermes, mostrar query type, reasoning band, confidence, alternatives. MLP argmax era silencioso — agora é visível | 🟡 Baixa | Sprint 27 | ~200 LOC. Log estruturado no intent_router_daemon. Sem dependências. |
| 185 | **Energy / Circadian Tracking** — Usuário reporta energia (1-10) via `/energy`. Scheduler casa tarefas com capacidade real. 15-25 min task chunks, dopamine hooks | 🟡 Baixa | Sprint 27 | ~150 LOC como skill. Requer #157 Usage Pattern Analyzer p/ correlação. |
| 186 | **AppForge / App Store** — Plataforma de apps com catalog, instalação one-click, hardware-aware filtering. Store backend sobre MCP + SFS | 🔴 Alta | Sprint 29+ | ~1500 LOC total. Frontend inviável sem framebuffer (💰 Sponsor). |
| 187 | **Multi-User / Multi-Persona** — Vários usuários com memória isolada, trust tiers diferentes. Certificate Authority + Dual-LLM split (quarentena/planejamento) | 🔴 Alta | Sprint 29+ | ~600 LOC. Redesign do scheduler. PerCpu → PerUser. TrustCache multicamada. |
| 188 | **Visual Workflow Builder** — Drag-and-drop pipeline DAG (Trigger/Tool/Agent/Condition/Loop/Gate). AI workflow designer via chat | ⏳ Futuro | — | Requer framebuffer VESA + mouse. CLI ASCII DAG perde valor visual. |
| 189 | **Federated Cluster / P2P Workers** — Mesh de AI compute (gaming PC, Mac, RPi, Android). Auto-descoberta, pareamento PIN, checkpoint distribuído | ⏳ Futuro → 🟡 ADR-0081 | — | Absorvido pela ADR-0081 (Malha Cognitiva Distribuída) — Brain Mesh + NoProto + Hermes dispatch. **Fase A+B ✅ SESSION_234** (descoberta P2P real entre 2 QEMUs + skill sync) |
| 190 | **Algorithm loop de 7 fases no Hermes** — THINK antes de agir (carrega contexto adicional), VERIFY depois (confirma ISC). Não só MLP→argmax→skill. THINK consulta KNOWLEDGE graph, VERIFY checa resultado contra critério | 🟡 Baixa | Sprint 27 | ~300 LOC. PAI Algorithm v6.3.0: OBSERVE→THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN. Ref: `docs/architecture/0021-life-os-ecosystem-analysis.md#2-o-algorithm-v630` |
| 191 | **Council skill** — Antes de decisão ambígua, 3 vozes (OtImista, Cético, Pragmático) votam. Argmax vence. Melhora qualidade de intent classification em bordas | 🟡 Baixa | Sprint 27 | ~150 LOC como skill. PAI Council skill: multiple perspective simulation. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 192 | **Loop Detection no NeuralExecutor** — Monitora repetição de `AgentTask.id`, força break/log warning após N≥3 repetições sem progresso | ✅ Imediata | Sprint 24 | ~80 LOC. PAI Loop skill detecta repeat patterns em tool calls. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 193 | **Bitter Pill Engineering** — Força etapas obrigatórias (cargo check antes de deploy, test antes de merge) mesmo que usuário peça atalho. Hermes recusa pular passos críticos | 🟡 Baixa | Sprint 27 | ~100 LOC no intent_router. PAI BitterPillEngineering skill. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 194 | **ISA como formato de sprint** — Cada sprint tem ISA (Ideal State Artifact) com ISCs verificáveis. Substitui verificação ad-hoc "cargo check + QEMU boot" por critérios binários formais | 🟡 Baixa | Sprint 27 | ~200 LOC + docs. PAI ISA: 12 seções (Problem→Vision→Goal→Criteria→Test Strategy→Verification). Ref: `0021-life-os-ecosystem-analysis.md#3-o-isa-ideal-state-artifact` |
| 195 | **Hermes Rating (Satisfaction Capture)** — Após cada resposta, usuário dá 👍/👎 via `/rate`. Alimenta TrustCache weight adjustment + Success Engine feedback loop | 🟡 Baixa | Sprint 27 | ~100 LOC no hermes_console_daemon. PAI SatisfactionCapture.hook.ts (18 KB). Ref: `0021-life-os-ecosystem-analysis.md#4-o-sistema-de-hooks-37-hooks` |
| 196 | **Evals skill** — Avalia respostas do Hermes contra critérios predefinidos antes de mostrar ao usuário. Se confidence < 0.7, re-executa com mais contexto | 🟠 Média | Sprint 28 | ~300 LOC. PAI Evals skill. Ref: `0021-life-os-ecosystem-analysis.md#skills-como-deterministic-units` |
| 197 | **Container Zones via TrustCache** — Trust token define quais regiões de memória/skills a skill pode acessar. Implementa containment zones do PAI em bare-metal | 🟠 Média | Sprint 28 | ~400 LOC sobre #176 Ed25519. PAI ContainmentGuard.hook.ts + containment-zones.ts. Ref: `0021-life-os-ecosystem-analysis.md#10-security--containment` |
| 198 | **Boot-time security policy (.pai-protected.json equivalente)** — Banco de regexes de segurança compilado no boot. Skills são validadas contra patterns antes de executar | 🟡 Baixa | Sprint 27 | ~100 LOC. PAI .pai-protected.json: 17 categorias, 100+ regexes. Ref: `0021-life-os-ecosystem-analysis.md#10-security--containment` |
| 199 | **IterationBudget com Grace Cycle** — Max poll cycles per AgentTask, um grace cycle extra após exhaustion para finalização limpa | ✅ Imediata | Sprint 24 | ~50 LOC. Hermes Agent `agent/iteration_budget.py`. Ref: ADR-0022. |
| 200 | **Skill Metadata Frontmatter** — `version`, `author`, `description` (≤60 chars), `tags` na Skill trait. Routing constraint de 60 chars para caber em linha VGA 80-col | ✅ Imediata | Sprint 24 | ~80 LOC. Hermes Agent `/learn` + OpenClaw marketplace. Ref: ADR-0022. |
| 201 | **Audit Ring Buffer** — Ring buffer fixo de eventos de auditoria no executor (task_id, tool_name, outcome, LAPIC tick). Expor via syscall | ✅ Imediata | Sprint 24 | ~80 LOC. GitAgent `.gitagent/audit.jsonl`. Ref: ADR-0022. |
| 202 | **Agent Identity Awakening Mode** — Duas personalidades Hermes: "Awakening" (primeiro boot) e "Established" (memória presente). MLP weights diferentes selecionados via HAS_MEMORY flag | ✅ Imediata | Sprint 24 | ~50 LOC. GitAgent `src/context.ts` + PAI SOUL.md. Ref: ADR-0022. |
| 203 | **Context Fencing + Streaming Scrubber** — Byte-level type markers no EventBus (`[UserInput]`, `[HardwareTelemetry]`). State machine scrubber remove na recepção | 🟡 Baixa | Sprint 27 | ~150 LOC. Hermes Agent `StreamingContextScrubber`. Ref: ADR-0022. |
| 204 | **Heartbeat Idle Gate com Open Work Digest** — Watchdog detecta idle vs active. Tick é idle só quando sem reminders E sem subagentes ativos | 🟡 Baixa | Sprint 27 | ~200 LOC. Lethe `scheduler/brainstem.rs`. Ref: ADR-0022. |
| 205 | **ProactiveRateLimiter com Deferred Outbox** — Rolling window (24h ticks) + cooldown. Outbox segura 1 msg deferida, msg nova superseded, stale >6h descartada | 🟡 Baixa | Sprint 27 | ~150 LOC. Lethe `scheduler/proactive.rs`. Ref: ADR-0022. |
| 206 | **Lifecycle Hooks via Pre/Post Poll Callbacks** — HookRegistry com slots fixos de function pointers. Hooks retornam Allow/Block/Modify | 🟡 Baixa | Sprint 27 | ~200 LOC. GitAgent `hooks/hooks.yaml`. Ref: ADR-0022. |
| 207 | **MemoryProvider + MemoryManager Trait** — Trait pluggável sobre MHI tiers. MemoryManager orquestra prefetch/sync em background via executor cooperativo | 🟠 Média | Sprint 28 | ~400 LOC. Hermes Agent `agent/memory_manager.py` + Lethe `memory/store.rs`. Ref: ADR-0022. |
| 208 | **Capability-Based Tool Permission Model** — TrustCache verifica (token, skill, tier) antes da execução. Skills declaram tiers de memória + tokens autorizados | 🟠 Média | Sprint 28 | ~400 LOC. Hermes Agent `acp_adapter/permissions.py` + Ironclaw WASM sandbox. Ref: ADR-0022. |
| 209 | **Actor Registry com Permission Model** — Registry de subagentes: spawn/terminate/kill, can_message() hierarchical, task state machine (Planned→Running→Blocked→Done), open_work tracking | 🟠 Média | Sprint 28 | ~500 LOC. Lethe `actor/registry.rs` (~46KB). Kameo-inspired. Ref: ADR-0022. |
| 210 | **Subagent Crash-Recovery Persistence** — Estado de subagentes persistido em região de memória reservada. Boot walk + rehidrata. Serialização postcard/bincode | 🔴 Alta | Sprint 29+ | ~600 LOC. Lethe `actor/persistence.rs` + Ironclaw state. Ref: ADR-0022. |
| 211 | **ComputeBackend Trait** — Abstrai 3 rings (NPU/GPU/CPU) atrás de trait. Intent router chama `COMPUTE_BACKEND.execute()` sem saber qual ring | 🔴 Alta | Sprint 29+ | ~800 LOC. ZeroClaw Peripheral trait + Ironclaw WASM/Docker. Ref: ADR-0022. |
| 212 | **Plugin System via Loadable Page Ranges** — Plugin = região page-aligned em RAM com PluginDescriptor + tools + hooks. Walk linked list de regiões | ⏳ Futuro | — | Requer SFS Layer 2. GitAgent `plugins/<id>/plugin.yaml`. Ref: ADR-0022. |
| 213 | **WASM + Docker Sandbox para Skills** — Ferramentas em WASM containers isolados com capability-based permissions + rate limiting + resource limits | ⏳ Futuro | — | Requer #183 WASM Sandbox + Network Sprint. ZeroClaw/Ironclaw. Ref: ADR-0022. |

**ADR-0021:** `docs/architecture/0021-life-os-ecosystem-analysis.md` — Análise de 20 repos Tier 1 (Life OS / Personal OS). 13 ideias extraídas + 9 do PAI deep-dive (#190-198). Total: 22 ideias.
**ADR-0022:** `docs/architecture/0022-personal-ai-assistant-ecosystem-analysis.md` — Análise de 21 repos Tier 2 (Personal AI Assistants). 15 ideias extraídas (#199-213). Deep-dives: Lethe (Rust), Hermes Agent (202k ★), GitAgent, Rust ecosystem (ZeroClaw, Ironclaw).

### 1.26. Tier 3 — Memory Systems & Second Brain (2026-06-25)

| # | Item | Destino | Target | Motivação |
|---|---|---|---|---|
| 214 | **SHA-256 Memory Dedup** — Port SHA-256 dedup (5min sliding window) to no_std; prevent duplicate EventBus messages and TrustCache entries | 🟡 Sprint 23 | Sprint 23 | agentmemory dedup.ts. ~100 LOC. Sem novas deps (SHA-256 via `sha2` ou manual). |
| 215 | **Privacy Filter for Memory** — Strip API keys, secrets, `<private>` tagged content before memory storage; regex + pattern matching | 🟡 Sprint 23 | Sprint 23 | agentmemory privacy.ts. ~80 LOC. Proteção zero-trust para TrustCache. |
| 216 | **Memory TTL/Eviction** — Auto-evict stale memory entries based on configurable TTL; EvictionPolicy enum (TimeToLive, ImportanceRank, AccessFrequency) | 🟡 Sprint 23 | Sprint 23 | agentmemory evict.ts, auto-forget.ts. ~150 LOC. |
| 217 | **Hybrid Search (BM25 + MLP)** — RRF fusion for Hermes intent routing; combine MLP intent classifier with BM25 keyword fallback; Reciprocal Rank Fusion with k=60 | 🟡 Sprint 24 | Sprint 24 | agentmemory search.ts. ~200 LOC. BM25 já implementável em no_std. |
| 218 | **4-Tier Memory Consolidation** — Working→Episodic→Semantic→Procedural pipeline in Hermes daemon; EventBus topics for each tier transition | ✅ Sprint 24 | Sprint 24 | agentmemory consolidation-pipeline.ts. ~400 LOC. Aproveita EventBus. **Implementado SESSION_237 (jcode-inspired):** `k_ai::tiers::consolidate_tiers` + SleepCycle CONSOLIDATE. |
| 219 | **Ebbinghaus Decay for TrustCache** — strength = importance × e^(-λ_eff × days) × (1 + recall_count × 0.2); λ_eff = 0.16 × (1 - importance × 0.8) | 🟡 Sprint 24 | Sprint 24 | nexo + YourMemory. ~120 LOC. libm expf. |
| 220 | **Session Replay** — Record Hermes daemon turns as atomic events; replay with speed control for debugging | 🟡 Sprint 24 | Sprint 24 | agentmemory replay.ts. ~200 LOC. |
| 221 | **Knowledge Graph on MHI** — Entity extraction + BFS traversal over semantic file system; GraphNode/GraphEdge with bi-temporal timestamps | 🟡 Sprint 25 | Sprint 25 | agentmemory graph.ts, temporal-graph.ts. ~500 LOC. |
| 222 | **Metacognitive Guard** — Before each Hermes skill execution, check TrustCache for past mistakes; inject known error patterns, schemas, blocking rules | 🟡 Sprint 25 | Sprint 25 | nexo guard system. ~300 LOC. |
| 223 | **Draft→Review→Merge Memory** — Memory changes staged as drafts on EventBus; Hermes daemon reviews before merge; approval/rejection workflow | 🟡 Sprint 25 | Sprint 25 | novyx-vault workflow. ~350 LOC. |
| 224 | **Atkinson-Shiffrin Cognitive Memory** — Full 3-tier memory: Sensory Register (48h) → STM (7d) → LTM (permanent, semantic-indexed); promotion on access frequency | 🟡 Sprint 26 | Sprint 26+ | nexo brain architecture. ~800 LOC. Maior item individual do Tier 3. |
| 225 | **Bi-temporal Knowledge Graph for MHI** — Track MHI tiers across time; append-only, old facts superseded never deleted | 🟡 Sprint 26 | Sprint 26+ | MemoryOS + agentmemory temporal-graph.ts. ~600 LOC. |
| 226 | **Team/Shared Memory** — Namespaced memory across neural-os-core instances; shared + private isolation per agent ring | ⏳ Pós-MVP | — | agentmemory team.ts. ~400 LOC. |
| 227 | **Memory Git Snapshots** — Version, rollback, diff memory state; SHA-256 commit chain for TrustCache and Hermes memory | ⏳ Pós-MVP | — | agentmemory snapshot.ts. ~500 LOC. |

**ADR-0023:** `docs/architecture/0023-memory-systems-second-brain-analysis.md` — Análise de 14 repos Tier 3 (Memory Systems & Second Brain). 14 ideias extraídas (#214-227). Deep-dives: agentmemory (24k ★, 60+ source files), nexo (Atkinson-Shiffrin + Ebbinghaus), novyx-vault (Draft→Review→Merge + Ghost Connections), MemoryOS (bi-temporal KG).

### Tier 4 — Agent Frameworks (#228-#249)

Ideias portadas de 6 repos Tier 4: Cline (63.9k ★), Agent Zero (18.2k ★), Microsoft Agent Framework (11.7k ★), OpenHands (77k ★), opencode/Crush (13.1k ★), open-agent (new).

| # | Item | Destino | Target | Fonte + Detalhes |
|---|---|---|---|---|
| 228 | **Tool Policy Registry** — Extend SkillRegistry with `{ enabled: bool, autoApprove: bool }` per tool, wildcard fallback, `validate_tool_call()` denies blocked tools | ✅ Sprint 23 | Sprint 23 | Implementado em skill-registry (ToolPolicy + set_policy + is_enabled + is_auto_approve). Cline agent-runtime.ts. |
| 229 | **Usage Tracker** — Lightweight token/metrics accumulator for hardware_context_tensor(): track input_chars, output_chars, cache_hits, iterations per daemon | ✅ Sprint 23 | Sprint 23 | Implementado em usage.rs (UsageTracker + record_call + snapshot + to_metrics_tensor). Cline AgentUsage. |
| 230 | **Auto-Compact Hermes Buffer** — After 3+ conversation cycles without user input, trigger summarize_context skill to compact buffer into single [System Note] | ✅ Sprint 23 | Sprint 23 | Implementado em hermes.rs (ConversationTracker + needs_compact + compact, AUTO_COMPACT_THRESHOLD=3). opencode pattern. |
| 231 | **Event-Sourced Conversation State** — Replace mutable String buffer in Hermes with VecDeque<ConversationEvent { type, payload, timestamp }> immutable event log | ✅ Sprint 23 | Sprint 23 | Implementado em conversation.rs (EventLog + events_since + last_n + summarize + ContextCompacted). OpenHands typed events. |
| 232 | **Cron Scheduler** — ScheduleService with CronSpec { prompt, schedule, enabled, model_id, tool_policies }, periodic poll via LAPIC timer, markdown report writer | 🟡 Sprint 24 | Sprint 24 | Cline CronRunner + SqliteCronStore. ~350 LOC. |
| 233 | **Session Checkpoint/MHI Snapshot** — checkpoint() saves kernel state + MHI tier stats to reserved frames; restore() rollback on Double Fault | 🟡 Sprint 37+ | Sprint 37+ | Cline ClineCore restore(). ~200 LOC. Passo natural apos SelfHealing. |
| 234 | **Plan/Execute Modes** — Hermes dual-mode: plan mode (analysis only, no tool execution), act mode (full execution with auto-approve) | 🟡 Sprint 24 | Sprint 24 | Cline + MS Agent. ~150 LOC. |
| 235 | **Graph-Based Multi-Daemon Orchestration** — Extend EventBus with sequential / concurrent / handoff patterns for daemon chains | 🟡 Sprint 24 | Sprint 24 | MS Agent graph-based workflows. ~250 LOC. |
| 236 | **Plugin Hub / MCP Index** — McpRegistry discovers and installs skill packages from remote index with AI-driven security scanning | 🟡 Sprint 25 | Sprint 25 | Agent Zero Plugin Hub (100+ plugins). ~400 LOC. |
| 237 | **Completion Terminal Skills** — lifecycle.completes_run: bool on McpManifest, SkillRegistry auto-routes terminal response to HERMES_RESPONSE | 🟡 Sprint 25 | Sprint 25 | Cline completesRun + completionPolicy. ~120 LOC. |
| 238 | **Claim-Based Daemon Lease** — LeaseDaemon skill: acquire lease with TTL, heartbeat via LAPIC timer, release on completion; prevents double-execution | 🟡 Sprint 25 | Sprint 25 | Cline claimDueRuns + claimLeaseHeartbeat. ~200 LOC. |
| 239 | **Time Travel / Workspace Snapshot** — Capture BitmapFrameAllocator state + MHI tiers at checkpoint, restore on Double Fault or /restore command | 🔲 — | — | Postergado apos SelfHealing (Sprint 37+). |
| 240 | **Context Compaction with Ebbinghaus Decay** — Conversation events decay via Ebbinghaus formula; summarize_context when budget < 20% remaining | 🟡 Sprint 25 | Sprint 25 | Cline + Tier 3 Ebbinghaus. ~150 LOC. |
| 241 | **OpenTelemetry-Like Observability** — Trace event logging via serial output, structured log format, per-daemon latency/usage metrics | 🟡 Sprint 26+ | Sprint 26+ | MS Agent OpenTelemetry. ~500 LOC. |
| 242 | **AI-Driven Security Scan for Skills** — Use Intent MLP to classify skill behavior as safe/suspicious/malicious before execution | 🟡 Sprint 26+ | Sprint 26+ | Agent Zero AI-driven plugin security scan. ~350 LOC. |
| 243 | **Hub Discovery / Multi-Instance Board** — EventBus instances discover each other via shared MHI memory region, coordinate task assignment | 🟡 Sprint 26+ | Sprint 26+ | Cline cline-hub Kanban multi-agent board. ~400 LOC. |
| 244 | **Human-in-the-Loop Approval** — request_tool_approval() blocks tool execution until keyboard confirmation via /approve or /deny | 🟡 Sprint 26+ | Sprint 26+ | MS Agent + Cline tool approval. ~250 LOC. |
| 245 | **Remote Agent Execution** — Hub daemon on separate machine, EventBus over VirtIO-net TCP | ⏳ Pós-MVP | — | Cline hub discovery. ~800 LOC. |
| 246 | **Skill Marketplace** — Signed, versioned MCP packages published to remote registry; Hermes /install <pkg> | ⏳ Pós-MVP | — | Agent Zero Plugin Index. ~600 LOC. |
| 247 | **Automatic Context Compaction Agent** — Dedicated daemon that monitors conversation budget and proactively compacts | ⏳ Pós-MVP | — | Cline + opencode. ~300 LOC. |
| 248 | **Docker Sandbox** — Containerized execution environment for skills | ❌ Descartado | — | Incompatível com bare-metal no_std (sem container runtime em Ring 0-2). |
| 249 | **Python/.NET Runtime** — Multi-language agent runtime support | ❌ Descartado | — | Barreira de linguagem; Python requer OS que neural-os-core substitui. |

**ADR-0024:** `docs/architecture/0024-agent-frameworks-analysis.md` — Análise de 6 repos Tier 4 (Agent Frameworks). 22 ideias extraídas (#228-249). Deep-dive: Cline (63.9k ★, 293 releases, 6.338 commits, AgentRuntime + ClineCore + CronRunner). Ideias imediatas: Tool Policy Registry, Usage Tracker, Auto-Compact Buffer, Event-Sourced Conversation (Sprint 23, ~230 LOC total).

---

### 1.28. Agent/Skill-First Architecture — A Grande Virada

**Bloco 11 — Sprints 39-42 (consolidado)**
**Data:** 2026-06-26
**Status:** ⚡ Paradigma fundamental. Sprints 39-40 implementados (SkillLoader, Agent trait, AgentRegistry, SystemAgent). Sprints 41-42 continuam migração.

#### O Princípio

> **Tudo no Neural OS Hermes é um Agente ou uma Skill.** Não existem "tasks", "services", "drivers" ou "daemons" como conceitos separados. Cada entidade no sistema é um Agente com identidade, manifesto, ciclo de vida e capacidades declaradas. Habilidades (Skills) são a interface de requisição-resposta dos Agentes.

#### Por que esta virada?

O projeto começou com 8 `async fn` hardcoded no executor (`system_daemon`, `hw_bridge_daemon`, `input_daemon`, etc.). Cada sprint adicionava mais uma task. Skills (EchoSkill, SystemStatusSkill) eram uma preocupação separada. Drivers (rtl8139, xhci) estavam fora do sistema de skills.

Isso criou **3 regimes ontológicos diferentes** — tasks, skills, drivers — cada um com suas próprias regras, apesar de todos serem, na prática, agentes.

A virada **Agent/Skill-First** unifica tudo:

| Antes | Depois |
|---|---|
| 8 `async fn` tasks | 8+ Agent instances no AgentRegistry |
| SkillRegistry separado | SkillRegistry = catálogo de skills dos agentes |
| Drivers mod.rs avulsos | Driver Agents com `HardwareDriver` capability |
| Boot linear de funções | Boot = chain de agent activations |
| Executor coopera loop | AgentScheduler coordena agentes |
| Trust por token | Trust por agente + token + capability |
| `/add_skill` via LLM | LLM cria Agent + Skills atomically |

#### O Agente

```rust
pub enum AgentKind {
    System,      // init, monitor, lifecycle
    Driver,      // hardware driver
    Inference,   // LLM, MLP
    Router,      // intent routing
    Console,     // I/O (keyboard, VGA, serial)
    Network,     // network stack poll
    Skill,       // pure skill agent (no persistence)
}

pub struct AgentManifest {
    pub name: &'static str,
    pub kind: AgentKind,
    pub capabilities: &'static [Capability],
    pub auto_start: bool,         // ativa no boot
    pub persist: bool,            // respawn on crash
    pub schedule: ScheduleKind,   // PollEveryNTicks, EventDriven, Continuous
    pub trust_tokens: &'static [u64],
}
```

#### As Skills

Cada Agente expõe zero ou mais Skills:
```rust
pub struct SkillManifest {
    pub name: &'static str,
    pub description: &'static str,
    pub agent: &'static str,         // agente dono
    pub required_tokens: &'static [u64],
    pub completes_run: bool,         // skill terminal?
    pub instructions: &'static str,  // para LLM routing
}
```

#### Boot = Agent Activation Chain

```
cargo run → bootloader → kernel_main
  ├─ [Agent] vga_buffer::init()           → ConsoleAgent (VGA+Serial)
  ├─ [Agent] interrupts::init_idt()       → SystemAgent (GDT+IDT+TSS)
  ├─ [Agent] memory::init_memory()        → MemoryAgent (PageTable)
  ├─ [Agent] allocator::init_heap()       → MemoryAgent (Heap)
  ├─ [Agent] simd::enable_simd()          → SystemAgent (CR0/CR4)
  ├─ [Agent] pci::init_pci()              → PCIAgent (PCI scan)
  ├─ [Agent] acpi::init_acpi()            → ACPIAgent (RSDP+MADT)
  ├─ [Agent] smp::init_smp()              → SMPAgent (AP boot)
  ├─ [Agent] inventory::hardware_scan()   → HwDiscoverAgent
  │   ├── detecta RTL8139 → NetDriverAgent
  │   └── detecta xHCI   → UsbDriverAgent
  ├─ [Agent] cortex::TransformerModel     → CortexAgent (LLM)
  ├─ [Agent] HermesAgent (intent + console + input)
  └─ AgentScheduler::run()
       └─ Cada tick: poll agents, route events, respawn mortos
```

#### Reclassification Grid (Implementado → Migrar)

| ID Antigo | Nome | Vira Agente | Tipo | Novo ID |
|---|---|---|---|---|
| task #1 | system_daemon | SystemAgent | System | A-001 |
| task #2 | hardware_monitor | MonitorAgent | System | A-002 |
| task #3 | hw_bridge | HwBridgeAgent | Router | A-003 |
| task #4 | network_agent | NetAgent | Network | A-004 |
| task #5 | input_daemon | InputAgent | Console | A-005 |
| task #6 | cortex_llm | CortexAgent | Inference | A-006 |
| task #7 | intent_router | HermesAgent | Router | A-007 |
| task #8 | hermes_console | ConsoleAgent | Console | A-008 |
| — | rtl8139 driver | NetDriverAgent | Driver | A-009 |
| — | xhci driver | UsbDriverAgent | Driver | A-010 |
| — | self_heal | SelfHealAgent | System | A-011 |
| — | memory/allocator | MemoryAgent | System | A-012 |
| — | pci, acpi, apic | PlatformAgent | System | A-013 |
| — | smp | SMPAgent | System | A-014 |
| — | trust cache | TrustAgent | System | A-015 |
| — | skill_loader | SkillManagerAgent | System | A-016 |

#### AgentRegistry + Scheduler

O `AgentRegistry` substitui o `SkillRegistry` atual como catálogo central:

```rust
pub struct AgentRegistry {
    agents: Vec<AgentInstance>,        // todos agentes registrados
    skills: BTreeMap<String, SkillRef>, // skills catalogadas por nome
    scheduler: AgentScheduler,
}

pub struct AgentInstance {
    manifest: AgentManifest,
    state: AgentState,  // Inactive, Active, Blocked, Crashed
    tick_budget: u64,
    last_poll: u64,
}
```

O `AgentScheduler` substitui o `NeuralExecutor`:
- Cada agente declara seu ScheduleKind
- A cada tick, scheduler pergunta: "este agente deve rodar agora?"
- Se sim, chama `agent.tick()` (que pode fazer polling de eventos, processar I/O, etc.)
- Se crash, verifica `persist` → respawn automático

#### | # | Item | Destino | Target | Motivação |

| A-001 | **Agent trait + AgentManifest** — nova trait unificada que substitui as 8 async fn avulsas. `Agent { manifest, tick(), skills() }` | 🟡 Sprint 40+ | Sprint 40 | Coração da virada. Cada task vira um Agent. |
| A-002 | **AgentRegistry** — catálogo central de todos agentes. Substitui parcialmente SkillRegistry (skills viram sub-recursos dos agentes) | 🟡 Sprint 40+ | Sprint 40 | Registry de agentes vivos + skills indexadas. |
| A-003 | **AgentScheduler** — substitui NeuralExecutor. Poll agents pelo schedule declarado, não por ordem fixa. Suporta: tick-based, event-driven, continuous | 🟡 Sprint 40+ | Sprint 40 | Executor v2. |
| A-004 | **Reclassification: 8 tasks → 8 agents** — system_daemon→SystemAgent, hw_bridge→HwBridgeAgent, input→InputAgent, cortex_llm→CortexAgent, intent_router→HermesAgent, hermes_console→ConsoleAgent, monitor→MonitorAgent, network→NetAgent | 🟡 Sprint 40+ | Sprint 40 | Refactor do main.rs. |
| A-005 | **Driver Agents** — rtl8139, xhci, pci, acpi, apic, smp viram agents com `AgentKind::Driver` ou `AgentKind::System`. Cada um expõe skills de hardware | 🟡 Sprint 41+ | Sprint 41 | Drivers entram no ecossistema de agentes. |
| A-006 | **AgentState Machine** — Inactive→Active→Blocked→Crashed→Respawn. Scheduling consciente do estado | 🟡 Sprint 40+ | Sprint 40 | Scheduler só polla agentes Active. |
| A-007 | **Capability-Based Routing** — EventBus roteia eventos para agents que declaram `Capability` relevante. Substitui match fixo do intent_router | 🟡 Sprint 41+ | Sprint 41 | Routing dinâmico. |
| A-008 | **Agent Identity + SOUL.md** — Cada agente tem identidade persistente (SOUL.md no SFS). CortexAgent tem persona, ConsoleAgent tem voice | ⏳ Pós-MVP | Sprint 42+ | Requer SFS. |
| A-009 | **SkillManagerAgent** — Agente especializado em criar, editar, remover skills. `/add_skill` vira delegado a este agente. LLM gera skill, SkillManagerAgent registra | 🟡 Sprint 40+ | Sprint 40 | Já parcialmente implementado via PENDING_SKILL. |
| A-010 | **TrustAgent** — Centraliza toda autorização. Substitui TrustCache avulso. Agents consultam TrustAgent antes de executar skills | 🟡 Sprint 40+ | Sprint 40 | Trust como agente, não cache solto. |
| A-011 | **SelfHealAgent** — Já implementado como SelfHeal struct. Migrar para AgentKind::System com skill `recover` | 🟡 Sprint 40+ | Sprint 40 | Self-healing como agente. |
| A-012 | **MemoryAgent** — Gerencia BitmapFrameAllocator, MHI tiers, Slab. Skills: `alloc`, `dealloc`, `status` | 🟡 Sprint 41+ | Sprint 41 | Memória como agente. |
| A-013 | **Agent Schedules** — Tick-based (poll a cada N ticks), Event-driven (só acorda com evento), Continuous (roda todo tick), Idle (só responde, nunca inicia) | 🟡 Sprint 40+ | Sprint 40 | Eficiência energética. |
| A-014 | **Agent Budget + Watchdog** — Cada agente tem tick_budget por ciclo. Se excede, watchdog pausa. Implementa IterationBudget (#199) | 🟡 Sprint 40+ | Sprint 40 | Previne runaway agents. |
| A-015 | **Agent Hooks** — Pre/Post tick hooks. HookRegistry com slots fixos de function pointers. Hooks retornam Allow/Block/Modify (#206) | 🟡 Sprint 41+ | Sprint 41 | Plugin system via hooks. |
| A-016 | **Multi-Agent Orchestration** — Graph-based: sequential, concurrent, handoff entre agents. EventBus padrão MS Agent (#235) | 🟡 Sprint 41+ | Sprint 41 | Composição de agentes. |
| A-017 | **Agent as Pure Function** — Event-sourced: `f(history) -> next action`. Cada tick do agent é um ConversationEvent (#231) | 🟡 Sprint 41+ | Sprint 41 | Replay, debug, rollback. |
| A-018 | **Agent Identity Awakening** — Duas personalidades por agent: "Awakening" (first boot) e "Established" (memória presente). MLP weights diferentes (#202) | ⏳ Pós-MVP | Sprint 42+ | Requer SFS para memória. |
| A-019 | **Council Agent** — Antes de decisão ambígua, 3 sub-agentes (Otimista, Cético, Pragmático) votam. Argmax vence (#191) | ⏳ Pós-MVP | Sprint 42+ | Qualidade de decisão. |
| A-020 | **HermesAgent como Supervisor** — HermesAgent (intent_router atual) coordena os demais agentes. Decide qual agente ativar baseado na intenção do usuário | 🟡 Sprint 40+ | Sprint 40 | Já é assim na prática. Formalizar. |

**Nota:** A refatoração agent-first é **aditiva**, não disruptiva. Cada agent pode ser introduzido um por vez, mantendo compatibilidade com o executor atual. A migração começa encapsulando as 8 async fn em `Agent::tick()`, depois substitui o NeuralExecutor pelo AgentScheduler.

---

## Seção 2 — Mapa de Calor

| Fonte | Total | ✅ No MVP | 🟡 Sprint | ⏳ Pós-MVP | 💰 Sponsor | ❌ Descarte |
|---|---|---|---|---|---|---|
| USB | 15 | 0 | 0 | 15 | 0 | 0 |
| SMP/APIC | 20 | 17 | 3 | 0 | 0 | 0 |
| NPU XDNA | 10 | 1 | 1 | 0 | 8 | 0 |
| AI Detection | 10 | 9 | 0 | 1 | 0 | 0 |
| MHI | 12 | 8 | 1 | 3 | 0 | 0 |
| Periféricos | 11 | 3 | 0 | 6 | 0 | 0 |
| Rede/Network | 8 | 0 | 6 | 2 | 0 | 0 |
| Áudio/Vídeo | 6 | 0 | 0 | 4 | 0 | 2 |
| Princípios | 6 | 6 | 0 | 0 | 0 | 0 |
| Roadmap Memória | 4 | 2 | 0 | 2 | 0 | 0 |
| Roadmap Kernel | 4 | 1 | 0 | 3 | 0 | 0 |
| Roadmap EventBus | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Skills | 3 | 1 | 0 | 2 | 0 | 0 |
| Roadmap Cognitive | 4 | 0 | 0 | 4 | 0 | 0 |
| Roadmap Timeline | 3 | 0 | 0 | 3 | 0 | 0 |
| Outras | 5 | 4 | 0 | 0 | 1 | 0 |
| Docs/ADRs | 3 | 3 | 0 | 0 | 0 | 0 |
| Neural Cortex LLM (1.18) | 16 | 0 | 14 | 2 | 0 | 0 |
| Transformer Engine (1.19) | 7 | 0 | 7 | 0 | 0 | 0 |
| Success Engine (1.20) | 4 | 0 | 0 | 4 | 0 | 0 |
| Treinamento (1.21) | 4 | 0 | 4 | 0 | 0 | 0 |
| Self-Optimization (1.22) | 7 | 0 | 5 | 2 | 0 | 0 |
| Crom Ecosystem (1.23) | 13 | 2 | 5 | 6 | 0 | 0 |
| Life OS Ecosystem (1.24) | 22 | 0 | 18 | 4 | 0 | 0 |
| Tier 2 PAI Ecosystem (1.25) | 15 | 4 | 6 | 3 | 0 | 2 |
| Tier 3 Memory Systems (1.26) | 14 | 0 | 9 | 5 | 0 | 0 |
| Tier 4 Agent Frameworks (1.27) | 22 | 0 | 17 | 3 | 0 | 2 |
| Self-Healing Kernel (Sprints 32-37) | 6 | 6 | 0 | 0 | 0 | 0 |
| Agent/Skill-First Architecture | 20 | 0 | 20 | 0 | 0 | 0 |
| Bugfix Estrutural (Sprint 45) | 6 | 5 | 1 | 0 | 0 | 0 |
| Sprint Planning (Seção 6) | 47 | 0 | 47 | 0 | 0 | 0 |
| **Total** | **336** | **73 (22%)** | **164 (49%)** | **84 (25%)** | **9 (3%)** | **6 (2%)** |

---

## Seção 3 — Hierarquia Técnica de Dependências (Pós-MVP)

Cada item ⏳ e 💰 abaixo tem seus pré-requisitos e bloqueios mapeados. A regra: **um item na camada N só começa quando todos os pré-requisitos das camadas < N estão estáveis.**

### Notação

```
Item [ID] — nome
  Pré: IDs dos pré-requisitos
  → Bloqueia: IDs que dependem deste
  Razão: por que está aqui
```

### Camada 0 — Já Existe (MVP Genesis)

```
[46-55] HardwareInventory + MLP 512→256→64→9
[56-67] MemoryHierarchy + AllocTier + alloc_by_tier(Dram)
[68-69] PCI scan CF8/CFC
[16-19] LAPIC/IOAPIC + MADT
[24-33] PerCpu + trampoline + SMP
[94] Slab Allocator
[91] Bitmap Frame Allocator
[95] Async Neural Executor
[99] EventBus + CapabilityToken
[102] Skill trait + MCP + Registry
```

Nada nesta camada depende de itens pós-MVP.

### Camada 1 — Drivers de Dispositivo (Sprint 23+)

```
[1] xHCI controller mínimo
  Pré: [68] PCI scan, [17] IOAPIC
  → Bloqueia: [2, 3, 6, 8, 9, 10, 11, 12, 13, 14, 84]
  Razão: PS/2 legacy funciona. USB = centenas de LOC, sem skill no MVP.

[2] identify_device() → VID/PID/class
  Pré: [1]
  Razão: sem xHCI, sem dispositivo USB.

[9] Nível 1 — HW Detection (xHCI sem IA)
  Pré: [1], [2]
  Razão: depende de xHCI funcionando.

[10] Nível 2 — Device Classification (MLP 7→5)
  Pré: [9], [47]
  Razão: primeiro hardware real para classificar.

[11] Nível 3 — Dynamic Interface Creation (WASM)
  Pré: [9], [103]
  Razão: requer WASM + xHCI.

[12] USB flow: desconhecido → porta desabilitada
  Pré: [1], [89] zero-autorun
  Razão: política, mas precisa de xHCI.

[13] USB flow: trust-once → auto-ON
  Pré: [1], [4] TrustCache
  Razão: TrustCache existe, falta xHCI.

[14] USB flow: usuário precisa inferir intenção
  Pré: [12]
  Razão: princípio + xHCI.

[15] "Zero autorun, zero superfície de ataque USB"
  Pré: [12, 13, 14] fluxos completos
  Razão: princípio final.

[79] UEFI framebuffer (BGRA32)
  Pré: BootInfo::framebuffer (do bootloader crate)
  → Bloqueia: [80, 81, 82]
  Razão: VGA text serve. Framebuffer é upgrade visual.

[80] Font rendering
  Pré: [79]
  Razão: sem framebuffer, sem render.

[60] AllocTier::Vram (BAR da GPU)
  Pré: [68] PCI + BAR mapeado, [79] ou driver GPU
  Razão: BAR existe, mas driver GPU não. MVP aloca em DRAM.
```

### Camada 2 — Armazenamento e Persistência (Sprint 24+)

```
[71] NVMe driver (PCI Class 01.08)
  Pré: [68] PCI, [17] IOAPIC/MSI-X, [25] PerCpu
  → Bloqueia: [61, 62, 72]
  Razão: MVP é stateless. Sem SFS, NVMe é peso morto.

[72] VirtIO-blk (PCI 1AF4:1001)
  Pré: [68] PCI, [17] IOAPIC
  → Bloqueia: [61, 62]
  Razão: alternativa NVMe. Mesma dependência SFS.

[73] VirtIO-net (PCI 1AF4:1041)
  Pré: [68] PCI, [17] IOAPIC/MSI
  Razão: MVP sem rede. Nenhuma skill precisa de rede.

[61] AllocTier::Nvme (alocar via SFS)
  Pré: [71] NVMe ou [72] VirtIO-blk + SFS
  → Bloqueia: [62]
  Razão: requer NVMe + SFS.

[62] AllocTier::Hdd (cold storage)
  Pré: [61] ou driver ATA + SFS
  Razão: cold storage = SFS sobre HDD.

[70] PCI bridges (hierarquia)
  Pré: [68] (scan cego funciona sem)
  Razão: scan bus 0..255 funciona. Bridges são refinamento.

[6] Trust Cache persistente no SFS
  Pré: [4] TrustCache (Block 5), SFS
  Razão: TrustCache existe, mas sem SFS é volátil.

[52] Atualizar MLP via WASM
  Pré: [103] WASM, [73] VirtIO-net (rede)
  Razão: requer WASM + rede.
```

### Camada 3 — VirtIO e Aceleração Gráfica (Sprint 24+)

```
[81] VirtIO-GPU 2D/3D
  Pré: [74] VirtIO-gpu básico
  Razão: VGA text é suficiente.

[82] Tensor visualization no framebuffer
  Pré: [79] framebuffer, [81] VirtIO-GPU
  Razão: depende de framebuffer + GPU.
```

### Camada 4 — Scheduler e Runtime (Sprint 24+)

```
[96] Agent Scheduler (round-robin)
  Pré: [95] Executor (existe), [24-33] SMP (>1 core)
  → Bloqueia: [97, 98, 105]
  Razão: Executor cooperativo funciona para 1-4 cores.

[97] Budget de execução (tokens_consumed)
  Pré: [96]
  Razão: sem scheduler, budget não tem onde atuar.

[98] MLP decide prioridade no scheduler
  Pré: [96], [47] MLP
  Razão: scheduler precisa existir antes.

[100] Topic enum completo
  Pré: [99] EventBus (existe)
  Razão: strings funcionam. Enum é segurança de tipo.

[101] ML-based routing no EventBus
  Pré: [99] EventBus, [47] MLP, [100] Topic enum
  Razão: inovação futura. BTreeMap resolve.
```

### Camada 5 — WASM Embedder (Sprint 25+)

```
[103] WASM embedder (wasmi no_std)
  Pré: [94] Slab, [96] Scheduler
  → Bloqueia: [8, 11, 52, 104]
  Razão: Skills Rust traits bastam. WASM é upgrade de portabilidade.

[104] Linear memory pool (256 KB/skill)
  Pré: [103]
  Razão: sem WASM, sem pool.

[8] WASM skill dispatch para USB
  Pré: [1] xHCI, [103] WASM
  Razão: USB + WASM = duplo pós-MVP.

[159] Auto-Skill Generator — cria skill WASM para workflow detectado
  Pré: [103] WASM, [157] Usage Pattern Analyzer
  Razão: requer WASM + detector de padrões de uso.
```

### Camada 6 — Memória Avançada (Sprint 23-24+)

```
[92] Huge Pages 2 MiB
  Pré: [91] BitmapAllocator (existe), page table 2 MiB mapper
  → Bloqueia: [93, 107]
  Razão: MVP não tem inferência pesada. MLP de arquitetura (37 KB) cabe em 1 página 4 KiB.

[93] Huge Pages 1 GiB
  Pré: [92], CPUID check
  → Bloqueia: [107]
  Razão: 1 GiB depende de 2 MiB + hardware real.

[107] Neural Cache (lookup table 50 ns)
  Pré: [92] Huge Pages, [105] Intent Planner
  Razão: cache de decisões só faz sentido com planner.
```

### Camada 7 — Cognitive Runtime (Fase 6)

```
[105] Intent Planner (sequência de SkillCommands)
  Pré: [96] Scheduler, [47] MLP, [103] WASM
  → Bloqueia: [106, 107]
  Razão: MVP classifica intent única. Planner multi-etapa requer scheduler + WASM.

[106] Success Engine (feedback loop online)
  Pré: [105] Planner, [47] MLP (pesos ajustáveis)
  Razão: pesquisa acadêmica. Ajuste online de pesos em no_std.

[51] Treinamento offline do MLP (10k profiles)
  Pré: [47] MLP Block 4, dataset sintético
  Razão: pesos heurísticos funcionam. Treinamento real depois.
```

### Camada 8 — Meta / MatMul-Free (Fase 7)

```
[108] MatMul-free LM (RWKV/Mamba/ternary pooling)
  Pré: [107] Neural Cache, [92] Huge Pages, [103] WASM
  Razão: futuro distante. Roadmap original já marcava Fase 7.
```

### Camada 9 — Self-Optimization & Workflow Learning (Sprint 27+)

```
[157] Usage Pattern Analyzer — LLM detecta workflow do usuário
  Pré: [126-131] Transformer Engine + Cortex Daemon (Sprint 25), [99] EventBus
  → Bloqueia: [158, 159, 161, 162, 163]
  Razão: precisa do LLM rodando para analisar padrões de intenção.

[158] Workflow Predictor — pré-carrega recursos por hora/padrão
  Pré: [157]
  → Bloqueia: [162]
  Razão: predição sem análise de padrão é chute.

[160] Dynamic Resource Scaling — MHI auto-ajuste por uso real
  Pré: [56-67] MHI tiers (existe), [157] Usage Pattern Analyzer
  Razão: MHI hoje é estático. Scaling dinâmico requer análise de uso.

[161] Self-Optimizing Scheduler — prioriza por workflow detectado
  Pré: [96] Agent Scheduler (Layer 4), [157] Usage Pattern Analyzer
  Razão: scheduler precisa existir antes de ser auto-otimizado.

[162] Workflow Profile — perfil exportável
  Pré: [157], [158] Workflow Predictor, SFS (Layer 2)
  Razão: requer análise + predição + persistência.

[163] Hardware Config Learning — SystemArchitecture evolve
  Pré: [135] LLM decide hardware arch (Sprint 26), [157]
  Razão: heurística de boot vira LLM query contínua.
```

### Camada S — Sponsor / Hardware Real

```
[36-40, 43, 45] NPU XDNA driver completo
  Pré: [68] PCI, AMD APU real, documentação XDNA
  Razão: sem hardware, sem QEMU com NPU, sem testabilidade.

[116] Port ARM/RISC-V
  Pré: nova arch target
  Razão: x86-64 é o target do MVP. ARM/RISC-V seria novo projeto.
```

### Grafo Resumido

```
MVPs ─── B1(PCI) ─── B2(SMP) ─── B3(Chat) ─── B4(MLP) ─── B5(Skills) ─── MVP
  │           │                                          │
  │           ▼                                          ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 1   │                            │ Layer 4      │
  │     │ S23+      │                            │ S24+         │
  │     │ xHCI/FB   │                            │ Scheduler    │
  │     └─────┬─────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 2   │                            │ Layer 5      │
  │     │ S24+      │                            │ S25+         │
  │     │ NVMe/SFS  │                            │ WASM         │
  │     └─────┬─────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 3   │◄── [107] NCache ◄── [105]  │ Layer 7      │
  │     │ VirtIO-GPU│                            │ Planner      │
  │     └───────────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 6   │◄────────────────────── [108]│ MatMul-Free  │
  │     │ HugePages │                            │ (Fase 7)     │
  │     └───────────┘                            └──────┬───────┘
  │           ▼                                        ▼
  │     ┌───────────┐                            ┌──────────────┐
  │     │ Layer 9   │◄── [157-163]                │ Self-Optim   │
  │     │ Workflow  │                            │ Sprint 27+   │
  │     │ Learning  │                            └──────────────┘
  │     └───────────┘
  │
  └── Layer S (Sponsor): NPU XDNA, ARM/RISC-V — sem data
```

---

## Seção 4 — Regras de Engenharia (derivadas da hierarquia)

1. **Camadas estritas:** Item na camada N só começa quando todos os pré-requisitos das camadas < N estão estáveis. Ex: NVMe (Layer 2) não começa antes de PCI (Layer 0) estar compilando e testado.

2. **Teto de camada por sprint:** Cada sprint tem um teto de camada. Sprint 23 → Layer 1. Sprint 24 → Layer 2. Sem dispersão.

3. **Sponsor = sem data:** A stack de software (PCI, APIC, SMP) estará pronta antes. NPU pode ser integrada assim que hardware chegar.

4. **Nada bloqueia o MVP:** Todo item pós-MVP tem caminho de volta para a chain principal (Block 1→5). Se MVP termina em S22, Layer 1 começa limpo em S23.

5. **Revisão contínua:** Se um pré-requisito muda de camada (ex: Huge Pages se torna essencial para MLP), o item sobe. A hierarquia é revisada a cada sprint review.

---

## Seção 6 — Sprint Planning (itens 🟡 remanescentes, consolidados por bloco)

Blocos reconsolidados após v0.47.0. Itens já implementados foram removidos. Blocos com afinidade de assunto foram fundidos.

### Bloco 12 — Network + Platform Drivers (fundido com Bloco 18)
**Foco:** MCP Server, Cron Scheduler, PCI bridges, Huge Pages, x2APIC

| Item | O que | LOC |
|---|---|---|
| #172 | MCP Server support via EventBus + JSON-RPC | ~400 |
| #236 | Plugin Hub / MCP Index com AI security scan | ~400 |
| #232 | Cron Scheduler baseado em LAPIC timer | ~350 |
| #18 | x2APIC mode (MSR-based, sem MMIO) | ~100 |
| #34 | acpi crate para parser MADT/PPTT | ~200 |
| #35 | raw-cpuid crate (features de CPU) | ~100 |
| #70 | PCI bridges (hierarquia multi-barramento) | ~100 |
| #92 | Huge Pages 2 MiB | ~200 |
| #93 | Huge Pages 1 GiB | ~100 |
| | **Total bloco** | **~1950 LOC** |

**Itens do Bloco 12 original já implementados:** DHCP (#251), ARP (#252), VirtIO-net (#73), requires_network (#122).

### Bloco 13 — Trust & Security
**Foco:** Upgrade do TrustCache, identidade criptográfica, pipeline de segurança

| Item | O que | LOC |
|---|---|---|
| #166 | Multi-mode Trust (TotalAccess/AskEveryTime/Scoped) | ~100 |
| #176 | Ed25519 Cryptographic Identity (substitui CapabilityToken) | ~300 |
| #256 | Path Confinement para Skills | ~60 |
| #257 | Mask Secrets no TrustCache | ~50 |
| #258 | Graduated Enforcement (Observe→Warn→Contain→Enforce) | ~80 |
| #259 | Posture-Aware Alerting (skills checam hardware antes) | ~40 |
| #198 | Boot-time security policy (regex patterns compilados) | ~100 |
| #260 | Event→Detector→Response Pipeline (5 detectores) | ~200 |
| | **Total bloco** | **~930 LOC** |

### Bloco 14 — Hermes Cognitive + Self-Optimization (fundido com Bloco 16)
**Foco:** HermesAgent upgrade, ReAct 7 fases, Usage Pattern Analyzer, scheduler adaptativo

| Item | O que | LOC |
|---|---|---|
| #178 | Runtime SDD (goal/context/plan/rollback antes de skill) | ~80 |
| #190 | Algorithm loop 7 fases (THINK→PLAN→BUILD→EXECUTE→VERIFY→LEARN) | ~300 |
| #191 | Council skill (3 vozes Otimista/Cético/Pragmático votam) | ~150 |
| #193 | Bitter Pill Engineering (força cargo check antes de deploy) | ~100 |
| #184 | Intent Transparency (mostrar confidence, alternatives) | ~200 |
| #203 | Context Fencing + Streaming Scrubber | ~150 |
| #180 | DA Identity Layer (SOUL.md para Hermes) | ~100 |
| #157 | Usage Pattern Analyzer (LLM detecta workflow) | ~250 |
| #158 | Workflow Predictor (pré-carrega MHI por hora/padrão) | ~200 |
| #160 | Dynamic Resource Scaling (MHI auto-ajuste) | ~200 |
| #161 | Self-Optimizing Scheduler (prioriza por workflow) | ~300 |
| #163 | Hardware Config Learning (SystemArchitecture evolve) | ~150 |
| #135 | LLM decide hardware arch (substitui heurística) | ~100 |
| #136 | LLM decide memory tier (roteia alocações) | ~100 |
| #139 | Reflex MLP threshold tuning (bypassa LLM se >0.9) | ~80 |
| | **Total bloco** | **~2460 LOC** |

### Bloco 15 — Memory Systems + Semantic Snapshot
**Foco:** Memória persistente, Ebbinghaus decay, grafo de conhecimento, CDC+XDI já base

| Item | O que | LOC |
|---|---|---|
| #214 | SHA-256 Memory Dedup (5min sliding window) | ~100 |
| #215 | Privacy Filter (stripa secrets antes de armazenar) | ~80 |
| #216 | Memory TTL/Eviction (TimeToLive, ImportanceRank) | ~150 |
| #219 | Ebbinghaus Decay para TrustCache | ~120 |
| #217 | Hybrid Search (BM25 + MLP) para intent routing | ~200 |
| #218 | 4-Tier Memory Consolidation (Working→Episodic→Semantic→Procedural) | ~400 |
| #222 | Metacognitive Guard (verifica erros passados antes de skill) | ~300 |
| #223 | Draft→Review→Merge Memory (workflow de aprovação) | ~350 |
| #224 | Atkinson-Shiffrin 3-tier (Sensory→STM→LTM) | ~800 |
| | **Total bloco** | **~2500 LOC** |

**Base já implementada:** CDC Rabin chunking (`chunker.rs`), XOR Delta (`delta.rs`), Semantic Snapshot via `SelfHeal::semantic_snapshot()`.

**✅ Implementado em v0.56.0:** SHA-256 Dedup (#214), Privacy Filter (#215), Memory TTL/Eviction (#216), Hybrid Search (#217), 4-Tier Consolidation (#218), Ebbinghaus Decay (#219), Metacognitive Guard (#222), Draft→Review→Merge (#223), Atkinson-Shiffrin (#224).

### Bloco 17 — Cortex LLM v2
**Foco:** Modelo maior, sampling, codebook compression, update HTTP

**✅ Implementado em v0.56.0:** Sampling (#148), Codebook VQ (#169), MODEL_UPDATE topic (#134).

| Item | O que | LOC |
|---|---|---|
| #148 | Sampling: argmax, top-k(3/5/10), temperature | ~80 |
| #133 | Modelo 1.5B params (distilado Llama 3.2 → ternário) | Python |
| #134 | Model update via HTTP (download .bitnet → hot-swap) | ~200 |
| #141 | 1.5B model benchmark (5-15 tok/s x86-64) | ~50 |
| #169 | Codebook Compression (VQ) para PackedTernaryTensor | ~300 |
| #170 | KV Cache Codebook (VQ no cache de atenção) | ~200 |
| | **Total bloco** | **~830 LOC + Python** |

### Bloco 21 — Foundation Quick Wins (Sprint 77)
**Itens independentes dos sprints 60/67/72 — sem dependências entre si**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 60.1b | Sprint 60 | Prompt `>` interativo (Hermes aguarda input) | ~30 |
| 67.0.3 | Sprint 67 | Pre-Flight Principle (Skill::verify pré-execução) | ~80 |
| 67.2.3 | Sprint 67 | Background Fan-out (delegação automática) | ~80 |
| 72.2 | Sprint 72 | TaskSchema + JobPreconditions (schema de tarefas) | ~200 |
| 72.6 | Sprint 72 | SkillIndex + MCP Catalog (índice + catálogo) | ~150 |
| 67.2.2 | Sprint 67 | Completion Contracts (verificação pós-skill) | ~100 |
| 67.2.1 | Sprint 67 | `/learn` command (SKILL.md generator) | ~120 |
| | | **Total bloco** | **~760 LOC** |

### Bloco 22 — Agentic Evolution + Memory Systems (Sprint 78)
**Completa Agentic Evolution (72) + Memory bridge + Loaders**

| Item | Origem | O que | LOC |
|---|---|---|---|
| 72.1 | Sprint 72 | Crew + FlowTrigger (orquestração multi-agente) | ~300 |
| 72.3 | Sprint 72 | IntentCache + OutputCache (cache de intenção/saída) | ~200 |
| 72.4 | Sprint 72 | WorkflowEngine + SelfCritique (engine de workflow) | ~250 |
| 72.5 | Sprint 72 | StateGraph Scheduler (agendamento por grafo) | ~200 |
| 60.8c | Sprint 60 | migrate_to_tier() (page table manipulation MHI) | ~170 |
| 62.2 | Sprint 62 | MHI+FS Bridge (suggest_tier_for_path) | ~300 |
| #278 | GGUF Loader (modelos 1B+ params) | ~500 |
| #306 | WASM Runtime (wasmi + WASI→Skill bridge) | ~800 |
| | | **Total bloco** | **~2720 LOC** |

### Bloco 23 — LLM Infrastructure + MoE (Sprint 79)
**Model loading, router MoE, training infrastructure**

| Item | O que | LOC |
|---|---|---|
| #311 | Trinity Router (MoE — classifica intenção, roteia expert) | ~500 |
| #312 | TrainingAgent (fine-tune/transfer/full on-device) | ~500 |
| #313 | Self-Learning OS (DataCollector dos próprios logs) | ~300 |
| — | AVX2 BitNet Kernel (intrinsics SIMD) | ~150 |
| | **Total bloco** | **~1450 LOC** |

### Bloco 24 — JARVIS Core Persona (Sprint 80)
**Foco:** SOUL.md personality engine, IPW monitoring, session compression, notification gate, sessionless thread

| Item | O que | LOC |
|---|---|---|
| #315.1 | SOUL.md Personality Engine — parser + persona + adaptive tone | ~300 |
| #315.2 | IPW Monitor — RAPL MSR 0x610, tokens/watt, cache ratio | ~150 |
| #315.3 | Session Compression — Summarize/DropLowest/MergeSimilar/SegmentMeans | ~200 |
| #315.4 | Notification Gate — 4 urgency levels, rate limiting, dedup | ~200 |
| #315.5 | Sessionless Thread — conversa contínua sem reset | ~100 |
| | **Total bloco** | **~950 LOC** |

### Bloco 25 — JARVIS Emotion + Cache + Pipeline (Sprint 81)
**Foco:** Emotion analysis, capability contracts, skill discovery, semantic cache, persona pipeline

| Item | O que | LOC |
|---|---|---|
| #315.6 | Emotion Analysis — BitNet classifier 7 emoções + adjust_tone | ~250 |
| #315.7 | Capability Contract + Consent Gates — 3 níveis de risco | ~200 |
| #315.8 | Skill Discovery — DSPy/ACE pipeline observe→analyze→propose | ~300 |
| #315.9 | ADE Pipeline — Spec→Execute→Review→Recover | ~200 |
| #315.10 | Semantic Cache — 5-tier routing (NabaOS-inspired) 97.5% reduction | ~150 |
| #315.11 | Persona Pipeline — 16 stages (OVOS-inspired) | ~100 |
| | **Total bloco** | **~1200 LOC** |

### Bloco 26 — JARVIS Cognitive Memory (Sprint 82)
**Foco:** Dreaming/consolidation, ego layer, proactive heartbeats, auto-skill generation, Babel-Index, SleepCycle

| Item | O que | LOC |
|---|---|---|
| #315.12 | Dreaming/Consolidation — CronAgent noturno, memória sintética | ~200 |
| #315.13 | Ego Layer — self-model, confidence tracking, can_answer() | ~250 |
| #315.14 | Proactive Heartbeats — JARVIS inicia conversa | ~100 |
| #315.15 | Tool-State Save Game — snapshot + rollback de skills | ~100 |
| #315.16 | Auto-Skill Generation — watch→pattern→propose→generate | ~150 |
| #315.17 | Babel-Index — entropy + contradiction + staleness monitoring | ~100 |
| #314 | SleepCycle Agent — 5 fases: REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT | ~780 |
| | **Total bloco** | **~1680 LOC** |

### Bloco 27 — JARVIS Security + AHCI (Sprint 83)
**Foco:** Fail-closed safety, Merkle audit trail, fluid persona, SATA driver

| Item | O que | LOC |
|---|---|---|
| #315.18 | Fail-Closed Safety Invariant — SMT-proof, 4 invariants | ~200 |
| #315.19 | Merkle Audit Trail — Ed25519 signed chain, ring buffer 4096 | ~200 |
| #315.20 | Fluid Persona — context-adaptive, coach/tutor/tool modes | ~100 |
| — | AHCI driver (SATA 6G NCQ) | ~700 |
| | **Total bloco** | **~1200 LOC** |

### Bloco 21c — GPU Foundations (Sprint 84)
**Foco:** GPU BAR mapping, secure boot (ACR/PSP/GuC), doorbell, SPSC job ring, VRAM allocator. NVIDIA/AMD/Intel.

| Item | O que | LOC |
|---|---|---|
| #67 | AllocTier::Vram — alocar no BAR da GPU | ~50 |
| #326 | GPU BAR0/BAR1 mapping UC (NVIDIA/AMD/Intel) | ~300 |
| #327 | GPU doorbell + SPSC job ring | ~400 |
| #328 | VRAM buddy allocator | ~400 |
| #352 | Secure Boot GPU — ACR/PSP/GuC pipeline | ~600 |
| #353 | GPU Compute Pipeline — submissão genérica | ~300 |
| | **Total bloco** | **~1700 LOC** |

### Bloco 21d — GPU Decode (Sprint 85)
**Foco:** BitNet decode na GPU. Prefill CPU, decode GPU.

| Item | O que | LOC |
|---|---|---|
| #329 | Agent.xpu prefill/decode split | ~400 |
| #330 | GPU matmul kernel ternário (PTX/AQL/GEN) | ~300 |
| #331 | CPU→GPU KV cache DMA | ~200 |
| #332 | XQueue preemptível (XSched, 3 níveis) | ~600 |
| | **Total bloco** | **~1500 LOC** |

### Bloco 29+ — AIOS Evolution (Sprint 85+, pós B-01)
**Itens de rede (LAN):** B-01 NIC/DHCP era o gatekeeper histórico. **B-01 MORTO** via SLIP (#415) — voz local **não** depende de NIC.

| Item | O que | LOC | Bloqueador |
|---|---|---|---|
| B-01 | RX fix (RTL8139 DHCP/RX) | ~500 | ✅ morto — SLIP serial tunnel (#415); NIC emulada ainda frágil |
| #307 | WWW Agents (Browser, Email, RSS, Search, Download, WS) | ~2600 | 🟡 parcial / bridge SLIP |
| #306b | Self-Update Agent (A/B slots + rollback) | ~800 | 🟡 pós-rede estável |
| #236 | Plugin Hub + Marketplace | ~400 | 🟡 pós-rede estável |
| #315.N+1 | Voice Pipeline (Piper TTS + Vosk STT + Wake Word + Wyoming) | ~1600 | ❌ supersedido — Piper+CTC nativo; sem Vosk/Wyoming. ADR-0045 / leftovers → Sprint Sound |
| #315.N+1b | Multi-device sync (CRDT) | ~300 | 🔴 rede |
| #315.N+2 | SKYNET Mesh Node | ~300 | 🔴 rede |
| B-29 | WiFi (Intel/Atheros/Realtek 802.11) | ~1000 | 🟡 em progresso (iwlwifi) |
| | | **~7500 LOC** | |

### Resumo dos 10 blocos futuros (Sprints 77-86)

| Bloco | Sprint | Foco | LOC estimado | Itens 🟡 |
|---|---|---|---|---|
| **21** | **77** | **Foundation Quick Wins** | **~760** | **7** |
| **22** | **78** | **Agentic Evolution** | **~2720** | **8** |
| **23** | **79** | **LLM Infrastructure + MoE** | **~1450** | **4** |
| **24** | **80** | **JARVIS Persona + IPW** | **~950** | **5** |
| **25** | **81** | **JARVIS Emotion + Cache** | **~1200** | **6** |
| **26** | **82** | **JARVIS Cognitive Memory** | **~1680** | **7** |
| **27** | **83** | **JARVIS Security + AHCI** | **~1200** | **4** |
| **21c** | **84** | **GPU Foundations** | **~1700** | **6** |
| **21d** | **85** | **GPU Decode** | **~1500** | **4** |
| **30** | **86** | **JARVIS Persona** | **~950** | **5** |
| **31** | **87** | **JARVIS Security+AHCI** | **~1200** | **4** |
| **32** | **88** | **JARVIS Emotion+Cache** | **~1200** | **6** |
| **33** | **89** | **SleepCycle+Memory** | **~2500** | **11** |
| **34** | **90** | **JARVIS Deep Cognitive** | **~1200** | **6** |
| **35** | **91** | **Polimento+Ecosystem** | **~2500** | **10** |
| **36+** | **92+** | **AIOS Evolution** | **~15000** | **25+** |
| | | **Total** | **~20.260 LOC** | **50** |

### Notas

1. **Readequação 2026-07-04:** Roadmap reorganizado por dependência evolutiva. Foundation → Agentic → LLM → JARVIS → GPU → AIOS.
2. **Blocos 21-23 novos:** Itens independentes dos sprints 60/67/72 que não dependem de B-01.
3. **Blocos 24-27:** JARVIS features reordenadas de Sprints 77-80 para Sprints 80-83.
4. **Bloco 21c (GPU Foundations):** Sprint 84, NVIDIA/AMD/Intel. HW real com GPU.
5. **Bloco 21d (GPU Decode):** Sprint 85, BitNet offload. Depende de Bloco 21c.
6. **Blocos 30-35 (JARVIS + Memory + Polimento):** Sprints 86-91, independentes de HW.
7. **Bloco 36+ (AIOS Evolution):** Sprint 92+, 🔴 bloqueado por B-01.
6. **Blocos legacy (12-17)** mantidos como estão — implementados ou com itens 🟡 independentes.

---


## Seção 1.9 — Itens Catalogados (#355-#408)

Itens adicionados via changelog (2026-07-06 a 2026-07-07).

| # | Item | Destino | Status |
|---|------|---------|--------|
| 355 | buddy-slab-allocator | Sprint 86 | ✅ |
| 356 | edge-dhcp | Sprint 88 | ✅ |
| 357 | khal-std | -- | ❌ |
| 358 | ruvix-net | -- | 🔵 |
| 359 | BGE Embedding | Sprint 89 | ✅ |
| 360 | Kokoro-82M TTS | — | ❌ supersedido (ADR-0045 → Piper) |
| 361 | Zero-Copy SFS | Sprint 96 | ✅ |
| 362 | Episodic memory NVMe | Sprint 95 | ✅ |
| 363 | Skills-as-Modules | Sprint 96 | ✅ |
| 364 | Zero-Trust Syscall | Sprint 92 | 🟡 |
| 365 | Neural Cache decisions | Sprint 92 | 🟡 |
| 366-374 | Self-Healing (9 itens) | Sprint 96 | ✅ |
| 375-377 | GGUF phased (3 itens) | Sprint 99+ | 🟡 |
| 378-382 | GPU Architecture (5 itens) | Sprint 98 | ✅ |
| 383-390 | AIOS Evolution (8 itens) | Sprint 93-97 | ✅ |
| 391-396 | WASM Apps (6 itens) | Sprint 93 | ✅ |
| 397-401 | Micro-Learning (5 itens) | Sprint 95 | ✅ |
| 402-403 | WASM pool + ABI | Sprint 93 | ✅ |
| 404 | NVMe SQ/CQ | Sprint 99+ | ✅ |
| 405 | Capability token crypto | Sprint 93 | 🟡 |
| 406 | VirtIO-GPU fix | Sprint 97 | 🟡 |
| 407-408 | WiFi FW load + async (S1/S4) | S0+prepS1 ✅ SESSION_159; ALIVE/S4 AWAITING | 🟡 |

## Seção 5 — Changelog do Idea Bank

| Data | Mudança | Responsável |
|---|---|---|
| 2026-06-23 | Criação do IDEA_BANK.md — seed dos 116 itens da ADR-0014 + ADR-0015 + dependências | IDA IA |
| 2026-06-23 | Sprint 18: Itens 16-19 (LAPIC/IOAPIC/MADT), 68-69 (PCI scan) → ✅ Block 1 | Dev + IDA IA |
| 2026-06-23 | Itens 34 (acpi crate) → 🟡 Sprint 18 (parser ACPI mínimo implementado, crate não usado) | Dev + IDA IA |
| 2026-06-23 | Sprint 19: Itens 20-33 (SMP multi-core boot) → ✅ Block 2; AP boots with -smp 2 and -smp 4; race fix with spin::Mutex on CPU_COUNT | Dev + IDA IA |
| 2026-06-23 | Sprint 20: Itens 114 (Hermes Chat) → ✅ Block 3; IntentMlp MLP + command parser + console daemon | Dev + IDA IA |
| 2026-06-23 | ADR-0016: Itens 117-125 (Network Strategy) → adicionados; VirtIO-net + smoltcp + HTTP movidos para Sprint 23; MVP+1 = Network Sprint | IDA IA |
| 2026-06-24 | ADR-0017: Itens CRÍTICOS corrigidos (e1000, DHCP, slab, nos, bridge, xsdt, mhi, nn) | IDA IA |
| 2026-06-24 | ADR-0018: Sprint 24 plan (12 HIGH + 16 MEDIUM + 12 LOW bugs) | IDA IA |
| 2026-06-24 | ADR-0019: Itens 126-156 (Neural Cortex BitNet LLM) → adicionados; Transformer Engine + Cortex Daemon + Success Engine + Training Pipeline | IDA IA |
| 2026-06-24 | Itens 157-163 (Self-Optimization / Workflow Learning) → adicionados; Usage Pattern Analyzer, Workflow Predictor, Auto-Skill Generator, Dynamic Resource Scaling, Self-Optimizing Scheduler, Workflow Profile, Hardware Config Learning | Dev + IDA IA |
| 2026-06-24 | Itens 164-175 (Crom Ecosystem Analysis) → adicionados; 12 ideias portadas de MrJc01/75 repos: XOR Delta, CDC, TV-DSL, Codebook VQ, ReAct loop, MCP Server, Workspace isolation | IDA IA |
| 2026-06-24 | ADR-0020 (Crom Ecosystem Rust Viability Analysis) → criado; código modelo no_std para 9 items (#164-175), ~1.780 LOC kernel + ~300 LOC Python | IDA IA |
| 2026-06-24 | Item 176 (Ed25519 Cryptographic Identity for TrustCache) → adicionado; upgrade do CapabilityToken(u64) para assinatura Ed25519 real; derivado de Crom-meueu | IDA IA + Auditoria Externa |
| 2026-06-25 | ADR-0021: Itens 177-189 (Life OS Ecosystem Analysis) → adicionados; 13 ideias extraídas de 20 repos: Spectrum Graph, Runtime SDD, FS as Context, DA Identity, Temporal KG, Proactive Push, WASM Sandbox, Intent Transparency, Energy Tracking, AppForge, Multi-User, Workflow Builder, Federated Cluster | IDA IA |
| 2026-06-25 | PAI Deep-Dive: Itens 190-198 → adicionados; 9 ideias extraídas de danielmiessler/LifeOS (PAI v5.0.0): Algorithm 7-phase loop, Council skill, Loop Detection, Bitter Pill, ISA format, Hermes Rating, Evals, Container Zones, Boot security policy | IDA IA |
| 2026-06-25 | ADR-0022: Itens 199-213 (Tier 2 PAI Ecosystem) → adicionados; 15 ideias extraídas de 21 repos: IterationBudget, Skill Metadata, Audit Ring, Awakening Mode, Context Fencing, Heartbeat Idle Gate, RateLimiter, Lifecycle Hooks, MemoryProvider, Tool Permissions, Actor Registry, Crash-Recovery, ComputeBackend, Plugin System, WASM Sandbox | IDA IA |
| 2026-06-25 | ADR-0023: Itens 214-227 (Tier 3 Memory Systems & Second Brain) → adicionados; 14 ideias extraídas de 14 repos: SHA-256 Dedup, Privacy Filter, TTL Eviction, Hybrid Search, 4-Tier Consolidation, Ebbinghaus Decay, Session Replay, Knowledge Graph, Metacognitive Guard, Draft→Review→Merge, Atkinson-Shiffrin Memory, Bi-temporal KG, Team Memory, Git Snapshots. Deep-dive: agentmemory (24k ★, 60+ source files). | IDA IA |
| 2026-06-25 | ADR-0024: Itens 228-249 (Tier 4 Agent Frameworks) → adicionados; 22 ideias extraídas de 6 repos: Tool Policy Registry, Usage Tracker, Auto-Compact Buffer, Event-Sourced Conversation, Cron Scheduler, Session Checkpoint, Plan/Execute Modes, Graph Orchestration, Plugin Hub, Completion Terminal Skills, Claim-Based Lease, Time Travel, Context Compaction, Observability, AI Security Scan, Hub Discovery, Human-in-the-Loop, Remote Execution, Skill Marketplace, Context Compaction Agent. Deep-dive: Cline (63.9k ★, 293 releases, 6.338 commits, AgentRuntime + ClineCore + CronRunner). | IDA IA |
| 2026-06-25 | Sprint 23 Bugfix: Itens 250-252 (e1000 DMA fix, /ping, DHCP/ARP refactor pendente) → adicionados; allocate_contiguous fix (start de next_free_bit), DHCP skip, /ping command. Boot QEMU validado: e1000 init OK, executor 11000+ ticks. | Dev + IDA IA |
| 2026-06-25 | Network Sprint: Itens 253-255 (e1000 TDT protocol fix, NUM_DESC 48, PTHRESH 8, Neural Network Architecture) → adicionados; TPT=0 ainda não resolvido (qemu_send_packet não chamado). Novo modelo: init_driver_network() → HW_NET_E1000 → network_bootstrap() → skill-based routing. | Dev + IDA IA |
| 2026-06-25 | ADR-0025: Itens 256-267 (Tier 3 Security Patterns) → adicionados; 12 padrões extraídos de 5 repos (InnerWarden 159★, ai-jail 595★, vexfs 24★, Chisel 12★, cori-kernel 17★). 7 itens viaveis Sprints 24-27 (256-264), 3 ideias futuras Sprint 28+ (265-267), 6 padrões descartados. Deep-dive: InnerWarden (2057 commits, 7900+ testes, 45 eBPF programas, 82 detectores, 69 regras correlação). | IDA IA |
| 2026-06-26 | **Section 1.28 Agent/Skill-First Architecture** — 20 itens (A-001 a A-020) adicionados. Reclassificação: tudo vira agente/skill, nada de tasks/serviços. Paradigma fundamental. | Dev + IDA IA |
| 2026-06-26 | **IDEA_BANK total: 275 itens.** Heat map atualizado: 68 ✅, 116 🟡, 76 ⏳, 9 💰, 6 ❌ | Dev + IDA IA |
| 2026-06-26 | **Seção 6 — Sprint Planning** adicionada: 7 blocos futuros (12-18) com 55 itens 🟡 consolidados. Total: 330 itens. | Dev + IDA IA |
| 2026-06-27 | **Bloco 12 implementado:** x2APIC, Huge Pages, PCI bridges recursivo, Cron Scheduler, MCP Server (#18, #70, #92, #93, #172, #232). | Dev + IDA IA |
| 2026-06-27 | **Bloco 13 implementado:** Multi-mode Trust, Path Confinement, Mask Secrets, Graduated Enforcement, Posture-Aware, Boot Policy, Security Pipeline, Ed25519 identity (#166, #176, #198, #256-260). | Dev + IDA IA |
| 2026-06-27 | **Bloco 14 implementado:** Hermes Cognitive completo — Identidade (#180), SDD (#178), ReAct (#190), Transparency (#184), Council (#191), Bitter Pill (#193), Context Fencing (#203), Usage Analyzer (#157), Workflow Predictor (#158), Dynamic Scaling (#160), Reflex Threshold (#139), Self-Scheduler (#161), Config Learner (#163), LLM arch/tier (#135/#136). | Dev + IDA IA |
| 2026-06-29 | **Ecosystem Batch 3 (IDEA #280):** 12 repos portados (redox, Theseus, Embassy, Tock, Swarm, RagaAI, Swarms, SuperAGI). 8 arquivos, 601 LOC. `cargo check --release`: 0 errors. | Dev + IDA IA |
| 2026-06-29 | **The Agency port (IDEA #277):** HwRegistry + HwAgent por dispositivo PCI. Agency struct com 12 divisoes, 30+ agentes especializados. LLM-aware: "quero video chamada" → ativa mic+camera+display+net. | Dev + IDA IA |
| 2026-06-29 | **GGUF loader research (IDEA #278):** Formato GGUF documentado. Portavel (~500 LOC) mas modelos 9B+ exigem heap >5GB. Alternativa: expandir .bitnet v3. | Dev + IDA IA |
| 2026-06-29 | **SmileyOS patterns (IDEA #279):** 11 padrões identificados — compositor multi-window, filesystem proprio, app SDK via trait, temas, v86 browser demo, shell 40+ comandos. Prioritario: temas + shell expandida. | Dev + IDA IA |
| 2026-06-30 | **VFS Layer + MHI ARC (IDEA #281):** VfsRegistry com mount table, resolve, lookup. MHI arc_suggest_tier() ZFS-inspired (MFU→Dram, MRU→Nvme, cold→Hdd). 8 mounts padrao. Path utils. | Dev + IDA IA |
| 2026-06-30 | **Storage Agents (IDEA #282):** FilesystemAgent trait. AtaAgent (/mnt/hdd/), DevFsAgent (/dev/), ProcFsAgent (/proc/). VFS bridge: read_vfs/write_vfs/list_vfs. | Dev + IDA IA |
| 2026-07-01 | **GPU Detection + VRAM Tier (IDEA #283):** PCI scan class 0x03, 30+ GPUs detectadas por device ID, BAR0/BAR2 mapping, VRAM bump allocator com next_offset, DEADBEEF test. GpuInfo com vendor/arch/vram/display/compute. | Dev + IDA IA |
| 2026-07-01 | **Intel Ring Buffer Compute (IDEA #284):** Intel Gen9+ ring buffer init, write/submit/wait_idle, exec_batch, gpu_blit (XY_SRC_COPY_BLT). MI_BATCH_BUFFER_START/END. unsafe impl Send para Mutex. | Dev + IDA IA |
| 2026-07-01 | **GPU Backend Selector (IDEA #285):** GpuAccel enum (Intel/CpuOnly), Mutex-safe, auto-select Intel→AMD→NVIDIA→CPU. gpu_matmul() com fallback CPU. | Dev + IDA IA |
| 2026-07-01 | **Desktop Cube Crossfade (IDEA #286):** Transicao entre workspaces sem float (FPU desabilitado). Inteiros step 0..50, AtomicBool em vez de static mut, split animado com fill_rect. | Dev + IDA IA |
| 2026-07-01 | **Bughunt GPU Sprint 66:** 24 bugs corrigidos (3 crit, 8 high, 6 med, 7 low). Destaques: vec! sem alloc, gpu_blit morto, vram_alloc sem bump, static mut UB, float sem FPU, BAR sem validacao, mod gpu ausente. commit 1d66a17. | Dev + IDA IA |
| 2026-07-01 | **TODO.md mestre:** docs/TODO.md com 28 pendências catalogadas, sub-itens, dificuldades, travas, fontes, esforço. Para qualquer AI DEV localizar e contribuir. | Dev + IDA IA |
| 2026-07-01 | **WiFi (B-29):** Intel Wireless / Atheros / Realtek 802.11. Scan, association, WPA2/WPA3. Thread: firmware loading, crypto, frame format. Bloqueado por B-01 (rede). | Dev + IDA IA |
| 2026-07-02 | **Boot Bughunt Sprint 71 (IDEA #300-#304):** Boot refatorado para agent-first com 8 fases (BOOT_PHASE events). DiagnosticSkill substitui 90+ linhas de teste inline. CortexAgent acorda antes do HW discovery. Xuvisco corrigido (framebuffer antes de VGA CRTC). FAT12 log finalmente funcional (boot_logger + BootLogAgent). `cargo check --release`: 0 errors. | Dev + IDA IA |
| 2026-07-03 | **305** | **TPM 2.0 Measured Boot (v0.74.1)** — TIS MMIO driver (279 LOC), SHA256 embedded, PCI config 0xFED40000 probe, locality 0 request, FIFO send/recv, PCR[8] extend com kernel hash. Fallback silencioso se TPM ausente. Depende de Ed25519 kernel signing. | ✅ v0.74.1 | v0.74.1 | 279 LOC for TIS driver + SHA256 |
| 2026-07-03 | **Sprint 74 (v0.74.0-0.74.2):** Seguranca — particao FAT mascarada como 0x1C (Hidden FAT32 LBA, bootloader aceita via mbr_nostd), assinatura Ed25519 do kernel com auto-verificacao, TPM 2.0 TIS driver (probe+fallback+PCA extend). Shutdown tracking (4 causas, persistencia FAT12+VFS, BootSelfHealAgent analisa). | Dev + IDA IA |
| 2026-07-03 | **Particao FAT 0x1C (v0.74.2):** Mascara tipo 0x0C→0x1C (Hidden FAT32 LBA) via build scripts (offset 0x1D2). Bootloader aceita (mbr_nostd 0.1.0 mapeia 0x1C como PartitionType::Fat32). Kernel aceita 0x1C em todos os checks. Fallback 0x73 mantido. QEMU boot OK, VB OK. | Dev + IDA IA |
| 2026-07-03 | **FAT32-only (v0.75.0):** Fat12Writer removido. write_boot_log, boot_log_agent, boot_logger, shutdown usam apenas FAT32. 102 LOC removidos. | Dev + IDA IA |
| 2026-07-03 | **DiskIntelligenceAgent (v0.75.1-0.75.6):** 6 controladoras (ATA, USB, NVMe), 10+ FS probes (FAT32 a ReFS), GPT, SED/OPAL, S.M.A.R.T., I/O Scheduler, ARC cache 1MB, tier migration MHI. ~2.400 LOC. 0 erros. | Dev + IDA IA |
| 2026-07-03 | **IDEA #306-#310:** AIOS Evolution — compatibilidade cross-OS (PE/ELF/Mach-O/APK + syscall-to-skill), Update/Upgrade Agent com rollback, WASM Skill Runtime + BitNet IDE, J.A.R.V.I.S. Layer, stack final Boot→Kernel→Cortex→Hermes→J.A.R.V.I.S. | Dev + IDA IA |
| 2026-07-03 | **311** | **Trinity Model Hub (MoE — Mixture of Experts)** — Múltiplos modelos microscópicos especializados (<150KB cada): `hw_identify` (68KB), `rust_coder` (444KB), `disk_diag` (50KB), `security` (50KB). Router BitNet (68KB) classifica a intenção e roteia para o expert correto. Modelo generativo 1.5B recebe output do expert como contexto. Self-hosting: "crie app" → rust_coder gera código. Novos experts treinados on-demand. | ✅ Sprint 97 | Sprint 97 | RUSTCODER_MODEL global + fast-path HermesAgent + loading FAT32 |
| 2026-07-03 | **DiskIntelligenceAgent (v0.75.1-0.75.6):** 6 controladoras (ATA, USB, NVMe), 10+ FS probes (FAT32 a ReFS), GPT, SED/OPAL, S.M.A.R.T., I/O Scheduler, ARC cache 1MB, tier migration MHI. ~2.400 LOC. 0 erros. | Dev + IDA IA |
| 2026-07-03 | **IDEA #306-#310:** AIOS Evolution — compatibilidade cross-OS (PE/ELF/Mach-O/APK + syscall-to-skill), Update/Upgrade Agent com rollback, WASM Skill Runtime + BitNet IDE, J.A.R.V.I.S. Layer, stack final Boot→Kernel→Cortex→Hermes→J.A.R.V.I.S. | Dev + IDA IA |
| 2026-07-03 | **ADR-0031: AIOS Evolution Research** — análise completa (self-update A/B dual-slot, WASM wasmi runtime + WASI mapping, J.A.R.V.I.S. conversational layer, hybrid kernel/WASM agent architecture). Viability scores, LOC estimates, dependency chain, recommended sprint order. `docs/architecture/0031-aios-self-update-wasm-jarvis.md` | Dev + IDA IA |
| 2026-07-03 | **312** | **TrainingAgent — On-Device + GPU Learning** — 3 modos: Fine-tuning (CPU ADD/SUB, 100ex, ~2s), Transfer (1000ex, adapta dominio), Full (GPU+internet, 100K+ex). Fontes: FS local, HTTP (B-01), Federated. Pipeline: GPU detect → collect → train → .bitnet → Trinity Hub register. | 🟡 Sprint 79 | Sprint 79 | ~500 LOC + B-01 |
| 2026-07-03 | **313** | **Self-Learning OS — Aprende dos próprios dados** — O AIOS coleta seus próprios EventBus events, boot logs, self-heal logs, SMART data, conversas Hermes, padrões de erro, e usa como dataset de treino. Sem internet. Sem humano. Pipeline: LogAgent → DataCollector → TrainingAgent → .bitnet → Trinity Hub. | 🟡 Sprint 78 | Sprint 78 | ~300 LOC DataCollector + integração |
| 2026-07-03 | **314** | **SleepCycle Agent — Aprendizado inspirado no sono humano** — 5 fases: REPLAY (reproduz eventos recentes) → DREAM (BitNet gera variações sintéticas) → CONSOLIDATE (EWC protege skills existentes) → PRUNE (zera pesos fracos, ~18% redução) → REFLECT (confidence tracking, preenche gaps). Guard rails por fase: REPLAY filtra eventos maliciosos, DREAM rejeita sonhos perigosos, CONSOLIDATE protege skills de segurança c/ EWC max, PRUNE exempt pesos críticos, REFLECT roteia gaps proibidos pra humano. Agendado pelo CronAgent em períodos idle. NENHUM sistema bare-metal implementa isso. Neural AIOS seria pionero. | 🟡 Sprint 82 | Sprint 82 | ~1280 LOC + guard rails |
| 2026-07-04 | **ADR-0036** | **J.A.R.V.I.S. Unified Interaction Layer** — ADR-0036 criado, unifica ADR-0034 + ADR-0035. 28 features, 5-layer architecture (Boot→Kernel→Cortex→Hermes→JARVIS), Sprints 77-80+N+1+N+2, ~5650 LOC total. JARVIS = persona do Hermes, não camada separada. Rust no_std com código de referência para todos os componentes. | ✅ Aceito | — | 1126 lines ADR |
| 2026-07-04 | **315** | **J.A.R.V.I.S. Unified Interaction Layer (IDEA #315)** — 28 features catalogadas de ADR-0036. Sprint 80: SOUL.md, IPW, Session Compression, Notification Gate, Sessionless Thread (~950 LOC). Sprint 81: Emotion, Contracts, Discovery, Cache, Pipeline (~1200 LOC). Sprint 82: Dreaming, Ego, Heartbeats, Auto-Skills, Babel-Index, SleepCycle (~1680 LOC). Sprint 83: Fail-Closed, Merkle, Fluid Persona, AHCI (~1200 LOC). N+1 (Sprint 85+): Voice, CRDT, Gamification (~1600 LOC). N+2 (Sprint 85+): SKYNET Mesh (~500 LOC). Substitui 310a/b (JARVIS Layer original). | 🟡 Sprint 80 | Sprint 80 | ~5650 LOC total |
| 2026-07-04 | **Roadmap readequado** | IDEA_BANK.md reorganizado: Blocos 21-24 → 21-30. Foundation Quick Wins (Sprint 77), Agentic Evolution (Sprint 78), LLM Infrastructure (Sprint 79), JARVIS reordenado para Sprints 80-83, GPU (Sprint 84), AIOS (Sprint 85+). #311/#314 sprint numbers corrigidos. Activation on Demand premise adicionada ao AGENTS.md. roadma p.md + README.md + STATE.md sincronizados. | IDA IA |
| 2026-07-05 | **316** | **VGA Buffer Clear fix (v0.79.1)** — Xuvisco na transição bootloader→kernel causado por VGA text buffer 0xB8000 nunca limpo + framebuffer nunca limpo + `[BOOT] FB ativo — VGA text mode desligado` mentiroso. Fix: `vga_buffer::clear_physical_buffer()` escreve zeros em 0xB8000 sem tocar CRTC; `fb::probe_uefi_framebuffer()` limpa FB para preto; ambos chamados antes de qualquer `println!`. Zero I/O a 0x3D4/0x3D5. | ✅ v0.79.1 | v0.79.1 | 3 files / ~10 LOC |
| 2026-07-05 | **317** | **WHPX AVX2 Detection (v0.80.0)** — WHPX emula instruções VEX/AVX2 como VM exits (~10k+ ciclos cada), tornando AVX2 **2x mais lento** que scalar puro. CPUID leaf 0x40000000 retorna vendor "Microsoft Hv". `has_avx2()` e `avx2_available()` agora detectam WHPX e retornam false, forçando scalar path que roda nativo. Descoberta crítica: `unpack_all` não era o gargalo — emulação VEX sim. Per-layer timing: 2218 ticks/layer (scalar, WHPX) vs 4443 (AVX2, WHPX). | ✅ v0.80.0 | v0.80.0 | 2 files / ~30 LOC |
| 2026-07-05 | **318** | **KV Cache (v0.80.1)** — `KvCache` struct per-layer Vec<f32> para K e V. `forward_with_kv()` processa só tokens novos, atenção GQA usa K/V concatenados (cache + novo). `generate_speculative` refatorado: prompt preenche cache, cada step gera 1 token via cache. Ganho: ~6h → ~84s (200x+ speedup). | ✅ v0.80.1 | v0.80.1 | +210/-36 LOC cortex.rs |
| 2026-07-05 | **319** | **SPSC ring lockless (bbqueue)** — Fila Single-Producer Single-Consumer lock-free baseada em BipBuffer. Ideal para comunicação IRQ→task, core→core, GPU→CPU. no_std, sem CAS em targets sem atomics. Base para toda comunicação cross-core. | 🟡 Bloco 25 | Sprint N | ~100 LOC |
| 2026-07-05 | **320** | **IPI vetorizado** — `send_ipi(lapic_id, vector)` para cross-core interrupts. Atualmente só INIT/SIPI existem. Necessário para TLB shootdown, wake AP, interrupção entre cores. | 🟡 Bloco 25 | Sprint N | ~150 LOC |
| 2026-07-05 | **321** | **PerCpu dinâmico** — Alocar struct PerCpu por AP + GS.base individual via wrmsr. Hoje só BSP_PCPU estático existe, todos APs recebem o mesmo ponteiro. Base para scheduler multicore, slab local, métricas por core. | 🟡 Bloco 25 | Sprint N | ~300 LOC |
| 2026-07-05 | **322** | **Work-stealing Chase-Lev scheduler** — Deques lock-free por core (Chase-Lev algoritmo). Quando queue local vazia, steal de core vizinho. Distribui agents entre 4 cores automaticamente. Referência: crossbeam-deque + fast-steal (crates.io no_std). | 🟡 Bloco 26 | Sprint N+1 | ~400 LOC |
| 2026-07-05 | **323** | **Parallel-for AVX2 matmul** — Chunk hidden dimension (2560) em 4 partes, cada core processa um chunk, barreira atômica (AtomicU32 spin). Sem lock — só barreira. Speedup estimado: 2-3.5× sobre single-core. | 🟡 Bloco 26 | Sprint N+1 | ~300 LOC |
| 2026-07-05 | **324** | **AgentScheduler multicore** — 4 run queues (uma por core), steal entre cores quando queue local vazia. AgentTier define affinity: Permanent→core fixo, UserDemand→work-stealing. RebalanceAgent monitora carga e move agents. | 🟡 Bloco 26 | Sprint N+1 | ~200 LOC |
| 2026-07-05 | **325** | **Per-CPU slab allocator** — Alocar sem lock no hot path. Slab local por core, lote de 64 frames quando vazio (lock curto no allocator global). Reduz contenção no LockedHeap compartilhado. | 🟡 Bloco 26 | Sprint N+1 | ~300 LOC |
| 2026-07-05 | **326** | **GPU BAR0/BAR1 mapping UC** — Mapear PCI BARs da GPU como uncacheable (PWT|PCD) para MMIO direto. BAR0 = register file, BAR1 = VRAM aperture. Genérico: NVIDIA (nova-core), AMD (amdgpu), Intel (i915). | 🟡 Bloco 27 | Sprint N+2 | ~300 LOC |
| 2026-07-05 | **327** | **GPU doorbell + SPSC job ring** — CPU escreve job descriptor no ring buffer, escreve doorbell register (BAR0 offset), GPU lê doorbell, executa job, atualiza tail. Ring com `alignas(64)` head/tail para false sharing prevention. | 🟡 Bloco 27 | Sprint N+2 | ~400 LOC |
| 2026-07-05 | **328** | **VRAM buddy allocator** — Gerenciar VRAM da GPU (GDDR NVIDIA/AMD, DRAM carveout Intel). Alocação contígua para kernels GPU. Free list com coalescing. Base para MSched evicção ótima (Belady) futura. | 🟡 Bloco 27 | Sprint N+2 | ~400 LOC |
| 2026-07-05 | **329** | **Agent.xpu prefill/decode split** — Prefill (processamento do prompt completo) fica na CPU (4 cores paralelos). Decode (geração token a token) vai para GPU via job ring. CPU faz tokenization + embedding, GPU faz matmul. Referência: arXiv 2506.24045. | 🟡 Bloco 28 | Sprint N+3 | ~400 LOC |
| 2026-07-05 | **330** | **GPU matmul kernel ternário** — BitNet ADD/SUB matmul implementado como compute shader para GPU. NVIDIA PTX, AMD AQL, Intel GEN assembly. Speedup estimado: 10-25× sobre CPU (VRAM bandwidth vs DDR). | 🟡 Bloco 28 | Sprint N+3 | ~300 LOC |
| 2026-07-05 | **331** | **CPU→GPU KV cache DMA** — Transferir KV cache de 307 MB entre RAM e VRAM via DMA engine. Estimado 200-400ms para swap completo (PCIe 3.0 x16 = ~16GB/s). Referência: dmaplane (arXiv 2603.10030). | 🟡 Bloco 28 | Sprint N+3 | ~200 LOC |
| 2026-07-05 | **332** | **XQueue preemptível (XSched)** — Fila de comandos GPU com 3 níveis de preempção: pending (não submetido), in-flight (submetido mas não executando), running (em execução). Política agnóstica de hardware. Referência: XSched (OSDI 2025). | 🟡 Bloco 28 | Sprint N+3 | ~600 LOC |
| 2026-07-05 | **333** | **burn-flex backend port** — Portar o backend CPU do burn-flex (tracel-ai/burn) para nosso kernel. SIMD gemm + quantization + fused ops. Elimina bitnet_avx2 manual (~800 LOC). 2-95× speedup documentado. Referência: github.com/antimora/burn-flex. | 🟡 Bloco 29 | Sprint N+4 | ~800 LOC |
| 2026-07-05 | **334** | **MSched evicção VRAM** — Belady (OPT) eviction policy para VRAM. Prediz working set do próximo kernel GPU, pré-carrega de DRAM, evicção ótima. Referência: arXiv 2512.24637. | 🟡 Bloco 29 | Sprint N+4 | ~500 LOC |
| 2026-07-05 | **335** | **CFS scheduler (Completely Fair)** — Substituir round-robin do AgentScheduler por CFS baseado em vruntime. Fairness entre agents. Referência: echOS-x64 + moss-kernel. | 🟡 Bloco 29 | Sprint N+4 | ~500 LOC |
| 2026-07-05 | **336** | **GPU + Display co-existência** — iGPU (Intel) faz display (framebuffer), dGPU (NVIDIA) faz compute. Quando só dGPU existe, time-sharing via XQueue. Referência: coconutOS. | 🟡 Bloco 29 | Sprint N+4 | ~300 LOC |
| 2026-07-05 | **337** | **SMP+GPU Research (ADR-0037 v1)** — 30 fontes analisadas (arXiv, GitHub, crates.io, listas kernel). | ✅ ADR-0037 | N | ~900 LOC ADR |
| 2026-07-05 | **338** | **Pesquisa Expandida AMD+Intel+NPU+Apple+NVIDIA bare-metal** — ADR-0037 v2 expandido. Cobertura completa: AMD ROCm/KFD/TrustOS, Intel Level Zero/Xe, NPU XDNA+Intel NPU, Apple Silicon honeycrisp/aruminium/metaltile, processadores modernos AMX/AVX-512/APX, abordagens multiplataforma Rust GPU. Matriz de decisão por HW real. | ✅ ADR-0037 v2 | N | +300 LOC ADR |
| 2026-07-05 | **339** | **TrustOS (nathan237) — Blueprint bare-metal AMD GPU em Rust** — 264K LOC, zero blobs. AMD GPU bring-up do zero: SDMA engine, ring buffer, firmware loading no RX 580X (Polaris 10). Root cause de 14 iterações: Graphics Memory Controller desinicializado. Prova que bare-metal GPU em Rust no_std é VIÁVEL. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **340** | **pascal-egpu (TheTom) — Blueprint NVIDIA Pascal GPU do zero** — GTX 1060 via BAR MMIO. Plano 8 fases: PCIe → BAR → ACR → FIFO → GR → Compute. **Referência NVIDIA.** Para AMD: amdgpu Linux driver + GPUOpen docs. Para Intel: i915 Linux driver. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **341** | **folkering-os — AI-native OS similar ao nosso** — SMP 4 cores, AVX2+FMA, VirtIO-GPU, WASM JIT, smoltcp, lock-free telemetry ring, self-healing, capability tokens. 4 meses de desenvolvimento solo. Prova que nossa arquitetura (Rust no_std, SMP, AVX2, WASM, AI-native) é viável em timeline curta. | 🔵 Referência | — | ADR-0037 |
| 2026-07-05 | **342** | **honeycrisp (cyberia-to) — Apple Silicon GPU/NEON/AMX/ANE bare-metal Rust** — Quatro crates: unimem (memória compartilhada), acpu (NEON+AMX), aruminium (Metal GPU puro Rust, 1.79× faster), rane (ANE). Prova que acesso bare-metal a todos compute units Apple é possível. | 🔵 Referência futuro (ARM64) | — | ADR-0037 |
| 2026-07-05 | **343** | **AMD ROCm + KFD — Compute via Linux kernel** — RDNA2/3 suportado via /dev/kfd. T0-GPU (50K LOC Rust) prova AMD compute em Rust via KFD. TheRock (AMD) tem pure-Python KFD driver. **Não bare-metal** — depende de Linux. | ❌ Descartado (nosso HW NVIDIA) | — | ADR-0037 |
| 2026-07-05 | **344** | **Intel Level Zero + Xe GPU Compute** — Intel Compute Runtime suporta Tiger Lake+ (Gen12+). **Skylake (i5-6400) NÃO é suportado** (Gen9 mínimo). Intel HD Graphics 530 não tem Level Zero, XMX, nem compute capability. | ❌ Descartado (incompatível HW) | — | ADR-0037 |
| 2026-07-05 | **345** | **NPU AMD XDNA — Spatial dataflow accelerator** — Firmware fechado, toolchain offline (MLIR-AIE/IRON/Triton-XDNA), sem programabilidade geral. XDNA1 abandonado pela AMD no Linux. XDNA2 suportado via Ryzen AI Software. Performance máxima: ~68 GFLOPS (512³ int16) no XDNA1. | ❌ Descartado (sem HW, firmware fechado) | — | ADR-0037 |
| 2026-07-05 | **346** | **NPU Intel — LEON RT + NCE tiles** — Firmware fechado, OpenVINO+Level Zero, offline compile. NPU 3720 (Meteor Lake) 9.5 TOPS, NPU 4000 (Lunar Lake) 48 TOPS. **Nosso i5-6400 não tem NPU.** | ❌ Descartado (sem HW) | — | ADR-0037 |
| 2026-07-05 | **347** | **AMX/AVX-512/APX — Feature set moderno** — AMX em Sapphire Rapids+ (Xeon 2023). AVX-512 em Skylake-SP (Xeon) e Rocket Lake (2021). APX em Nova Lake+ (2026+). P-cores/E-cores em Alder Lake+ (2021). **i5-6400 (Skylake client) não tem nenhum.** AVX2+FMA é o máximo disponível. | ❌ Descartado (HW incompatível) | — | ADR-0037 |
| 2026-07-05 | **348** | **any-gpu (cochranblock) — Tensor engine wgpu multi-GPU** — CausalLM, SDPA, GQA, LayerPager. Roda em qualquer GPU via wgpu (Vulkan/Metal/DX12). 309 testes. **Não bare-metal** — depende de driver gráfico do SO. Inspiração para design de API. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-05 | **349** | **Firmware GPU disponível em linux-firmware** — NVIDIA Pascal (FECS+GPCCS signed), AMD RDNA (PSP MIT license), Intel Gen6+ (GuC/HuC open). Todos disponíveis para download e redistribuição. Documentação: nouveau, amdgpu, i915. | ✅ FIRMWARE DISPONÍVEL | N+2 | ADR-0037 |
| 2026-07-05 | **350** | **nova-core (NVIDIA/KHaddock) — Driver NVIDIA oficial em Rust** — Em desenvolvimento no LKML (2025-2026). Suporta Ampere+ (GSP). Pascal NÃO é target (Pascal usa Falcon, não GSP). **Referência arquitetural:** estrutura de driver GPU profissional em Rust, BAR mapping, doorbell, channel management. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-05 | **351** | **Matriz de Decisão Multi-Vendor** — O neural-os-core deve suportar qualquer HW detectado. Prioridade de implementação: NVIDIA (primeiro, firmware + docs mais acessíveis) → AMD (segundo, GPUOpen) → Intel (terceiro, i915 público) → NPU (futuro). SMP é pré-requisito para todos. | ✅ ADR-0037 v2 | N | ADR-0037 |
| 2026-07-05 | **352** | **Secure Boot GPU — Pipeline multi-vendor** — NVIDIA ACR: FECS blobs → WPR → LS ucode → signature. AMD PSP: firmware MIT → PM4 init ring. Intel GuC: firmware open → HuC auth → submission. Pipe: linux-firmware → kernel → BAR0 → GPU engine. | 🟡 Bloco 27 (GPU Foundations) | Sprint N+2 | ~600 LOC |
| 2026-07-05 | **353** | **GPU Compute Pipeline completo** — Pipeline de submissão genérico: BAR0 MMIO → GPU boot (ACR/PSP/GuC) → command ring init → compute dispatch. Implementação por vendor: NVIDIA (PFIFO + PUSH_BUFFER), AMD (PM4 ring), Intel (GEN ring buffer). Pipeline de execução: CPU prepara comando → ring entry → doorbell → GPU executa → completion. | 🟡 Bloco 27-28 | Sprint N+2 a N+3 | ~2500 LOC total |
| 2026-07-05 | **354** | **TrustOS lessons para GPU bare-metal** — Lições chave de TrustOS: (1) GMC (Graphics Memory Controller) e VM são a causa raiz de falhas — não registros, não PCIe link; (2) Ring buffer precisa estar em GART (Graphics Aperture Remap Table); (3) RPTR/WPTR avançando = firmware responsivo; (4) Firmware loading é sequencial e frágil — um passo errado e tudo aborta. | 🔵 Referência arquitetural | — | ADR-0037 |
| 2026-07-06 | **355** | **buddy-slab-allocator (arceos-hypervisor)** — Substituir slab.rs + gpu/vram.rs pelo crate no_std maduro (30K downloads). Per-CPU slab caches, remote-free lock-free, buddy allocator. Usado no ArceOS (bare-metal Rust OS similar). Apache-2.0. | 🟡 Sprint 86 | Sprint 86 | ADR-0038 |
| 2026-07-06 | **356** | **edge-net (edge-dhcp)** — Cliente DHCP no_std + no-alloc para resolver B-01. 225★ GitHub, 42 forks. Pode operar sem heap, antes do smoltcp. | 🟡 Sprint 88 | Sprint 88 | ADR-0038 |
| 2026-07-06 | **357** | **khal-std (dimforge/khal)** — GPU compute shaders Rust→SPIR-V/PTX/CPU. **Não viável diretamente** (requer wgpu/std), mas arquitetura inspira nossa futura GPU compute. | ❌ Inviável (std) | — | ADR-0038 |
| 2026-07-06 | **358** | **ruvix-net (ruvnet/ruvector)** — Kernel cognitivo Rust bare-metal similar ao nosso. Stack de rede mínima para "RuVix Cognition Kernel". Referência arquitetural. | 🔵 Referência | — | ADR-0038 |
| 2026-07-06 | **359** | **BGE-Small-EN-v1.5 Embedding (BAAI)** — Modelo de embedding semântico 33.4M params, 384-dim, ONNX, MIT license. Converter para .bitnet e integrar como skill semantic_search no HermesAgent. MTEB 62.17, 62M downloads/mês. | 🟡 Sprint 89 | Sprint 89 | ADR-0038 v2 |
| 2026-07-06 | **360** | **Kokoro-82M TTS (ONNX Community)** — Modelo TTS 82M params, 24kHz, 28 vozes, Apache-2.0. Formato ONNX com quantizações (86 MB Q8). Converter para .bitnet e integrar como skill TTS. Único modelo TTS viável para bare-metal. | ❌ supersedido | — | Histórico. TTS primário = Piper VITS + formant. ADR-0045. |
| 2026-07-06 | **361** | Zero-Copy SFS via zerocopy crate — Transmuting &[u8] ↔ &Tensor sobre páginas NVMe mapeadas. Serialização sem serde, sem alocação. | 🟡 Sprint 96 | Sprint 96 | ADR-0010 |
| 2026-07-06 | **362** | Episodic memory via battery-backed NVMe — KV-cache persistida entre reboots. Páginas físicas mantidas via NVMe battery-backed ou S3 sleep. | 🟡 Sprint 95 | Sprint 95 | ADR-0010 |
| 2026-07-06 | **363** | Skills-as-Modules capability import — Allowlist-based validação (nn:silu, tensor:matmul como imports declarados por skill). | 🟡 Sprint 96 | Sprint 96 | ADR-0010 |
| 2026-07-06 | **364** | Zero-Trust Syscall Categories — 4 classes: Read-only (always allow), Ephemeral allocate (budget), Persistent write (Cortex eval), Hardware access (always deny). | 🟡 Sprint 92 | Sprint 92 | ADR-0010 |
| 2026-07-06 | **365** | Neural Cache decisions per token — Cache de avaliações LLM por capability token para evitar latência repetida. | 🟡 Sprint 92 | Sprint 92 | ADR-0010 |
| 2026-07-06 | **366** | Failure Taxonomy Enum — FailureClass::MemoryFault, ExecutionFault, ResourceFault, LogicFault, ExternalFault. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **367** | Exception Handlers + SelfHeal — Page fault e GPF handlers: coletam contexto, publicam KERNEL_ERROR, chamam SelfHeal::analyze(), tentam recovery. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **368** | Corrective Prompting — Erro → consulta LLM com contexto + histórico + lições. "Erro X no daemon Y. Qual melhor estratégia?" | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **369** | Verifier Pós-Recovery — Verifica recovery: task completou 1 tick sem panic? Skill registrada no SKILL_REGISTRY? Se falhou, próxima estratégia. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **370** | Erros no EventLog (EventKind::KernelError) — Nova variante no EventLog para publicar erros de kernel. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **371** | Budgeted Recovery — Tentativas limitadas por budget durante self-healing. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **372** | Silent Failure Detection — LLM verifica se output está correto mesmo sem erro explícito. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **373** | Multi-level Failure Architecture — Supervisionado (classificação) + não-supervisionado (anomalias) para detecção de falhas. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **374** | Failure Prediction — Predizer falhas antes de ocorrerem baseado em tendências. | 🟡 Sprint 96 | Sprint 96 | ADR-0027 |
| 2026-07-06 | **375** | GGUF phased implementation (Phase 1-3) — P1: parser header+metadata (150 LOC), P2: Q4_0 dequantization (200 LOC), P3: streaming ATA/USB (150 LOC). | ✅ P1–P3 MVP | SESSION_127 | ADR-0028/0046 |
| 2026-07-06 | **376** | f16_to_f32() manual conversion — u16 + função manual para dequantização GGUF Q4_0. | ✅ OK | Sprint 96+ | ADR-0028 |
| 2026-07-06 | **377** | GGUF streaming from ATA/USB — Page table mapping para modelos >4GB, streaming blocks on demand. | ✅ AirLLM MVP (layer-wise); mmap P9 ≠ AirLLM | SESSION_127 | ADR-0046 |
| 2026-07-06 | **378** | Per-vendor GPU driver LOC breakdown — intel.rs (~700), nvidia.rs (~1500), amd.rs (~2000), virtio.rs (~400). | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **379** | NVIDIA Pascal Push Buffer channel layout — Register map: PUSH_BUFFER 0x002000, size 0x002004, tail 0x002008. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **380** | AMD RDNA PM4 packet types — PKT3_WRITE_DATA, PKT3_ACQUIRE_MEM, PKT3_DMA_DATA, PKT3_RELEASE_MEM, PKT3_SET_BASE. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **381** | Model swap flow (/model \<path.gguf\>) — Detecta GPU, checa VRAM, carrega modelo se couber, fallback DRAM. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **382** | iGPU display + dGPU compute architecture — iGPU display, dGPU compute. GPU única faz ambos via VBIOS/UEFI GOP. | 🟡 Sprint 98 | Sprint 98 | ADR-0029 |
| 2026-07-06 | **383** | Detalhado WASI→Skill mapping (20 syscalls) — fd_read→FileAgent, clock_time_get→TimeAgent, poll_oneoff→EventBusAgent. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **384** | Tier 0-4 Agent Classification — Core, Hardware, Runtime, WASM, External MCP. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **385** | WASM Host Function Interface signatures — vfs_read, skill_invoke, clock_time, http_get, event_publish, agent_yield. | 🟡 Sprint 93 | Sprint 93 | ADR-0031 |
| 2026-07-06 | **386** | Performance budget table (kernel vs WASM) — vfs_read 50µs kernel vs 500µs WASM = 10x overhead. | 🟡 Sprint 93 | Sprint 93 | ADR-0031 |
| 2026-07-06 | **387** | ChromeOS A/B update reference architecture — GPT partition attributes, dm-verity Merkle tree, Omaha protocol. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **388** | J.A.R.V.I.S. Context Window Manager — Cortex (LLM) + Hermes (orchestrator) + Kernel (execution). JARVIS persona com proactive notification bus. | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **389** | J.A.R.V.I.S. Notification Gate rules — Startup grace 60s, dedup 30s, priority queue Critical>High>Info>Debug, interrupt gate. | 🟡 Sprint 96 | Sprint 96 | ADR-0031 |
| 2026-07-06 | **390** | Update channel strategy — Stable (3600s), nightly (600s), security (60s). | 🟡 Sprint 97 | Sprint 97 | ADR-0031 |
| 2026-07-06 | **391** | AgentManifest JSON format specification — name, kind, schedule, auto_start, persist, description, required_tokens, version, author, icon. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **392** | Developer contract for WASM agents — manifest(), tick(tick, tick_count) → u32, teardown(). | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **393** | 15 specific WASI→skill mappings for WASM agents — disk_read/write, net_http_get/post, time_now/sleep, publish/subscribe events. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **394** | BitNet IDE with HOWTO feature — IDE rodando no AIOS com Cortex-assisted code generation. Editor + Cortex Panel. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **395** | Marketplace/App Store agent — HTTP agent: search, install, Ed25519 verification, versioning, ratings. | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **396** | DiskMonitor example agent — 48 KB WASM agent: manifest(), tick() checando SMART a cada 1000 ticks, publish_event(). | 🟡 Sprint 93 | Sprint 93 | ADR-0032 |
| 2026-07-06 | **397** | SleepCycle guard rails per phase — REPLAY rejeita security_bypass, DREAM rejeita weapon/exploit, CONSOLIDATE protege safety skills. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **398** | BitNetTrainer implementation — PackedTernaryWeights, train_step() com STE, signum() gradient update, clamp(-1,1). | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **399** | Candle Trainer sidecar — ELF binary com candle crate. Kernel carrega ELF, aloca stack+heap, mapeia segments, executa treino GPU. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **400** | Task Spawner (ELF loader) — spawn_elf() com goblin::elf::Elf::parse(), allocate_contiguous_pages(), map_elf_segment(), TSS stack. | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **401** | Three data sources for on-device training — Local FS (SMART logs, self-heal), Pre-loaded (/data/rust_training/), Internet (crawl). | 🟡 Sprint 95 | Sprint 95 | ADR-0033 |
| 2026-07-06 | **402** | WASM linear memory pool (256 KB per skill) — 64-page slabs pre-alocadas por skill, alocadas do heap. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **403** | Skill ABI design — Como skills comunicam resultado ao Cortex: tensor return path, logging, completion signaling. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **404** | NVMe submission/completion queue architecture — Submission/completion queues, MSI-X, PRP lists, SGL. | 🟡 Sprint 99+ | Sprint 99+ | ADR-0010 |
| 2026-07-06 | **405** | Capability token cryptographically signed — Skill recebe contexto com token criptografado scoped a operações permitidas, verificado pelo kernel. | 🟡 Sprint 93 | Sprint 93 | ADR-0010 |
| 2026-07-06 | **406** | VirtIO-GPU GET_DISPLAY_INFO pending fix — Bug do QEMU TCG onde GET_DISPLAY_INFO retorna 0x0. | 🟡 Sprint 97 | Sprint 97 | ADR-0029 |
| 2026-07-07 | **407** | iwlwifi FW load (S1) — DID→blob(+pnvm); FAT; TLV/seções; PCIe DMA/TFD; wake→ALIVE. Inventário API77 SESSION_154 (~7,51 MB; sem `.pnvm`). iwlwifi = **FW MAC**, não SoftMAC clássico. | 🟡 S0+prepS1 ✅; **secondary** pós SESSION_160 | S1 ALIVE | ~800 | `iwl_fw.rs` + `wifi_iwlwifi.rs` + FAT |
| 2026-07-18 | **407b** | ath10k QCA6174 Note1050 — DID `168C:003E`; FW hw3.0 FAT; BMI/CE → fw_ready. | 🟡 A3 wired SESSION_161; PASS só com log Note | A3 runtime Note | ~900 | `ath10k_ce_bmi.rs` + `wifi_ath10k.rs` |
| 2026-07-07 | **408** | Async executor bare-metal (Embassy-style) — APIC/MSI-X. **Reclassificado SESSION_154:** não assumir SoftMAC ACK ~10µs (ath9k); iwlwifi ACK no FW. Só se S2 provar timing host-side (fase S4). | ▶️ `depends_on: wifi` | S4 condicional | ~1500 | embassy-rs + MSI-X |
| 2026-07-07 | **409** | USB CDC ECM HardMAC (S5) — dongles RTL8188/8192 via xHCI se PCIe iwlwifi bloqueado. | ▶️ `depends_on: wifi` | S5 alt | ~500 | xHCI + `generic_wifi.rs` |
| 2026-07-07 | **410** | Bridge smoltcp::phy::Device ↔ WifiChipset — após RF link (pós-S3); inject hook MSI-X já existe. | ▶️ `depends_on: wifi` | S3+ | ~50 | `generic_wifi.rs` + `netstack.rs` |
| 2026-07-08 | **411** | SkillOpt (Microsoft Research) — Otimizador de agent skills em espaço textual. Optimizer model (BitNet) gera add/delete/replace edits no SKILL.md, aceitos só se melhoram score de validação. SleepCycleAgent como scheduler de épocas. ~145 LOC. | 🟡 Sprint 99 | Sprint 99 | ~145 | agents.rs + cognitive.rs + skill_loader.rs |
| 2026-07-08 | **412** | SGLang Structured Decoding (Stanford/Berkeley) — FSM comprimido para geração constraint a JSON/SKILL.md/shell. Mascara logits no BitNet decoder para só tokens válidos. ~120 LOC. | 🟡 Sprint 99 | Sprint 99 | ~120 | cortex.rs + cognitive.rs |
| 2026-07-08 | **413** | vLLM PagedAttention (UC Berkeley, SOSP 2023) — KV cache paginado com COW entre prefixos. Frame allocator + page table já existem. Ganho marginal para single-user. | ⏳ Sprint 100+ | Sprint 100+ | ~100 | cortex.rs + allocator |
| 2026-07-08 | **414** | FlashAttention (Stanford, NeurIPS 2022) — IO-aware exact attention com tiling no cache L1. Aplica-se ao BitNet CPU: processar atenção em blocos de 16 tokens que cabem no L1 (32 KB). ~3-5× speedup para sequências >256 tokens. ~100 LOC. | ⏳ Sprint 100+ | Sprint 100+ | ~100 | cortex.rs |
| 2026-07-09 | **415** | ✅ **B-01 MORTO** — Serial tunnel TCP bridge. `slip.rs` driver COM2 + `serial_bridge.py` TCP server. Bypassa NICs emuladas (RTL8139/E1000). `-serial tcp:127.0.0.1:4444` (QEMU cliente). Primeiro RX: 304 bytes. | ✅ Implementado | v0.109.3 | ~82 | slip.rs + serial_bridge.py |
| 2026-07-10 | **416** | **Boa JS Engine** (boa_engine crate) � Engine JavaScript 100% Rust puro, compativel com no_std. ES2023+, ~180K LOC, ~5MB. Alternativa ao V8 para executar JS no kernel sem C++. BrowserAgent local usaria Boa como engine JS; Obscura (host) como fallback via serial tunnel para V8+CDP+stealth. | ?? IDEA_BANK | v2.0 | ~investigar | boa_engine |

| 2026-07-10 | **417** | **exFAT + BlockDevice+ write** — Driver exFAT + BlockDevice write_sectors (ATA/AHCI/USB-MSC). Pendrives >4GB. | ✅ r/list + **write opt-in** (`EXFAT_WRITE=1`, `exfat_write.rs`); NTFS/EXT write ⏳ | ADR-0040 | SESSION_144 | `exfat.rs` + `exfat_write.rs` |
| 2026-07-10 | **418** | **DiskIntelligenceAgent v2** — multi-FS probes, SMART/hotplug/ARC, MHI register. Mount FilesystemDriver pleno + cloud = residual. | ✅ parcial probes+VFS+ARC; cloud NetFs TCP+smoke (lan ✅; peer runtime) | ADR-0040 | SESSION_144+Pós-LAN | `netfs.rs` + `tools/netfs_peer.py` |
| 2026-07-10 | **419** | **FilesystemDriver trait + FS Manager App** — Trait unificado detect/mount/list. App Storage Manager = residual UI. | ✅ trait + CLI `storage_report`; ⏳ App UI (cauda) | ADR-0040 | SESSION_144 | `fs_driver.rs` + `storage_manager.rs` |
| 2026-07-10 | **420** | **MHI Ativo com DMA ring** — soft-migrate metadata + DRAM memcpy; DMA NVMe/VRAM deferido. Registry unificado k_nano. | ✅ soft-MVP; ▶️ `[MHI-DMA] AWAITING_REAL_HW` peer DMA | ADR-0040 | SESSION_146 | `k_nano/mhi.rs` |
| 2026-07-10 | **421** | **Instalador Neural com IA** — SysInstaller pendrive→HD/SSD/NVMe. | ✅ ADR-0079 M0-M4 → **ADR-0086 §2 (canônica, deprecado 2026-08-05)** | ADR-0079 | SESSION_227 | `crates/k_nano/src/sys_installer.rs` + `hw_profiler.rs` + `installer_agent.rs` + `self_check.rs` + `rollback.rs` + `hw_change.rs` + `cortex/src/install_adviser.rs` + `k_ai/src/self_heal_disk.rs` + `hermes/src/net_fallback.rs` + `jarbas/src/cards/install_card.rs` |
| 2026-07-10 | **422** | **NeuralFS — FS nativo CoW** — btree/journal/volume + NeuralFsAgent `/mnt/neural`; RAM I/O ✅; disco fisico / multi-level. | ✅ RAM I/O; ⏳ `por_fazer` disco | ADR-0040 / NeuralFS.md | SESSION_123/125 | `neural_fs/` |
| 2026-07-10 | **423** | **Tutti-style GPU Direct Storage** — NVMe→VRAM sem CPU. | ▶️ `[GDS-HW] AWAITING_REAL_HW` stub SESSION_146 | ADR-0040 | SESSION_146 | `k_hal/gpu/direct_storage.rs` |
| 2026-07-14 | **424** | **K³CHJ Capability Rings MVP C** — Dois AddressSpace + CR3 switch + SharedSpscRing + Cap bitflags + trap `int 0x90`. Demo boot non-fatal. Base Ring0↔Ring0; Ring3 = #429. | ✅ PoC | Sprint 107 | ~400 | ADR-0041 + `address_space.rs` + `ipc/` + `syscall.rs` |
| 2026-07-14 | **425** | **Hermes WASM host Caps (P3)** — Host-functions (`aios_send_tcp`, WriteRing) gated por `Cap` / CapabilityGate; negar sem Cap + log serial. Sem POSIX. | ✅ CapGate | Sprint 107 | ~150 | `capability_gate.rs` + `aios_api.rs` + ADR-0041 |
| 2026-07-14 | **426** | **SFI WASM + Cap contract** — Sandbox WASM com fuel + Cap por import sensível (net/ipc/FB). CapGate (#425) cobre o mínimo; SFI/AS pleno pendente. | 🟡 pós-P9 | Sprint 107+ | — | `wasm_rt.rs` + Cap |
| 2026-07-14 | **427** | **JARBAS FB MMIO + double-buffer (P4)** — Cap MAP_FB/WRITE_FB, AS map em JARBAS_FB_VA, backbuffer+present+vsync stub. Path bootloader FB; VirtIO-GPU BAR = follow-up. | ✅ PoC | Sprint 107 | ~280 | `jarbas_fb.rs` + ADR-0041 |
| 2026-07-14 | **428** | **K-IA DMA pin + Cortex weight mmap (P5)** — Cap PIN_DMA/MAP_DMA/MAP_WEIGHTS; pin frames + AS K-IA; mmap pesos eager. Evoluções: #430/#431/#432. | ✅ PoC | Sprint 107 | ~350 | `k_ia_dma.rs` + `cortex_mmap.rs` + ADR-0041 |
| 2026-07-14 | **429** | **Ring3 user-mode real (P6)** — GDT user + TSS.RSP0 + `iretq` + stub USER + Cap::ENTER_USER. Demo non-fatal; QEMU estável e ELF/preempt = follow-up. | ✅ PoC | Sprint 107 | ~250 | `user_mode.rs` + `interrupts.rs` + ADR-0041 |
| 2026-07-14 | **430** | **Demand-paging via #PF (P7)** — lazy registry + reserve NOT PRESENT + #PF cure (frames pré-alocados); Cap DEMAND_PAGE. Sem I/O no fault (GGUF pré-fill = #432). | ✅ PoC | Sprint 107 | ~280 | `demand_page.rs` + `cortex_mmap.rs` + ADR-0041 |
| 2026-07-14 | **431** | **VirtIO vring + DMA pin (P8)** — Virtqueue layout-compatible sobre frames pinnados; Cap VRING_SETUP; NIC live untouched. QUEUE_NOTIFY device = follow-up. | ✅ PoC | Sprint 107 | ~220 | `virtio_vring.rs` + `k_ia_dma.rs` + ADR-0041 |
| 2026-07-14 | **432** | **GGUF/FAT file-backed mmap (P9)** — pré-fill FAT `read_file_range` + demand-page lazy; Cap MAP_FILE; magic/fallback NFIL. Streaming on-fault / >4 pág. = follow-up. | ✅ PoC | Sprint 107 | ~260 | `gguf_mmap.rs` + `demand_page.rs` + ADR-0041 |
| 2026-07-14 | **433** | **Cadeia K³CHJ canônica** — `k-nano → k-ai → cortex → hermes → jarbas` + identidades (legível / HW-AI / cérebro / orquestra / ego+10%). Gate **v2.0.0 = N1–N5**. | ✅ Doc | Sprint 107 | — | ADR-0042 |
| 2026-07-14 | **434** | **N1 k-nano legível** — Telemetria LOADED\|ABSENT\|FAILED; Cap authority; probe NVIDIA gated; métricas scheduler; sem SUCCESS falso. | ✅ | Sprint 107 / v1.7.0 | — | ADR-0042 |
| 2026-07-14 | **435** | **N2 k-ai HW-AI / SelfHeal / HMI** — Heal/noop; HEALTH_ISSUE; inventário VID-gated (+ subclass); Trust (agent,skill). **N2.5 wired v1.7.8.** | ✅ CLOSED | ADR-0042 | v1.8.0 | SESSION_112 |
| 2026-07-14 | **436** | **N3 cortex cérebro** — Modelo real; MoE; Cap MAP_WEIGHTS; prompt→texto. **N3.5 wired v1.7.9.** | ✅ CLOSED | ADR-0042 | v1.8.0 | SESSION_113/117 |
| 2026-07-14 | **437** | **N4 hermes orquestra** — WASM SFI; skills; intent e2e; IPC→jarbas. **N4.6 wired v1.7.10.** | ✅ CLOSED | ADR-0042 | v1.8.0 | SESSION_114/118 |
| 2026-07-14 | **438** | **N5 jarbas ego/persona/+10%** — Compositor vivo; humor/UI; voz como expressão (stack pleno → Sprint Sound); só via Hermes; feedback preferências. **CLOSED v1.7.7 + N5.7 wired v1.7.11.** | ✅ CLOSED | ADR-0042 + Sound (voz) | v1.8.0 | ADR-0042 + SESSION_115/119 |
| 2026-07-16 | **439** | **Wire pattern K³CHJ** — Alias dep `*-crate` + `pub use` + deletar espelhos; `k_nano` sem `global-alloc`; bridge `memory`/`EVENT_BUS` → k_nano globals; residuals = integração bin-only. **Extensão SESSION_163:** emagrecer ondas 0–6 (`diff_bin_crate`, ATA/TIMER únicos). | ✅ + 🟡 follow-up | ADR-0042 | SESSION_117–119/163 | #467 |
| 2026-07-16 | **440** | **Marco v1.8.0** — ADR-0042 N1–N5 funcional + wire crates completo; Sprint 107 fechada; gate v2.0.0 = review (não auto-declare). | ✅ | v1.8.0 | — | STATE + CHANGELOG |
| 2026-07-16 | **441** | **USB HW unificado** — `usb_hw.img` GPT ESP + FAT dados; Rufus DD 1 stick; BITNET+PIPER+experts+116 FW. | ✅ | HW real | — | `build_image.py --hw --unified` |
| 2026-07-16 | **442** | **Sprint Sound backlog** — STT PCM; Mic→Wake gate; neural-lite; UAC parse; VAD/SER. Soft-float/VITS + cutover abertos. | ✅ parcial; Onda 4: UAC-HW AWAITING + soft-float ⏳ | Sprint Sound | SESSION_145 | ADR-0045 |
| 2026-07-16 | **443** | **N-gram speculative decoding** — LCG hash N=8 → draft M=4 da ocorrência anterior (last-writer-wins por índice); `verify_draft` + truncate KV; `draft[0]` gated pelos logits do passo anterior; sem double-forward. Zero deps. | ✅ OK | ADR-0047 §3.7 | SESSION_125 | `cortex/ngram_spec.rs` + `generate_speculative` |
| 2026-07-16 | **444** | **LatentBus** — canal `[f16;256]` paralelo ao EventBus; projection mean-pool; publish no generate; Hermes drain. | ✅ MVP | ADR-0047 P1 | SESSION_126 | `event-bus/latent.rs` + `cortex/projection.rs` |
| 2026-07-16 | **445** | **Evolve WASM hot-swap** — ledger + sandbox test + rollback; SleepCycle DREAM hook. | ✅ MVP | ADR-0047 P2 | SESSION_126 | `hermes/evolve.rs` |
| 2026-07-16 | **446** | **NeuOS Probe fase 1** — weight stats Healthy/Degraded + soul-vector stub; sem mutar pesos. | ✅ MVP | ADR-0047 P3 | SESSION_126 | `cortex/neuos_probe.rs` |
| 2026-07-16 | **447** | **GPU G1/G2 work-queue** — persistent queue + matmul HW/CPU_FALLBACK gate. | ✅ MVP | ADR-0047-GPU | SESSION_126 | `jarbas/gpu/work_queue.rs` |
| 2026-07-16 | **448** | **HMI H1+H4** — UI_SPEC JSON → compositor; avatar telemetria via LatentBus norm. | ✅ MVP | ADR-0047-HMI | SESSION_126 | `jarbas/display/ui_spec.rs` |
| 2026-07-16 | **449** | **N-gram bench empírico** — counters accept/forward + microbench padrão + `speedup_est`. | ✅ | ADR-0047 | SESSION_127 | `ngram_spec.rs` |
| 2026-07-16 | **450** | **Evolve Genesis** — 1 parent→1 child WASM, ratchet MAX_GENESIS=1. | ✅ MVP | ADR-0047 P2 | SESSION_127 | `hermes/evolve.rs` |
| 2026-07-16 | **451** | **GPU G3 SASOS-lite + G4 H2O/pages + G5 pipeline CPU** | ✅ MVP | ADR-0047-GPU | SESSION_127 | `sasos.rs` `kv_h2o.rs` `pipeline_g5.rs` |
| 2026-07-16 | **452** | **HMI H2+H5** — embedding points + thought splats no FB. | ✅ MVP | ADR-0047-HMI | SESSION_127 | `embed_viz.rs` |
| 2026-07-16 | **453** | **Descartes ADR-0047** — NeuOS ISA plena; LatentBus adapter; H3 diffusion. | ❌ | ADR-0047 | SESSION_127 | docs |
| 2026-07-16 | **454** | **NVIDIA Compute Multigeração** — contrato comum; backends Legacy ACR e GSP; Kernel Pack multi-ISA; Pascal é primeiro gate HW, não limite do produto. | ▶️ `[GPU-HW] AWAITING` SESSION_146; Degrau ✅ | ADR-0048 | SESSION_146 | golden silício |
| 2026-07-16 | **455** | **AMD Compute Multigeração** — IP Discovery; backends KIQ (GFX9–10) e MES (GFX11+); `AMD_KERNEL_PACK` HSACO offline; iGPU/APU display + dGPU AI; sem ROCm no alvo. | ▶️ `[GPU-HW] AWAITING` SESSION_146; Degrau ✅ | ADR-0049 | SESSION_146 | golden silício |
| 2026-07-16 | **456** | **Intel Compute Multigeração** — GMD_ID; famílias Gen9→Xe3; GuC + walkers; `INTEL_KERNEL_PACK` zebin offline; iGPU vídeo + dGPU IA; sem Level Zero no alvo. | ▶️ `[GPU-HW] AWAITING` SESSION_146; Degrau ✅ | ADR-0050 | SESSION_146 | golden silício |
| 2026-07-16 | **449** | **AirLLM GGUF streaming** — `GGUFStreamingModel` layer-wise ATA; soft PrefetchEngine; Q5_0/Q8_0 dequant; hot-swap ATA+Net→FAT→`set_model`; DMA/stream-to-disk/K-quant deferred. | ✅ MVP ATA+Net path; ▶️ `[AIRLLM-DMA]`; PreFlight `airllm-net` PARTIAL (e2e) | ADR-0046 | SESSION_148/152 | `gguf_streaming.rs` + `gguf.rs` |
| 2026-07-18 | **461** | **Rebrand K³CHJ** — terceira K = `k_hal`; cadeia k-nano→k-hal→k-ai→cortex→hermes→jarbas; K²CHJ histórico. | ✅ documentação | v1.8.6+ | ADR-0042 §0 | INDEX + AGENTS + README |

| 2026-07-18 | **460** | **Marco v1.8.6 TEST** — ADR-0041 H4+/H5+/AS + HalOffer Cap + slog canônico; continua não estável; ≠ v2.0.0. | ✅ documentação/release | v1.8.6 teste | SESSION_140 | STATE + CHANGELOG + tag |
| 2026-07-17 | **458** | **HW USB diagnostics sem serial** — `BOOT.LOG` FAT + console FB (`console_clear`/`console_print`) + ckpts K0–K17; MBR dados+ESP montável no Windows; vendor bootloader BltOnly→SetMode. | ✅ MVP debug | HW real | SESSION_139 | `boot_logger.rs` + `fb.rs` + `build_usb_unified.py` + `vendor/bootloader*` |
| 2026-07-18 | **459** | **k-HAL R1 + HalOffer + H4+/H5+/AS** — L0–L4; DeviceCap; QUEUE_NOTIFY real; Cap grant no bind; AS shallow PoC; VirtIO=transporte BE. | ✅ PoC v1.8.6 | ADR-0041 §9–§10 | SESSION_140 | `crates/k_hal`, `hermes/hal_offer.rs` |
| 2026-07-18 | **462** | **Auditoria ideias antigas** — STALE→✅/🔄; VIABLE→Onda+tags `depends_on: lan`; DEFER/💰/❌; legenda AWAITING_HW; zero 🟡 órfão nos IDs tocados. | ✅ docs | governança | SESSION_143 | IDEA_BANK + TODO + STATE |
| 2026-07-18 | **463** | **Marco v1.9.0 TEST** — Residuals 0–7 + Pós-LAN B-01 (net_bridge, NetFs PASS, TLS BLOCKED); continua não estável; ≠ v2.0.0. | ✅ documentação/release | v1.9.0 teste | SESSION_153 | CHANGELOG + tag `v1.9.0` |
| 2026-07-18 | **464** | **Neural Device LEGO** — L0 Bus / L1 HalOffer / L2 DeviceRecipe; UnlockDAG; trust Ed25519+blob_hash; community Adopt-a-Chip; SDIO≠bring-up; H1 bind table; NeuralFS `ecosystem/devices/`; slot `/market fetch` v3. | 🟡 MVP docs+H1 | ADR-0056 | community + `device_recipe.rs` | Install≠Ready |
| 2026-07-19 | **467** | **Emagrecer neural-kernel** — Cutover cirúrgico bin→crates: promote-if-bin-ahead → `pub use` stub; `tools/diff_bin_crate.py`; unificar `ATA_DRIVER`/`TIMER_TICKS`; pending_route em cortex. Residuals: cortex/bpe/agents/net/audio/smp/boot_logger. Marco **v1.9.5 TEST**. Superseded by #511 / ADR-0075 (SESSION_215, plano E0–E4 detalhado com recon de 29.431 LOC). | ✅ superseded by #511 | ADR-0042 + #439 | SESSION_163 | BIN_CRATE_DIFF.md |
| 2026-07-19 | **468** | **FitPolicy Neural (llmfit-inspired)** — Host `llmfit_pack_filter.py` + `FIT_GATE`; guest `cortex::model_fit` (re-export k_ai) → MemoryAgent + ModelHub; sem port llmfit/`std`. | ✅ | ADR-0019 + #466 | SESSION_164 | model-fit-and-pack.md |
| 2026-07-20 | **467b** | **Compute Dispatch SMP+GPU+NPU** — Camada única de dispatch da LLM (`cortex::compute`): NPU→GPU→CPU-SMP→AVX2→scalar. **WS-A ✅:** wake multi-AP por SIPI direcionado sequencial + stack/PerCpu por-AP + contador unificado + emagrece `neural-kernel::smp` (QEMU `-smp 4` → APs=3, CorePools r0=1 r1=2 r2=1). **WS-B/C ✅ wired:** `parallel_ternary_matmul` (particiona colunas, decode m=1 escala) + dispatcher nos choke points. **WS-D** GPU registra só se `BackendState::Ready` (canário silício); kernel W2A8 = Layer S/HW. **WS-E** NPU XDNA/Intel: detecção PCI + veredito honesto + fallback software (Ring0 MLP CPU, #51); driver/firmware = Layer S/sponsor. | 🟡 WS-A ✅ evidenciado; WS-B/C/D/E wired; speedup+GPU+NPU = HW/Layer S | ADR-0057 | SESSION (setup Cloud) | `cortex/compute.rs`, `k_nano/smp`, `k_hal/{npu,gpu/compute_dispatch}` |
| 2026-07-21 | **468b** | **Generative Card Desktop (UI/Desktop Jarbas)** — Fundação `embedded-graphics` (`DrawTarget` sobre `DoubleBuffer`) + toolkit no_std (matrix-gui zero-alloc/int-anim; embedded-gui gauge/chart/list; kolibri immediate-mode) — todos MIT/Apache. Camada declarativa `UiDeclaration`/`UiRenderer` (cards) gerada como **dados** por Hermes/Trinity/Cortex (constrangido por ADR-0057 #412 structured decode) ou por **skill WASM** (RustCoder/Codex, ADR-0052) + repetição Cron. WM stacking mantido (árvore de janelas retida; aposenta enum `AppId`). Ex.: "clima de amanhã" → WeatherCard. Supersede parcial ADR-0047-HMI (H3 ❌). **S1–S4 ✅** (QEMU: 3 cards + orb responsivo + HUD; self-tests PASS; clique fecha card). S5/A-V residual. | ✅ S1–S4 (S5/A-V residual) | ADR-0058 | pesquisa crates.io + código | `jarbas/src/display/{eg,card,compositor,agent}.rs` |
| 2026-07-21 | **469** | **Runtime App Factory — WASM real (wasmi) + geração validada por IA** — Substitui a VM `Op` custom por **wasmi** (no_std, fuel, Apache-2.0) como runtime real de skills/`agent-wasm`. Pipeline "app por IA em runtime": Hermes detecta gap → Cortex/Trinity/LLM geram op-IR/DSL sob decodificação restrita por gramática → montador op-IR→wasm → teste em sandbox wasmi → promover assinado. Caminho A ✅ (wasmi `add(2,3)=5` PASS). F3-F7 ✅ (W^X arena nativo `mov eax,42`→42 PASS). B/C (Cranelift) gated ring+HITL. F6 Ring3 → ADR-0077. | 🟢 A ✅ / F7 W^X ✅ / B/C gated | ADR-0059 | pesquisa + código | `hermes/{wasmi_rt,wasm_build,app_factory}.rs`, `neural-kernel/exec_arena.rs` |
| 2026-07-27 | **470** | **StreamPacket Protocol** — 14 tipos de pacote tipado (ReasoningStart/Delta/Done, ToolStart/Delta/Done, MessageStart/Delta, Stop, etc.) trafegando via EventBus. Substitui HERMES_RESPONSE texto plano. encode/decode compacto pipe-delimited. | ✅ | ADR-0057 #412 | SESSION_226 | `hermes/src/stream_packet.rs` |
| 2026-07-27 | **471** | **ChatWindow Onyx-style** — UI de chat por turnos com timeline de tools, mensagens streaming, histórico, input buffer, mic button, FocusMode (Chat vs Ambient). Renderizado via WM cosmic no painel esquerdo. | ✅ | — (fix) | SESSION_226 | `jarbas/src/display/chat_window.rs` |
| 2026-07-27 | **472** | **Render Registry** — `RENDER_REGISTER`/`RENDER_WINDOW` topics no EventBus. Agentes registram `RenderFn` e publicam janelas dinâmicas sem modificar compositor. AIOS gera skills de render dinamicamente via ADR-0059 (WASM). | ✅ | ADR-0059 | SESSION_226 | `jarbas/src/display/render_registry.rs` |
| 2026-07-27 | **473** | **COSMIC Desktop Visual Refinements** — `draw_rounded_rect(r=4/8)`, gaps entre tiles (4px), painel Hermes translúcido, barra de status estilo COSMIC, botão OFF com bordas arredondadas. | ✅ | — (fix) | SESSION_226 | `compositor.rs`, `decorations.rs` |
| 2026-07-27 | **474** | **Áudio no ChatWindow** — `MIC_ACTIVE` flag, botão `[MIC]`/`[REC]`, VoiceAgent escuta sem wake word, STT alimenta input buffer, TTS automático na resposta. | ✅ | ADR-0045 | SESSION_226 | `chat_window.rs`, `voice.rs` |
| 2026-08-01 | **#218** | **Jcode-inspired memory integration** — 4-tier consolidation (`k_ai::tiers::consolidate_tiers` → SGDB L3/L4/L5 + `MEMORY_TIER` topic, SleepCycle CONSOLIDATE); BGE statics single-source fix (bin `pub use k_ai::memory_systems::*` → recall BGE 384d real); recall gate blacklist (10 padrões); skill embedding `[SKILL-HINT]` (semantic_search ≥ 0.4); CHANGE_NOTIFY swarm; ADR-0059 F5 promote wired (wasmi + hot_swap). | ✅ | ADR-0059 | SESSION_237 | `k_ai/tiers.rs`, `hermes/{cognitive_bridge,skill_loader,self_evolve,evolve,agents}.rs`, `neural-kernel/{memory_systems,agents}.rs` |
| 2026-08-03 | **475** | **Isolamento Ring3 de Produção (ADR-0082)** - Depreca ADR-0041 §P9+ p/ escopo Ring3. F1: `create_sandbox_as()` from-scratch (kernel supervisor-only, sem PTs compartilhadas) + `TSS_ARRAY[8]` per-process + SYSCALL/SYSRET fast path (gated por `probe_done() && hv∈{None,Kvm}` — WHPX rejeita wrmsr LSTAR/STAR/FMASK → #GP; fallback int 0x90). F2: ELF64 loader (RX/RW por segmento + relocations R_X86_64_RELATIVE) + `ring3_run_native()` + `run_elf()`. F3: arena W^X USER (`jit_write_exec_user` — escrita via HHDM no frame, VA sandbox ∉ CR3 kernel → #PF) + app_factory B/C gated por `isolation_ring_available()`. F4 validada: boot TCG 2c 8G -NoDisk → P6 Ring3 OK (marker=3352494e470001), ELF+USER arena PASS, P7/P8/P9 OK, 54 agents, WASMI A. Commits 8d3eb90/1450108/6b073bf/4c7a2e9. | ✅ F1-F4 (HW real/KVM pendente) | ADR-0082 | SESSION_243 | `address_space.rs`, `interrupts.rs`, `syscall.rs`, `elf_loader.rs`, `isolation_ring.rs`, `exec_arena.rs`, `capability_gate.rs`, `user_mode.rs` |
| 2026-08-03 | **476** | **Auditoria Segurança 6.1–6.4 (modelo de confiança)** - 6.1 portão ÚNICO ADR-0052: verify_skill_md delega p/ verify_artifact_md(PackageKind::Skill); verify_and_register sign-first; generators contrato completo; seeds embedded via register_trusted_skill. 6.2 docs honestos (anéis = organização de código, não fronteira). 6.3 CapabilityToken::Ed25519 => false (fail-closed). 6.4 mix_session_seed usa hw_rng RDRAND (gate probe_done&&rdrand). | ✅ SESSION_245 | v1.9.9 TEST | SESSION_245 | self_evolve.rs, skill_{gen,observer,loader}.rs, matrix_learn.rs, agents.rs (bin+hermes), capability.rs, identity.rs, AGENTS.md |
| 2026-08-04 | **477** | **HW Expert v4 artefato validado** - retreino com split honesto 90/10 por (vid,did) único seed 42 + early stopping + threshold de export tunável (0.5/0.25/0.1/0.05, escolhe acc do ARQUIVO com fração não-zero ≥1%) + embed ROW-MAJOR; `validate_hw_expert_v4.py` = port Rust-exact (parse_end, header, fração não-zero GATE, predições não-constantes, holdout do arquivo). Root cause H2: threshold 0.5 vs init ±1/√128≈±0.088 → artefato 100% zeros. | ✅ | ADR-0082 + ADR-0041 HW-PnP | SESSION_247 | `tools/retrain_hw_expert_v4.py`, `tools/validate_hw_expert_v4.py`, `tools/hw_sweep/`, `cortex.rs` (loader v5 prefixed) |
| 2026-08-04 | **478** | **cargo test host + CI** - lib crates `#![cfg_attr(not(test), no_std)]`; HW-only gated `#[cfg(target_os="none")]` (não cfg(test) — inerte em dep); IDT cfg(not(windows)); p2p_sim gated feature; ProofGate now explícito; NVMe layout 72B pinado. Workflow GitHub Actions: check + test + build + QEMU boot smoke (Phase 6 + tick=). | ✅ | auditoria item 4 | SESSION_247 | `.github/workflows/ci.yml`, gates em k_nano/k_ai/hermes/cortex |
| 2026-08-04 | **479** | **F1 decode branchless (m=1)** - unpack `(pair&1)-(pair>>1)` do AVX-512 → path AVX2 (hoje `match` por peso, bitnet_avx2.rs:139-142); acumulador YMM por bloco de N com flush no fim (hoje reload/store f32 por t). Evidência bitnet.cpp: weight-parallel vence 1.29× em decode. | 🟡 | ADR-0084 | SESSION_247 | `cortex/src/bitnet_avx2.rs` (~40-80 LOC) |
| 2026-08-04 | **480** | **F2 activation-parallel prefill (m≥8)** - inverter ordem i→t→j p/ t→i→j (unpack da linha t 1×, FMA com m ativações); reativar `avx2_bitwise_matmul` (bug OOB n%4≠0 de SESSION_162) com guard de cauda + memoização do unpack por t + 2 layouts de packing; dispatch por m (m==1 → weight-parallel, m≥8 → activation-parallel). Evidência: ~2× em m≥32. | 🟡 | ADR-0084 | SESSION_247 | `bitnet_avx2.rs`, `compute.rs`, `tensor.rs` (~80-150 LOC) |
| 2026-08-04 | **481** | **M1 fidelidade relu2** - FFN do 2B4T é `down(ffn_sub_norm(relu2(gate(x))·up(x)))`, nosso forward usa silu → função diferente da treinada. `relu2(x)=max(x,0)²` — só mul, sem FPU, no_std-safe. | 🟡 | ADR-0084 | SESSION_247 | `cortex.rs`, `nn.rs` + `tools/bitnet_fwd_parity.py` |
| 2026-08-04 | **482** | **M2 SubNorms 2B4T** - alinhar as 4 RMSNorms/layer (attn_sub_norm, ffn_sub_norm) ao forward; validar contra formato (`rms_inner_attn`, `ffn_layernorm` já existem no header). | 🟡 | ADR-0084 | SESSION_247 | `cortex.rs`, `nn.rs` |
| 2026-08-04 | **483** | **M3 RoPE theta parametrizado** - 2B4T usa theta=500000, nosso default 10000 (frequências ~50× fora; `layer_features bit2` só liga/desliga RoPE). Parametrizar theta no header `.bitnet` (default 10000 próprios, 500000 2B4T). | 🟡 | ADR-0084 | SESSION_247 | header `.bitnet` + `cortex.rs` |
| 2026-08-04 | **484** | **M4 embed Q6_K** - embed/lm_head do 2B4T é BF16 tied; ternário em embedding = N/A (modelo quebra, bitnet.cpp src/README); Q6_K ~grátis (17.149 vs 17.109 PPL). Conversor embed BF16→Q6_K + dequant no loader (dequant já existe em `gguf.rs` K-quants) + **bump de versão `.bitnet`**; +190MB RAM slot 2B (82→270MB, heap cap 2GB OK). Sem retreino — re-conversão offline. | 🟡 | ADR-0084 | SESSION_247 | `tools/convert_bitnet.py`, `cortex.rs`, `gguf.rs` |
| 2026-08-04 | **485** | **F4 kernel I2_S/maddubs W2A8 (GATED)** - kernel oficial ggml-bitnet-mad: unpack shift+mask sem branch, `_mm256_maddubs_epi16` (u8×i8→i16, 32 MACs/inst), acumulação i32, scale f32/linha + si per-token no epílogo com desconto do viés {0,1,2,3}→{-1,0,1}. Ganho ~2-4× (29ms TPOT do 2B4T); **só WHPX/HW real** (TCG sem AVX2 = ganho nulo). Gate: Fases 1-3 completas + gaps de geração/contexto + execução primária WHPX/HW. | 🟡 gated | ADR-0084 | SESSION_247 | `bitnet_avx2.rs` (~150-300 LOC + scales) |
| 2026-08-04 | **486** | **F5 tiling configurável** - `ROW_BLOCK_SIZE=4`, `COL_BLOCK_SIZE=128`, `PARALLEL_SIZE=4` como consts de tuning por HW (p∈[2,4,8], row∈[2..32], col∈[32..1024]). | 🟡 | ADR-0084 | SESSION_247 | `bitnet_avx2.rs` (~20 LOC) |
| 2026-08-04 | **487** | **Receita treino 1-bit (próximo treino)** - tanh logit scaling 30×, LR constante + cooldown linear sem warmup, LRs separados por tipo de param (embed ~10× alto, betas (0.8,0.95)), QAT por expectativa softmax sobre codebook {-1,0,1} com pressure/anneal (ataca dead-zone do STE em <3B), Muon+Newton-Schulz 5 opcional, grad accumulation. Escopo: host PyTorch (HW Expert v6/router v2/novos); `BitNetTrainer` on-device recebe só o que porta barato (cooldown LR, lerp pressure). | 🟡 | ADR-0084 | SESSION_247 | `tools/train_*.py`, `BitNetTrainer` |
| 2026-08-04 | **488** | **bitnet_fwd_parity.py como gate** - paridade host vs kernel p/ todo item numérico (fidelidade M1-M3, F1/F2); política ADR-0084: sem mudança de formato que quebre arquivos legados sem bump de versão. | 🟡 | ADR-0084 | SESSION_247 | `tools/bitnet_fwd_parity.py` |
| 2026-08-04 | **489** | **HW Expert v6 (receita 1-bit)** - próximo treino do HW Expert com a receita ADR-0084 §4 (tanh scaling, LR cooldown, LRs separados, QAT suave). Escopo host PyTorch. | 🟡 | ADR-0084 | SESSION_247 | `tools/train_hw_expert_v*.py` |
| 2026-08-04 | **490** | **NVMe layout 64B spec** - `SubmissionEntry`/`NvmeRegisters` atuais 72B (spec 64B): `rsvd1: [u64;2]` (DW2-5 vs DW2-3) desloca mptr/dptr; `_reserved1: [u32;3]` desloca csts/aqa. Fix do STRUCT (não do teste) quando driver for exercitado em HW real. | ▶️ AWAITING_HW | ADR-0062 | SESSION_247 | `crates/k_nano/src/storage/nvme.rs` |
| 2026-08-04 | **491** | **Formato canônico `.bitnet v6` + registro K³CHJ** - padronizar o pipeline inteiro (requisito do dono): header autodescritivo (act_type p/ M1, embed_type p/ M4, feat computado do escrito, num_params u64 único, tie_flag, reserved=0), body por model_type (llm/hwexpert/router), scales SEMPRE presentes, rms_ffn_norm canônico = intermediate, tied ⇒ sem unembed, writer único `tools/bitnet_writer.py` + paridade byte-exact vs `save_model` (golden), loader v6 estrito + legado v3/v4 com WARN + `migrate_bitnet_v6.py` (re-conversão, não cirurgia), registro `cortex::model` (ModelView) + ModelHub `register_bytes`. STT/Piper/BGE/ViT ficam como exceções. Fases F0-F4. Auditoria de layout que destrava ADR-0084 Fase 3. | 🟡 | ADR-0085 | SESSION_247 | `tools/bitnet_writer.py`, `cortex.rs`, `cortex::model`, 8 conversores |

| 2026-08-04 | **492** | **Re-habilitação da NN no build_card** — restaurar branch HW Expert v4 em uild_card + provar ≥65% específico no protocolo honesto (split 90/10 por device seed 42 + sweep QEMU). Dependências: modelo futuro que prove o gate. | ⏳ gated | ADR-0082 + SESSION_248 | SESSION_248 | crates/k_ai/src/hw_capability.rs:build_card |
| 2026-08-04 | **493** | **MLP como alternativa ao transformer** — MLP contínuo pequeno (~130-260KB f32, embed+2×fc) como classificador de família de driver p/ devices nunca vistos; teto medido 63.27% (stage-2 sem imbalance). Só com modelo que prove ≥65% específico. | ⏳ gated | SESSION_248 | SESSION_248 | 	ools/probe_mlp_vendor*.py |
| 2026-08-04 | **494** | **Extração de Class= de .inf SDIO** — os .inf dentro dos DriverPacks têm [Version] Class=Network adapters etc.; extrair para enriquecer ground-truth de classe. Bloqueio: download pesado (gigabytes); valor marginal enquanto o alvo for família de driver (nomes pci.ids já cobrem — os .inf dariam classes genéricas, não famílias). | 💰 AWAITING | SESSION_248 | SESSION_248 | models/SDIO/, 	ools/extract_sdio_hw.py |
| 2026-08-04 | **495** | **Vocab 256 tokenizer** — pack_vid_did atualmente usa vocab 64 (descarta 2 bits/byte: vocabs 0x80/0x40/0xC0 → token 0). Vocab 256 ganha 0.02pt no controle (insignificante), mas o lift pode ser maior em modelos com mais parâmetros. | 🟡 marginal | SESSION_248 | SESSION_248 | cortex.rs:pack_vid_did:387 |
| 2026-08-04 | **496** | **Teto de sinal vid:did → família de driver** — medido em múltiplas arquiteturas: o teto é ~59-63% com nomes pci.ids cobrindo 54.7% dos devices (NÃO é imbalance, NÃO é capacidade, NÃO é tokenizer — é sinal nos dados). Documentado; reabrir só com nova fonte de dados (ex: SDIO .inf Class= de verdade ou labels curados manualmente). | ✅ medido | SESSION_248 | SESSION_248 | docs/evidence/hwexpert-architecture-verdict-20260804.md |
| 2026-08-04 | **497** | **Infra de prova/refutação de modelos HW** — sweep QEMU multi-device + validator Rust-exato + split honesto + controle contínuo + MLP probes; o protocolo está no repo e documentos; qualquer modelo futuro é provado ou refutado em 30-90 min sem tocar no kernel. | ✅ entregue | SESSION_248 | SESSION_248 | 	ools/hw_sweep/, alidate_hw_expert_v4*.py, probe_*.py, 
elabel_*.py |
| 2026-08-04 | **498** | **Tabela curada packed exata** — substituir os ~18 pares de 	able_lookup por uma tabela binária packed (vid,did→código de família) cobrindo os 22.806+ devices do pci.ids que têm família conhecida; ~50-100KB, 100% de acerto. A NN seria só para complementar os unknown. | 🟡 agendada | ADR-0082 + SESSION_248 | SESSION_248 | crates/k_ai/src/hw_capability.rs:table_lookup |
| 2026-08-05 | **499** | **ADR-0086 Instalação + Update OTA (processo canônico)** — consolida ADR-0079 (deprecada) + ADR-0031 §1 (deprecado) + ADR-0074 (lacuna sem arquivo) + #308/#421 num documento único; **10 gaps fechados em 8 commits, 0 erros**: U1 (switch_slot promove slot→kernel.elf, zero mudança no Limine), U2 (shell `update`), U4 (rollback com guarda tries + BootSelfHeal em PANIC), U6 (filtro ESP 0xEF + UPDATE.CFG na ESP), I9 (`boot_mode::mode()`), I10 (SELF.STATE na SGDB + record_life_event — autobiografia), I6/I3 (AutoInstallerAgent registrado + executa install real), I4/I5 (ModelProvisioner + leitura NeuralFS no boot), I11 (telemetria POST /api/logs), I7 (VRAM real via BAR0), I8 (self_check real), I12 (`build_image --mini`). Stub morto CHANNEL_MANIFEST_URL removido (URL do server só no UPDATE.CFG). U3 (Ed25519/TPM) = defer (hardening). | ✅ 10 gaps | ADR-0086 | SESSION_252 | `k_nano/{boot_mode,installer_agent,hw_profiler,self_check}.rs`, `k_ai/self_state.rs`, `hermes/self_update.rs`, `neural-kernel/{model_provisioner,log_agent}.rs`, `tools/{serve_update,build_image,mkfat32}.py` |
| 2026-08-05 | **500** | **NeuralFS correções F1-F16 (revisão profunda) + compatibilidade NeuralFS/MHI/SGDB C1-C10** — F1 CRÍTICO alocador contíguo (free-stack LIFO corrompia re-escrita de arquivo); F2 ordem CoW (dados→commit→reclaim; freeing deferido BAFS/LiberFS); F3 mount seguro (probe backup; nunca formata volume existente); F5 journal corrompido recusa mount; F6 format zera journal; F8 `read_range` (AirLLM streaming); F10 `valid_name`; F12 dead code removido (extent/checksum_tree); F13/F14/F15/F16 flush barrier. Licença BAFS MIT→GPL-3.0 (upstream congelado v1.2). **Compatibilidade:** C1 CRÍTICO TickvLite LBA 2048 colidia com ESP+NeuralFS (brick NVMe real) → fim do disco; C2 RAM volátil logado; **C2 ✅ (v1.9.9) → TICKV write-through no NeuralFS**: backend=ram (sem NVMe) espelha `put()` em `/mnt/neural/kv/<hex>` via bridge VFS; mount restaura do mirror (SELF.STATE/episódica/HANR/audit sobrevivem ao reboot). Fail do mirror = warn, nunca falha o put; nvme não espelha; C4 episodic tail O(n) removido; C9 ponte provision↔SGDB (pkg/model meta). Pendências: C6 ArcCache morto, C5 MHI hinting-only, C7 tiers, C8 rebuild. | ✅ F1-F16 + C1/C2/C4/C9 | NeuralFS.md + ADR-0086 | SESSION_252 + sessão F4b | `k_nano/neural_fs/{volume,superblock,neural_fs_agent,mod}.rs`, `k_nano/storage/{flash,tickv,storage}.rs`, `k_ai/{cognitive,sgdb}.rs`, `neural-kernel/{main,model_provisioner}.rs` |

| 2026-08-08 | **501** | **PR microsoft/BitNet: ativação relu² no 2B4T** — o `bitnet-b1.58-2B-4T` oficial declara `hidden_act: relu2` mas o submodule (`isHuangXin/llama.cpp` @ 390c3077) hardcoda `LLM_FFN_SILU` no `build_ffn` (`src/models/bitnet.cpp:133`) → logits wrong-but-finite em todos os backends (mesma classe do PR #586). O plumbing já existe (`llama_hparams::llm_ffn_op`, `LLM_KV_HIDDEN_ACT`, `LLM_FFN_RELU_SQR` em llama-graph.cpp:1690, padrão ModernBert) — só não é usado no arch bitnet; conversores não gravam `<arch>.hidden_activation`. Fix: mapa "relu2" + leitura no `load_arch_hparams` (fallback SILU p/ GGUFs antigos) + `add_hidden_act("relu2")` nos 2 conversores. Patches prontos e validados contra os arquivos upstream reais. | 🟡 aguardando submissão | docs/archive/pr-bitnet-relu2 | SESSION_249 | `docs/archive/pr-bitnet-relu2/{fix-cpp.patch,fix-converters.patch,issue.md,README.md}` || 2026-08-09 | **502** | **Reserva da stack do Limine no frame allocator (fix crash ip=0)** - o kernel pedia stack de 2MB ao Limine mas nunca lia `StackSizeResponse.address` (ABI incompleto) nem reservava a região → com loader BITNET2B @4GB, o watermark do frame allocator subia até a stack (~2.44GB) e entregava frames da própria stack → return address corrompido → #PF ip=0x0. Fix: `address` no response + `reserve_range(stack, 2MB)`. | ✅ 8901d97 | SESSION_254 | v1.9.9 | `crates/k_nano/src/limine.rs`, `crates/neural-kernel/src/main.rs` |
| 2026-08-09 | **503** | **Heap lazy AIOS: piso 512MB + grow_bump_auto sob demanda** - `resize_bump_heap(1024/1536)` eager no T+0 mapeava ~1GB sem necessidade, subindo o watermark e expondo bugs de reserva (crash de stack). Com piso 512MB, o LazyBumpAllocator cresce via `grow_bump_auto` (256MB/passo, OOM) — 2B v6 carrega (auto-grow 512→768→1024MB) sem gordura. | ✅ 8901d97 | SESSION_254 | v1.9.9 | `crates/neural-kernel/src/main.rs`, `crates/k_nano/src/allocator.rs` |
| 2026-08-09 | **504** | **HW Expert v6 (formato canônico mt=1) + loader F1b + imagem HW com BITNET2B v6** - conversor `tools/convert_hwexpert_v5_to_v6.py` (v5 prefixos → v6 sem prefixos, feat=0x03, sem rope; parity byte-exact + predições idênticas), loader `load_hwexpert_v6` (cortex.rs) + dispatch mt=1 (model.rs) + call sites v6-primeiro (main.rs), teste host com arquivos reais PASS; imagem `PACK_LLM=2b build_image --hw --unified --size 6144` → usb_hw.img com HWEXPRT4.BIN v6 (265620B) + BITNET2B.BIN v6 (755MB). Decisão crítica: preservar q_dim=32 (forward trunca atenção; colapsar p/ hidden muda predições). | ✅ SESSION_255 | SESSION_255 | v1.9.9 | `tools/convert_hwexpert_v5_to_v6.py`, `crates/cortex/src/cortex.rs`, `tools/mkfat32.py` |
| 2026-08-09 | **514** | **Revogação causal `BL/` (ADR-0089 Fase A)** — `TaskType::Revoke` no NoProto (payload: revoked_node_id u8, revoked_pubkey 32B, causal_sequence u64 LE, issuer u8, master_signature Ed25519 64B) + chave `BL/`+SHA256(node_id) no TickvLite (LWW por causal_sequence) + gatekeeper `is_peer_revoked` O(1) no handshake mesh. Propagação via `crdt_sync::crdt_sync_global` + heartbeat existentes. Menor risco; reusa 100% do mesh/ADR-0081. | 🟡 proposta | ADR-0089 | — | `k_nano/src/net/noproto.rs:44` (novo TaskType), `k_ai/src/sgdb/store.rs` (ns `BL/`), `k_nano/src/net/mesh.rs` (gatekeeper) |
| 2026-08-09 | **515** | **Agente efêmero com lifecycle (ADR-0089 Fase B/C)** — máquina de estados UNINSTANTIATED→ALLOCATING→RUNNING→(SUSPENDED⇄RUNNING)→COMMITTING→TERMINATED sobre `MpmcQueue`/`EventBus` + wasmi pool; envelope `TaskType::Replicate` (agent_id 36B, bytecode_hash 32B, energy_budget u64, ttl_hops u8, src_node_id u8, wasi_capabilities u32, input ≤64KB — respeitando teto FRAG). Gatekeeper determinístico: autônomo = zero-caps de escrita (`caps & 0x14 == 0`). Local-first (B), depois transporte P2P (C). | 🟡 proposta | ADR-0089 | — | `k_ai/src/sgdb` (commit via ARQ), `hermes/src/{wasmi_rt,app_factory}.rs`, `k_nano/src/sync/mpmc.rs` |
| 2026-08-09 | **516** | **Crate `mesh_proto` compartilhada (ADR-0089 Fase E)** — protocolo NoProto/UDP 42069 (heartbeat `PK\0`+pk+`CAP\0`, FRAG\0/FRACK\0, assinatura tiered HMAC/Ed25519) como crate no_std reutilizável pelo kernel (R0) E por apps host Rust std. Fonte única de contrato — evita duplicação de protocolo entre kernel e Runner Host app (guarda P8/check_duplication). | 🟡 proposta | ADR-0089 | — | `crates/mesh_proto/` (novo), extrai de `k_nano/src/net/{mesh,udp_broadcast,noproto}.rs` |
| 2026-08-09 | **517** | **Runner Host app (ADR-0089 Fase E)** — binário Rust std (Windows/Linux/macOS/Android/TV) que fala NoProto/42069 com o kernel; relay WAN (TCP/TLS no SO host, kernel só cliente — `log_agent.rs:4`); Wi-Fi Direct/BLE/Thread via stacks do SO (impossível no bare-metal); hospeda SLMs locais (tiers). Análogo cross-OS do kernel (ADR-0086). Maior gap de engenharia da malha global. | 🟡 proposta | ADR-0089 | — | novo app host + `crates/mesh_proto` (#516) |
| 2026-08-09 | **518** | **Host import `aios_ai::infer` (ADR-0089 Fase B)** — agente WASM chama `aios_ai::infer(model_id, input)` → host despacha para `cortex` (BitNet/embeddings) ou `jarbas` (STT/TTS). Substitui a proposta WASI-NN/llama.cpp (inexistentes no tree); padrão = `aios_net::http_get` existente em `wasmi_rt::install_host_abi`. | 🟡 proposta | ADR-0089 | — | `hermes/src/wasmi_rt.rs:79` (novo import host) |
| 2026-08-09 | **519** | **C-Watt voucher (ADR-0089 Fase E, pós-v2.0)** — custo = (fuel wasmi × k1) + (tokens SLM × k2) + (bytes × k3); voucher Ed25519 (cliente bloqueia saldo, provedor devolve fuel+hash+resultado, cliente valida e assina; `sequence_number` anti-replay). **Sem ZK** (inviável no_std). Ledger `sys/cwatt/*` no TickvLite; estende `k_ai/economy.rs` (hoje orçamento local sem moeda). | 🟡 proposta | ADR-0089 | — | `k_ai/src/economy.rs`, `k_nano/src/net/noproto.rs` (novo TaskType opcional) |
| 2026-08-09 | **520** | **RUSTCDR3.v6 — conversão do RustCoder grande (~321MB, v5)** — `RUSTCDR3.BIN` em target1/ é formato v5 sem fonte no tree (zero `.gguf`/`.safetensors`). Header v5 (prefixos u32 len+scale) não carrega act_type/embed_type/feat que o v6 exige — conversão fiel só com a fonte original. Bloqueio: obter safetensors/GGUF do RustCoder 3B-class. Enquanto isso: fallback `.BIN` funcional (fat_names_for `model_hub.rs:337`). | ⏳ aguardando fonte | ADR-0085 (formato) / ADR-0089 (hub) | — | `target1/RUSTCDR3.BIN`, `tools/convert_bitnet.py` (padrão) |
| 2026-08-09 | **521** | **RERANKER.v6 — conversão do cross-encoder (~165MB, v5)** — idem #520: `RERANKER.BIN` em target1/ é v5 sem fonte no tree; conversão fiel exige a fonte original. Fallback `.BIN` funcional. VISION.BIN (ViT) é exceção v6 por design (ADR-0085 §D7) — NÃO converte. | ⏳ aguardando fonte | ADR-0085 (formato) / ADR-0089 (hub) | — | `target1/RERANKER.BIN`, `tools/convert_bitnet.py` (padrão) |
| 2026-08-10 | **522** | **neural-sgdb — SGDB como projeto comunitário standalone (extração Modo 1) + Maturation v0.3** — núcleo `k_ai::sgdb` extraído para repo separado `github.com/msrovani/neural-sgdb`, MIT OR Apache-2.0, zero deps, dual-mode `no_std`+`std`. Topologia decidida (HITL): evolução independente; OS mantém `k_ai::sgdb` AGPL interno, SEM fiação (porta futura = versão crates.io); interop por NMD1 + **TKLV/TKCK byte-idênticos**. Entregue v0.1 (núcleo + Storage trait + p2p + MCP) + **v0.3 maturação** (SESSION_257, 14 commits): VectorClock semântico, CRDT multi-value/own_writes, FileStorage recovery endurecido (CRÍTICO tombstone ressuscitava), BQ bounded top-k (320µs vs 592µs), durability+fsync, compact atômico, rebuild público, MemoryState sem quebrar NMD1, fuzz adversarial + revisão independente (5 fixes). **Roadmap 6/6 — TKLV interop concluído**; 66+1/44+1/75+1 testes, no_std limpo. | ✅ implementada | ADR-0063 | SESSION_256+257 | `C:\DEV\neural-sgdb` (repo separado), `docs/api.md`, `docs/architecture/01..06` |
| 2026-08-11 | **523** | **Wrap 2^64 no grow_bump_auto sem guard (bughunt s258)** - heap_start + current_limit (high-half) envolve para VAs baixas em offset ~2GB; boot empírico cresceu 2560MB (offset ~2305MB) sem crash (VAs baixas caíram em RAM livre), mas é a classe de corrupção do s254 reaberta. Fix: check de wrap antes de mapear (ou HEAP_EXT_BASE p4[508] com check HUGE_PAGE em todos os níveis - falhou em s249b). | 🟡 agendado | - | SESSION_258 (achado) | crates/k_nano/src/allocator.rs:102 |
| 2026-08-11 | **524** | **2o probe QEMU-loader hardcoda BITNET_2B_V4_BYTES (604MB)** - fix autodescritivo 6_file_size só no 1o site (main.rs:2806); o espelhado em 0x120000000 (main.rs:2889-2892) ainda trunca v6 de 792MB nesse endereço. Fix: aplicar v6_file_size nos dois sites. | 🟡 agendado | - | SESSION_258 (achado) | crates/neural-kernel/src/main.rs:2889 |
| 2026-08-11 | **525** | **PDPTE com HUGE_PAGE no nível errado (map_region_uc_2mb_at)** - grava 2MB no PDPTE (deveria ser PDE); 512 slots 2MB de 1GB alias na MESMA página física. Latente (GPU AWAITING_HW) mas catastrófico: 8GB VRAM → 4 páginas alcançáveis. Fix: indexar com p2_index no nível PDE. | 🟡 agendado | - | SESSION_258 (achado) | crates/k_nano/src/apic.rs:440-446 |
| 2026-08-11 | **526** | **deallocate_frame sem ownership check** - double-free/stale phys (dma.rs, gguf_mmap.rs, k_ia_dma.rs) libera frame vivo (kernel/stack/PT) silenciosamente; o DMA pode sobrescrever memória viva (residual s252/ora-1). Fix: bitmap de ownership + recusar dealloc de frame não-entregue. | 🟡 agendado | - | SESSION_252 residual + 258 | crates/k_nano/src/memory.rs:270-283 |
| 2026-08-11 | **527** | **Expert loader-scan hardcoded cai dentro do arquivo 2B v6** - janelas 0x129000000-0x129400000 (main.rs:3161-3162,3253) ficam dentro do BITNET2B v6 (792MB→0x131800000) com PACK_LLM=2b; QEMU_LOADER_SCAN_START (set em 2872) nunca é lido. Fix: usar a variável + bounds dinâmicos. | 🟡 agendado | - | SESSION_258 (achado) | crates/neural-kernel/src/main.rs:3161-3162,3253 |
| 2026-08-12 | **528** | **LEDs do teclado (cmd 0xED) + self-test 8042 (0xAA→0x55)** — feedback visual de CapsLock/NumLock/ScrollLock + diagnóstico de bringup (interface test 0xAB: clock/data stuck). Referência completa minerada do BrokenThorn T19 (mempalace room brokenthorn-osdev). Valor baixo-médio: LED CapsLock já é sentida pelo usuário; self-test ajuda debug HW real. | 🟡 agendado | - | SESSION_259 (mine BrokenThorn) | `crates/k_nano/src/scancode_to_ascii.rs`, `crates/k_nano/src/interrupts.rs` |
| 2026-08-12 | **529** | **Teclas estendidas E0 (setas/Insert/Home/End/Delete) + scancode set 2** — InputAgent ignora prefixo 0xE0 (byte chega como break fake de 0x60) e a tabela é set-1 only; setas/Home/End/Delete não geram nada no buffer (WM trata 0x53 via `scancode_to_keycode` no jarbas). Gap real: navegação por setas no prompt do Hermes. | 🟡 agendado | - | SESSION_259 (mine BrokenThorn + scan) | `crates/neural-kernel/src/agents.rs` (InputAgent), `crates/hermes/src/agents.rs`, `crates/k_nano/src/xhci/mod.rs` (hid_to_scancode) |
| 2026-08-12 | **530** | **USB-MSC: validar CSW tag + DMA em páginas WB (crash HW s260, ora-1)** - csw_sig==CSW_SIGNATURE && csw_status==0 nunca compara o **tag** (usb_msc.rs:148-160); CSW stale em DMA page reutilizada passa como sucesso com transferência falha/mis-landed → dir FAT atualizado sobre dados não-escritos. DMA em páginas WB sem map_page_uc (usb_msc.rs:232-238) — classe do bug e1000: sem snooping no chipset, CSW read pode ser stale. Fix: comparar tag + mapear DMA UC. | 🟡 agendado | - | SESSION_260 (ora-1) | crates/k_nano/src/usb_msc.rs:148-160,232-238 |
| 2026-08-12 | **531** | **overwrite_boot_log com escrita de dir cluster não-atômica** - reboot/crash no meio da reescrita do dir (boot_logger.rs:182-209, 1 WRITE10/setor, 10+ flushes antes do K33) rasgava o FAT32 do pendrive (corrupção agravada a cada boot). Fix: tornar a escrita do dir atômica (journal de 1 setor ou write dir por último + flag de commit). | 🟡 agendado | - | SESSION_260 (ora-1) | crates/k_nano/src/boot_logger.rs:182-209 |
| 2026-08-12 | **532** | **Grafo de mesh persistente via SGDB** — "a base fica mais esperta a cada run" (tweet Kimi Agent Swarm): o grafo não morre quando fecha a aba; a próxima execução soma no mesmo grafo. Hoje `MESH_GRAPH` é snapshot volátil do tick (peers + RTT). Persistir nós/arestas (peer↔peer, peer↔skill, hub) no SGDB (`sys/mesh_graph/*`) e reidratar no boot → clusters/hubs históricos (ponto único de falha) visíveis mesmo sem peers ativos. | 🟡 agendado | - | SESSION_261 | crates/jarbas/src/display/agent.rs (MESH_GRAPH), crates/k_ai/src/sgdb |
| 2026-08-12 | **533** | **K130 freeze Alienware Meteor Lake — granular checkpoints + agent-core fix** — freeze no K130 (pre-smokes) em Alienware Intel Core 7 240h (Meteor Lake). Adicionados checkpoints por smoke test (K130: hw_gate/ipc_bus/async_io/git_thin; K131: wifi_softmac/wpa2_hs/dhcp; K132: limine_esp/ath10k/tls_trust/self_update/ntp; K133: theme_bridge/clipboard/boot_chime/vconsole/screensaver/manpages/image_viewer/fts_search/user_accounts/fw_cfg; K134: async_rt/cf_challenge/xhci/btrfs/luks/ext4/vfs/smp/gpu_i225/hda/acpi/firewall/capgate/bt_hci/elf_loader/gsp; K135: platform_probe/simd). Hipótese: `async_io::boot_smoke()` spawna HTTP/TCP/FAT jobs ANTES de drivers init (linha 1706+) — falha rápida mas edge cases de alocação/locking em topologia híbrida Meteor Lake. Fix estrutural em `agent-core/lib.rs`: métodos fora de impl blocks (merged register/activate/set_budget/get/set_urgency/active_count/agents_by_affinity_ring/poll_order_by_affinity + Default impl). Build 0 erros; usb_hw.img 3199MB gerado. Próximo: boot HW real, ler último K-checkpoint no framebuffer. | 🟡 agendado | - | SESSION_268 | crates/neural-kernel/src/main.rs, crates/agent-core/src/lib.rs |
| 2026-08-12 | **533** | **Click-to-inspect no grafo de mesh** — mouse PS/2 já integrado (handle_pointer_click); clique num peer (satélite) abre card UI_SPEC (ADR-0058) com RTT/p99/TX/ACK/fail (dados já no `MESH_GRAPH`); clique no hub → persona Jarbas. Sem labels na tela (poluição visual); inspeção sob demanda. | 🟡 agendado | - | SESSION_261 | crates/jarbas/src/display/agent.rs, crates/jarbas/src/display/card.rs |
| 2026-08-22 | **534** | **ADR-0090: Jarbas Desktop v2.0 — 15 features em 4 Tiers (~3.010 LOC, ~66 dias)** — Tier 1 Quick Wins: glyph cache, grid pre-render, sin LUT, dock rendering (4 dias). Tier 2 Polish: window animations, chat scrollback, hover states, voice waveform (12 dias). Tier 3 Desktop Real: per-window buffers, chat markdown, file manager card, terminal card (20 dias). Tier 4 Transformacional: client-side rendering, GPU compositing, multi-monitor (30 dias). Comparação com Redox Orbital (per-window buffers, damage tracking), COSMIC (tiling, animations), Theseus (modular display). docs/architecture/0090-jarbas-desktop-v2-roadmap.md | 🟡 agendado | - | ADR-0090 | crates/jarbas/src/display/* |
| 2026-08-22 | **536** | **ADR-0091: Migração neural-sgdb — substrato de memória cognitiva** — 9 commits, ~1150 LOC, 0 regressão. Fase 0+1: TickvStorageAdapter (neural_sgdb::Storage → TickvLite). Fase 2: NSGDB Bridge global (SafeSgdb Send+Sync, recall/rag via neural-sgdb). Fase 2.5: Hits tipados (12 campos), OsEmbedder, recall_lexical, lifecycle_tick, scoping, cognitive ops. Fase 3.0: Memory Interpreter (gated_rag_context com content_type), Memory-Aware Route, sgdb_agent lexical, lifecycle no SleepCycle. | ✅ implementado | ADR-0091 | SESSION_284 | crates/k_ai/src/sgdb/{tickv_adapter,nsgdb_bridge}.rs, crates/hermes/src/{cognitive_bridge,agents,sgdb_agent}.rs |
| 2026-08-22 | **537** | **[RESOLVIDO via sync_write] Migrar 75 callers de put_kv/get_kv para neural-sgdb** — Fase 3 restante do ADR-0091. Os callers em hermes/neural-kernel que usam  precisam ser migrados para usar o neural-sgdb como write primário (dual-write eliminado). Impacto: ~4-5 dias, 75 callers em 10+ arquivos. | ⏳ | ADR-0091 | - | hermes/src/{wifi_agent,skill_market,net,memory_store,package_hub}.rs, neural-kernel/src/{tls_trust,model_provisioner,bei_init,main}.rs |
| 2026-08-22 | **538** | **[RESOLVIDO via get_os_embedder] neural-sgdb set_embedder upstream** — O Embedder trait do neural-sgdb não tem  no Sgdb. O OsEmbedder implementa o trait mas precisa ser injetado no momento do open. Criar PR upstream para  ou . | ⏳ | ADR-0091 | - | crates/neural-sgdb/src/sgdb.rs |
