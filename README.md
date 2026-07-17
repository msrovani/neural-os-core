# Neural OS Hermes — AI-native Bare-metal Operating System

**Versão:** **v1.8.5** (teste / não estável) · codename **K²CHJ Core — Cognição**
Bare-metal Rust (`no_std`, `no_main`). Sem Linux. Sem POSIX. **0 erros** de compilação.

```
╔══════════════════════════════════╗
║  Neural OS Hermes v1.8.5 TEST   ║
║  "K²CHJ Core — Cognição"        ║
╚══════════════════════════════════╝

  ✦ ~26.000 LOC · 180+ arquivos Rust · 247+ agentes
  ✦ Cadeia K²CHJ wired: k_nano → k_ai → cortex → hermes → jarbas
  ✦ ADR-0042 N1–N5 ✅ + wire crates N2.5→N5.7 ✅ (v1.8.0)
  ✦ Self-Evolve · NeuralFS · AirLLM · LatentBus/GPU/HMI MVPs
  ✦ Sprint 107 Voice ✅ (PASS parcial forte+)
  ✦ MicroPython/WASM sandbox + SkillOpt (Python→Rust no_std)
  ✦ BitNet 2B LOADED · HW Expert v3 · SelfHeal I3/I4
  ✦ HDA capture + playback · Piper TTS · STT CTC
  ✦ 3 camadas visuais · GPU multi-vendor · WiFi iwlwifi
  ✦ Skills a quente via LLM (nada hardcoded no enum Intent)

  → Port do JARVIS .NET MAUI para bare-metal Rust
  → github.com/msrovani/jarvis (app original)
```

## Status Atual

| Métrica | Valor |
|---------|-------|
| **Versão release** | **v1.8.5 teste / não estável** (2026-07-16) |
| **Gate v2.0.0** | **Fechado** — review, `por_fazer` e OK do maintainer pendentes |
| Sprints completos | 106 + 107 fechada |
| Arquivos Rust | ~180+ (5 crates K²CHJ + bin `neural-kernel`) |
| LOC total | ~26.000 |
| Agentes | 247+ |
| IDEIAS (IDEA_BANK) | 440+ |
| Compilação | `cargo nk` → **0 erros** |
| Boot dev | QEMU UEFI/WHPX, VirtualBox |
| Boot HW | `target/usb_hw.img` (unificado, Rufus DD) |

**Fonte de verdade:** `docs/memory/STATE.md`

## Arquitetura K²CHJ

| Crate | Anel | Função |
|-------|------|--------|
| `k_nano` | 0 | HAL, drivers, PCI, memory, interrupts |
| `k_ai` | 1 | SelfHeal, Trust, inventário HW |
| `cortex` | 2 | LLM BitNet, Trinity MoE, tensores |
| `hermes` | 2 | Orquestração: WASM, rede, skills, intent |
| `jarbas` | 2 | HCI: display, GPU FB, persona JARVIS |
| `neural-kernel` | — | **Bin de boot** — integra crates + residuals bin-only |

**Cadeia:** `k-nano → k-ai → cortex → hermes → jarbas` (sem ciclos).

**Residuals no bin** (integração, não duplicação): `cortex.rs`, `bpe.rs`, `audio/*` (ADR-0045 truth), `agents.rs`, `net*`, `fs/*`, `jarbas_fb.rs`.

**Princípio:** ring 2 não acessa ring 0 diretamente — VFS via `neural_kernel::fs::read_vfs()`.

## Destaques

### Agent/Skill-First
Tudo é Agente ou Skill. Drivers, daemons e serviços são agentes com manifesto, schedule e trust tokens.

### JARVIS Desktop
DisplayAgent + compositor no framebuffer UEFI (sem X11/Wayland): Orb FFT, Hermes CLI, dock, BitNet IDE (F4).

### WASM + MicroPython (Sprint 106)
`wasm_rt`, `micropython_wasm`, `skill_opt` — skills efêmeras → WASM persistente → Rust `no_std` via Cortex.

### Voz (Sprint 107 + Sound)
HDA + Piper + STT CTC + WakeWord registrado. Clima e2e: `'O tempo esta'` + TTS + FB paint. **Backlog voz → Sprint Sound** (STT real, Mic→Wake runtime, Piper VITS pleno).

### Boot Agent-First (8 fases)
SafeHarbor → MemoryCore → … → AgentFleet → Runtime. LLM/Cortex antes do fleet completo.

---

## Roadmap Resumido

| Sprint / Track | Status |
|----------------|--------|
| 100 Code Freeze v1.0.0 | ✅ |
| 101–105 Cognição + K²CHJ migration | ✅ |
| 106 Ecossistema de Anéis Lógicos | ✅ |
| 107 Voice I/O | ✅ FECHADA (parcial forte+) |
| **ADR-0042** Adequação N1–N5 + wire | ✅ **v1.8.0** |
| **Sound** pipeline voz | ✅ parcial honesto |
| 108 Self-Evolving Agents | ✅ |
| ADR-0040/0046/0047 MVPs | ✅ parcial / residuals abertos |

**Documentação:** `ROADMAP.md` · `TODO.md` · `AGENTS.md` · `HOWTO.md` · `docs/memory/STATE.md`

---

```
J.A.R.V.I.S. — Just A Rather Very Intelligent System
"Thoughtful. Precise. Alive."
```
