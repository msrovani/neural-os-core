# Neural OS Hermes ⚡ — v0.47.0

**The first AI-native operating system. Bare-metal Rust. No Linux. No POSIX. No legacy.**

```
"We don't need an OS that runs AI. We need an OS that IS AI."
```

## Architecture at a Glance

```
                     ┌──────────────────────────────────────┐
                     │           AgentRegistry              │
                     │  ┌──────────────────────────────┐   │
                     │  │  8 Boot Agents (Oneshot)     │   │
                     │  │  Platform→Memory→SelfHeal→  │   │
                     │  │  Trust→NetDriver→USB→GPU→    │   │
                     │  │  HwDetect                    │   │
                     │  └──────────────────────────────┘   │
                     │  ┌──────────────────────────────┐   │
                     │  │  8 Runtime Agents (Poll)     │   │
                     │  │  System→Monitor→HwBridge→    │   │
                     │  │  Net→Input→Cortex→Hermes→   │   │
                     │  │  Display                     │   │
                     │  └──────────────────────────────┘   │
                     └──────────────────────────────────────┘
```

## What Makes It Unique

### 1. Everything is an Agent or Skill
No tasks, no services, no standalone drivers. **16 native Agent instances** with manifests, capabilities, and lifecycle. Skills are request-response interfaces owned by agents.

### 2. Bare-Metal Transformer LLM
4-layer BitNet with ternary weights (ADD/SUB only, zero multiplications). 272K params in 68KB. Char-level tokenizer. Autoregressive generation via `generate_text()`. Model file `.bitnet` format.

### 3. Self-Healing Kernel
When a Page Fault, Double Fault, or GPF occurs:
```
error → FailureClass::classify() → SelfHeal::analyze() → RecoveryAction
  → RestartDaemon | CreateSkill | LogAndContinue
  → LLM_REQUEST com contexto de erro → LLM sugere recuperação
  → lessons.push() → already_tried() → estratégia alternativa
  → Checkpoint restore on Double Fault
```

### 4. Semantic Snapshot with CDC + XOR Delta
Checkpoint system uses **Rabin fingerprint chunking** to split the 128KB allocator bitmap into variable-sized content-defined chunks, and **XOR delta** to store only modified chunks between saves. First bare-metal implementation of semantic compression in a kernel.

### 5. IRQ-Safe Locks
`IrqSafeLock<T>` disables interrupts on acquire, restores on release — preventing deadlocks in exception handlers. `TicketLock` FIFO prevents starvation in 7 global mutexes.

### 6. Multi-NIC Support
RTL8139 (I/O ports) + VirtIO-net (manual driver, 0 external deps) unified via `NetPhy`. DHCP auto-config via `smoltcp::socket::dhcpv4`. Fallback to static IP.

### 7. VirtIO-GPU (95%)
PCI capabilities parser → MMIO mapping → queue setup → feature negotiation. Manual driver without `virtio-drivers` crate (blocked by `zerocopy-derive` + MinGW). GET_DISPLAY_INFO pending.

### 8. CDC Rabin Chunking + XOR Delta
`chunker.rs` — rolling hash polynomial for content-defined chunk split. `delta.rs` — bit-exact archive of `PackedTernaryTensor` via XOR residual. Foundation for semantic memory compression.

## Quick Start

```powershell
# Build
cargo bootimage --release

# Run in QEMU (RTL8139 NIC)
qemu-system-x86_64 -m 2G -serial stdio -nic user,model=rtl8139 `
  -drive format=raw,file=target\x86_64-unknown-none\release\bootimage-neural-kernel.bin `
  -no-reboot -smp 2 -nographic

# Or with VirtIO-GPU
qemu-system-x86_64 -m 2G -serial stdio -device virtio-gpu-pci `
  -drive format=raw,file=target\x86_64-unknown-none\release\bootimage-neural-kernel.bin `
  -no-reboot -smp 2 -nographic
```

## Module Map (v0.47.0, ~5000 LOC kernel)

| Module | LOC | Function |
|---|---|---|
| `agent-core/` | 210 | Agent trait, AgentRegistry, AgentScheduler + watchdog |
| `display/` | 400 | Framebuffer BGRA32, DrawTarget, NeuralConsole, DisplayAgent |
| `virtio_net.rs` | 344 | VirtIO-net driver manual (I/O ports, descriptors) |
| `virtio_gpu.rs` | 420 | VirtIO-GPU PCI caps + MMIO + control queue (95%) |
| `rtl8139.rs` | 246 | RTL8139 driver via I/O ports |
| `cortex.rs` | 366 | Transformer 4 layers, BitNet, generate_text() |
| `interrupts.rs` | 235 | IDT full coverage 0-31, PIC/APIC EOI, 32 handlers |
| `apic.rs` | 364 | LAPIC/IOAPIC, SVR, SMP IPI, map_mmio_page |
| `pci.rs` | 198 | PCI scan, capabilities parser (VirtIO), BAR reader |
| `self_heal.rs` | 220 | FailureClass, Checkpoint, SelfHeal, semantic_snapshot |
| `chunker.rs` | 110 | CDC Rabin rolling hash, chunk_data/merge_chunks |
| `delta.rs` | 130 | XOR delta, ArchiveTensor, reconstruct |
| `dma.rs` | 60 | DmaBuf alloc/free with UC pages |
| `sync/irq_lock.rs` | 105 | IrqSafeLock<T> — cli/sti wrapper over TicketLock |
| `sync/ticket_lock.rs` | 55 | Re-export from ticket-lock crate |
| `network_agent.rs` | 110 | DHCP + DNS + HTTP state machine |
| `netstack.rs` | 350 | smoltcp interface, NetPhy unified NIC |

## Agent Landscape

| Code | Agent | Type | Schedule | Function |
|---|---|---|---|---|
| A-001 | **SystemAgent** | System | Oneshot | SYSTEM_READY, EchoSkill |
| A-002 | MonitorAgent | System | Oneshot | Publica SYSTEM_READY |
| A-003 | HwBridgeAgent | Router | Continuous | Scancode IRQ → EventBus |
| A-004 | NetAgent | Network | Continuous | smoltcp poll + HTTP |
| A-005 | InputAgent | Console | Continuous | Keyboard → USER_INTENT |
| A-006 | **CortexAgent** | Inference | Continuous | LLM generate_text() |
| A-007 | HermesAgent | Router | Continuous | Intent routing + skills |
| A-008 | **DisplayAgent** | Console | Continuous | VGA + framebuffer output |
| A-009 | NetDriverAgent | Driver | Oneshot | RTL8139 init |
| A-010 | UsbDriverAgent | Driver | Oneshot | xHCI port scan |
| A-011 | BootSelfHealAgent | System | Oneshot | SelfHeal init |
| A-012 | BootTrustAgent | System | Oneshot | TrustCache init |
| A-013 | PlatformAgent | System | Oneshot | PCI+ACPI+APIC+SMP |
| A-014 | MemoryAgent | System | Oneshot | MHI + Arch inference |
| A-015 | **GpuDriverAgent** | Driver | Oneshot | VirtIO-GPU detection |
| A-016 | HwDetectAgent | System | Oneshot | HwIdentifySkill |

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| bootloader | 0.9.34 | UEFI/BIOS handoff, map_physical_memory |
| x86_64 | 0.14.13 | IDT, GDT, TSS, paging, ports, MSR |
| smoltcp | 0.13 | TCP/IP stack, DHCP, DNS |
| embedded-graphics | 0.8.2 | DrawTarget for framebuffer |
| spin | 0.9 | Mutex (being replaced by TicketLock) |
| ticket-lock | workspace | FIFO TicketLock |
| event-bus | workspace | IPC publish/subscribe |
| skill-registry | workspace | Skill trait + MCP |
| libm | 0.2 | expf, sqrtf for neural net |
| pic8259 | 0.10 | ChainedPics (fallback) |

## Builds

- `v0.47.0+build77` — CDC Rabin + XOR Delta + Semantic Snapshot + IrqSafeLock + DmaBuf
- `v0.46.0+build76` — IrqSafeLock, TicketLock generalization, watchdog, DmaBuf
- `v0.45.0+build75` — Bugfix H3-H12, VirtIO-GPU PCI caps, DisplayAgent
- `v0.44.0+build74` — VirtIO-GPU PCI capabilities + MMIO + queue setup
- `v0.43.0+build73` — DisplayAgent + Framebuffer + embedded-graphics + font
- `v0.42.0+build72` — VirtIO-GPU detection + MMIO + GpuDriverAgent
- `v0.41.0+build71` — VirtIO-net manual driver, NetPhy unified
- `v0.41.0+build70` — DHCP, ARP, requires_network

## Custom Commands (Hermes Chat)

```
/status              → System status (memory, agents)
/echo <text>         → Reverse text
/hw                  → Hardware info
/netdiag             → Network diagnostics
/fetch http://ip/p   → HTTP GET
/ping <ip>           → ICMP echo
/add_skill <n> <d>   → LLM gera skill
/show_skills         → Lista skills ativas
/rm_skill <n>        → Remove skill
/reload_skills       → Recarrega seed
/trust allow <t> <s> → Autoriza token
/trust deny <t> <s>  → Revoga token
```

## License — MIT
