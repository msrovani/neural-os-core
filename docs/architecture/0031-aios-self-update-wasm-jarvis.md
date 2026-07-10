# ADR-0031: AIOS Evolution — Self-Update, WASM, J.A.R.V.I.S. & Hybrid Agent Architecture

**Status:** Implementado (com desvios do plano original)  
**Date:** 2026-07-03 (revisao: 2026-07-10)  
**Author:** IDA IA + Dev  
**Ref:** IDEA_BANK #306–#310  
**Nota:** O plano original recomendava `wasmi` como runtime WASM. A implementacao real usa um bytecode VM custom (`wasm_exec.rs`) com `Op` enum proprio, mais leve e sem dependencia externa. WASI-to-skill mappings, MemoryPool, fuel metering, e capability tokens foram mantidos conforme o plano.  

## Executive Summary

This document analyzes four interlocking architectural decisions for neural-os-core's evolution from a
bare-metal kernel into a complete AIOS: self-updating OS mechanics, WASM as a native skill format,
a J.A.R.V.I.S.-style conversational AI layer, and the kernel-vs-WASM agent boundary.

**Key finding:** All four systems converge on a single hybrid architecture where critical-path agents
(Disk, Net, Display, Cortex) remain kernel-level Rust while user-extensible agents, sandboxed skills,
and the J.A.R.V.I.S. persona layer run as WASM modules with capability-token gating. The update
mechanism uses dual-kernel-slot A/B partitioning on the existing FAT32 boot volume with Ed25519
signature verification.

**Recommended implementation order:** Self-Update → WASM Runtime → J.A.R.V.I.S. → Hybrid Agent Boundary.
This respects dependency chains: WASM needs update for skill distribution; J.A.R.V.I.S. needs WASM for
safe persona plugins; hybrid boundary formalizes what already exists.

---

## Part 1: Self-Updating OS — Update/Upgrade Agent

### 1.1 Industry Reference: ChromeOS A/B Updates

**How it works:**
- Two complete root filesystem partitions (ROOT-A, ROOT-B), identical size (~2-4 GB each).
- Kernel lives in a dedicated KERN-A/KERN-B partition pair (16 MB each, GPT type `FE3A2A85`).
- `cgpt` (GPT manipulation tool) sets partition attributes: `priority` (0-15), `tries` (0=unlimited),
  `successful` flag.
- Bootloader reads GPT attributes, picks highest-priority bootable partition with `tries > 0`.
- On each boot attempt: decrement `tries`. If boot succeeds, userspace marks `successful=1` and
  resets `tries`. If `tries` hits 0, bootloader falls to the other slot.
- Default: `tries=6` for ChromeOS Flex (production), `tries=1` for developer builds.
- **Verification:** dm-verity — a Merkle hash tree computed over the entire rootfs at build time.
  The kernel verifies every block read against the hash tree. Any mismatch causes read failure, and
  the kernel panics (triggering a reboot → tries decrement → eventual fallback).
- **OTA:** Omaha protocol (Google's update server). The client polls every 45 min, downloads a
  delta update (Courdere-3 diff for ~50 MB typical), writes to the inactive slot, verifies the
  hash, and sets priority=TRIES_MAX on the new slot.

**Key data points:**
| Parameter | ChromeOS | Android A/B | CoreOS |
|---|---|---|---|
| Slots | 2 (KERN + ROOT) | 2 (boot + system) | 2 (usr + boot) |
| Partition size | 2-4 GB rootfs | 2-4 GB system | ~1 GB /usr |
| Tries before fallback | 6 (prod), 1 (dev) | 7 (reset every OTA) | N/A (manual rollback) |
| Verification | dm-verity (Merkle tree) | dm-verity + AVB (vbmeta) | Checksum on download |
| Update transport | Omaha (HTTP) | OTA (HTTP) | HTTP fetch + reboot |
| Atomicity | Write to inactive slot | Write to inactive slot | Write to inactive, reboot |

### 1.2 Android A/B (Treble-era differences)

Same concept but key differences:
- Uses `boot` (kernel+ramdisk) and `system` partitions (not separate kernel).
- AVB (Android Verified Boot): `vbmeta.img` contains hashes of `boot` and `system`, signed with
  a key fused in the device at factory. Chain of trust: Boot ROM → Bootloader → vbmeta → boot → system.
- Recovery partition coexists as a **fallback** for A/B systems — if both slots fail, recovery is
  a minimal OS that reflashes the device.
- `misc` partition stores bootloader commands (e.g., "wipe data", "update").

### 1.3 CoreOS / Container Linux

Different model — doesn't use A/B partitions. Instead:
- `/usr` is read-only and updated atomically: new `/usr` written while old one is still running,
  then `reboot` switches to new version.
- Rollback: `ostree` keeps previous deployment as a directory, bootloader entry points to it.
  Manual `ostree rollback` restores the previous tree.
- Ignition config: JSON-based first-boot provisioning. Runs once, writes config to disk, never
  runs again unless disk is wiped. Equivalent to cloud-init but designed for immutable OS.

### 1.4 neural-os-core Update Architecture (Recommended)

**Current state:**
- FAT32 boot partition with: `KERNEL~1` (bootloader entry → kernel binary), `BOOT-S~1`, `BOOT-S~2`
  (bootloader config slots).
- Ed25519 kernel signing (IDEA #176): `ed25519-compact` crate, `KERNEL~1.SIG` with Ed25519 signature.
- TPM 2.0 measured boot (v0.74.1): PCR[8] extend with kernel SHA-256 hash.
- BootSelfHealAgent: reads boot log, detects crash patterns.
- smoltcp 0.13: TCP/IP, DHCP (pending fix B-01).

**Proposed design — Dual Kernel Slot A/B:**

```
FAT32 Boot Partition (type 0x1C, Hidden FAT32 LBA):
  KERNEL~1          ← active kernel (bootloader loads this)
  KERNEL~1.SIG      ← Ed25519 signature of KERNEL~1
  KERNEL~2          ← inactive slot (update target)
  KERNEL~2.SIG      ← Ed25519 signature of KERNEL~2
  BOOTCFG .JSON     ← { "active": 1, "tries": 3, "last_good": 1, "tpm_extended": true }
  BOOTLOG .LOG      ← boot log for SelfHeal agent
  UPDATE  .MANIFEST ← { "channel":"stable", "version":"0.77.0", "hash":"SHA256..." }
```

**Update flow (UpdateAgent, ~500 LOC):**

```
1. Poll update server (HTTP GET /updates/stable/manifest.json) every 3600s
   → Response: { version, url, sha256, min_firmware_version, size }
2. Compare version with current (UPDATE.MANIFEST or hermes version string)
3. If newer:
   a. HTTP GET kernel binary → write to KERNEL~2 (temporary, not yet bootable)
      - Chunked download (4 KB chunks) with SHA-256 hash verification per chunk
      - Resume support: Range: bytes=X- header if download interrupted
      - Progress: serial_println!("[UPDATE] {pct}% {downloaded}/{total}")
   b. Verify full SHA-256 of KERNEL~2
   c. HTTP GET KERNEL~2.SIG → verify Ed25519 signature against embedded public key
   d. Write BOOTCFG.JSON: { "active": 2, "tries": 3, "last_good": 1 }
   e. Shutdown(Normal | UpdateReboot)
4. Bootloader reads BOOTCFG.JSON → loads KERNEL~2
5. If boot succeeds:
   a. BootSelfHealAgent marks BOOTCFG.JSON: { "active": 2, "tries": 3, "last_good": 2 }
   b. Copy KERNEL~2 → KERNEL~1 (synchronize slots for next update)
6. If boot fails (3 tries exhausted):
   a. Bootloader falls back to last_good slot (KERNEL~1)
   b. BootSelfHealAgent reads BOOTLOG.LOG → detects update failure
   c. Publishes UPDATE_FAILED event → Hermes notifies user
   d. Deletes KERNEL~2 (bad update)
```

**Key design decisions:**
- **No dm-verity:** We don't have a block-layer I/O stack. Instead: Ed25519 signature + SHA-256
  hash verified at download time and at boot time (TPM PCR extend). Kernel is ~1 MB, not 4 GB
  rootfs — full hash is practical.
- **No separate KERN partition:** Reuse the existing FAT32 boot partition. KERNEL~1 and KERNEL~2
  coexist. FAT32 can hold both (~2 MB total, negligible).
- **No recovery partition:** The bootloader (limine/custom) is tiny and immutable. If both kernel
  slots fail, the system is bricked — but the probability of a bad Ed25519-signed kernel in both
  slots is effectively zero.
- **Channels:** stable (sprint releases), nightly (HEAD commits), security (hotfix only). Channel
  selected via BOOTCFG.JSON channel field.

**Update channels:**
| Channel | Poll interval | Typical size | Risk |
|---|---|---|---|
| stable | 3600s | ~1 MB (release) | Minimal — tested in QEMU + VirtualBox |
| nightly | 600s | ~1 MB (HEAD) | Medium — may have regressions |
| security | 60s | ~200 KB (hotfix) | Minimal — only critical fixes |
| none | never | — | Manual update only |

**Viability: 9/10**
- All dependencies exist: smoltcp TCP (needs DHCP fix), Ed25519 (implemented), FAT32 (implemented),
  TPM (implemented), ShutdownAgent (v0.74.0).
- BootSelfHealAgent already has crash detection logic.
- ~500 LOC kernel + ~100 LOC bootloader glue (BOOTCFG.JSON parser).
- **Blocks:** B-01 (DHCP/RX fix) — without network, no OTA. Static IP fallback works for testing.
- **Complexity:** Medium. The atomicity of writing KERNEL~2 is the hard part — power loss mid-write
  must not corrupt the active slot (KERNEL~1). FAT32 is not journaled, but we only write to the
  inactive slot, then atomically switch via BOOTCFG.JSON (single 512-byte sector write).

---

## Part 2: WASM as Native Skill Format

### 2.1 Industry Reference: WASM Runtimes

| Runtime | Language | Size | Speed | no_std? | License | Notes |
|---|---|---|---|---|---|---|
| **wasmi** | Rust | ~15K LOC | Interpreter (5-20x slower than native) | **YES** | MIT/Apache-2.0 | Pure Rust, no unsafe code in core. Used by Substrate/Polkadot (parity). Best no_std option. |
| wasmtime | Rust | ~100K LOC | JIT (0.8-1.5x native) | No (needs `std`) | Apache-2.0 | Bytecode Alliance. Cranelift JIT backend. Heavy dependencies. |
| Wasmer | Rust | ~90K LOC | JIT + interpreter | No (needs `std`) | MIT | Multiple backends (LLVM, Cranelift, Singlepass). Heavy. |
| WAMR | C | ~50K LOC | Interpreter + AOT | **YES** | Apache-2.0 | Bytecode Alliance. "WebAssembly Micro Runtime". Embedded-friendly. C codebase (FFI needed). |
| wasm3 | C | ~12K LOC | Interpreter (slower) | **YES** | MIT | Ultra-lightweight. C codebase. FFI needed. |
| wazero | Go | ~40K LOC | Interpreter | No (Go runtime) | Apache-2.0 | Pure Go. Not relevant for Rust kernel. |

**Recommendation: `wasmi` v0.42+**
- no_std with `alloc` only (no `std`). Compiles for `x86_64-unknown-none`.
- ~15K LOC in the crate but the kernel only needs the core interpreter (can be vendored/trimmed).
- Active maintenance (paritytech, 1.7k stars, 156 contributors).
- Supports WASM MVP 1.0 + bulk-memory + reference-types + SIMD proposals.
- Fuel metering built-in: `wasmi::FuelCosts` per instruction, configurable fuel limit per call.
- Memory limits: `wasmi::Config::set_mutable_global_limit(16)` → 64 KB pages (wasm page = 64 KB).

**Fallback option: WAMR (C FFI)**
- If wasmi proves too large or slow, WAMR (WebAssembly Micro Runtime) is the embedded-industry
  standard. Used in Android's Microdroid, IoT, OpenHarmony. C code via `extern "C"` bindings.
  More complex to integrate but faster (AOT compilation to native code via `wamrc`).

### 2.2 WASI Syscalls → Agent Skill Mapping

WASI Preview 1 defines ~20 syscalls. Each maps to an AIOS agent skill:

| WASI Syscall | Arguments | Maps To | Agent | Skill |
|---|---|---|---|---|
| `fd_read(fd, iovs)` | File descriptor, buffers | `FileAgent::read_skill(path, offset, len)` de/filesystem_agent | FileAgent | read_skill |
| `fd_write(fd, iovs)` | File descriptor, buffers | `FileAgent::write_skill(path, data)` → $fd → FAT32 | FileAgent | write_skill |
| `fd_close(fd)` | File descriptor | Decrement refcount, flush | FileAgent | close_skill |
| `fd_seek(fd, offset, whence)` | Seek position | Update fd_table offset entry | FileAgent | seek_skill |
| `fd_prestat_get(fd)` | Pre-open dir info | Return VFS mount point for fd | VfsAgent | prestat_skill |
| `fd_prestat_dir_name(fd, path)` | Pre-open dir path | Return VFS canonical path | VfsAgent | dirname_skill |
| `path_open(fd, dirflags, path, oflags, fs_rights)` | Open file at path | `FileAgent::open_skill(path, mode)` → alloc new fd | FileAgent | open_skill |
| `environ_get` | Environment vars | `SysAgent::environ_skill()` → static env block | SysAgent | environ_skill |
| `environ_sizes_get` | Count + buffer size | `SysAgent::environ_sizes_skill()` | SysAgent | envsize_skill |
| `args_get` | Command-line args | `SysAgent::args_skill()` → spawn arguments | SysAgent | args_skill |
| `args_sizes_get` | Count + buffer size | `SysAgent::args_sizes_skill()` | SysAgent | argsize_skill |
| `clock_time_get(id, precision)` | Monotonic/real-time | `TimeAgent::clock_skill(id)` → LAPIC timer / RTC | TimeAgent | clock_skill |
| `clock_res_get(id)` | Clock resolution | `TimeAgent::resolution_skill(id)` | TimeAgent | res_skill |
| `random_get(buf, len)` | Random bytes | `SysAgent::random_skill(len)` → LAPIC TSC + rdrand | SysAgent | random_skill |
| `poll_oneoff(in, out, nsub)` | Poll for I/O events | `EventBusAgent::poll_skill(subscriptions)` → events | EventBusAgent | poll_skill |
| `proc_exit(code)` | Exit process | Trap to supervisor → `SkillScheduler::terminate(code)` | SkillScheduler | exit_skill |
| `proc_raise(sig)` | Send signal | `SysAgent::signal_skill(pid, sig)` → interrupt agent | SysAgent | signal_skill |
| `sched_yield` | Yield execution | Return to scheduler → next agent poll | SkillScheduler | yield_skill |
| `sock_accept` | Accept TCP connection | `NetAgent::accept_skill(port)` → smoltcp accept | NetAgent | accept_skill |
| `sock_recv` | Receive data on socket | `NetAgent::recv_skill(fd, buf)` → smoltcp recv | NetAgent | recv_skill |
| `sock_send` | Send data on socket | `NetAgent::send_skill(fd, data)` → smoltcp send | NetAgent | send_skill |
| `sock_shutdown` | Shutdown socket | `NetAgent::shutdown_skill(fd)` → smoltcp close | NetAgent | shutdown_skill |

**Design principle:** WASI syscalls are *host functions* implemented by the WASM runtime. The host
(in our case, the kernel) translates each WASI call into an agent skill invocation. The WASM module
never directly accesses hardware — it goes through the WASI→agent→skill chain.

### 2.3 WASM as Agent Sandbox

Each `.wasm` file = one agent in the AgentRegistry. Key constraints:

```
WasmAgent:
  wasm_bytes: &[u8]               ← loaded from FAT32 /agents/*.wasm
  instance: wasmi::Instance       ← instantiated with fuel limit
  memory: wasmi::Memory           ← 256 KB linear memory (4 pages)
  capability_tokens: Vec<CapabilityToken>  ← tokens granted at spawn
  fuel_per_tick: u64 = 100_000    ← instructions per scheduler tick
  agent_state: AgentState         ← same lifecycle as kernel agents
```

**Capability token gating:**
- The agent's WASM code only gets tokens != 1 (CapabilityToken::None).
- The `SkillRegistry::authorize(token, skill_name)` check is performed in the WASM host functions.
- If the WASM calls a skill it lacks tokens for, the host returns `ERRNO_NOTCAPABLE` (WASI errno 76).
- This is the same mechanism as kernel agents (TrustCache) — WASM agents are not special-cased.

**Memory limits:**
- Each WASM instance: 4 pages = 256 KB linear memory (configurable per agent manifest).
- Stack: 1 page (64 KB). Heap: 3 pages (192 KB) for WASM allocations.
- Host functions communicate via shared memory: WASM writes request to offset 0x0000, host reads
  and writes response to offset 0x8000 (64 KB buffer each).
- This avoids serialization overhead — direct memory copy.

**CPU limits (fuel metering):**
- `wasmi::Store::set_fuel(total_fuel)` → configurable per tick.
- Default: 100,000 fuel units per `agent.poll()` call (~5 µs at interpreter speed, ~500k instr).
- If fuel exhausted: host pauses the agent, saves state, resumes next tick.
- If agent exceeds fuel 3 consecutive ticks: host kills the agent (publishes AGENT_FUEL_EXHAUSTED).

### 2.4 WASM Agent Lifecycle

```
1. Load: read /agents/myagent.wasm from FAT32 → parse WasmAgentManifest
2. Compile: wasmi::Module::new(&engine, wasm_bytes)
3. Instantiate: wasmi::Instance::new(&mut store, &module, &imports)
   → Host functions registered: fd_read, fd_write, clock_time_get, random_get, etc.
   → Memory allocated (4 pages), fuel set (100k)
4. Register: AgentRegistry → agent_id, capabilities
5. Start: call wasm export "_start" (or "main" for WASI)
6. Poll: every scheduler tick → call "poll" export with fuel budget
7. Yield: WASM calls sched_yield → returns to scheduler
8. Terminate: WASM calls proc_exit → host deallocates instance, publishes AGENT_EXITED
```

**Viability: 8/10**
- wasmi is no_std-capable and well-maintained. Integration ~800 LOC (see B-14 in TODO.md).
- WASI syscall mapping ~200 LOC (host function impls).
- Capability token integration: reuse existing TrustCache (~50 LOC).
- **Risk:** wasmi binary size (~150 KB compiled) may be large for the current kernel (~1 MB).
  Mitigation: LTO + no-default-features (`wasmi` without `std`, without `virtual_memory`).
- **Alternative:** WAMR C FFI if wasmi proves unsuitable. But Rust purity is preferred.

---

## Part 3: J.A.R.V.I.S. Layer — Conversational AI Above Hermes

### 3.1 Industry Reference: AI Companion Systems

| System | Architecture | Context Persistence | Voice | Personality | Proactive? |
|---|---|---|---|---|---|
| **Iron Man's J.A.R.V.I.S.** (concept) | Master orchestrator: reads sensors, controls devices, talks naturally | Full session + long-term memory | Natural speech synthesis | Witty, loyal, proactive | Yes — "Sir, incoming missile" |
| **Project Jarvis** (Google, 2025) | Browser agent: takes over Chrome via CDP, performs web tasks | Session only | Text-based | Professional, task-oriented | No — user-initiated |
| **OpenAI ChatGPT Desktop** | Mac/Windows app, voice mode, screen share, clipboard | Sessions + memory (opt-in) | Whisper+GPT-4o voice | Helpful, neutral | No — reactive only |
| **Apple Intelligence** (2025) | On-device LLM (3B) + Private Cloud Compute (server). System-wide tools. | On-device semantic index | Siri voice | Friendly, private-first | Yes — notifications, suggestions |
| **Copilot** (Microsoft) | OS-integrated: recalls screen, keyboard, documents. Recall AI snapshot. | Local semantic index (Recall) | Optional voice | Professional, integrated | Yes — "You were working on..." |
| **Gemini** (Google) | Multi-modal, integrated into Android/Pixel. Extensions ecosystem. | Apps/screen context | Gemini Live voice | Conversational | Yes — "Based on your calendar..." |

**Common patterns across all systems:**
1. **LLM as the brain** — all use a large model (GPT-4o, Gemini Ultra, Llama 3) at their core.
2. **Context persistence** — some form of memory across sessions. Apple uses on-device semantic index.
   Copilot uses Recall (screenshots + OCR). OpenAI has cross-chat memory.
3. **Multi-modal input** — voice, text, image, screen. Voice is the natural I/O for J.A.R.V.I.S.
4. **OS-level integration** — system tools (calendar, files, settings, notifications). The assistant
   *is* the OS interface, not just an app.
5. **Proactive push** — the system initiates conversation. "Your disk is 90% full." "You have a
   meeting in 10 minutes." This is the differentiator between a chatbot and a true J.A.R.V.I.S.

### 3.2 neural-os-core AI Stack (Current)

```
Physical Hardware
  ├── Kernel (agents + skills + hardware drivers)
  ├── CortexAgent (BitNet 272K params, ternary, 4-layer Transformer)
  │     ├── generate_text(prompt) → autoregressive char-level text
  │     └── Medusa speculative decode (1→5 tokens/step)
  ├── HermesAgent (intent routing, ReAct 7 phases, Council, multi-agent orchestration)
  │     ├── OBSERVE → THINK → PLAN → BUILD → EXECUTE → VERIFY → LEARN
  │     ├── SDD (Structured Decision Document)
  │     ├── Identity Layer (HERMES_NAME, HERMES_MOTTO)
  │     ├── Context Fencing + Bitter Pill Engineering
  │     └── Skill dispatch: /hw, /diag, /sdd, /council, /chat
  └── DisplayAgent (framebuffer BGRA32, NeuralConsole)
```

### 3.3 J.A.R.V.I.S. Layer Design

The J.A.R.V.I.S. layer is a persona that sits *above* Hermes, not replacing it. Hermes is the
technical orchestrator (routes intents to skills, manages agents). J.A.R.V.I.S. is the *human-facing
personality* — voice, natural conversation, proactive notifications, long-term context.

**Architecture:**

```
USER (voice, keyboard, gestures)
  ↓ ↑
┌─────────────────────────────────────────────────┐
│              J.A.R.V.I.S. Layer                   │
│  ┌───────────┐ ┌──────────┐ ┌───────────────┐   │
│  │ Voice I/O  │ │Personality│ │ Proactive Bus  │   │
│  │ (TTS/STT)  │ │ (SOUL.md) │ │ (notifications)│   │
│  └───────────┘ └──────────┘ └───────────────┘   │
│  ┌──────────────────────────────────────────┐   │
│  │   Context Window Manager                   │   │
│  │   (MemoryTree + KnowledgeGraph +           │   │
│  │    Atkinson-Shiffrin 3-tier memory)        │   │
│  └──────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────┐   │
│  │   Conversation Engine                      │   │
│  │   (greeting, farewell, mood detection,     │   │
│  │    topic tracking, pronoun resolution)     │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
  ↓ ↑
┌─────────────────────────────────────────────────┐
│              HermesAgent                          │
│  ┌──────────┐ ┌─────────┐ ┌──────────────────┐  │
│  │ Intent   │ │ ReAct   │ │ Skill Dispatch    │  │
│  │ Router   │ │ Loop    │ │ + Council         │  │
│  └──────────┘ └─────────┘ └──────────────────┘  │
└─────────────────────────────────────────────────┘
  ↓ ↑
┌─────────────────────────────────────────────────┐
│              CortexAgent (BitNet LLM)             │
│  generate_text(), hw_identify(), classify_intent()│
└─────────────────────────────────────────────────┘
  ↓ ↑
┌─────────────────────────────────────────────────┐
│              Kernel: Agents + Skills + Drivers    │
│  Net, Disk, Display, USB, ATA, PCI, SMP, TPM     │
└─────────────────────────────────────────────────┘
```

### 3.4 Key Components

#### A. Personality Engine (`SOUL.md`)

Not hardcoded — a file on the FAT32 partition that defines the persona:

```markdown
# SOUL.md — J.A.R.V.I.S. Persona

name: "J.A.R.V.I.S."
codename: "Just A Rather Very Intelligent System"
voice_style: "british butler, warm, slightly witty, always respectful"
greeting: "Good morning, sir. All systems nominal. How may I assist?"
farewell: "Very good, sir. I shall be here if you need me."
mood_thresholds:
  formal: { user_energy: ">7", time: "morning" }
  casual: { user_energy: "<4", time: "night" }
  alert: { disk_health: "<10%", security_event: true }
```

The personality engine loads SOUL.md at boot, provides template strings for Cortex to use as
conversation context. Cortex already supports prompt-based generation.

#### B. Voice I/O (Post-MVP)

- **STT (Speech-to-Text):** Requires audio input (HDA, USB-Audio). Post-MVP. In the interim:
  keyboard input (already working). Text is the primary medium.
- **TTS (Text-to-Speech):** Requires audio output. Post-MVP. Interim: text display on framebuffer.
  Option: PC speaker beep patterns for alerts (150 LOC, usable now).

#### C. Context Window Manager

Already partially implemented:
- **MemoryTree** (event-bus crate): `add()`, `query()`, TTL eviction.
- **KnowledgeGraph** (event-bus crate): subject→predicate→object with temporal validity.
- **Atkinson-Shiffrin** 3-tier: Sensory (raw events, 0.5s TTL) → STM (last 7±2 items, 30s) → LTM
  (persistent to FAT32, Ebbinghaus decay).
- **Session continuity:** On reboot, J.A.R.V.I.S. reads the last conversation from BOOTLOG.LOG
  and MemoryTree, greets with context: "Welcome back, sir. We were discussing the network driver.
  Shall I continue debugging?"

#### D. Proactive Notification Bus

J.A.R.V.I.S. subscribes to agent events and decides when to push to the user:

```
Event Sources:
  DiskAgent     → "DISK_HEALTH: free=5%, SMART=WARNING"  → J.A.R.V.I.S.: "Sir, your disk is
                                                            critically low. Shall I archive
                                                            unused files?"
  CronAgent     → "CRON_JOB_DONE: backup completed"        → "Sir, the backup finished. 1,247
                                                            files archived."
  SecurityAgent → "SECURITY_ALERT: 3 failed auth attempts" → "Sir, we have a security incident.
                                                            Locking console."
  UpdateAgent   → "UPDATE_AVAILABLE: v0.78.0 (security)"   → "Sir, a security update is ready.
                                                            Install now?"
  SafetyAgent   → "SAFETY_INTERCEPT: skill blocked"        → "Sir, that action was blocked by
                                                            safety policy."
  BootSelfHealAgent → "CRASH_DETECTED: update v0.78.0"     → "Sir, the last update caused a crash.
                                                            I have rolled back to v0.77.5."
```

**Proactive gating rules (NotificationGate, ported from Lethe #200):**
1. Startup grace period: first 60 seconds, suppress all non-critical notifications.
2. Dedup: same event within 30s → suppress duplicate.
3. Priority queue: Critical (show immediately) > High (show within 30s) > Info (log only) > Debug (silent).
4. Interruptibility gate: if Cortex is mid-generation (expensive), defer until next idle tick.

### 3.5 Conversation Flow Example

```
[BOOT]
DisplayAgent: "╔════════════════════════╗"
              "║  Neural OS v0.77.0     ║"
              "║  Hermes Cognitive      ║"
              "╚════════════════════════╝"
J.A.R.V.I.S.: "Good morning, sir. All systems nominal. CPU: 2.4 GHz, Memory: 512 MB free,
              Disk: 82% healthy. How may I assist?"

User types:   "check the network"
J.A.R.V.I.S.: → Intent: NETWORK_DIAGNOSTIC (classified by Cortex)
              → Hermes: THINK→PLAN→EXECUTE
              → Calls: NetAgent::dhcp_status_skill()
                + NetAgent::dns_resolve_skill("google.com")
                + NetAgent::http_ping_skill("1.1.1.1")
J.A.R.V.I.S.: "Network status: RTL8139 at PCI 02:00.0. DHCP: 10.0.2.15/24 obtained.
              Gateway: 10.0.2.2. DNS: 1.1.1.1 reachable (12ms). All green, sir."

User:          "organize my photos by date"
J.A.R.V.I.S.: → Intent: FILESYSTEM_ORGANIZE (Cortex)
              → Hermes: THINK→PLAN (decompose task)
                BUILD→"1. scan /mnt/data/photos/
                       2. extract EXIF date or use file timestamp
                       3. create directories: /mnt/data/photos/YYYY/MM/
                       4. move files
                       5. report summary"
              → Hermes: EXECUTE → DiskIntelligenceAgent::scan_tree_skill()
                + DiskIntelligenceAgent::rename_batch_skill()
              → VERIFY: confirms all moved, reports count
J.A.R.V.I.S.: "Done, sir. 1,847 photos organized into 42 monthly folders.
              Duplicates found: 23 (moved to /photos/duplicates/)."

[CronAgent fires at 3:00 AM]
J.A.R.V.I.S.: (no display update — user may be sleeping)
              Logs: "CRON: nightly backup completed. 42 MB written. System health: OK."

[Next morning boot]
J.A.R.V.I.S.: "Good morning, sir. Overnight: backup completed (42 MB). New security update
              available (v0.78.1). Install? [Y/n]"
```

### 3.6 Viability Assessment

| Component | Viability | LOC Estimate | Dependencies |
|---|---|---|---|
| SOUL.md personality engine | 9/10 | ~100 LOC | FAT32 reader (exists) |
| Context window persistence | 8/10 | ~200 LOC | MemoryTree + KG (exist) |
| Proactive notification bus | 8/10 | ~150 LOC | EventBus (exists) |
| Conversation engine (greeting, mood) | 7/10 | ~300 LOC | Cortex generate_text (exists but 272K params — limited) |
| Voice TTS (PC speaker) | 5/10 | ~150 LOC | PIT timer (exists) |
| Voice STT | 2/10 | — | Audio hardware + model (post-MVP) |
| Natural task decomposition | 6/10 | ~400 LOC | Hermes ReAct (exists) + Cortex |
| **Total (text-only)** | **8/10** | **~1,150 LOC** | Most dependencies exist |
| **Total (voice)** | **5/10** | **~2,500 LOC** | Audio hardware + model training |

**Key limitation:** CortexAgent's current model (272K params) is too small for rich conversation.
The J.A.R.V.I.S. personality will be template-driven (SOUL.md templates + keyword matching) rather
than fully generative until a larger model (1.5B+ GGUF, IDEA #278) is available.

**Mitigation:** J.A.R.V.I.S. can use Cortex for intent classification (already working) and
template-based responses for conversation. This gives a functional "butler" experience even with
the small model, with upgrade path to full generative conversation when the larger model lands.

---

## Part 4: Agent Architecture — Kernel vs WASM

### 4.1 Comparative Analysis

| Dimension | Kernel Agents (Rust, no_std) | WASM Agents (wasmi, no_std) |
|---|---|---|
| **Performance** | Native x86-64. Direct hardware access (I/O ports, MMIO, MSR). Zero IPC overhead between agents (shared memory). | Interpreter overhead: ~5-20x slower than native. No direct hardware — all I/O goes through WASI→agent→skill chain. |
| **Memory** | Stack: 8 KB (fixed at compile). Heap: allocated from kernel heap (shared). | Linear memory: 256 KB per instance (isolated). OOM kills agent only. |
| **Crash isolation** | Agent crash (page fault, GPF) → kernel panic or SelfHeal restart. Affects whole system. | Agent crash → trap to host → host kills instance → other agents continue. |
| **Hot-reload** | Requires reboot (or complex COW page table magic). Kernel binary is immutable. | Replace `.wasm` file → AgentRegistry reloads → agent restarts from clean state. No reboot. |
| **Development speed** | Slow: compile kernel → build image → QEMU boot → test. 30-60s cycle. | Fast: `cargo build --target wasm32-wasi` → 5s compile → drop .wasm into FAT32. |
| **Ecosystem** | Only devs who understand bare-metal x86-64 + no_std Rust. ~100 people worldwide. | Any Rust dev (1M+ worldwide) can write `no_std` Rust and target `wasm32-wasi`. |
| **Security** | Agents run in Ring 0 (kernel mode). Any agent can access any memory, any I/O port. Trust-based (Ed25519 tokens). | Agents run in WASM sandbox (Ring 3 via interpreter). Memory isolated. Host functions gate all I/O. |
| **Size overhead** | Kernel agent: ~200-500 LOC of Rust → ~2-4 KB compiled. | WASM runtime: ~150 KB (wasmi). Per-agent: ~10-50 KB .wasm. |
| **Debugging** | QEMU+GDB (cumbersome but powerful). Serial prints. | WASM has standard debugging (wasm-gc, DWARF). Can debug on host machine. |

### 4.2 Recommended Hybrid Architecture

**Principle: Critical path = kernel. User-extensible = WASM.**

| Tier | Agent Type | Runtime | Examples | Rationale |
|---|---|---|---|---|
| **Tier 0: Core** | System-critical | Kernel (Ring 0) | MemoryAgent, PlatformAgent, BootTrustAgent, BootSelfHealAgent, SystemAgent | These run before the WASM runtime is initialized. System won't boot without them. |
| **Tier 1: Hardware** | Device drivers | Kernel (Ring 0) | NetDriverAgent, UsbDriverAgent, GpuDriverAgent, AtaAgent, DiskIntelligenceAgent | Need direct I/O port/MMIO/MSR access. WASM host functions can't virtualize hardware safely. |
| **Tier 2: Runtime** | Infrastructure agents | Kernel (Ring 0) | HermesAgent, CortexAgent, DisplayAgent, SecurityAgent, SafetyAgent, CronAgent, OptimizerAgent | Performance-critical. CortexAgent does LLM matmul — WASM overhead kills inference speed. Hermes dispatches every user intent — can't afford 10x slowdown. |
| **Tier 3: WASM** | User-extensible agents | WASM (wasmi) | Any .wasm binary loaded from /agents/ | Game agents, weather fetchers, RSS readers, productivity tools, custom skills. Sandbox prevents rogue agents from crashing the kernel. |
| **Tier 4: External** | Remote MCP agents | TCP/IP (future) | External tools, cloud skills, community plugins | Communicate via MCP over network. Sandboxed by network boundary + capability tokens. |

**Concrete agent assignment (current 20 → future 20+WASM):**

| Agent | Current Location | Future Location | Reason |
|---|---|---|---|
| SystemAgent | Kernel | Kernel (Tier 0) | Boot-critical |
| MonitorAgent | Kernel | Kernel (Tier 0) | Boot-critical |
| HwBridgeAgent | Kernel | Kernel (Tier 1) | Scancode IRQ — needs I/O ports |
| NetAgent | Kernel | Kernel (Tier 2) | Performance: smoltcp poll every tick |
| InputAgent | Kernel | Kernel (Tier 2) | Ring buffer — shared memory with IRQ handler |
| CortexAgent | Kernel | Kernel (Tier 2) | LLM matmul — 10x+ in WASM = unusable |
| HermesAgent | Kernel | Kernel (Tier 2) | Intent routing every user interaction |
| DisplayAgent | Kernel | Kernel (Tier 2) | Framebuffer MMIO — direct hardware |
| NetDriverAgent | Kernel | Kernel (Tier 1) | RTL8139 I/O ports, e1000 MMIO |
| UsbDriverAgent | Kernel | Kernel (Tier 1) | xHCI MMIO + transfer rings |
| BootSelfHealAgent | Kernel | Kernel (Tier 0) | Boot-critical |
| BootTrustAgent | Kernel | Kernel (Tier 0) | Boot-critical |
| PlatformAgent | Kernel | Kernel (Tier 0) | PCI+ACPI+APIC+SMP — boot-critical |
| MemoryAgent | Kernel | Kernel (Tier 0) | Page tables — boot-critical |
| GpuDriverAgent | Kernel | Kernel (Tier 1) | GPU MMIO + ring buffers |
| HwDetectAgent | Kernel | Kernel (Tier 2) | Calls Cortex for HW identification |
| CronAgent | Kernel | Kernel (Tier 2) | Timer-based — reliable tick scheduling |
| SecurityAgent | Kernel | Kernel (Tier 2) | Packet inspection — real-time |
| SafetyAgent | Kernel | Kernel (Tier 2) | Intercepts every skill call — can't afford WASM |
| OptimizerAgent | Kernel | Kernel (Tier 2) | System-wide metrics — needs full visibility |
| **J.A.R.V.I.S. Layer** | **(new)** | **WASM (Tier 3)** | Personality, templates, conversation — sandbox safe |
| **WeatherAgent** | **(new)** | **WASM (Tier 3)** | HTTP fetch → parse → display — user-extensible |
| **RssAgent** | **(new)** | **WASM (Tier 3)** | RSS parser — user-extensible |
| **GameAgent** | **(new)** | **WASM (Tier 3)** | Snake, Tetris — user-extensible |
| **User skills** | **(new)** | **WASM (Tier 3)** | Arbitrary user-written tools |

### 4.3 WASM Host Function Interface

The WASM runtime exposes kernel capabilities as host functions:

```rust
// Host function signatures (imported by WASM modules)
#[link(wasm_import_module = "aios")]
extern "C" {
    // VFS operations (→ FileAgent)
    fn vfs_read(path_ptr: *const u8, path_len: u32, buf_ptr: *mut u8, buf_len: u32) -> i32;
    fn vfs_write(path_ptr: *const u8, path_len: u32, data_ptr: *const u8, data_len: u32) -> i32;
    fn vfs_list(path_ptr: *const u8, path_len: u32, buf_ptr: *mut u8, buf_len: u32) -> i32;

    // Skill invocation (→ HermesAgent)
    fn skill_invoke(skill_name_ptr: *const u8, skill_name_len: u32,
                    args_ptr: *const u8, args_len: u32,
                    out_ptr: *mut u8, out_len: u32) -> i32;

    // Time (→ TimeAgent)
    fn clock_time() -> u64;

    // Network (→ NetAgent)
    fn http_get(url_ptr: *const u8, url_len: u32, buf_ptr: *mut u8, buf_len: u32) -> i32;

    // Event bus (→ EventBusAgent)
    fn event_publish(topic_ptr: *const u8, topic_len: u32,
                     data_ptr: *const u8, data_len: u32);
    fn event_subscribe(topic_ptr: *const u8, topic_len: u32) -> i32;

    // Agent lifecycle
    fn agent_log(msg_ptr: *const u8, msg_len: u32);
    fn agent_yield();
}
```

These are registered at WASM instantiation time and validated against the agent's capability tokens.

### 4.4 Performance Budget

| Operation | Kernel Agent (native) | WASM Agent (wasmi) | Overhead |
|---|---|---|---|
| `vfs_read` (small file) | ~50 µs (ATA PIO + FAT32 parse) | ~500 µs | 10x |
| `skill_invoke` (simple) | ~5 µs (direct fn call) | ~50 µs (host bridge) | 10x |
| `http_get` (network) | ~10 ms (smoltcp poll) | ~15 ms | 1.5x (I/O bound) |
| `event_publish` (bus) | ~1 µs (AtomicUsize update) | ~10 µs (host bridge) | 10x |
| Cortex matmul (128x128) | ~2 ms (native) | ~40 ms (wasmi interpreter) | 20x → **unacceptable** |

**Conclusion:** WASM agents are viable for I/O-bound tasks (network, file I/O, user interaction) but
not for compute-bound tasks (LLM, tensor ops). This reinforces the hybrid boundary: Cortex stays
kernel, user skills go WASM.

### 4.5 Migration Path

**Phase 1 (now):** All agents = kernel. No WASM runtime. (current state)

**Phase 2 (v0.76+, IDEA #309):** Add wasmi → WASM agents can be loaded from /agents/. Existing kernel
agents unchanged. New user skills written as .wasm.

**Phase 3 (v0.80+, IDEA #309b/c):** BitNet IDE lets users write, compile, and test .wasm agents
directly in the OS (assisted by Cortex). J.A.R.V.I.S. layer as WASM agent. Skill marketplace.

**Phase 4 (long-term):** Critical agents (Tiers 0-2) remain kernel for performance. Ecosystem of
community WASM agents grows independently. J.A.R.V.I.S. orchestrates agents regardless of runtime.

---

## Summary: Combined Viability Scores

| Component | Viability | LOC Estimate | Blocked By | Recommended Sprint |
|---|---|---|---|---|
| **Self-Update Agent** (dual slot) | **9/10** | ~500 kernel + ~100 bootloader | B-01 (DHCP) | Sprint 76-77 |
| **Update channels + rollback** | **9/10** | ~200 | Self-Update Agent | Sprint 77 |
| **WASM Runtime** (wasmi integration) | **8/10** | ~800 | None (no_std exists) | Sprint 75-76 |
| **WASI syscall mapping** | **8/10** | ~200 | WASM Runtime | Sprint 76 |
| **WASM sandbox + tokens** | **8/10** | ~200 | WASM Runtime + TrustCache | Sprint 76 |
| **J.A.R.V.I.S. — SOUL.md + personality** | **8/10** | ~100 | FAT32 reader | Sprint 78 |
| **J.A.R.V.I.S. — context persistence** | **8/10** | ~200 | MemoryTree + KG | Sprint 78 |
| **J.A.R.V.I.S. — proactive notifications** | **8/10** | ~150 | EventBus | Sprint 78 |
| **J.A.R.V.I.S. — conversation engine** | **7/10** | ~300 | Cortex (272K limited) | Sprint 79 |
| **J.A.R.V.I.S. — voice (PC speaker)** | **5/10** | ~150 | PIT timer | Sprint 80 |
| **J.A.R.V.I.S. — voice (TTS/STT)** | **2/10** | ~1500 | Audio HW + model | Post-MVP |
| **Hybrid agent boundary** | **9/10** | ~100 (policy config) | WASM Runtime | Sprint 76 |
| **BitNet IDE** | **5/10** | ~2000 | WASM + J.A.R.V.I.S. + Cortex 1.5B | Post-MVP |
| **Skill marketplace** | **4/10** | ~800 | WASM + network + MCP | Post-MVP |

### Recommended Implementation Order

```
Sprint 75-76:  WASM Runtime (B-14) — wasmi integration, WASI host functions, sandbox
Sprint 76-77:  Self-Update Agent — dual slot A/B, Ed25519 verification, rollback
               Hybrid boundary — formalize kernel vs WASM agent tiers
Sprint 78:     J.A.R.V.I.S. Layer — SOUL.md, context persistence, notifications
Sprint 79:     J.A.R.V.I.S. conversation — greetings, task decomposition, mood
Sprint 80+:    Update channels, voice (PC speaker), BitNet IDE (requires GGUF model)
Post-MVP:      Voice TTS/STT, skill marketplace, full generative J.A.R.V.I.S.
```

**Why WASM before Self-Update:** WASM runtime has no external dependencies (wasmi compiles in no_std
today). Self-Update needs network (B-01). They can proceed in parallel — WASM doesn't need network.

**Why J.A.R.V.I.S. after WASM:** The J.A.R.V.I.S. persona layer itself could be a WASM agent,
demonstrating the ecosystem. It doesn't *require* WASM (it can be a kernel agent), but doing it
as WASM validates the entire WASM agent toolchain end-to-end.

### Architecture Diagram (Final State)

```
┌──────────────────────────────────────────────────────────────────┐
│                        J.A.R.V.I.S. (WASM Agent)                  │
│  Personality | Voice | Context | Notifications | Task Decomp     │
└────────────────────────────┬─────────────────────────────────────┘
                             │ skill_invoke()
┌────────────────────────────▼─────────────────────────────────────┐
│                        HermesAgent (Kernel)                        │
│  Intent Router | ReAct Loop | SDD | Council | Skill Dispatch     │
└────────────────────────────┬─────────────────────────────────────┘
                             │ generate_text()
┌────────────────────────────▼─────────────────────────────────────┐
│                       CortexAgent (Kernel)                         │
│  BitNet LLM | Medusa | Sampling | HW Identify                    │
└────────────────────────────┬─────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
┌───────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ Kernel Agents │  │  WASM Agents     │  │  External MCP   │
│ (Ring 0)      │  │  (wasmi sandbox) │  │  (future)       │
│               │  │                  │  │                  │
│ Net, Disk     │  │ RssAgent.wasm   │  │ Cloud skills    │
│ Display, USB  │  │ Weather.wasm     │  │ Remote agents   │
│ PCI, ACPI     │  │ Games.wasm       │  │ Community       │
│ SMP, TPM      │  │ User apps        │  │ plugins         │
│               │  │                  │  │                  │
│ Direct HW     │  │ Sandboxed I/O    │  │ Network gated   │
│ Zero overhead │  │ Crash-safe       │  │ Capability token│
└───────────────┘  └─────────────────┘  └─────────────────┘
```

**This architecture delivers:**
1. Native performance for critical path (Disk, Net, Display, LLM)
2. Sandbox safety for user-extensible code (WASM agents)
3. Zero-trust security (Ed25519 capability tokens at every boundary)
4. Hot-reload for ecosystem agents (no reboot for new .wasm)
5. Conversational UX (J.A.R.V.I.S. persona with SOUL.md customization)
6. Atomic self-updates with automatic rollback (dual kernel slot + TPM integrity)
