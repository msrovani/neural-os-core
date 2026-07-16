# SESSION INDEX — neural-os-core v2.0

**Propósito:** Catálogo de sessões. Sessions de Sprints 1-68 foram arquivadas (conhecimento consolidado no código e neste índice). Sessions de Sprint 79+ mantidas individualmente.

---

## Sessões Mantidas (Sprint 79+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|--------|--------|-------|--------|----------------------|
| 079 | 79 | 23 | LLM Infrastructure | BitNet-b1.58 850M, BPE, Trinity MoE, QEMU loader, Xuvisco fix |
| 080 | 80 | 24 | AVX2 Debug + WHPX | WHPX emula VEX como VM exits. has_avx2() detecta hypervisor via CPUID. KV Cache 200× speedup |
| 081 | 81-83 | 21a/b/e | SMP + GPU Research | SPSC ring, IPI, PerCpu, Work-Stealing, GPU architecture |
| 082 | 97 | — | RustCoder Expert | Expert Rust (1.6M params) treinado com 41.2K amostras |
| 083 | 86-87 | 30-31 | JARVIS Persona+Security | SOUL.md, I1-I4, AUDIT_TRAIL, AHCI |
| 089 | 88-89 | 32-33 | JARVIS Emotion+Cache+SleepCycle | ADE, Pipeline 16 stages, KG bitemporal, BGE |
| 093 | 93+ | — | SDIO Pipeline | 45 packs, 95.812 entradas, loss 0.38 |
| 094 | 100-108 | — | v1.0 v2.0 Migration | Code Freeze, v2.0 cognição, K²CHJ Workspace Migration |
| 095 | 106 | — | Sprint 106: Ecossistema de Anéis Lógicos | k_ia→k_ai, jarvis→jarbas, VFS, MicroPython/WASM, SkillOpt |
| 106.3 | 106-3 | — | SOUL.md parser fix | jarbas usa `neural_kernel::fs::read_vfs()`, 0 refs ATA_DRIVER |
| 106.5 | 106-5 | — | RustPython viabilidade | Não no_std nativo — rota WASM (106-6) é principal |
| 107 | 107 | — | Boot A/B + Cap P0–P9 + ADR-0042 | Runtime QEMU OK; cadeia k-nano→…→jarbas; próximo N1 legível |
| 108 | 107 | — | N1 ✅ + BitNet 2B LOADED → v1.7.0 | Soft-float/cargo nk; 2B ~590MB L=30 FWD; TTS empty generate; e2e clima PARCIAL |
| 109 | 107 | — | ADR-0045 Sound Voice Stack | Truth=`neural-kernel/audio`; jarbas espelho; sherpa/Vosk/Kokoro/Wyoming/Rustpotter ❌; v1.7.1 docs |
| 110 | 107 | — | Sprint 107 loops 1–5 clima e2e → v1.7.2 | GEN 'O tempo esta'; Piper neural-lite; WakeWord registrado; STT ctc=''; HWEXPERT FAIL |
| 111 | Sound / ADR-42 | — | Handoff voz 107→Sprint Sound; pista limpa ADR-0042 | Docs v1.7.3; 107 Voice ✅ FECHADA; leftovers Sound; N2 próximo |
| 112 | ADR-42 N2 | — | N2 SelfHeal VID+Trust CLOSED | v1.7.4; QEMU `[N2-SELFHEAL]`+`[TRUST]`; N2.5 allocator; pista N3→N5 |
| 113 | ADR-42 N3 | — | N3 cortex LOADED + Trinity CLOSED | v1.7.5; `[N3-CORTEX] criteria=MET`; N3.5 allocator |
| 114 | ADR-42 N4 | — | N4 Hermes orchestrator CLOSED | v1.7.6; `[N4-HERMES] criteria=MET`; N4.6 allocator |
| 115 | ADR-42 N5 | — | N5 jarbas ego/UI CLOSED | v1.7.7; `[N5-JARBAS] criteria=MET`; N5.7 allocator; N1–N5 ✅ |
| 117 | ADR-42 N3.5 | — | cortex crate wired no bin | v1.7.9; 9 espelhos removidos; residuals cortex/bpe/global_arena/cortex_mmap |
| 118 | ADR-42 N4.6 | — | hermes crate wired no bin | v1.7.10; 37 espelhos removidos; residuals agents/net*/fs/aios_api |

## Sessões Anteriores (Sprints 1-68 — Arquivadas)

Conhecimento consolidado no código-fonte e no `SESSION_INDEX.md` abaixo. Arquivos individuais removidos.

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

---

## MAPA DE MEMÓRIA (MemPalace + Docs)

| Domínio | Onde encontrar | Propósito |
|---------|---------------|-----------|
| Estado atual | `docs/memory/STATE.md` | Versão, sprint atual, arquitetura, pendências |
| Ideias | `docs/memory/IDEA_BANK.md` | 354+ ideias catalogadas com status e sprint |
| Decisões | `docs/architecture/*.md` | 38+ ADRs (ADR-0001 a ADR-0037+) |
| Plano | `docs/sprint-plan-84-95.md` | 9 sprints (84-95) com items do IDEA_BANK |
| Checklist | `TODO.md` | Checklist mestre com sub-itens, goals, dificuldades |
| Sessões (aprendizado) | `docs/memory/SESSION_*.md` | 42+ sessões com descobertas e correções |
| Este índice | `docs/memory/SESSION_INDEX.md` | Catálogo de sessões + lições críticas |
| Sprints detalhados | `docs/SPRINT-106.md` | Sprint 106-1 a 106-10 com ações e resultados |
| Roadmap completo | `ROADMAP.md` | v1.0 → v2.0 com status de cada sprint |
| CHANGELOG | `CHANGELOG.md` | Histórico de versões |
| Código fonte | `crates/neural-kernel/src/` | Kernel bare-metal (135+ arquivos Rust) |
| Workspace crates | `crates/k_nano/`, `crates/k_ai/`, `crates/cortex/`, `crates/hermes/`, `crates/jarbas/` | Anéis lógicos K²CHJ (v2.0) |
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
**Pista ativa:** ADR-0042 **N1–N5 ✅ CLOSED** (v1.7.7). Sprint Sound = voz leftovers; wire crates N2.5–N5.7 deferred. Ver SESSION_115.

