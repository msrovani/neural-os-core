# SESSION_373 — k_hal bughunt AIOS (honesty + offsets + trim)

**Foco:** Premissas ADR-0088 / 0041 R1 / 0087 / 0092 — Ring 1 `k_hal` (HalOffer, DeviceCap, MMIO BE, GPU/WiFi).

**Canvas:** [k-hal-bughunt-s373](file:///C:/Users/msrov/.cursor/projects/c-DEV-neural-os-core-latest/canvases/k-hal-bughunt-s373.canvas.tsx)

## Achados → fixes (ordem aplicada)

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | `execute_gen_shader` retorna true | função + load NOOP apagados |
| HIGH | `gpu_blit` opcode 0x41 (XY_COLOR) | apagado; BCS vivo em `blit.rs` |
| HIGH | canary/init slog TRACE | sev `ok`/`warn`/`fail` |
| HIGH | slog sub `init`/`intel`/`BAR`/`GEN9`/`D4` mudos | `from_sub` aliases → Ok |
| HIGH | `ring.rs` doorbell `0x120038` (START, hex a mais) + push sem `cmd` | Intel doorbell noop; write `cmd`; HEAD software=0 |
| HIGH | iwlwifi CSR_RESET=0x028 HBUS=0x200 inventados | Linux `iwl-csr.h`: RESET 0x020, GP 0x024, HBUS 0x400 |
| HIGH | `MI_BATCH_BUFFER_END` 0x00500000 + emitido no ring | 0x05000000; `dispatch_compute` → false; gen9 sem BB_END no ring |
| HIGH | NVIDIA probe poke `DEADBEEF` (SESSION_260 D3 hang) | skip poke; D-state gate no map |
| HIGH | `gpu_matmul` `or_else(cpu_matmul)` | `None`; CPU só no caller (bench skip TFLOPS) |
| HIGH | generic_wifi Sintetizado-IA→Ethernet Ok + DMA VA | deny `wifi_map_unknown`; `virt_to_phys` |
| MED | wait_idle/fence/GuC/PFIFO spin count | `k_hal::wait::until` TSC 2s |
| MED | xqueue/NKP timeout+mismatch `info` | `warn`/`fail`; unsigned skip=`warn` |
| MED | XPU `gpu_dispatches` com compute CPU | só `cpu_fallbacks` + warn intenção |
| LOW | NPU AWAITING `info`; libm/lazy_static/ticket-lock 0 use | warn/ok; trim `Cargo.toml` |
| LOW | kv_dma copy sem `len` | refuse se slice < seq×hidden |

## Já ok (não tocado)

Intel RCS 0x2000 / BCS 0x22000 / CTL 0x3001 (s355); iwlwifi sem SSID fabricado; CapGate DENY; BCS XY_SRC_COPY 0x54F00008; cortex dep (Tensor / HardwareRegisterMap).

## Upstream

- spin 0.9.9 no lock — manter
- libm / lazy_static / ticket-lock removidos de k_hal (0 callers)
- x86_64 0.15 — **adiar** (GDT/IDT)
- iwl-csr.h Linux master — offsets aplicados

## Verify

- `cargo test -p k-hal --lib` → **54 pass** (incl. `wait::until`)
- `cargo test -p k-nano --lib slog::` → **4 pass**
- GPU compute sem KernelPack = `None`, nunca CPU contado como GPU

## Residual pós-auditoria ([Deep k_hal honesty](f943dacc-3a20-48a9-af58-5bd95f526f03))

HIGH 1–9 já estavam no commit s373. Ainda abertos e fechados agora:

| Sev | Achado | Fix |
|-----|--------|-----|
| HIGH | `SYS_MAP_FB` devolvia VA de heap `0x4000_0000_0000` | devolve o phys; caller mapeia |
| HIGH | Gen9 copia 3×n f32 em 4 KiB sem teto | `n*12>4096` → false |
| MED | `ucode_loaded=true` sem alive | só se `0x5A5A`; senão Err |
| MED | AMD doorbell escrevia `0x1B0` genérico | noop + warn |
| MED | Ethernet `send` `Ok(())` descartava pacote | `Err("ethernet_unwired")` |
| MED | `init_gtt` sempre true | refuse pages 0 ou >512 |

Ainda não tocado (MED, não quebra o boot path): FORCEWAKE probe, offer backoff, `is_integrated` por vendor, hub_msc budget TSC, tautologia do canário já ausente.
