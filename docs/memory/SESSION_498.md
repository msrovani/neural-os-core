# SESSION_498 — Ampere GSP escada D0–D5 + caminhos paralelos

**Sprint:** v1.9.99-s498 TEST  
**Data:** 2026-10-19  
**Plano:** GSP Ampere Compute (BAR || Pascal || NKP || escada GSP)

## Premissa

Blobs no stick ≠ FLOPS. Metal 10de:25ac exige GSP-RM; valor imediato = BAR Mapped + pack sm_86 assinado + escada tipada até canário.

## Feito

### S0 — baseline FAT
- `tools/verify_gsp_fat.py`: GSP.BIN (~38MB) / GSPBOOT / GSPLOAD presentes em `target/usb_hw.img` — **PASS host**.
- Metal slog `fw preload … 3/3` = **AWAITING** reflash+boot.

### A1 — BAR Mapped
- `detect.rs`: rejeita size com bits altos lixo; NVIDIA aperture ≠ BAR0 MMIO.
- `vram.rs`: skip se `bar2==bar0`; hi-garbage mask.
- `backend.rs`: map UC até 2GB (não cap 256MB) p/ Falcon3-1B packed.

### D0–D5 — `k_hal::gpu::nvidia_gsp`
- Módulo novo: `hs_hdr` (parse Nouveau bin/HS) + `bring_up` (D0 blobs → D1 SEC2/booter → D2 alive/`GpuGspBooted` → D3 RPC DMA → D4 channel stub → D5 `try_gsp_vector_add`).
- Wire: `firmware::secure_boot_gpu` GSP path; `nvidia::probe` + `try_vector_add_gsp`.
- Idempotente (secure_boot + probe).
- Host tests: 4/4 `nvidia_gsp`.

### A3 — NKP sm_86
- CUBIN real (nvcc+MSVC) `NKP_SM86.BIN` / `NKP_W2A8_SM86.BIN`.
- Assinatura Ed25519 via `ota_sign` + seed `target/nkp_lab_seed.hex` (gitignore).
- Pubkey pinada em `k_nano::identity` TRUSTED (3ª chave).
- `sig_ok=True` host.

### A2 — Pascal lab
- Slog explícito `step=legacy_acr lab=pascal` no probe LegacyAcr (sem misturar GSP).

## Honestidade

| Claim | Status |
|-------|--------|
| FAT GSP short names | VERIFIED host |
| GSP-RM alive / GpuCompute | UNKNOWN até metal D1–D5 |
| NKP signed envelope | VERIFIED host |
| BAR Mapped metal 25ac | UNKNOWN (código A1 pronto) |
| Canário GSP PASS | IMPLEMENTED path; Ready só com ChannelReady+launch (ainda residual launch) |

## Evidência host

```
python tools/verify_gsp_fat.py → PASS
cargo test -p k-hal --lib nvidia_gsp → 4/4
NKP sig_ok True (ota_sign + nkp lab key)
```

## Próximo (metal)

1. Reflash `usb_hw.img` (Rufus DD).
2. Grep: `fw preload pkg=gpu.nvidia.ga107`, `GSP D0/D1/D2`, `CapToken=GpuGspBooted` se WPR avança.
3. BAR: `VRAM aperture_owner` + roles ≠ quarantine.
4. NKP: `verified=1` no canário; D5 ainda pode FailDispatch até RPC launch.

## Lições

- Short names FAT são o contrato da recipe — versioned linux-firmware names sozinhos = dead weight.
- GSP filter por chip no path (não liberar ad102 porque ga102 ∈ allow).
- NKP signed exige seed host + pubkey pinada; stub unsigned ≠ Ready (H4).
