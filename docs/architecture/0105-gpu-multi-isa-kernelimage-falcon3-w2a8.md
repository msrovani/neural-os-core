# ADR-0105 — GPU Multi-ISA KernelImage + Falcon3 W2A8 AIOS

**Status:** Accepted (MVP código)  
**IDEA:** #550 (atualizada), #536  
**SESSION:** 361

## Contexto

KernelPack NKP1 existia, mas QMD/Walker usavam magic numbers (`regs=16`, `param=0x140`) e o lab assumia sm_61-only / BitNet-2B shapes. Aceite silício = Pascal + Gen9; Falcon3 1.58bit (1B/3B/7B/10B) é o LLM.

## Decisão

1. **KernelImage** parseado de CUBIN/HSACO/zebin — `regs`/`shared`/`param_*` vêm do blob.
2. **AIOS adapt** (`aios_adapt`): Observe→Plan→Act→Verify→Remember escolhe SKU Falcon3 + dual iGPU/dGPU + OpProfile sem hardcode.
3. **Ready** só com pack verified + canário golden; stub pack ≠ Ready.
4. **gpu_ternary** valida pack+image+shape Falcon3; device dispatch = Layer S (None → CPU ladder).

## Consequências

- Packers: `--op w2a8` + multi-SM; mkfat32 embute `NKP_W2A8_*`.
- AMD: Degrau + parse HSACO; golden silício aberto.
- #550: produtores Rust/nvcc cobrem sm_61+ (não só Ampere).

## Não-fazer

PTX JIT no kernel; fingir aceleração sem golden; hardcodar shapes BitNet-2B.
