# ADR-0112: BAR Compute — VRAM universal como memória de pesos W2A8 (vendor-agnostic)

**Data:** 2026-09-29
**Status:** Implemented (stage `Mapped`: código + canário + wiring; `ComputeDevice` = upgrade incremental)
**Lifecycle:** `fazendo`
**Sprint:** v1.9.99-s418 → v2.0.0
**Relacionadas:** ADR-0047-GPU (SASOS), ADR-0057 (dispatch), ADR-0087 (MHI DMA multi-tier), ADR-0105 (W2A8 canônico), ADR-0048/49/50 (compute multigeracao — o "muro")

---

## 1. Problema

O Falcon3 decode (GEMV W2A8) roda 100% no host: os pesos (544MB no 1B, ~1.5GB no 3B W2) morrem no heap bump do kernel — janela endereçável de ~2030MB (SESSION_415-417: OOM → #PF → hlt no AP). O caminho "executar a GEMV na GPU" (ADR-0048/49/50) exige firmware de vendor (ACR/PSP/GuC), command streamer por geração e ISA de shader (CUBIN/HSACO/zebin) — a muralha que mantém o compute em `AWAITING_HW` há dezenas de sessões, em qualquer vendor.

## 2. O insight (fora da caixa, por cima do muro)

**A VRAM não precisa ser destino de compute para servir o compute.** A aperture BAR é, para a CPU, memória PCIe comum: o que o kernel grava nela, lê de volta no mesmo barramento — independentemente de vendor, geração, firmware ou driver. Três ganhos reais sem executar NADA na GPU:

1. **RAM liberada (o ganho imediato):** pesos residentes na VRAM saem do heap bump → decode longo deixa de OOMar; a janela de ~2030MB passa a servir runtime (TTS, bus, memória, KV) em vez de pesos.
2. **Prefetch overlap (o ganho de latência):** lendo pesos via BAR com prefetch ahead (`prefetcht0` do tile j+1), a latência PCIe (~1µs) esconde atrás do compute (~50µs/tile) — o barramento busca enquanto a CPU calcula; duplo buffer de ativações no ring mantém o fluxo.
3. **Caminho único vendor-agnostic:** a MESMA matemática (quant signed, round-half, coluna-major i8) sobre bytes idênticos — só muda o endereço de origem (BAR em vez de RAM). O device path específico (CE Pascal 0xc1b5 / SDMA RDNA / BCS Gen9) pluga DEPOIS como **upgrade** com o mesmo contrato de buffers.

## 3. Estágios honestos (`vram_stream::StreamStage`)

| Estágio | Significado | Gate |
|---|---|---|
| `Off` | Sem aperture/canário — streaming inexistente | `init_vram_tier` falhou (D3/barramento/QEMU VGA) |
| `Mapped` | **Pesos residem na VRAM**; GEMV no host lê via BAR; compute continua CPU/SMP p/ FLOPS | Canário round-trip golden (write pattern 64KB + read-back + TSC bandwidth) |
| `StreamsW2a8` | Duplo buffer X (ativações) no ring + overlap medido | 2º canário com streaming |
| `ComputeDevice` | Motor por-vendor **executa** a GEMV | CE/SDMA/BCS golden (ADR-0087 fase 4b, ADR-0105 B4) |

Honestidade (SESSION_354/387): `note_gpu_compute` **não** é chamado no stage `Mapped` — SYS_HEALTH gpu segue UNKNOWN (a GPU não fez a conta); o WIN medido é **RAM liberada + estabilidade**, nunca tok/s inventado. Sem canário, sem lane.

## 4. Implementação

| Arquivo | Papel |
|---|---|
| `k_hal/gpu/vram_stream.rs` | Stream ring (4 slots × 8×9216 i8), canário de bandwidth com TSC, telemetria lock-free (`BYTES_RESIDENT/STREAMED`, `CANARY_GBPS_X10`), `stream_x_slot` (host→VRAM com sfence) |
| `k_hal/gpu/bar_compute.rs` | `upload_layer_weights` (determinístico, alinhado 2MB após o ring), `SHAPE_INDEX` (k,n)→mat, `gemv_from_vram` (quant host + read via `read_volatile` no BAR + prefetcht0 ahead), `vram_ternary` (entry TernaryFn, slot alternante = duplo buffer) |
| `cortex/compute.rs` | Novo lane `VRAM_TERNARY` no dispatcher — **antes** do GPU device (não exige firmware), depois do NPU |
| `cortex/cortex.rs` | Seam `register_vram_upload_hook` + `current_model_layers_snapshot()` (refs sem clone) — chamado no `set_model` |
| `main.rs` | Após `init_vram_tier` OK: `init_bar_compute(g)` + `register_vram_upload_hook(on_model_loaded)`; `register_compute_if_ready` registra o lane se `bar_compute_enabled()` |

Compatibilidade vendor (zero código por vendor no stage Mapped): NVIDIA (BAR1 aperture), AMD dGPU (BAR0 = VRAM — `detect` já resolve), Intel iGPU (DRAM compartilhada — `is_integrated` → `vram_size=0` → `Off` honesto, sem ganho inventado), dGPU em D3 (`init_vram_tier` recusa antes de tocar — SESSION_260).

## 5. Reconciliação com ADRs existentes

- **ADR-0087 §2.0.1:** SASOS (ponteiro) vs CE (bulk) — BAR Compute é a **terceira natureza**: residência + leitura de streaming. Reusa o mapeamento UC do `init_vram_tier` e o buddy (`vram_alloc`), alimenta a telemetria do MHI.
- **ADR-0105:** o contrato de buffers W2A8 (`W2a8DeviceBuffers`, quant signed, golden host) é preservado byte-a-byte; o device path (B4) continua sendo o upgrade final.
- **ADR-0057:** o lane VRAM entra na escada de dispatch como novo degrau (`N_VRAM` na telemetria).
- **SESSION_415-417:** o upload de pesos para VRAM é a resposta estrutural ao OOM do heap bump (544MB-1.5GB saem da janela de ~2030MB).

## 6. Verificação

- `cargo check --release -p boot` (alvo bare-metal): **0 erros**.
- Testes: k-hal **232/232** (inclui ring plan, gate honesto host, upload recusado sem stage); cortex **105/105**; k-nano **232/232**; hermes **257/257** (`--test-threads=1`).
- Crashes pré-existentes do harness host (não regressão, provado via stash): `k_ai` STATUS_STACK_BUFFER_OVERRUN, `hermes` STATUS_PRIVILEGED_INSTRUCTION em paralelo.
- Lab (residual): medir round-trip real na GTX 1050 (ReBAR off ≈ 256MB — 3B família up/down ~9216×3072 i8 packed ≈ 7MB/matr × 7 famílias ≈ 50MB cabe; 1B inteiro cabe) + efeito no decode longo (estabilidade vs tok/s).

## 7. Residual / upgrade path

1. `StreamsW2a8`: 2º canário com duplo buffer ativo + medição de overlap.
2. `ComputeDevice`: CE Pascal (nvidia_pascal_ce.rs já tem channel) lendo pesos da VRAM e escrevendo resultado — o device passa a executar de verdade; AMD SDMA / Intel BCS idem.
3. iGPU: quando DRAM compartilhada, `Off` honesto (o ganho seria zero — não mentir).
4. ReBAR: com aperture >256MB, subir o modelo 3B inteiro residente.
