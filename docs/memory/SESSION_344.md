# SESSION_344 — Gates de decisão G1–G3 (plano GPU Rust ecosystem)

**Data:** 2026-09-13
**Sprint:** s344
**Objetivo:** Executar a Fase 1 do plano `docs/implementation/2026-09-13-gpu-rust-ecosystem-plan.md` —
responder as 3 perguntas binárias dos gates antes de qualquer código kernel.

---

## G1 — `cuda-oxide` suporta sm_61 (GTX 1050, Pascal)? → ❌ **NEGATIVO**

**Fontes:**
- Feature matrix oficial: https://nvlabs.github.io/cuda-oxide/appendix/supported-features.html
  - "PTX Output ... Targets **sm_80 through sm_100a**"
- README GitHub NVlabs/cuda-oxide: requisitos = nightly-2026-08-28, CUDA 13.0+, Linux (testado Ubuntu 24.04).

**Detalhes que agravam:**
1. O piso é **sm_80 (Ampere)** — não Turing+ como o plano assumia. Turing (sm_75) TAMBÉM não
   é suportado. O gate "destrava com GPU Turing+" do plano está errado: destrava com **Ampere+**.
2. Requisito de SO: **Linux**. O host de dev atual é Windows (MinGW toolchain). O cuda-oxide
   nem roda como produtor offline neste host sem WSL.
3. `HMM` (unified memory) requer Turing+, Linux 6.1.24+.

**Veredicto:** Track Rust→PTX via cuda-oxide é **impossível para nosso silício de lab**
(GTX 1050 sm_61) e exige Linux. Infra NKP1 permanece válida para o futuro, mas o produtor
Rust aguarda GPU sm_80+ em host Linux.

---

## G2 — `cutile-compiler` emite PTX/CUBIN offline? `cutile-ir` é no_std? → 🟡 **PARCIAL**

**Testado localmente (probe `gate_probe`, Windows/MinGW, stable):**

### G2a — `cutile-ir` 0.3.1 ✅ **SIM, offline e puro Rust**
- Compila standalone sem CUDA toolkit (deps: half, indexmap, thiserror, zerocopy — nenhuma CUDA).
- `Module::new("name")` → `write_bytecode(&module)` → 34 bytes de bytecode válido →
  `decode_bytecode` round-trip OK. **Zero linkage com libcuda.**
- Doc-header: "No LLVM or MLIR dependency is required."
- ⚠️ Porém: não é `no_std` (usa std). Para nossa Fase 2 (produtor **host** em tools/), isso é irrelevante.
- O output é **Tile IR bytecode** (consumido por `tileiras`), NÃO PTX.

### G2b — `cutile-compiler` 0.3.1 🟡 **Offline sim, mas requer CUDA 13.2+ toolkit no host**
- `build.rs` é só warning (nunca falha o build — "emitting bytecode and cross-building are
  legitimate on machines without a 13.2+ toolkit").
- MAS: a dependência `cuda-bindings` 0.3.1 tem build.rs que **falha hard** sem
  `CUDA_TOOLKIT_PATH`/`CUDA_HOME` (não há toolkit no host atual → `cargo add cutile-compiler` não compila aqui).
- Pipeline: Rust DSL → `cutile-ir` bytecode → **`tileiras --gpu-name sm_XXX -o out.cubin in.bc`**
  (padrão assembler offline tipo ptxas — `Command::new(tileiras)` em
  `cuda_tile_runtime_utils.rs:651,895`; sem GPU no host).
- `tileiras` ships no CUDA toolkit **13.2+** (`MIN_TILE_CUDA_VERSION = 13020`).

**Veredicto:** O caminho offline existe (bytecode → tileiras → cubin, sem GPU), mas exige
CUDA toolkit 13.2+ instalado no host produtor. E o hardware alvo do Tile model é
**sm_80+** (Ampere/Ada/Hopper/Blackwell — fonte: NVIDIA blog CUDA 13.2 "compute capability
8.X", cutile-rs README "minimum supported architecture: sm_80"). **sm_61 está fora do
suporte do Tile model também.**

---

## G3 — `khal` funciona sem wgpu/cudarc (feature `cpu` isolada)? → ❌ **NEGATIVO (bug upstream)**

**Testado localmente:** `khal = { version = "0.3.0", default-features = false, features = ["cpu"] }`

**Erro:** `error[E0432]: unresolved import crate::backend::webgpu` —
`any_backend.rs:30` tem `use crate::backend::webgpu::WebGpuTimestamps;` **sem** o
`#[cfg(feature = "webgpu")]` (as linhas 17 e 27 vizinhas têm o gate; a 30 não).

**Veredicto:** Bug de packaging no khal 0.3.0 — `cpu`-only não compila. Seria um
one-liner de PR upstream, mas não é nosso papel consertar para validar um gate.
khal fica como **referência conceitual** para `k_hal::gpu::backend` (como o plano
já antecipava no G3 negativo). **Nenhum valor de dependência hoje.**

---

## Decisão consolidada

| Gate | Pergunta | Resultado | Consequência |
|------|----------|-----------|--------------|
| G1 | cuda-oxide emite PTX p/ sm_61? | ❌ NÃO (piso sm_80 + Linux-only) | Fase 2 track cuda-oxide não executa |
| G2a | cutile-ir offline/no_std? | ✅ offline (não no_std, irrelevante p/ host) | **cutile-ir = IR de referência válido** |
| G2b | cutile-compiler PTX/CUBIN offline? | 🟡 sim via tileiras, mas exige CTK 13.2+ host e alvo sm_80+ | Produtor offline válido só p/ Ampere+ |
| G3 | khal sem wgpu/cudarc? | ❌ NÃO (bug upstream, cpu-only quebra) | khal = referência conceitual apenas |

**Conclusão:** Fase 2 do plano NÃO executa para nosso silício (sm_61). Os gates negativos
**destravam com GPU Ampere+ (sm_80) no lab + host Linux** — não Turing+ como o plano
assumia. O entregável honesto da Fase 1 é:

1. ✅ G2a positivamente provado: `cutile-ir` é um IR puro-Rust serializável (probe testado).
2. ❌ Nenhum produtor Rust→CUBIN cobre sm_61 hoje. Pipeline NKP1 permanece com nvcc
   (`tools/pack_nvidia_kernels.py --source cu`) como único produtor.
3. 📌 Correção do plano: gate de destrave é **sm_80 + Linux**, não sm_75.

## Próximos passos possíveis (se retomar)

- Subir PR upstream no khal consertando o import ungated (~1 linha) — **PREPARADO, não submetido**
  (sessão 344b): fix commitado em branch local `fix/cpu-feature-ungated-webgpu-import` do clone
  `%TEMP%\khal-pr`, verificado em 3 configs (`cpu` ✅ antes quebrado, `cpu,derive` ✅, default webgpu ✅).
  Patch exportado em `docs/patches/khal-cpu-feature-fix.patch` — submeter via fork+PR no GitHub web
  quando houver token, ou `git am docs/patches/khal-cpu-feature-fix.patch` num fork local.
- Re-avaliar gates quando o lab tiver GPU sm_80+ e/ou host Linux.
- `cutile-ir` pode ser usado como **formato de referência** para descrever kernels W2A8
  no offline pipeline (documentação viva), sem dependência de runtime.
