# SESSION_274 — Revisão GPU compute (iGPU/dGPU Intel·AMD·NVIDIA)

**Sprint:** v1.9.99 TEST  
**Data:** 2026-08-17  
**Escopo:** uso das GPUs para computação — ADR-0087/0057/0047-GPU vs código real.

## Mapa (auditoria)

O stack GPU compute vive em `k_hal/src/gpu/` (~40 módulos); jarbas/bin são facades (sem duplicação SESSION_237). BAR roles **medidos** (ADR-0087 pré-req 4a OK). Canário `vector_add`/CE gated honesto. **Mas o BitNet matmul é sempre CPU** — e três mentiras/seams mortos existiam:

| Achado | Onde | Problema |
|--------|------|----------|
| `nvidia_matmul` fingia GPU | `backend.rs` | Com `Ready`, fazia upload+readback 64B por chamada (custo sem uso) e computava `a.matmul(b)` na CPU — `work_queue::drain(true)` contava como GPU. |
| Intel `gpu_matmul` idem | `intel.rs` | `a.matmul(b)` na CPU dentro da fn "gpu_", contada como GPU. |
| `boot_report.gpu_ok = true` | `k_nano/boot_report.rs:73` | Placeholder hardcoded — claim sem evidência. |
| `mhi_tier0_copy` sem caller | `nvidia_pascal_ce.rs` | Seam CE (ADR-0087 F4b) pronto, mas `mhi_tick` nunca promovia Dram→Vram com dados (metadata-only + AWAITING sempre). Lição F2: política sem wiring é ruído. |
| `msched_*` sem callers | `vram.rs` | Preditor Belady morto: `msched_init`/`msched_record` nunca chamados. |
| Código morto | `detect.rs::mark_compute_ready`, `backend.rs::gpu_forward` | Zero callers. |
| A-015 GpuDriverAgent | bin `agents.rs` | Manifest "GPU backend detect" mas só iniciava VirtIO — não reportava o backend real. |

## Decisão

| Anel | Mudança |
|------|---------|
| `k_hal::gpu::backend` | `nvidia_matmul` → `None` (device math = Layer S; canário já prova VRAM 1x no boot); telemetria `drain(result.is_some())`; `gpu_forward` deletado; `note_gpu` no boot_report em todos os terminais. |
| `k_hal::gpu::intel` | `gpu_matmul` → `None` (staging do shader mantido; dispatch MEDIA_OBJECT = Layer S). |
| `k_nano::boot_report` | `note_gpu(name, ok)` + `gpu_ok` real (sem nota = false) + `BootEvent::Gpu`. |
| `k_nano::mhi` | `register_tier0_copier(copy, free)` + `try_tier0_promote` — Dram→Vram com DADOS via engine quando hook registrado; falha = rollback VRAM (lição CoW F2) + fallback metadata/AWAITING. |
| `k_hal::nvidia_pascal_ce` | `probe_global` registra `mhi_tier0_copy` no MHI **só com canário CE golden**; sucesso de cópia → `record_access(dst)`. |
| `k_hal::gpu::vram/sasos` | `msched_init()` no `init_vram_tier`; `sasos_vram_ptr` registra acesso (MSched+MHI) — ADR-0087 §2.0.1 (CE = transfer, SASOS = acesso). |
| bin A-015 | GpuDriverAgent reporta `gpu_status()` + `vram_status()` reais após o VirtIO FE. |

Em QEMU nada muda de comportamento (CE nunca Ready → hook não registra → metadata+AWAITING como antes); em HW real com GTX 1050 e canário golden, o `mhi_tick` passa a promover working set quente para VRAM com dados reais.

## Pesquisa (crates/sites Rust)

- **cuda-oxide (NVlabs, 2026)**: rustc backend `#[kernel]`→PTX, single-source; host Linux + CUDA 12.x + LLVM 21. **Host toolchain**, não roda no bare-metal — candidato para gerar o **KernelPack W2A8** offline (ADR-0057 Layer S) em Rust puro.
- **kaio (v0.5)**: `#[gpu_kernel]`→PTX **sem CUDA toolkit** (kaio-core zero-dep); matmul tensor-core ≈cuBLAS @4096³. Depende do driver NVIDIA p/ JIT/launch — no nosso caso serviria só como gerador de PTX; o SASS final ainda exige ptxas offline (sem JIT no kernel).
- Nenhuma crate roda GPU compute **dentro** de um kernel `no_std` sem driver — confirma a arquitetura local: QMD/PM4/GPGPU_WALKER próprios + KernelPack assinado offline.

## O que NÃO foi feito (honesto)

- Kernel W2A8 no device (NVIDIA QMD / Intel GPGPU_WALKER / AMD) — continua **Layer S**: exige toolchain offline + golden em silício (GTX 1050). `gpu_ternary` segue `None`.
- AMD SDMA engine (ADR-0087 Fase 6) — AWAITING_HW RDNA, sem código inventado.
- GSP (Turing+) — scaffold PARTIAL como estava.
- Limpeza dos módulos semi-mortos maiores (`xpu.rs`, `kv_dma.rs`, `xqueue.rs`) — candidatos a delete/wire em sessão própria (listados aqui para não perder).

## Testes

- `cargo test -p k-nano --target-dir target/check-s274-nano mhi` (10, inclui `tier0_promote_requires_hook_and_moves_registry`)
- `cargo test -p k-hal --target-dir target/check-s274-hal`
- `cargo check -p neural-kernel --features fat-boot-log --target-dir target/check-s274-nk`

## Lição

“GPU Ready” tem três verdades distintas: BAR/canário OK (bring-up), engine de cópia OK (CE), e matemática no device (kernel W2A8) — misturá-las gera telemetria mentirosa (`drain(true)` com matmul na CPU). Cada uma tem seu gate; o HUD/report só pode afirmar a que passou.
