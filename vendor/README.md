# Vendor: bootloader 0.11.15 (patch neural-os)

## Por que

Em notebooks com Intel HD/UHD Graphics (ex.: HD 620), o UEFI GOP
pode iniciar em `PixelFormat::BltOnly` — sem framebuffer linear.
O `bootloader-x86_64-uefi` stock chama `gop.frame_buffer()` e panica:

```
Cannot access the framebuffer in a Blt-only mode
```

Além disso, escolher o **maior** modo linear (ex. 2048×2048 no OVMF)
sobrecarrega o compositor soft-float. Queremos UI amigável com resolução
decente, sem fingir controle total de Hz.

## Patch

`bootloader-x86_64-uefi/src/main.rs` → `init_logger` + módulo `fb_pick.rs`:

1. Detecta **QEMU** (CPUID TCG/KVM; SMBIOS `"QEMU"`/`"BOCHS"` — cobre WHPX)
2. Lê EDID via `EdidActive` / `EdidDiscovered` (DTD0 = preferred W×H + Hz; usado no **HW**)
3. Cascata de modos **só** `Rgb`/`Bgr` (ignora `BltOnly`/`Bitmask`):
   - **QEMU:** `qemu_1280x720` → `qemu_1280x800` → `qemu_mid` (1024–1280 × 600–800, mais perto de 1280×720) → `qemu_cap_max` (teto 1280×800)
   - **HW:** `edid_exact` → `edid_scaled` (±2% aspect, ≤1920×1080) → `cap_max` (prefere ≥1280×720) → `uncapped_max`
   - senão: boot **sem** FB (sem panic)
4. `set_mode` antes de acessar o FB
5. Log: `FB pick reason=... mode=WxH ... qemu=true/false`

**Hz:** informativo (EDID). `SetMode` não recebe refresh — o firmware amarra o timing ao modo.

`bootloader/build.rs` instala o UEFI a partir deste `vendor/`
(em vez de crates.io).

Workspace `[patch.crates-io] bootloader = { path = "vendor/bootloader" }`.
