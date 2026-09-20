# GPU KernelPack (NKP1) — host only

`nvcc` / `ocloc` / `clang amdgcn` **nunca** rodam no bare-metal. Só geram blobs
no host → FAT → OS faz parse/`KernelImage`/QMD.

## Lab host (SESSION_361+)

| Item | Valor |
|------|-------|
| GPU | RTX 3050 Laptop (`compute_cap=8.6`) → `IsaTag::Sm86` |
| Toolkit | **CUDA 13.4** (`nvcc` @ `CUDA\v13.4\bin`) + VS 2022 BuildTools |
| Artefatos reais | `target/nkp-lab/NKP_SM{75,80,86,89}.BIN` + `NKP_W2A8_SM*.BIN` |
| Payload | IR=`Cubin` (ELF `7F454C46`), **não** stub |

Intel/AMD neste host: **sem** `ocloc`/`clang-amdgcn` → packs Gen9/gfx1030 continuam `CPU_*_STUB` (honesty).

## Requisitos

| Vendor | Toolkit | Notas |
|--------|---------|--------|
| NVIDIA | **CTK 12.9** p/ ISA ≤ `sm_70`; **CTK 13+** p/ `sm_75+` (lab=13.4) | `nvcc -cubin -arch=sm_XX` |
| Intel | `ocloc` (IGC) | Gen9=`skl`, Arc=`dg2` |
| AMD | clang `amdgcn-amd-amdhsa` / ROCm | gfx1030+ |

Sem toolkit → payload `CPU_*_STUB` (OS = `CpuOnly`, nunca Ready falso).

## Comandos

```powershell
# Lab Ampere/Ada — vector_add + W2A8 → target/nkp-lab/ (ADR-0105 B0)
.\tools\pack_nkp_lab.ps1
# opcional: -IncludeSm61 -IntelStub -AmdStub

# PATH: CUDA bin + vcvars64.bat (MSVC host compiler)
$env:Path = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.4\bin;" + $env:Path

# Manual por SM
foreach ($sm in 'sm_75','sm_80','sm_86','sm_89') {
  python tools/pack_nvidia_kernels.py --sm $sm --op vector_add --unsigned -o "target/nkp-lab/NKP_$($sm.Replace('sm_','SM')).BIN"
  python tools/pack_nvidia_kernels.py --sm $sm --op w2a8 --unsigned -o "target/nkp-lab/NKP_W2A8_$($sm.Replace('sm_','SM')).BIN"
}
```

Assinatura opcional: `NKP_SIGNING_SEED_HEX` (64 hex) ou `--unsigned` + `promote_with_session` no boot.

## FAT

`mkfat32` lê `target/nkp-lab/` (`NKP_SM86`, `NKP_W2A8_SM86`, SM80/89, …). Regenerar imagem após re-pack.

## Aceite honesty

| Ambiente | Esperado |
|----------|----------|
| QEMU / stub pack | `CpuOnly`, slog fallback |
| Metal + pack CUBIN real + canário | `Ready`, `isa=sm_86`, `profile=` |
| W2A8 device | AWAITING_HW até fence+golden; senão CPU ladder |
| Intel/AMD sem toolkit | stub permanente até ocloc/ROCm |
