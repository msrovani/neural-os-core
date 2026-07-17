# ADR-0049: AMD Compute Multigeração — PSP, KIQ/MES e Kernel Pack

**Data:** 2026-07-16  
**Status:** Proposed  
**Lifecycle:** `por_fazer`  
**Ideia:** #455  
**Complementa:** ADR-0029, ADR-0037, ADR-0047-GPU e ADR-0048  
**Hardware de validação inicial (preferido):** qualquer dGPU AMD com IP Discovery válido; ideal RDNA2 (`gfx1030`) — menos dependência de MES; RDNA3/3.5 como segundo gate  

## 1. Contexto

O neural-os-core precisa de compute AMD quando houver GPU AMD instalada, sem
assumir ROCm/KFD no alvo e sem limitar o produto a uma geração. O código atual
mapeia BAR em `amd.rs`, registra “PM4 futuro” e trata `amd_psp_load` como stub
`NoFirmware`. A detecção por lista curta de PCI DID é insuficiente.

“AMD GPU” não é um protocolo único:

- GFX9–GFX10 (Vega / RDNA1 / RDNA2) usam KIQ+MEC e doorbells clássicos;
- GFX11+ (RDNA3 / RDNA3.5) substituem KIQ por MES (+ MES_KIQ / uni_mes);
- GFX12 (RDNA4) evolui MES/`uni_mes` e ISA WMMA;
- firmware de runtime (PSP SOS/TA, SMU, MEC/MES, SDMA) é **assinado** e
  obrigatório em RDNA moderno;
- QEMU não valida esse caminho — permanece `CPU_FALLBACK`.

A política de coexistência já esboçada em `display_coex::plan_assignment`
permanece: **iGPU/APU = display**, **dGPU = AI**, com fallback CPU. Esta ADR
não inventa PRIME, dma-buf, P2P nem IOMMU.

## 2. Decisão

Adotar arquitetura AMD multigeração espelhando ADR-0048:

1. contrato de compute comum e estável;
2. backends de bring-up por família de **IP** (não por marketing);
3. seleção runtime após IP Discovery;
4. `AMD_KERNEL_PACK` offline por `gfxNNNN`;
5. falha não fatal e fallback CPU obrigatório.

```text
PCI class 0x03 + VID 0x1002
 → BARs + ATOM/VBIOS
 → IP Discovery → AmdIpId { gc, sdma, psp, smu, … }
 → select backend
 ├── AmdKiQBackend     GC 9.x / 10.1 / 10.3  (Vega + RDNA1/2)
 ├── AmdMesBackend     GC 11.0 / 11.5        (RDNA3 + RDNA3.5)
 └── AmdMesV12Backend GC 12.0 / 12.1        (RDNA4)
 → AMD_KERNEL_PACK (HSACO flatten) → PM4 dispatch → fence
 Falha / FW ausente / QEMU → CPU
```

PCI ID é apenas hint de nome. Capacidade compute só após discovery + probe
seguro.

## 3. Contrato comum

```rust
pub trait AmdComputeBackend {
    fn capabilities(&self) -> &AmdCapabilities;
    fn initialize(&mut self) -> Result<(), AmdError>;
    fn upload_program(&mut self, image: &AmdKernelImage) -> Result<ProgramId, AmdError>;
    fn upload_buffer(&mut self, bytes: &[u8]) -> Result<GpuBuffer, AmdError>;
    fn dispatch(&mut self, job: &ComputeJob) -> Result<Fence, AmdError>;
    fn wait(&mut self, fence: Fence, timeout_ticks: u64) -> Result<(), AmdError>;
    fn quarantine(&mut self, reason: AmdError);
}
```

Cortex não conhece PSP, MES, PM4 nem `gfx_id`. Tipos são contrato arquitetural.

### 3.1 Capacidades

`AmdCapabilities` deve incluir:

- `gc` IP version (`IP_VERSION(maj,min,rev)`);
- alvo LLVM/`gfxNNNN` e wave size (32/64);
- scheduler (`KiQ` | `Mes` | `MesV12`);
- VRAM medida (não estimada pelo nome comercial);
- features: `dot4` / `wmma_i8` / `mfma_i8` quando presentes;
- limites VGPR/SGPR/LDS/kernarg e grid.

Doorbell **não** é offset único `BAR+0x1B0`. Layouts vêm de tabelas por geração
(referência comportamental: `amdgpu_doorbell.h`).

## 4. Bring-up headless (dGPU)

Ordem canônica (comportamento amdgpu / docs oficiais):

```text
PCI BARs + bus master
 → ATOM/VBIOS
 → IP Discovery (VRAM TMR / DRIVER_SCRATCH / fallback .bin)
 → PSP (SOS/TA) — autentica FW no TMR
 → SMU — power/clocks
 → GMC — VRAM, GART, hubs, VMID
 → SDMA — cópia host↔VRAM (GMC antes do ring)
 → CP: KIQ+MEC (≤GFX10) OU MES (≥GFX11)
 → MQD/HQD + doorbell + ring PM4
 → DISPATCH + fence/EOP → canário
```

**Headless:** não inicializar DCN/display na dGPU. Display permanece na iGPU
(ou GOP/UEFI FB) via política de coexistência.

Não declarar “PM4 compute OK” sem canário + fence em silicon.

## 5. AMD Kernel Pack

Host compila; alvo **não** embute ROCm/HSA/HIP runtime.

```text
HIP/C/asm / LLVM IR
 → clang -target amdgcn-amd-amdhsa -mcpu=gfxNNNN -mcode-object-version=5 -nogpulib
 → lld → HSACO (COV4/5)
 → extrator host (resolve relocs)
 → AMD_KERNEL_PACK
 → FAT32/NeuralFS
```

Cada entrada mínima:

- `gfx_id`, COV pin (MVP: **COV5** ou COV4; evitar generics COV6 no MVP);
- código `.text` já linked;
- `kernel_descriptor_t` (KD) — dispatch aponta ao KD;
- VGPR/SGPR/LDS/kernarg/private;
- op / nome / golden_vector_id;
- hash + assinatura Ed25519.

### 5.1 Toolchains (ranking)

1. **clang/LLVM AMDGPU → HSACO → pack + PM4** (primário);
2. HIP host + builtins/rocWMMA patterns → mesmo pack;
3. CK/Tensile/Triton só como geradores/referência no host;
4. **Rejeitar no alvo:** ROCr/HSA, SPIR-V JIT (`amdgcnspirv`), AMDIL, device-libs.

ACO/Mesa é diagnóstico aberto, não contrato de produto.

### 5.2 Perfis BitNet

Não há opcode “ternary”. Empacotar `{−1,0,+1}` offline e usar:

- RDNA2: GEMV/dot genérico + scalar/int8;
- RDNA3+: WMMA i8 / sudot4 para prefill; Wave32 GEMV para decode;
- CDNA (opcional, fora do MVP notebook): MFMA i8.

## 6. Coexistência display / compute

Invariantes (compartilhados com ADR-0048/0050):

1. **DisplayOwner** único escreve no scanout/FB (iGPU/APU ou GOP);
2. **ComputeOwner** ≠ DisplayOwner quando houver dGPU AMD/NVIDIA/Intel;
3. falha de compute → CPU; **nunca** reset da GPU de display;
4. quarantine é local à dGPU;
5. AI→UI, se existir, via **host bounce** (RAM), não P2P implícito;
6. APU solo: `SingleGpu` — display prioritário; IA só com cota/timeout.

Memória heterogênea (ReBAR, IOMMU, PASID, P2P) fica em escada futura e **não**
é pré-requisito desta ADR. Até lá: M0 = bounce CPU↔VRAM.

## 7. Integração com Cortex

Igual ADR-0048: `TensorOp` → work queue → variante AMD do pack → fence →
validação amostral. Preferência de vendor quando várias dGPUs: política do
`GpuAssignment` (NVIDIA lab atual primeiro; AMD se for a única dGPU compute).

## 8. Segurança e tolerância a falhas

- blobs PSP/SMU/MES/SDMA redistribuídos **inalterados** (linux-firmware);
- pack assinado; rejeitar `gfx_id`/COV mismatch;
- timeout / VM fault / hang → quarantine + CPU;
- canário `vector_add` antes de INT8/WMMA;
- QEMU e discovery falho → `CPU_FALLBACK`;
- nenhum erro AMD é fatal ao boot.

## 9. Metas e critérios de aceite

### P0 — Detecção

- [ ] IP Discovery + `GC_HWIP` no path de probe;
- [ ] publicar `KiQ` / `Mes` / `CpuFallback` honestamente;
- [ ] remover dependência de lista DID como única fonte.

### P1 — Kernel pack host

- [ ] gerar pack com ao menos um `gfx1030` ou `gfx1100`;
- [ ] rejeitar assinatura/ISA incompatíveis;
- [ ] compiladores fora do bare-metal.

### P2 — Bring-up

- [ ] PSP carrega SOS/TA e autentica FW mínimo;
- [ ] GMC+SDMA+sched (KIQ ou MES) sem fault;
- [ ] logs: blob presente ≠ engine pronto.

### P3 — Primeiro compute

- [ ] `vector_add` + fence/EOP;
- [ ] fault injection → CPU, display intacto.

### P4 — BitNet

- [ ] microkernel INT8/dot ou WMMA bit-exato vs CPU;
- [ ] GEMV W2A8 packed; benchmark honesto vs AVX2.

### P5 — Multigeração

- [ ] segundo `gfx_id` (ex. RDNA3 após RDNA2) no mesmo contrato;
- [ ] nenhum offset KIQ usado no path MES.

## 10. Consequências

### Positivas

- AMD deixa de ser stub eterno e ganha rota verificável;
- IP-first evita tabela PCI frágil;
- Kernel Pack espelha NVIDIA e permite evoluir kernels sem rebuild do OS;
- política iGPU/dGPU preservada.

### Negativas

- dois/três bring-ups (KIQ/MES/MES12) de alta complexidade;
- firmware assinado obrigatório — sem “zero blobs”;
- validação depende de HW real AMD.

### Riscos aceitos

- RDNA3.5 APU compartilha SMU/PSP com plataforma — headless dGPU é mais limpo;
- generics COV6 podem cortar dots — MVP usa gfx exato;
- desempenho inicial pode não bater AVX2.

## 11. Alternativas rejeitadas

1. **ROCm/KFD no OS** — incompatível com `no_std` (IDEA #343 já descartada como runtime).
2. **Lista PCI como única detecção** — anti-padrão frente a IP Discovery.
3. **Doorbell/offset genérico cross-gen** — causa hang silencioso.
4. **Declarar PM4 OK por BAR map** — presença de MMIO ≠ compute.
5. **PRIME/P2P como MVP** — fora de escopo; host bounce basta.
6. **SPIR-V JIT no alvo** — exige lowerer tipo comgr/ROCm.

## 12. Gaps vs código atual

| Item | Hoje | Esta ADR |
|------|------|----------|
| Detecção | DID curtos | IP Discovery + GC |
| `amd.rs` | BAR map | backends KIQ/MES |
| `amd_psp_load` | `NoFirmware` | SOS/TA + TMR |
| Doorbell | `+0x1B0` | layout por gen |
| FW pack FAT | foco NVIDIA | espelho `amdgpu/*` por IP |

## 13. Fontes

- amdgpu discovery: <https://github.com/torvalds/linux/blob/master/drivers/gpu/drm/amd/amdgpu/amdgpu_discovery.c>
- amdgpu driver core: <https://docs.kernel.org/gpu/amdgpu/driver-core.html>
- LLVM AMDGPUUsage: <https://llvm.org/docs/AMDGPUUsage.html>
- GPUOpen ISA / MES overview: <https://gpuopen.com/amd-gpu-architecture-programming-documentation/>
- Machine-readable ISA: <https://gpuopen.com/machine-readable-isa/>
- Mesa RADV: <https://docs.mesa3d.org/drivers/radv.html>
- linux-firmware `amdgpu/`: <https://gitlab.com/kernel-firmware/linux-firmware>
- TheRock supported GPUs: <https://github.com/ROCm/TheRock/blob/main/SUPPORTED_GPUS.md>
- TrustOS (Polaris SDMA lesson): <https://github.com/nathan237/TrustOS>
- BitNet 2B4T packing: <https://arxiv.org/abs/2504.12285>
