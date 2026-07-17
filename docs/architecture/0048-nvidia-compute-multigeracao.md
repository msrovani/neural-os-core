# ADR-0048: NVIDIA Compute Multigeração — ACR, GSP e Kernel Pack

**Data:** 2026-07-16  
**Status:** Proposed  
**Lifecycle:** `por_fazer`  
**Ideia:** #454  
**Complementa:** ADR-0029, ADR-0037 e ADR-0047-GPU  
**Hardware de validação inicial:** NVIDIA GTX 1050 Mobile (Pascal/GP107, `sm_61`)  

## 1. Contexto

O neural-os-core deve usar compute na GPU NVIDIA disponível, sem limitar o
produto à Pascal presente no notebook de desenvolvimento. A implementação atual
detecta algumas placas por PCI ID, mapeia BARs e possui PoCs de PFIFO, WPR e
VRAM. Entretanto, ainda não existe execução verificável de um programa de
compute NVIDIA.

“NVIDIA” não representa um único protocolo de inicialização:

- Maxwell, Pascal e Volta usam o caminho clássico Nouveau, com ACR/WPR,
  FECS/GPCCS, MMU, instância de canal e GPFIFO;
- Turing e gerações posteriores podem usar GSP-RM, com firmware e protocolo
  diferentes;
- classes compute, QMD, formato de código e recursos variam por geração;
- QEMU não emula esse hardware e deve permanecer em fallback CPU.

Os blobs FECS/GPCCS já presentes são necessários, mas não tornam o GR pronto
isoladamente. O PoC atual não implementa toda a cadeia ACR, GR, MMU, runlist e
canal exigida pelo hardware.

Também não é necessário escrever SASS manualmente para o primeiro MVP.
Programas podem ser compilados offline para a ISA exata da placa e carregados
pelo kernel sem CUDA runtime no alvo.

## 2. Decisão

Adotar uma arquitetura NVIDIA multigeração com:

1. contrato de compute comum e estável;
2. backends de bring-up separados por família;
3. seleção de backend e programa em runtime;
4. pacote próprio de kernels pré-compilados por ISA;
5. falha não fatal e fallback CPU obrigatório.

```text
PCI/PMC probe
    │
    ├── LegacyAcrBackend ── Maxwell/Pascal/Volta
    │       ACR → FECS/GPCCS → GR/MMU → GPFIFO → QMD
    │
    └── GspBackend ──────── Turing/Ampere/Ada/Blackwell+
            GSP-RM → VM/channel → geração específica de QMD

KernelPack: imagem exata da ISA → upload → dispatch → semaphore
Falha/ausência/QEMU → CPU AVX2 ou scalar
```

Pascal é o primeiro gate em hardware real porque é a máquina disponível. Ela
valida o backend pré-GSP, mas não define o limite da interface pública.

## 3. Contrato comum

O módulo NVIDIA deve expor operações independentes da geração:

```rust
pub trait NvidiaComputeBackend {
    fn capabilities(&self) -> &NvidiaCapabilities;
    fn initialize(&mut self) -> Result<(), NvidiaError>;
    fn upload_program(&mut self, image: &KernelImage) -> Result<ProgramId, NvidiaError>;
    fn upload_buffer(&mut self, bytes: &[u8]) -> Result<GpuBuffer, NvidiaError>;
    fn dispatch(&mut self, job: &ComputeJob) -> Result<Fence, NvidiaError>;
    fn wait(&mut self, fence: Fence, timeout_ticks: u64) -> Result<(), NvidiaError>;
    fn quarantine(&mut self, reason: NvidiaError);
}
```

Os tipos são contrato arquitetural, não assinatura final de implementação.
Nenhum chamador em Cortex deve conhecer ACR, GSP, PFIFO ou uma versão de QMD.

### 3.1 Capacidades

`NvidiaCapabilities` deve ser produzido por detecção, não por suposição:

- chipset e família;
- compute capability/ISA suportada;
- backend de firmware (`LegacyAcr` ou `Gsp`);
- classe de canal e compute;
- perfil QMD;
- DP4A, FP16, Tensor Core e atomicidade disponíveis;
- VRAM realmente medida por BAR/VBIOS, não estimada pelo nome comercial;
- limites de registers, shared memory, grid e block.

Uma placa desconhecida pode ser identificada por `PMC_BOOT_0`, VBIOS e tabelas
geradas, sem precisar adicionar manualmente cada device ID. Desconhecido não
significa `has_compute=true`: a capacidade só é publicada após probe seguro.

## 4. Backends de inicialização

### 4.1 `LegacyAcrBackend`

Alvo inicial: GP107 do notebook.

O backend deve portar por comportamento verificável o caminho Nouveau:

1. confirmar VBIOS/DEVINIT e PCI bus mastering;
2. configurar instmem e MMU Pascal v2;
3. construir WPR, headers LSB e descritores Falcon;
4. carregar ACR HS (`bl.bin`, `ucode_load.bin`);
5. autenticar e iniciar FECS/GPCCS com suas assinaturas;
6. aplicar `sw_ctx`, `sw_nonctx`, `sw_bundle_init` e `sw_method_init`;
7. criar instance block, RAMFC, runlist e canal GPFIFO;
8. bindar a classe compute da geração;
9. liberar dispatch somente após canário e semaphore válidos.

Para GP107/GP108, a referência é `PASCAL_COMPUTE_B` (`0xC1C0`) e o perfil QMD
Pascal correspondente aos headers oficiais. Números devem vir de módulos
tipados por geração, não de offsets genéricos espalhados.

Não será alegado “ACR completo” enquanto a cadeia acima não tiver evidência em
hardware real.

### 4.2 `GspBackend`

Alvo: Turing e posteriores, conforme suporte confirmado por chipset e firmware.

Esse backend deve reutilizar conceitos e protocolos públicos de
`open-gpu-kernel-modules`, Nouveau GSP e Nova, sem importar dependências Linux
para o kernel. Ele possui lifecycle, mensagens, VM e criação de canais próprios;
não deve herdar offsets ou estruturas Pascal.

Quando uma geração aceitar mais de um caminho, a política é:

1. GSP quando suportado e validado;
2. backend alternativo apenas se implementado explicitamente para aquela
   geração;
3. nunca “tentar offsets” de outra família.

## 5. NVIDIA Kernel Pack

Programas de compute serão gerados no host e empacotados em formato próprio,
versionado e validável. O alvo bare-metal não conterá `nvcc`, `ptxas`, NVRTC,
CUDA runtime ou JIT PTX.

```text
fonte CUDA/SPIR-V/NIR
   → compilador host
   → CUBIN/SASS por ISA
   → extrator host
   → NVIDIA_KERNEL_PACK
   → FAT32/NeuralFS
```

Cada `KernelImage` deve conter, no mínimo:

- versão do pack e ABI;
- ISA/compute capability exata;
- classe compute e perfil QMD compatíveis;
- código e constant banks;
- parâmetros, alinhamentos e relocações já resolvidas;
- register count, shared/local memory e barreiras;
- grid/block permitidos;
- hash e assinatura Ed25519;
- golden vectors e identificador do algoritmo.

O runtime seleciona somente imagem compatível. Não haverá fallback silencioso
entre ISAs nem PTX JIT no kernel.

### 5.1 Toolchains

- Pascal/Maxwell/Volta: CUDA 12.9 é o último toolkit NVIDIA para compilação
  offline dessas arquiteturas;
- Turing+: usar toolkit atual compatível e imagens específicas;
- NAK/NIR pode ser adotado futuramente como compilador host aberto;
- CuAssembler e engenharia reversa de SASS são ferramentas de diagnóstico e
  otimização pós-MVP, não dependências do boot.

Artefatos CUDA serão código próprio voltado a GPUs NVIDIA. Ferramentas do SDK
permanecem no host e não são redistribuídas com o OS.

## 6. Perfis de kernel

Uma operação lógica pode possuir várias imagens:

- `scalar-int8`: compatibilidade e canário;
- `w2a8-dp4a`: decode ternário em `sm_61+`;
- `tensor-int8/fp16`: Turing+ quando a capacidade estiver presente;
- variantes por tile, shared memory e VRAM.

O primeiro programa é `vector_add` com semaphore. O segundo é um microkernel
DP4A com golden vector. Somente depois entra GEMV W2A8 do BitNet.

O formato ternário packed permanece em VRAM. Descompactar os 2B parâmetros para
INT8 integralmente não cabe na GTX 1050 de 2 GB e desperdiça banda.

## 7. Integração com Cortex

Cortex envia operações sem conhecer o fabricante:

```text
TensorOp::BitLinearW2A8
    → GPU work queue
    → variante NVIDIA compatível
    → fence
    → validação amostral
    → resultado
```

Prefill, decode, RMSNorm, softmax e attention são incorporados
incrementalmente. H2O/PagedAttention continuam política de Cortex; o backend
recebe buffers e descritores já validados.

O n-gram speculative decoding não deve ser confundido com DP4A: n-gram reduz
quantos forwards são necessários; DP4A acelera os forwards executados.

## 8. Segurança e tolerância a falhas

- toda entrada do kernel pack possui limites e overflow checks;
- imagens exigem hash e assinatura antes do upload;
- endereços GPU devem pertencer a mappings autorizados;
- timeout, MMU fault, PGRAPH fault ou semaphore inválido põem o backend em
  quarentena até reboot ou recuperação explícita;
- o primeiro dispatch após boot é um canário destrutivamente isolado;
- resultado de novos kernels é comparado com CPU em amostras antes de promoção;
- QEMU, GPU desconhecida ou firmware ausente resultam em `CPU_FALLBACK`;
- nenhum erro de GPU é fatal para o boot.

## 9. Metas e critérios de aceite

A antiga meta de 50 μs/token da ADR-0047-GPU não é critério end-to-end. Na GTX
1050, ler aproximadamente 590 MB de pesos por token já impõe milissegundos pelo
limite de banda. A meta passa a ser medida contra o baseline da mesma máquina.

### P0 — Detecção

- [ ] identificar família por hardware, não apenas por lista curta de PCI IDs;
- [ ] medir VRAM e publicar capabilities honestas;
- [ ] selecionar `LegacyAcr`, `Gsp` ou `CpuFallback`.

### P1 — Kernel pack host

- [ ] gerar e validar pack com uma imagem `sm_61`;
- [ ] rejeitar versão, assinatura, offsets e ISA incompatíveis;
- [ ] manter compiladores fora do target bare-metal.

### P2 — Pascal bring-up

- [ ] ACR HS autentica FECS/GPCCS no GP107;
- [ ] GR/MMU/channel/runlist ficam prontos sem fault;
- [ ] logs distinguem blob presente, autenticado e engine pronto.

### P3 — Primeiro compute

- [ ] `vector_add` produz golden vector;
- [ ] QMD e semaphore encerram sem timeout;
- [ ] fault injection cai para CPU e mantém o boot.

### P4 — BitNet DP4A

- [ ] microkernel DP4A é bit-exato contra CPU;
- [ ] GEMV W2A8 usa pesos packed residentes em VRAM;
- [ ] GPU entrega pelo menos 1,5× o throughput CPU no notebook ou registra
  resultado negativo honesto;
- [ ] benchmark informa bytes lidos, latência e tokens/s, sem extrapolação.

### P5 — GSP multigeração

- [ ] uma placa Turing+ executa o mesmo `vector_add` pelo `GspBackend`;
- [ ] o mesmo contrato seleciona outra imagem do pack;
- [ ] nenhuma estrutura Pascal é usada no caminho GSP.

### P6 — Pipeline

- [ ] decode end-to-end medido em hardware real;
- [ ] meta indicativa para o modelo ~590 MB: 8–25 ms/token, revisada pelos
  resultados reais;
- [ ] n-gram/Medusa reportados separadamente do ganho bruto do kernel.

## 10. Consequências

### Positivas

- suporte NVIDIA não fica preso ao notebook atual;
- Pascal recebe uma rota realista e verificável;
- placas novas usam GSP sem contaminar o backend legado;
- kernels podem evoluir sem recompilar o OS;
- CPU fallback preserva portabilidade e QEMU.

### Negativas

- dois bring-ups de alta complexidade precisam ser mantidos;
- cada geração exige firmware, QMD e imagens próprias;
- validação final depende de hardware real;
- Pascal aberto pode operar em boot clocks, limitando desempenho.

### Riscos aceitos

- CUDA 12.9 precisa ser preservado como toolchain legado de build;
- o primeiro resultado Pascal pode não superar AVX2 devido a clocks e banda;
- GSP não elimina a necessidade de VM, canais e sincronização corretos;
- engenharia reversa manual do ISA permanece opcional e de alto risco.

## 11. Alternativas rejeitadas

1. **Driver único “Pascal+”:** protocolos divergem e offsets compartilhados
   causariam faults ou corrupção.
2. **Somente GSP:** exclui o notebook Pascal e não satisfaz o hardware atual.
3. **Somente Pascal:** não atende à exigência de usar qualquer NVIDIA instalada.
4. **Portar CUDA runtime:** incompatível com `no_std` e desnecessário para
   imagens offline.
5. **PTX JIT no kernel:** amplia TCB, memória e complexidade antes do primeiro
   compute.
6. **SASS manual como MVP:** custo e risco maiores que CUBIN/NAK offline.
7. **Declarar sucesso por upload de firmware:** presença de blobs não prova
   autenticação, GR pronto ou compute.

## 12. Fontes

- NVIDIA Open GPU Docs: <https://github.com/NVIDIA/open-gpu-doc>
- NVIDIA Open GPU Kernel Modules: <https://github.com/NVIDIA/open-gpu-kernel-modules>
- Nouveau/NVKM: <https://github.com/torvalds/linux/tree/master/drivers/gpu/drm/nouveau>
- Mesa NVK/NAK: <https://gitlab.freedesktop.org/mesa/mesa>
- linux-firmware NVIDIA: <https://gitlab.com/kernel-firmware/linux-firmware/-/tree/main/nvidia>
- Pascal compute RFC: <https://lists.freedesktop.org/archives/mesa-dev/2017-April/152705.html>
- CUDA architecture support: <https://developer.nvidia.com/blog/navigating-gpu-architecture-support-a-guide-for-nvidia-cuda-developers/>
- BitNet GPU W2A8: <https://github.com/microsoft/BitNet/tree/main/gpu>
- BitNet b1.58 2B4T: <https://arxiv.org/abs/2504.12285>
- T-MAC: <https://arxiv.org/abs/2407.00088>
- RSR-core: <https://github.com/UIC-InDeXLab/RSR-core>

