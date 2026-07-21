# SESSION INDEX — neural-os-core v1.9.1 TEST

**Propósito:** Catálogo de sessões. A pasta viva `docs/memory/` mantém `SESSION_107+`; sessões históricas anteriores ficam em `docs/archive/sessions/`.

---

## Sessões Mantidas (107+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|--------|--------|-------|--------|----------------------|
| 163 | UI/SMP | ADR-0057/0058 | Compute Dispatch + Generative Card Desktop | wake multi-AP sequencial (-smp4 APs=3); dispatch NPU→GPU→SMP; #412 decode; embedded-graphics cards (close/mover/resize); orb+HUD preservados |
| 162 | LLM | BitNet ladder 850 | #PF AVX2 + BPE SP32/MRG1 | OOB store n%4; encode=HF; coh semântica residual |
| 161 | WiFi | ath10k A3 | BMI→fw_ready | CE0/1+LZ; FW_IMAGE 681KB; PASS só FW_IND pós-DONE |
| 160 | WiFi | ath10k | pivô Note1050 QCA6174 | 168C:003E; FW 1.45MB; BMI/CE scaffold; iwl secondary |
| 159 | WiFi | S0+prepS1 | honesty + DID/FAT | VERDICT AWAITING; fw_resolve=SKIP QEMU; sem ALIVE |
| 158 | TLS | N4 | #123 PKI híbrido | pins+TOFU; smoke root_learn→root_pin; CertVerify residual |
| 157 | TLS | N4 | #123 smoke PASS | google HTTPS 80952 B; WHPX qemu64; soft polyval/aes/sha2 |
| 156 | TLS | N4 | #123 wire https_get | NetTcpIo+KernelRng; VERDICT=WIRED trust=unsecure |
| 155 | TLS | — | #123 probe A PASS | `embedded-tls` 0.19 soft-float ✅ `tools/check-tls` |
| 154 | pesquisa | — | TLS #123 (B) + WiFi firmware/plano | Opções A–D N4; API77 ~7,51 MB; FW-MAC; S0–S5; #408 reclass |
| 153 | release | — | **v1.9.0 TEST** | Pós-LAN + Residuals 0–7; tag v1.9.0; ≠ v2.0.0 |
| 152 | Pós-LAN | — | B-01 fila unlock ondas 0–5 | net_bridge; NetFs PASS; TLS BLOCKED; deadlock NETSTACK pós-L5 fix |
| 151 | planos | — | Fecho Residuals 0–7 | PreFlight pass_marker; LAN✅; WiFi AWAITING; TLS/418 PARTIAL |
| 150 | Onda 7 | — | DNS raw + HTTP L4/L5 | skip_dns_name; OK raw google; L5 301; internet smoke |
| 149 | Onda 7 | — | LAN RX destravado (e1000 TX) | aliases 0x0420 no-op QEMU; TDT=0x3818; L3.5 PASS |
| 148 | Onda 6–7 | — | AirLLM-DMA + NET/WIFI-HW | residuals ATA; lan fila BLOCKED; PreFlight cache |
| 147 | Trilha R | — | soft-float/VITS pesquisa | defer hardfloat; neural-lite permanece |
| 146 | Onda 5 | — | MHI-DMA + GPU-HW AWAITING | Vram hook #67; GDS stub; [GPU-HW] [MHI-DMA] sem fake Ready |
| 145 | Onda 4 | — | USB Trust + UAC-HW | usb_trust/usb.tbl; enforce+disable_port; [UAC-HW] AWAITING; soft-float defer |
| 144 | Onda 3 | — | exFAT write + FS agents fecho | `exfat_write` + EXFAT_WRITE=1; 282e–g ✅; 282h ⏳; #418 BLOCKED |
| 143 | docs | — | Auditoria ideias antigas | STALE→✅; VIABLE→Onda+lan; DEFER/💰/❌; fecho por ID |
| 142 | Ondas 0–1 | — | PreFlight + NeuralFS evidência | preflight_wave.py; depends_on: lan; Onda 7 crônicos; smoke_level2/power_loss; NRFS-HW AWAITING |
| 142 | Models | — | Multi-model hub | TinyStories/3B/850M slots; gguf_wasm SkillMarket; RustCoder 2B/3B FAT |
| 141 | ADR-55 | — | FeatureGate + SMP revision | PlatformProbe; WHPX SMP off; TCG APs=1; RSDP BootInfo; CorePools |
| 140 | ADR-41 | — | H4+/H5+/AS + HalOffer Cap → v1.8.6 | QUEUE_NOTIFY real; Cap grant bind; AS shallow PoC; 1.8.x |
| 139 | HW | — | USB BOOT.LOG + console FB | FAT 0x0C+ESP; BltOnly patch; console_clear |
| 107 | 107 | — | Boot A/B + Cap P0–P9 + ADR-0042 | Runtime QEMU OK; cadeia k-nano→…→jarbas; próximo N1 legível |
| 107_CLOSE | 107 | — | Fecho Voice I/O | 5 loops WHPX; PASS parcial forte+; handoff de gaps para Sprint Sound |
| 108 | 107 | — | N1 ✅ + BitNet 2B LOADED → v1.7.0 | Soft-float/cargo nk; 2B ~590MB L=30 FWD; TTS empty generate; e2e clima PARCIAL |
| 109 | 107 | — | ADR-0045 Sound Voice Stack | Truth=`neural-kernel/audio`; jarbas espelho; sherpa/Vosk/Kokoro/Wyoming/Rustpotter ❌; v1.7.1 docs |
| 110 | 107 | — | Sprint 107 loops 1–5 clima e2e → v1.7.2 | GEN 'O tempo esta'; Piper neural-lite; WakeWord registrado; STT ctc=''; HWEXPERT FAIL |
| 111 | Sound / ADR-42 | — | Handoff voz 107→Sprint Sound; pista limpa ADR-0042 | Docs v1.7.3; 107 Voice ✅ FECHADA; leftovers Sound; N2 próximo |
| 112 | ADR-42 N2 | — | N2 SelfHeal VID+Trust CLOSED | v1.7.4; QEMU `[N2-SELFHEAL]`+`[TRUST]`; N2.5 allocator; pista N3→N5 |
| 113 | ADR-42 N3 | — | N3 cortex LOADED + Trinity CLOSED | v1.7.5; `[N3-CORTEX] criteria=MET`; N3.5 allocator |
| 114 | ADR-42 N4 | — | N4 Hermes orchestrator CLOSED | v1.7.6; `[N4-HERMES] criteria=MET`; N4.6 allocator |
| 115 | ADR-42 N5 | — | N5 jarbas ego/UI CLOSED | v1.7.7; `[N5-JARBAS] criteria=MET`; N5.7 allocator; N1–N5 ✅ |
| 116 | ADR-42 N2.5 | — | k_ai crate wired no bin | v1.7.8; allocator único no bin; espelhos trust/self_heal removidos |
| 117 | ADR-42 N3.5 | — | cortex crate wired no bin | v1.7.9; 9 espelhos removidos; residuals cortex/bpe/global_arena/cortex_mmap |
| 118 | ADR-42 N4.6 | — | hermes crate wired no bin | v1.7.10; 37 espelhos removidos; residuals agents/net*/fs/aios_api |
| 119 | ADR-42 N5.7 | — | jarbas crate wired no bin | v1.7.11; 29 espelhos removidos; audio truth residual ADR-0045 |
| 120 | v1.8.0 | — | Marco K³CHJ pós-jornada | ADR-0042 N1–N5 + wire N2.5→N5.7; docs; tag v1.8.0; pista Sound |
| 121 | 108 | — | Self-Evolving Agents CLOSED | self_evolve engine; verify; SIL wired; SelfEvolveAgent; SleepCycle REFLECT |
| 122 | Sound | — | Sprint Sound CLOSED (parcial honesto) | Mic→Wake gate; STT PCM; UAC parse; neural-lite; VAD/SER; soft-float/VITS aberto |
| 123 | FS | — | NeuralFS I/O usavel (RAM) | B-tree insert/delete; create/read/write; /mnt/neural; smoke OK |
| 124 | ADR-0040 | — | FS MVP aceite (por_fazer→completa) | MHI soft-migrate; exFAT driver; IDEA #417–423; defer #421–423 |
| 125 | ADR-0040 | — | Triagem residuals deferidos | Nenhum viável agora; todos `por_fazer`; ADR MVP intacta |
| 125 | Cortex / ADR-0047 | — | N-gram speculative decoding OK | Cache por posição LWW; verify draft[0]; KV truncate sem double-forward; IDEA #443 |
| 126 | ADR-0047 família | — | LatentBus+Evolve+Probe+GPU/HMI MVP | L1–L3 + G1/G2 + H1/H4; residuals G3–G5/H2–H5 |
| 127 | ADR-0046 | — | AirLLM GGUF streaming MVP | Layer-wise load/forward; soft prefetch; ATA set_model; Net/DMA/K-quant defer |
| 126 | ADR-0047 | — | Família MVP PoC Accepted parcial | LatentBus+Evolve+Probe+GPU G1/G2+HMI H1/H4; IDEA #444–448 |
| 127 | ADR-0047 | — | Wave2 bench/Genesis/G3–G5/H2/H5 | Descartes ISA/adapter/H3; IDEA #449–453 |
| 128 | ADR-0046 | — | Hot-swap Net→FAT→AirLLM | `hot_swap_from_net` + FAT 8.3 write fix; `/model-fetch`; L3.5/RX honest fail |
| 129 | v1.8.5 | — | Consolidação pós-v1.8.0 (teste) | Self-Evolve, Sound, NeuralFS, AirLLM, ADR-0047 e propostas GPU 0048–0050 |
| 130 | fix / polish | — | SkillMarket total_cmp + bpp dinâmico + 0 warn | `wasm_rt::top` NaN-safe; FB bpp do GOP; release 0/0 warnings |
| 131 | HW-PnP | — | HwCapabilityCard + Expert v4 schema | cards EventBus; sem OA5US free-text; train_hw_expert_v4 seed |
| 132 | FS | — | NeuralFS multi-nivel + USB teste + exFAT dados | B-tree 3+; MBR 0x7F USB; mkexfat; VolumeLength@72 |
| 133 | FS | — | Fechar residuals implementáveis | USB format lock; GPT NeuralFS; unified exFAT; boot checksum |
| 134 | ADR-0051 | — | Agency/nativos → AGENT.md | 214+41 manifests (stubs); VFS bridge; seed embutido |
| 135 | ADR-0052 | — | Contrato artefatos + deny stubs | Regra Cursor; validate deny; AGENCY=0; stubs apagados |
| 136 | ADR-0053 | — | HANR parity marketplace+trust | Session Ed25519; market; memory; MCP mínimo |
| 137 | ADR-0053+ | — | Cognitive Bridge superior ao HANR | BGE+Trinity prompt; SOUL≠PERSONA; budget; `/search`; REFLECT nudge |
| 138 | ADR-0048–50 | — | GPU Multivendor Unlock fundação | Caps honestas; NKP; canário; LegacyAcr/Gsp/Gen9/Arc/KiQ; packers host |
| 139 | HW / USB | — | BOOT.LOG + console FB legível | MBR FAT dados+ESP; BltOnly SetMode; heap_ready; console_clear/print; K0–K17 |

## Sessões Históricas e Lacunas

Sessões 079, 080, 081, 082, 083, 089, 093, 106.3 e 106.5 estão preservadas em `docs/archive/sessions/`. Os identificadores **094** e **095** aparecem em registros históricos, mas os arquivos individuais nunca estiveram disponíveis nesta sanitização; o conteúdo está consolidado em `CHANGELOG.md`, no plano arquivado da Sprint 106 e neste índice.

| Sprints | Tópicos Chave |
|---------|---------------|
| 1-13 | Toolchain, VGA, IDT, Memory, SIMD, Tensor, BitLinear, 2-bit Packing, PIC, SMP |
| 19-38 | PCI, ACPI, ATA, FAT12, RTL8139, e1000, Neural Cortex, Transformer, xHCI, Self-Healing |
| 56-68 | FAT32, Bootloader 0.11, DisplayAgent, VFS, GPU Architecture, Auto-Skills, USB-MSC |

---

## Lições Críticas (NÃO REPETIR)

Estes são caminhos já trilhados que terminaram em dead-end ou soluções já encontradas:

1. **Xuvisco no boot (Sprint 71, 79):** UEFI/OVMF não mapeia 0xB8000 → page fault pre-IDT. Solução: VGA sequencer I/O ports (0x3C4/0x3C5) para Screen Off. **Nunca escrever em 0xB8000 antes da IDT.**

2. **AVX2 sob WHPX (Sprint 80):** Instruções VEX/AVX2 causam VM exit (~10k+ ciclos). Scalar é 2x+ rápido. `has_avx2()` deve detectar hypervisor via CPUID 0x40000000 e retornar false.

3. **e1000 TDT bug (Sprint 23-24):** `send()` escrevia REG_TDT = idx (== TDH) → hardware via ring vazio. **TDT = (idx+1) % NUM_DESC.** NUM_DESC RX mínimo = 48 para 82540EM.

3b. **e1000 TX aliases QEMU (SESSION_149):** `TDBAL/TDT` em `0x0420/0x0438` são aliases Intel **não wired** no QEMU → write no-op, `TDT` fica 0, ARP nunca sai, RX=0. **Usar `0x3800/0x3818`.**

3c. **DNS name compression (SESSION_150):** ao pular nomes DNS, **não seguir** pointer `0xC0xx` no offset de continuação — só avançar 2 bytes no wire (`skip_dns_name`). Seguir o pointer corrompe o parse do A record.

3d. **Hermes net espelho (SESSION_152):** FE (Browser/Search/Market) **não** usa `hermes::net` (NETSTACK vazio). **Registrar `net_bridge` no boot** → `neural-kernel::resolve_and_http_get_safe`.

3e. **Deadlock NETSTACK (SESSION_152):** `smoke_if_online`/`tcp_exchange` **dentro** de `NETSTACK.lock()` em `bootstrap_early` → hang pós-L5. Smoke só **após** return do bootstrap.

3f. **HTTPS / TLS soft-float (SESSION_152–157):** **nunca** strip `https://`→:80. Build precisa `polyval_force_soft`+`aes_force_soft`+`sha2/force-soft` (LLVM CLMUL). Smoke: WHPX `-cpu qemu64` (não `host`/APX); TCG soft-AES muito lento.

3g. **iwlwifi ≠ SoftMAC clássico (SESSION_154):** AX200/210 = **FW MAC**; ACK no firmware. Não priorizar Embassy/#408 como SoftMAC ACK ~10µs (ath9k). Plano S0–S5; gap `.pnvm`.

4. **Ramdisk via bootloader (Sprint 79):** FAT partition autosized ~64MB insuficiente para modelos >100MB. **Usar QEMU loader (`-device loader,addr=0x100000000`) para dev, NVMe/FAT32 para HW real.**

5. **QEMU loader com -m 2G (Sprint 79):** Modelo em 512MB conflita com frame allocator do bootloader. **Usar -m 4G+ e addr=0x100000000** (acima de 4GB).

6. **VirtIO-GPU GET_DISPLAY_INFO (Sprint 45):** Resposta 0x0 no QEMU TCG. Bug de emulação. **Framebuffer UEFI é mais confiável.**

7. **FAT12 removido (Sprint 75):** FAT32-only. 102 LOC eliminados. **Novos FS devem ser FAT32+.**

8. **Partition mask 0x1C (Sprint 74):** Hidden FAT32 LBA. Bootloader aceita (mbr_nostd mapeia 0x1C→Fat32). SO não monta. **Usar 0x0C para compatibilidade com outros OS.**

9. **TPM fallback (Sprint 74):** TPM ausente → 0xFFFF FFFF no probe. **Fallback silencioso.** Ed25519 é enforcement primário.

10. **Hermes event-driven (Sprint 76):** ReAct cycle só avança com entrada real. **84 linhas/seg → 0 quando ocioso.**

11. **Sprint 106-1: Cargo workspace (2026-07-13):** Workspace com 5 membros (k_nano, k_ai, cortex, hermes, jarbas) com resolver="2". Isolamento de dependências entre camadas lógicas. **Dependências não devem vazar entre anéis.**

12. **Sprint 106-2: Rename crates (2026-07-13):** k_ia → k_ai (Ring 1 Lógico), jarvis → jarbas (Ring 2 HCI). Backups preservados. **Nomes devem refletir a arquitetura de anéis.**

13. **Sprint 106-5: RustPython viabilidade (2026-07-13):** RustPython **NÃO é no_std nativo** — depende de `std`. **Rota principal = MicroPython/WASM (106-6)** via `wasm_rt.rs` e `micropython_wasm.rs`.

14. **Sprint 106-6: MicroPython/WASM (2026-07-13):** Compilado MicroPython para .wasm, sandbox isolado. **WASM é sandbox seguro para skills.**

15. **Sprint 106-7: Page faults (2026-07-13):** Ordem correta: allocator → events → agents. lazy_init!() para agentes dependentes de heap. **Inicialização deve seguir ordem estrita.**

16. **Capability ADR-0041 P0–P9 (2026-07-14):** Platform sync ANTES dos drivers. Toda demo Cap é **non-fatal**. Ring3 PoC existe (`iretq`/stub) mas **default off** (`TRY_ENTER_RING3=false`) no boot estável. VirtIO vring = layout+pin **sem QUEUE_NOTIFY**. #PF = PRESENT only (**sem I/O no fault**). Syscall soft = `int 0x90` (não 0x80).
17. **Adequação ADR-0042 (2026-07-14):** Cadeia `k-nano → k-ai → cortex → hermes → jarbas`. Identidades: legível / HW-AI+SelfHeal / cérebro / orquestra / ego+10%. Boot OK = N0; implementar **N1→N5** sem regredir Runtime. Boot OK ≠ visão completa. **`v2.0.0` só quando N1–N5 prontos**; até lá tags `1.x` (ex. 1.5.7 Cap, **1.7.0** N1+2B LOADED).

18. **v1.7.0 / soft-float + 2B LOADED (2026-07-15):** Nightly SSE em `x86_64-unknown-none` → soft-float + `cargo nk`. FAT free-scan por setor (não 1 I/O/entry). BitNet 2B real ~590MB/30L (não confiar ficheiro ~203MB truncado). QEMU load+FWD: timeout serial **≥~5 min**. **LOADED ≠ generate**: `[JARBAS-TTS] FAILED empty generate` é known issue.

19. **ADR-0045 Sound (2026-07-16):** Voz bootável = HDA + Piper (+formant) + STT CTC nativos em `neural-kernel/src/audio`. **Não** reabrir sherpa-onnx / Vosk / Kokoro-primário / Wyoming / Rustpotter como stack de kernel. `jarbas/audio` é espelho não wired. WakeWord **registrado** (Loop 5); leftovers (Mic→WAKE e2e, STT retrain, Piper VITS, UAC, jarbas wire) → **Sprint Sound (reaberta)** — ver SESSION_111.

20. **HW USB / FB console (SESSION_139, 2026-07-17):** Stick removable Windows = MBR slot0 FAT32 dados (`0x0C`) + slot1 ESP (`0xEF`) — não 0xEE-first nem GPT-only. Sem serial: `console_clear` + `console_print` (limpa faixa); `fb_print`→mesmo path; limpar TRACE no probe. `BOOT.LOG` só após heap (`heap_ready`). GOP Intel `BltOnly` → vendor bootloader `SetMode` Rgb/Bgr. Early boot: sem alloc/`println` em `disable_vga_plane`.

---

## MAPA DE MEMÓRIA (MemPalace + Docs)

| Domínio | Onde encontrar | Propósito |
|---------|---------------|-----------|
| Estado atual | `docs/memory/STATE.md` | Versão, sprint atual, arquitetura, pendências |
| Ideias | `docs/memory/IDEA_BANK.md` | 354+ ideias catalogadas com status e sprint |
| Decisões | `docs/architecture/*.md` | 38+ ADRs (ADR-0001 a ADR-0037+) |
| Planos históricos | `docs/archive/sprints/` | Planos concluídos, incluindo Sprints 92–100 e Sprint 106 |
| Checklist | `TODO.md` | Checklist mestre com sub-itens, goals, dificuldades |
| Sessões (aprendizado) | `docs/memory/SESSION_*.md` | 42+ sessões com descobertas e correções |
| Este índice | `docs/memory/SESSION_INDEX.md` | Catálogo de sessões + lições críticas |
| Sprints detalhados | `docs/archive/sprints/SPRINT-106.md` | Sprint 106-1 a 106-10 com ações e resultados (arquivado) |
| Roadmap completo | `ROADMAP.md` | v1.0 → v2.0 com status de cada sprint |
| CHANGELOG | `CHANGELOG.md` | Histórico de versões |
| Código fonte | `crates/neural-kernel/src/` | Kernel bare-metal (135+ arquivos Rust) |
| Workspace crates | `crates/k_nano/`, `crates/k_ai/`, `crates/cortex/`, `crates/hermes/`, `crates/jarbas/` | Anéis lógicos K³CHJ (v2.0) |
| Config VM | `tools/` | Scripts de build, QEMU launch, image creation |

---

## SPRINT 106 — RESUMO (2026-07-13)

| Sprint | Status | Descrição |
|--------|--------|-----------|
| 106-1 | ✅ | Cargo workspace estrito (k_nano, k_ai, cortex, hermes, jarbas) |
| 106-2 | ✅ | Rename crates (k_ia→k_ai, jarvis→jarbas) |
| 106-3 | ✅ | SOUL.md parser: `neural_kernel::fs::read_vfs()` — 0 refs ATA_DRIVER em jarbas |
| 106-4 | ✅ | Trinity MoE router: roteia para Hermes agents |
| 106-5 | ✅ | RustPython no_std (embed #![no_std], bridge abi_x86_interrupt) |
| 106-6 | ✅ | MicroPython/WASM (sandbox isolado) |
| 106-7 | ✅ | Page faults: allocator → events → agents |
| 106-8 | ✅ | AIOS API (aios_net, aios_fs via RAG) |
| 106-9 | ✅ | Escalonamento Evolutivo (Python→WASM via SkillOpt) |
| 106-10 | ✅ | SkillOpt: Tradução Python→Rust no_std |

**Status v2.0:** ✅ Sprint 106 concluída (10/10). ✅ Sprint 107 Voice FECHADA (PASS parcial forte+).  
**Pista ativa:** gate `v2.0.0` review + residual soft-float/VITS. Sprint Sound ✅ (`SESSION_122.md`). ADR-0042 **N1–N5 + wire N2.5–N5.7 ✅** no marco v1.8.0.

---

**Índice atualizado para v1.8.5 TEST:** release não estável; política viva `SESSION_107+`; históricos em `docs/archive/sessions/`.

