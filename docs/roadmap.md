# Roadmap — neural-os-core

**Última atualização:** 2026-06-22  
**Documento vivo:** Alinhado ao ADR-0010 (Estratégico) e ADR-0013 (Estado da Arte 2026).

---

## Fase 3 — Córtex BitNet b1.58 (Atual)

**Sprints:** 9–11  
**Target:** Q3 2026  
**Status:** Sprint 10 concluído, 1 sprint restante

### Concluído

- `TernaryTensor` — armazenamento `i8` com valores {-1, 0, +1}
- `matmul_hybrid()` — kernel ADD/SUB-only (zero multiplicações FPU)
- `BitLinear` — camada densa ternária com forward pass
- `PackedTernaryTensor` — 4 pesos por byte (2-bit encoding)
- `quantize_to_packed(tensor, threshold)` — calibração f32 → ternário
- Compressão de 12× (24 bytes f32 → 2 bytes packed)
- ADR-0011, ADR-0012

### Metas Restantes (Sprint 11)

- [ ] **Bitmap/Slab FrameDeallocator** — reuso real de frames físicos
- [ ] **Benchmark ternário vs f32** — medir perf em QEMU para fechar Phase 3

### Metas Pós-Sprint 11 (Refinamento Fase 3)

- [ ] **Substituição de `libm::expf` por Aproximação Racional de Padé** — usar `tinyml-rs` para aproximar SiLU sem chamada de função de ponto flutuante (redução de latência em ~40% no forward do Intent Router)
- [ ] **Integração de Kernels TL/I2_S** — lookup table para eliminar branches condicionais do `matmul_hybrid()`. Packing de 16 pesos por DWORD (32 bits). Kernel branchless: `LUT[input_bits ^ weights_dword]` substitui `match w { 1=>add, -1=>sub }`
- [ ] **Suporte a parsing GGUF** — carregar pesos pré-treinados diretamente em `PackedTernaryTensor` via formato GGUF (inspirado por MerlionOS)

---

## Fase 4 — Memory Fabric & SFS Zero-Copy

**Sprints:** 12–15  
**Target:** Q4 2026  
**Status:** Planejamento

### Visão

Substituir o block I/O clássico por um **Semantic File System** onde armazenamento NVMe é mapeado diretamente no VAS do kernel via DMA, sem nenhuma cópia de buffer. Memória é endereçada por *conteúdo semântico* (embeddings, KV-cache), não por blocos ou inodes.

### Metas

- [ ] **Driver NVMe mínimo** — filas de submissão/completion, mapeamento de BARs PCIe
- [ ] **Huge Pages (2 MiB / 1 GiB)** — mapear SFS com páginas enormes para reduzir TLB misses durante inferência (inspirado por MerlionOS)
- [ ] **`zerocopy` transmutation** — `Tensor` ↔ `&[u8]` sobre páginas mapeadas, sem `serde`
- [ ] **Episodic Memory** — KV-cache persistente via páginas físicas mantidas ativas entre boot cycles (NVMe com battery-backup ou S3 sleep)
- [ ] **Namespace SFS** — faixa de endereços virtuais `0x5000_0000_0000 – 0x6000_0000_0000`

### Dependências

- Fase 2 (page tables, `OffsetPageTable`)
- Fase 3 (tensores ternários para storage de embeddings)
- QEMU com emulação NVMe (`-device nvme`)

---

## Fase 5 — Skills-as-Modules (WASM Component Model + MCP)

**Sprints:** 16–18  
**Target:** Q1 2027  
**Status:** Planejamento

### Visão

Agentes efêmeros executados como módulos WASM isolados em Ring 2, instanciados sob demanda pelo Córtex Neural e descartados após execução. Zero processos zumbis, zero garbage collector, zero instalação.

### Metas

- [ ] **`wasmi` embedder** — interpretador WASM com host functions para `tensor.matmul`, `nn.silu`, `sfs.read`
- [ ] **Memory pool** — slabs pré-alocados de 256 KB por skill
- [ ] **Capability-based imports** — cada skill declara imports; Córtex valida contra allowlist
- [ ] **MCP (Model Context Protocol)** — skills se comunicam com o Córtex via mensagens estruturadas (não syscalls)

---

## Fase 6 — Success Engine & Neural Syscalls

**Sprints:** 19–21  
**Target:** Q2 2027  
**Status:** Planejamento

### Visão

Todo syscall de Ring 2 (WASM) é interceptado e avaliado semanticamente pelo Neural Córtex antes de executar no silício. Política *default-deny*: nenhum acesso a hardware ou persistência sem permissão explícita.

### Metas

- [ ] **Dispatch table** — `#[no_mangle] extern "C"` entry points para syscalls
- [ ] **Capability token** — token criptográfico por instância de skill, escopo de operações permitidas
- [ ] **Neural evaluation** — Linear → argmax decide allow/deny em nível semântico
- [ ] **Success Engine** — Córtex aprende com sucesso/fracasso das ações para ajustar pesos online

---

## Meta Futura: MatMul-free LM

**Target:** Pós-Fase 6 (Q3 2027+)

Eliminar operações de multiplicação de matrizes (self-attention) do pipeline de inferência, substituindo por mecanismos de pooling ternário ou estados recorrentes (RWKV, Mamba). Se bem-sucedido, o neural-os-core executará modelos com **zero multiplicações FPU** — da camada de embedding à saída logits.

### Referências

- RWKV: Recurrent Neural Networks with Linear Attention (2024)
- Mamba: Selective State Space Models (2024)
- BitNet b1.58: Elimination of FPU multiplications in FFN layers

---

## Timeline Consolidada

| Fase | Sprints | Target | Depende de |
|---|---|---|---|
| **3** Córtex BitNet b1.58 | 9–11 | Q3 2026 | Fase 2 |
| **3+** Refinamento TL/I2_S + Padé | — | Contínuo | Fase 3 |
| **4** Memory Fabric & SFS | 12–15 | Q4 2026 | Fase 2, NVMe em QEMU |
| **5** WASM Skills & MCP | 16–18 | Q1 2027 | Fase 2 + 4 (SFS) |
| **6** Success Engine & Neural Syscalls | 19–21 | Q2 2027 | Fase 5 |
| **7** MatMul-free LM | 22+ | Q3 2027+ | Fase 3–6 |

---

*Consulte ADR-0010 (Strategic Roadmap) e ADR-0013 (Executive Summary / Estado da Arte 2026) para fundamentação arquitetural completa.*
