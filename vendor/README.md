# Vendor: bootloader 0.11.15 (patch neural-os)

## Por que

Em notebooks com Intel HD/UHD Graphics (ex.: HD 620), o UEFI GOP
pode iniciar em `PixelFormat::BltOnly` — sem framebuffer linear.
O `bootloader-x86_64-uefi` stock chama `gop.frame_buffer()` e panica:

```
Cannot access the framebuffer in a Blt-only mode
```

## Patch

`bootloader-x86_64-uefi/src/main.rs` → `init_logger`:

1. Enumera modos GOP e escolhe o maior com `Rgb`/`Bgr`
2. `set_mode` antes de acessar o FB
3. Se so houver BltOnly/Bitmask → boot sem FB (sem panic)

`bootloader/build.rs` instala o UEFI a partir deste `vendor/`
(em vez de crates.io).

Workspace `[patch.crates-io] bootloader = { path = "vendor/bootloader" }`.
