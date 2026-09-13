# Plano de Implementação — Novidades GPU/Rust (set/2026) → KernelPack

**Data:** 2026-09-13
**Origem:** pesquisa web + fact-check contra fontes primárias (sessão @librarian)
**Escopo:** aproveitar CUDA Rust / cutile / khal como **produtores offline** de kernel para o
envelope NKP1 que **já existe**. Nenhum destes roda bare-metal.

---

## 0. Achados verificados

| Item | Status | Fonte | Encaixe no repo |
|---|---|---|---|
| `cuda-oxide` (NVlabs, Rust→PTX via Pliron IR, nightly, **sm ≥ 8.0**) | CONFIRMADO (04/set/2026) | developer.nvidia.com/blog/introducing-cuda-rust-two-tracks-for-writing-gpu-kernels · github.com/NVlabs/cuda-oxide | produtor de PTX/CUBIN p/ `tools/pack_nvidia_kernels.py` |
| `cutile-rs` (`cutile`, `cutile-ir`, `cutile-compiler`, v0.3.1, **stable 1.89+**, JIT Tile IR) | CONFIRMADO | github.com/NVlabs/cutile-rs · crates.io/crates/cutile | `cutile-ir` = IR em Rust puro (leitura/emissão offline) |
| `khal` / `vortx` / `inferi` (Dimforge, mai/2026) | CONFIRMADO | crates.io/crates/{khal,vortx,inferi} | referência de abstração p/ `k_hal::gpu::backend` |
| rust-gpu "Rust on every GPU" (core/std split, SPIR-V) | CONFIRMADO (jul/2025) | rust-gpu.github.io/blog/2025-07-25/rust-on-every-gpu | referência p/ zebin (Intel) |
| Nova (DRM Rust) no mainline | CONFIRMADO, **Linux 6.15 / mai-2025**, Daniel Almeida = **Collabora** (não NVIDIA, não 2026) | docs.kernel.org/gpu/nova | caminho futuro Turing+ (GSP) |
| rcore-os/virtio-drivers (virtio-gpu no_std, ativo 2026) | CONFIRMADO | github.com/rcore-os/virtio-drivers | referência VirtIO-GPU guest |
| rutabaga_gfx (magma-gpu) · rust-vmm/vhost-device gpu | CONFIRMADO | github.com/magma-gpu/rutabaga_gfx | referência virgl/Venus |
| wgpu bare-metal · e4m/crucible no_std · papers arXiv "DNN bare-metal" | **NÃO EXISTE / nada novo** | gfx-rs/wgpu#6826 (aberta) | — |

**Conclusão do fact-check:** o resumo recebido estava ~85% correto; único erro material = data/autoria
do driver Rust no mainline (é Nova, 6.15, mai/2025, Collabora).

---

## 1. Por que isto encaixa sem tocar no metal

Pipeline **já implementado**:

```
tools/gpu_kernels/src/lib.rs        (lógica CPU golden, no_std, sem CUDA/Vulkan)
  → tools/pack_nvidia_kernels.py    (nvcc -cubin -arch=sm_61|75|89 → envelope NKP1 + FNV + Ed25519 opcional)
  → k_hal::gpu::kernel_pack::parse_and_verify / load_named / find_active_pack / promote_with_session
  → k_hal::gpu::canary              (sem KernelPack assinado = nunca Ready)
  → k_hal::gpu::compute_dispatch    (TODO Layer S: despachar QMD/PM4/GPGPU_WALKER)
```

`PackOp::BitLinearW2A8 = 2` e `GoldenId::BitLinearW2A8` **já estão reservados**;
`IsaTag` já cobre `Sm61/Sm70/Sm75/Sm80/Sm89/Gen9/Dg2/Gfx*`.

As novidades entram **só no primeiro elo** (quem produz o blob). O metal não muda.

**Bloqueio real hoje:** nosso HW de lab é GTX 1050 = **sm_61 (Pascal, CC 6.1)**, e `cuda-oxide`
declara **CC ≥ 8.0**. Logo o track Rust→PTX da NVIDIA provavelmente **não cobre nosso silício**.
Isso é o gate G1 abaixo — decidir antes de escrever qualquer código.

---

## 2. Fase 0 — Registro (governança, ~1h)

1. `docs/memory/IDEA_BANK.md` — 4 entradas ⏳:
   - **Rust→PTX offline como produtor de KernelPack** (`cuda-oxide`), gate sm_61.
   - **`cutile-ir` como IR de referência** p/ emitir o kernel W2A8 (Rust puro, sem CUDA runtime).
   - **`khal`/`vortx`/`inferi` como referência de abstração** p/ `k_hal::gpu::backend`.
   - **Nova/GSP como caminho Turing+** (firmware-first, RPC mínima) — sem código até ter o HW.
2. `TECNOLOGIAS.md` — linhas novas (cuda-oxide, cutile, khal/vortx/inferi, rust-gpu, Nova, virtio-drivers)
   com ADR/IDEA/arquivo/sprint; rodar `python tools/update_tecnologias.py`.
3. `docs/architecture/INDEX.md` — reservar ID da ADR da Fase 3 (próximo livre após 0104).

**Verificação:** `update_tecnologias.py` sem erro; IDEA_BANK com estado ⏳ (não ✅ — nada landou).

---

## 3. Fase 1 — Gates de decisão (research, 3 lanes @librarian paralelas)

Nenhuma linha de código kernel nesta fase. Cada lane responde **uma** pergunta binária.

| Gate | Pergunta | Se NÃO | 
|---|---|---|
| **G1** | `cuda-oxide` emite PTX/CUBIN p/ **sm_61** (ou aceita `--arch` menor que 8.0)? | Track NVIDIA morto p/ nosso lab → só valor futuro (Turing+) |
| **G2** | `cutile-compiler` produz **PTX/CUBIN offline** (sem driver CUDA em runtime)? E `cutile-ir` é `no_std`-legível? | cutile vira só leitura de design |
| **G3** | `khal` tem camada de abstração utilizável sem wgpu/cudarc (feature `cpu` isolada)? | khal = referência conceitual apenas |

**Entregável:** 1 nota curta por gate em `docs/memory/SESSION_3xx.md` + decisão registrada no IDEA_BANK
(⏳ → ✅ adotado / ❌ descartado com motivo).

**Regra AIOS (premissa 4):** gate negativo **não** é bypass silencioso — registra o motivo e o que
destravaria (ex.: "G1 destrava com GPU Turing+ no lab").

---

## 4. Fase 2 — Spike offline (só se G1 **ou** G2 = sim)

Objetivo: provar que um kernel **escrito em Rust** vira um NKP1 que o parser do kernel aceita.
**Zero mudança no metal.**

### 2.1 Estender o packer existente (não criar script novo)

**Arquivo:** `tools/pack_nvidia_kernels.py`

Adicionar `--source {cu,rust}` (default `cu`, comportamento atual intocado):

```python
# --source rust: compila Rust → PTX via cuda-oxide (ou cutile-compiler), depois ptxas → CUBIN
def compile_cubin_from_rust(sm: str, out_cubin: Path) -> bool:
    # 1. rust source (tools/gpu_kernels/src/ptx/vector_add.rs) → PTX
    # 2. ptxas -arch=<sm> -o out_cubin kernel.ptx
    # fallback honesto: retorna False → CPU stub (IR=CpuStub), como hoje sem nvcc
```

**Por que estender e não criar arquivo novo:** o envelope, FNV, assinatura Ed25519,
`--unsigned` e o fallback CPU stub já estão certos nesse script. Duplicar = segunda verdade.

### 2.2 Kernel Rust mínimo

**Arquivo novo:** `tools/gpu_kernels/src/ptx/vector_add.rs` — mesmo contrato POD de
`VectorAddParams` (`n`, `a_pa`, `b_pa`, `c_pa`) que já existe em `tools/gpu_kernels/src/lib.rs`.

### 2.3 Enum do envelope (1 linha, só se necessário)

`crates/k_hal/src/gpu/kernel_pack.rs::CompilerId` tem `Cuda129=1 … HostCpuLogic=5`.
Se o parser **rejeita** valor desconhecido → adicionar `CudaOxide = 6` (+ braço no match de parse).
**Não** bumpar `NKP_ABI` se o campo for lido como u32 tolerante — verificar antes.

**Verificação da Fase 2:**
- `python tools/pack_nvidia_kernels.py --source rust --arch sm_61 --unsigned -o target/nkp_rust.bin`
- teste host: `parse_and_verify(bytes)` → `Some(pack)` com `ir == Cubin`, `payload_len > 0`
- `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` sem regressão
  (baseline conhecido: 784 pass / 6 fail pré-existentes)
- `cargo check --release` 0 erros

---

## 5. Fase 3 — ADR curta: "GPU Compute — IR/Emit Strategy" (≤60 linhas)

Decisão a documentar:

1. **Não** importamos `cutile-rs`/`cuda-oxide`/`khal`/`inferi` como dependência de runtime do kernel
   (todos exigem CUDA runtime ou wgpu em userspace).
2. Adotamos **produção offline** de blob nativo (PTX→CUBIN / zebin / HSACO) embrulhado no NKP1 existente.
3. `cutile-ir` é referência de **forma** do kernel W2A8 (tile partitioning + ownership), não dependência.
4. Path de execução no metal continua `cortex::compute::dispatch_ternary` → `k_hal::gpu::compute_dispatch`
   (CPU W2A8 `bitnet_w2a8` ≠ GPU `BitLinearW2A8` — não confundir, ver SESSION_328).
5. Turing+/GSP segue modelo **Nova** (firmware-first) quando o HW existir.

**Verificação:** ADR com ID novo + linha em `docs/architecture/INDEX.md`.

---

## 6. Fase 4 — Device-side (condicional, só com blob real + HW real)

Único ponto onde o metal muda, e só se a Fase 2 produzir CUBIN válido:

**Arquivo:** `crates/k_hal/src/gpu/compute_dispatch.rs` (TODO Layer S já marcado)

- Despachar QMD (`nvidia_pascal_qmd.rs`) com o payload do pack + ler resultado da VRAM.
- Gate inegociável: `canary` **Ready** + `find_active_pack(vendor, isa, op)` Some + golden confere.
- Sem canário → `None` (comportamento atual, honesto — SESSION_274).
- **AWAITING_HW:** QEMU não emula NVIDIA; aceite só em HW real (GTX 1050, sm_61).

**Verificação:** boot HW real → log `GPU canary Ready` + `W2A8 device PASS` vs golden CPU
(`tools/gpu_kernels::bitlinear_w2a8_ref`).

---

## 7. Não-fazer (YAGNI)

- ❌ Rodar `cutile-rs`/`khal`/`inferi`/`wgpu` dentro do kernel.
- ❌ Trocar o nightly pinado (`nightly-2026-07-05`) por causa do `cuda-oxide` — ele é ferramenta **host**.
- ❌ Reescrever `k_hal::gpu::backend` "inspirado" em khal antes de ter o HW alvo.
- ❌ Suporte AMD/Intel nesta rodada — `pack_amd_kernels.py`/`pack_intel_kernels.py` continuam como estão.
- ❌ Declarar aceleração GPU com base em aceite de UI (SESSION_328: aceite UI ≠ prova de aceleração).

---

## 8. Riscos

| Risco | Mitigação |
|---|---|
| G1 negativo (sm_61 fora do suporte `cuda-oxide`) | Fase 2 vira "infra pronta p/ Turing+"; registrar e arquivar sem código morto |
| `cutile` JIT exige driver CUDA → inútil offline | G2 testa `cutile-compiler` isoladamente antes de qualquer integração |
| `CompilerId` novo quebra packs antigos | não bumpar ABI; teste host de parse de pack **antigo** continua passando |
| Blob não-assinado em produção | `promote_with_session` já re-assina com chave de sessão; HITL trust token `(install, gpu_kernel, hash)` |
| Toolchain host instável (alpha, 5 dias de vida) | pinar versão no `tools/` + documentar; nada entra no `Cargo.toml` do workspace |

---

## 9. Esforço

| Fase | Tipo | Esforço | Gate |
|---|---|---|---|
| 0 | docs/registro | ~1h | — |
| 1 | research (3 lanes paralelas) | 1 rodada | G1/G2/G3 |
| 2 | packer + kernel Rust + teste host | ~4-6h | G1 **ou** G2 = sim |
| 3 | ADR curta | ~1h | — |
| 4 | dispatch device + golden | ~1-2 dias | blob real + **HW real** |

**A alavanca é a Fase 1.** Sem G1/G2 positivos, o entregável honesto é: registro + ADR +
"infra NKP1 pronta, produtor Rust aguardando GPU Turing+ no lab".
