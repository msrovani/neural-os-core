# SESSION_342 — GPU research: virtio-drivers + Nova GSP + NVIDIA CUDA Rust

**Data:** 2026-09-13
**Sprint:** s342 (pós-s341)
**Status:** ✅ Implementado

## Pesquisa realizada

### 1. Crates Rust no_std para GPU
- **virtio-drivers (rcore-os)** — Única lib no_std madura de VirtIO guest (virtio-gpu 2D framebuffer). Commits ativos 2026. Usado por Redox/Scarlet OS.
- **virtio-accel** — Protocolo virtio para aceleradores (NPU/GPU). Referência de protocolo para futuros NPUs.
- **wgpu no_std** — Não existe. HAL continua assumindo OS.
- **rust-gpu** — "Rust em toda GPU" — split core/no_std. SPIR-V codegen. Útil como inspiração, não como driver.

### 2. NVIDIA CUDA Rust (Set 2026 — 5 dias atrás!)
- **cuda-oxide** — Rust → PTX via custom rustc codegen. Requer nightly + CUDA 12.x+. Compute ≥ 8.0.
- **cutile-rs** — Tile-based GPU programming em Rust stable 1.89+. Publicado no crates.io. Já usado em HuggingFace Grout.

### 3. Nova driver (NVIDIA)
- Merged no Linux 6.15, escrito em Rust.
- Padrão: "driver offloads boring stuff to GSP firmware".
- **Pascal não tem GSP** — confirmação de que MMIO manual (nosso caso) é industrial-correct.
- Para GPUs Turing+ (com GSP): seguir padrão Nova (GSP RPC).

### 4. Outros
- **rutabaga_gfx** — virgl/Venus/gfxstream server em Rust. Referência protocolo virtio-gpu.
- **vhost-device-gpu** — vhost-user-GPU em Rust puro.
- **Asahi Linux** — AGX driver Rust subindo no mainline.
- **arXiv** — Nenhum paper em "DNN bare metal sem OS". Nicho aberto.

## Decisões

1. **virtio-drivers** é o path correto para unificar VirtIO no neural-os-core
2. **NVIDIA Pascal** continua sendo MMIO manual (sem GSP) — correto
3. **cutile-rs** pode ser referência futura para GPU compute (stable Rust, HuggingFace)
4. **wgpu bare-metal** não existe — não esperar
