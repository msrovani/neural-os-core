# SESSION_386 — W2A8 / KernelPack honesty bughunt

**Data:** 2026-09-21  
**Premissas:** ADR-0084 (CPU W2A8 fidelity) · ADR-0105 (Ready=pack+golden) · ADR-0057 WS-D · SESSION_274 (matmul→None) · ADR-0088 AIOS · ADR-0092 slog

## Premissas apreciadas

| Fonte | Regra | Status pós-fix |
|-------|-------|----------------|
| ADR-0084 | CPU gated; ≠ GPU; fidelidade | F4 CPU ladder ✅; device = 0105 |
| ADR-0105 | Ready = pack+golden; stub≠Ready | register só pack verified |
| ADR-0057 WS-D | register GPU só Ready | canary Ready ≠ ternary on |
| SESSION_274 | device matmul None até pack | mantido + shape Falcon3-only |

## Achados → fix (ordem)

| ID | Fix |
|----|-----|
| H1 | `gate_status` → `READY_NO_DISPATCH` até `completed_hw>0` |
| H2 | `w2a8_enabled` exige `probe_done()` + TCG false |
| H3 | gfx90c → MadInt8 (não WmmaI8); `falcon3_w2a8` string alinhada |
| H4 | `note_gpu(name, probed, compute_ready)` + SCORE `ready\|probe\|none` |
| H5 | CE slog `CE_OK`/`CE_FAIL` sev ok/warn |
| M1 | GPU VERDICT sev ok/warn (não `info`) |
| M2 | `register_gpu_ternary` só se pack BitLinearW2A8 verified não-stub |
| M3 | shape bypass `k,n≥512` removido — só Falcon3 GEMV |
| M4 | `promote_with_session` refuse CpuStub + `is_cpu_stub_pack` |
| M5 | XPU sem submit fila até Layer S (`device_xpu_live=false`) |
| M6 | slog path=`scalar_quant`/`maddubs` 1× |
| L1 | `cpu_matmul` morto removido; `unpack_byte` só `cfg(test)` |
| L2 | ADR-0084 Status Accepted + INDEX lifecycle; F4 B3 wired |
| L3 | upstream bitnet crates.io = idea-only; PR#580 = scalar já espelhado |
| L4 | `drain` sempre CPU até Layer S |

## Residuals reforçados (honestos)

- Device BitLinearW2A8 fence+golden (ADR-0105 B1.2/B2.4/B4) = AWAITING_HW  
- XPU `dispatch_gpu_op` real = Layer S  
- IDEA #536/#550 host CUBIN/HSACO/zebin producers  

## Gates

- `cargo check -p cortex -p k-hal --release` 0 erros  
- `cargo check -p neural-kernel --release` 0 erros  
- testes: `cpu_stub_pack_refuses_promote`, `gfx90c_profile_is_mad_not_wmma`, `bitnet_w2a8` 3/3  

## Canvas

`canvases/w2a8-kernelpack-bughunt-s386.canvas.tsx` — 15/15 aplicados  
