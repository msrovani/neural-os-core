# Dead Ends & Blockers — Sprints v1.x

Este documento registra bloqueios técnicos, NDA, firmware signing, e outras
barreiras que impedem implementações diretas. Cada entrada documenta:
- O que foi tentado/pesquisado
- O bloqueio real (não sintoma)
- Soluções laterais encontradas
- O que DESBLOQUEARIA o item

---

## CRM-001: GPU Compute (NVIDIA PFIFO + Shader)

### O que queremos
Executar `C = A × B` (matmul ternário) na GPU NVIDIA via PFIFO channel.

### O que funciona
- `pushbuffer_submit()` — submete comandos via GPFIFO entry + doorbell
- DMA handshake VRAM via BAR2 (`cpu_to_vram` + `vram_to_cpu`)
- CPU fallback com AVX2 (via `tensor.rs`)

### Bloqueio real (não sintoma)
**A GPU não executa nosso código porque:**
1. **ACR firmware não está carregado** — sem os blobs signed, VRAM fica em P8 mode (toda leitura retorna lixo). O Falcon (microcontrolador da GPU) precisa de firmware autenticado para habilitar acesso completo à VRAM.
2. **Não temos um shader binary** — a ISA das GPUs NVIDIA (CUBIN/PTX) e Intel (GEN) é proprietária, coberta por NDA. Sem o shader, a GPU não tem "o que executar".

### Pesquisa lateral

| Fonte | Resultado |
|-------|-----------|
| `linux-firmware.git` | Blobs FECS+GPCCS disponíveis desde 2017, MIT license (~100KB cada). Download via `git clone`, não HTTP (kernel.org bloqueia). |
| Driver nouveau (Linux) | Implementa WPR loading completo. Código aberto em `drivers/gpu/drm/nouveau/nvkm/subdev/acr/`. ~150 LOC relevantes. |
| Microsoft `BitNet/gpu/` | Kernels CUDA open-source para BitNet b1.58. Exigem NVIDIA driver + CUDA runtime — não rodam em bare-metal. |
| `microsoft/BitNet-b1.58-2B-4T-gguf` | Modelo em formato GGUF. Kernel tem `GgufBackedModel` que potencialmente carrega. |
| Intel i915 driver | MEDIA_OBJECT dispatch code aberto. GEN ISA requer engenharia reversa (~2 semanas). |

### Soluções laterais funcionais

| Solução | Status | Ganho |
|---------|--------|-------|
| CPU matmul com AVX2 | ✅ Já conectado via `tensor.rs` | 2-6× sobre scalar |
| DMA pipeline (dados, não compute) | ✅ `pushbuffer_submit` + VRAM | Pipeline provado |
| `gpu_blit()` (cópia 2D) | ✅ Intel blitter funciona | Framebuffer acceleration |

### O que DESBLOQUEARIA

**Prioridade 1 (firmware):**
```bash
git clone --depth 1 https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git
# Firmwares: nvidia/gp108/acr/fecs_*.bin + gpccs_*.bin (~80KB total)
cp linux-firmware/nvidia/gp108/acr/*.bin target/firmware/nvidia/
```
Depois: implementar WPR loading em `gpu/firmware.rs` (~150 LOC, baseado no nouveau).
Resultado: VRAM funcional, PFIFO pode executar comandos.

**Prioridade 2 (shader):**
Sem NDA, a única alternativa é engenharia reversa da ISA a partir de:
- Driver i915 (Intel, código aberto)
- Mesa/Gallium (NVIDIA, via nouveau)
- Microsoft BitNet kernels CUDA (precisa de NV driver)
Resultado: GPU executa matmul real.

---

## CRM-002: PCA9685 / Servo Driver I²C (referência futura)

Documentar blockers de hardware aqui conforme forem identificados.

---

## Glossário

| Sigla | Significado |
|-------|-------------|
| ACR | Authenticated Code Radix (NVIDIA secure boot) |
| WPR | Wide Payload Register (região de memoria para firmware) |
| FECS | Falcon Engine Code Segment (firmware NVIDIA) |
| GPCCS | GPUFIFO Command Code Segment (firmware NVIDIA) |
| PFIFO | Push FIFO (canal de comandos NVIDIA) |
| GEN | Intel Graphics Execution Native (ISA) |
| NDA | Non-Disclosure Agreement |
