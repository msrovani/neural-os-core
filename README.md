# Neural OS Hermes v2.0.0 — AI-native Bare-metal Operating System 🏆

**The first AI-native operating system in the world. Bare-metal Rust. No Linux. No POSIX. No legacy. 0 errors.**

```
╔══════════════════════════════════╗
║  Neural OS Hermes v2.0.0        ║
║  "K²CHJ Core — Cognição"        ║
╚══════════════════════════════════╝

  ✦ v2.0.0 — 0 erros, ~26.000 LOC, 247+ agentes
  ✦ K²CHJ Workspace: k_nano → k_ai → cortex → hermes → jarbas
  ✦ Sprint 106 concluída — ecossistema de anéis lógicos isolados
  ✦ MicroPython/WASM sandbox + SkillOpt (Python→Rust no_std)
  ✦ SOUL.md via VFS — jarbas isolado de ring0 (sem ATA_DRIVER direto)
  ✦ ATA PIO bug CORRIGIDO — disco finalmente lê!
  ✦ HW Expert v3 (61.453 VID/DID, 1M params)
  ✦ SelfHealing Firmware Pipeline (I3/I4)
  ✦ WiFi Intel AX200/AX210 com ucode loading real
  ✦ 3 camadas visuais: Orb + Hermes CLI + Window Manager
  ✦ HDA capture + playback (microfone + auto-falante)
  ✦ BrowserAgent real: HTTP GET via smoltcp TCP
  ✦ GPU compute: Intel ring, NVIDIA PFIFO, AMD PM4, VirtIO-GPU
  ✦ WASM Runtime + BitNet IDE (F4) + skills a quente via LLM
  ✦ JARVIS: sleep cycle, ego layer, emoção, memória episódica

  → Port do JARVIS .NET MAUI para bare-metal Rust
  → github.com/msrovani/jarvis (app original)
```

## Status Atual

| Métrica | Valor |
|---------|-------|
| Versão | **v2.0.0** (Sprint 106 concluída) |
| Sprints completos | **106** (1-106) |
| Arquivos Rust | ~180+ (5 crates K²CHJ + monólito) |
| LOC total | ~26.000 |
| Agentes | 247+ |
| IDEIAS no Banco | **416+** |
| Erros de compilação | **0** (warnings: dead code policy) |
| GPU suportadas | NVIDIA, AMD, Intel (30+ modelos) |
| Kernel boot | QEMU (UEFI/TCG), VirtualBox, HW real (Intel 6xx) |

## Arquitetura K²CHJ (v2.0)

| Crate | Ring | Função |
|-------|------|--------|
| `k_nano` | 0 | HAL, drivers, PCI, memory, interrupts |
| `k_ai` | 1 | Sondagem, SelfHeal, Trust |
| `cortex` | 2 | LLM, BitNet, BPE, Trinity MoE |
| `hermes` | 2 | Orquestração: WASM, MicroPython, rede, skills |
| `jarbas` | 2 | HCI: display, áudio, persona JARVIS |
| `neural-kernel` | — | Bin de integração (monólito → migração gradual) |

**Princípio:** ring 2 não acessa ring 0 diretamente — filesystem via `neural_kernel::fs::read_vfs()`.

## O que torna único

### JARVIS Desktop com IDE e Ícones WASM
DisplayAgent renderiza o Compositor diretamente — sem X11, sem Wayland:
- **F1**: Hermes Chat (comandos, histórico)
- **F2**: Settings (tema, voz, memória, avatar)
- **F3**: Power (shutdown, reboot, hibernate)
- **F4**: **BitNet IDE** — digite `[GEN]` para criar WASM skills → viram ícones no desktop
- Avatar JARVIS pulsante no canto inferior direito

### WASM + MicroPython Sandbox (Sprint 106)
- `wasm_rt.rs`: WASM Skill Runtime com MemoryPool de 256 KB por skill
- `micropython_wasm.rs`: MicroPython compilado para `.wasm`, bridge WASI→Skill (20+ mapeamentos)
- `skill_opt.rs`: Python efêmero → WASM persistente → tradução Rust no_std via Cortex LLM
- `HybridRegistry`: agentes kernel + WASM + MCP externo

### GPU Compute Bare-metal
- BAR0/BAR1 mapping UC para NVIDIA/AMD/Intel
- SPSC job ring com doorbell por vendor
- VRAM buddy allocator power-of-2 (4KB-4GB)
- Secure Boot GPU: ACR (NVIDIA), PSP (AMD), GuC (Intel)

### JARVIS Cognitive Engine
- SOUL.md Personality + Fluid Persona (3 modos: Coach/Tutor/Tool)
- Emotion Analysis (7 emoções + sarcasmo)
- Session Compression + Notification Gate + Dreaming/Consolidation
- Ego Layer (confidence tracking) + Auto-Skill Generation
- Memory Systems: KG bitemporal, BGE embedding, Ebbinghaus decay

### Boot Agent-First (8 fases)
O LLM carrega ANTES do PCI scan. Cortex acompanha o hardware sendo descoberto.

---

## Roadmap Resumido

| Sprint | Bloco | Status |
|--------|-------|--------|
| 100 | Code Freeze v1.0.0 | ✅ |
| 101-102 | TTS/STT, GPU Compute, HW Expert v3 | ✅ |
| 103-105 | K²CHJ Migration + Ponytail Audit | ✅ |
| **106** | **Ecossistema de Anéis Lógicos (10/10)** | ✅ |
| **107** | Voice I/O Pipeline (TTS→STT→LLM→TTS) | ⏳ |
| **108** | Self-Evolving Agents | ⏳ |

**Documentação:** `ROADMAP.md` · `TODO.md` · `AGENTS.md` · `docs/memory/STATE.md`

---

```
J.A.R.V.I.S. — Just A Rather Very Intelligent System
"Thoughtful. Precise. Alive."
```
