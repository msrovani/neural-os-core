# SESSION_397 — Onda 3: §3 GPU + §6 Storage (loop TECNOLOGIAS)

**Canvas:** `docs/architecture/canvas-ondas34-gpu-storage-rede.md`. Onda 3: §3.1–3.11 + §6.1–6.8.

## Fixes aplicados (fixer)
- **H1** ahci CFL bit ATAPI removido (read=5/write=0x45) + PRDTL no DW correto.
- **H2** Intel MI opcodes recalculados (`const fn mi`, validados c/ i915 `intel_gpu_commands.h`; MI_FLUSH_DW 0x4C000001 já estava correto).
- **H3** AHCI PRDT por página (248 entradas/4MB, refuse >4MB) + DMA não-contíguo; **H4** command table estática + NVMe bounce estático (512 páginas) — fim de alloc/leak por I/O.
- **H5** NVMe 4Kn: RMW em loop por setor, bail >8 setores; **H6** ATA IDENTIFY words 117-118 = words/sector (`bps*2`, gate bit12 106). Testes atualizados.
- **M3** firmware não-ATA (qualquer device) + presença por dir-cache; **M6** sys_installer closure `with_device` + ptr-eq source/target.

## Upstream (lib-4)
virtio-drivers 0.13, zerocopy 0.8.57 candidato · Nova (NVIDIA Rust GPU) consolidada em Linux 6.15 (Pascal continua PFIFO-legado) · Xe-only HW Intel novo · bitnet.cpp = TL1/TL2 LUT + GPU kernel oficial · linux-firmware tag 20260916 (pin recomendado).

## Verificação
`cargo check -p k-nano -p k-hal --release` 0 erros; `cargo test -p k-nano` 192/192 e `cargo test -p k-hal` 56/56. Sem DUP novo. Commit + tag `v1.9.99-s397`.
