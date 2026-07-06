# ADR-0038: Otimização do Ecossistema — Substituições via Hugging Bay + crates.io

**Data:** 2026-07-06  
**Status:** Aceito  
**Substitui:** N/A (complementa ADR-0037)  
**Sprint Target:** 86 (buddy-slab), 88 (edge-net), 91 (khal-inspired)

---

## 1. Contexto

Revisão sistemática do ecossistema Rust bare-metal via Hugging Bay e crates.io identificou 3 candidatos a substituição/incorporação:

| Tecnologia | Achado em | Votos | Descrição |
|---|---|---|---|
| **buddy-slab-allocator** | crates.io | 30K downloads | Alocador Buddy+Slab no_std, per-CPU, ArceOS |
| **edge-net (edge-dhcp)** | crates.io | 225★ GitHub | DHCP/DNS/HTTP no_std + no-alloc |
| **khal-std** | crates.io | 46★ GitHub | GPU shaders → SPIR-V/PTX/CPU (inspiração) |
| **ruvix-net** | crates.io | — | Kernel cognitivo similar (referência) |

---

## 2. Decisões

### 2.1 buddy-slab-allocator → SUBSTITUI slab.rs + gpu/vram.rs parcial

**Decisão:** Substituir nosso slab allocator (`slab.rs`) e o backend do VRAM buddy allocator (`gpu/vram.rs`) pelo `buddy-slab-allocator` crate.

**Motivo:**
- Código maduro (30K downloads, usado no ArceOS — projeto similar ao nosso)
- Per-CPU slab caches com remote-free lock-free (melhor que nosso slab atual)
- Buddy allocator com splitting/merging (equivalente ao nosso, mas testado)
- `no_std` puro, Apache-2.0

**Risco:** Baixo — é no_std + já opera em bare-metal (ArceOS). Adaptação requer mapear nosso `GLOBAL_ALLOCATOR` + `FrameAllocator` para a interface do crate.

**Prazo:** Sprint 86 — preparar interface de adaptação, manter nosso código como fallback.

### 2.2 edge-net (edge-dhcp) → COMPLEMENTA smoltcp para B-01

**Decisão:** Usar `edge-dhcp` como implementação DHCP alternativa/de fallback para resolver B-01.

**Motivo:**
- DHCP no_std + no-alloc — funciona sem heap
- Pode operar antes do smoltcp estar inicializado
- 225★ GitHub, 42 forks, ativo

**Risco:** Baixo — é uma camada adicional, não substitui smoltcp.

**Prazo:** Sprint 88 ou antes, se priorizado para B-01.

### 2.3 khal-std → INSPIRAÇÃO (NÃO VIÁVEL DIRETAMENTE)

**Decisão:** Não incorporar como dependência. khal-std requer `wgpu` (std-only) para runtime GPU. Mas a **arquitetura** (shader Rust → SPIR-V/PTX/CPU) inspira nossa futura GPU compute.

**Motivo da rejeição:**
- Depende de `wgpu` (não no_std)
- Requer toolchain externo (`cargo-gpu`, `cargo-cuda`)
- Runtime hosteado (não bare-metal)

**Aproveitamento:** Usar o pattern de compilação cruzada de shaders (Rust → múltiplos targets) como referência para quando implementarmos GPU compute real.

### 2.4 ruvix-net → REFERÊNCIA ARQUITETURAL

**Decisão:** Não incorporar, mas usar como referência de arquitetura para cognitive kernel networking.

---

## 3. Dependências

| Item | Depende de | Bloqueia |
|---|---|---|
| buddy-slab-allocator | Nenhuma | N/A |
| edge-dhcp | B-01 (RX fix) | N/A |
| khal-inspired shader infra | GPU compute funcional | Sprint 85 |

---

## 4. Riscos e Mitigações

| Risco | Prob. | Impacto | Mitigação |
|---|---|---|---|
| buddy-slab-allocator requer adaptação da interface GlobalAlloc | Baixa | Médio | Adapter layer + testes |
| edge-dhcp incompatível com smoltcp Device trait | Média | Baixo | Usar como fallback, não substituição |
| khal-inspired approach inviável sem std | Alta | Baixo | Manter GPU drivers manuais como fallback |

---

## 5. Recursos

- `buddy-slab-allocator`: https://github.com/arceos-hypervisor/buddy-slab-allocator
- `edge-net`: https://github.com/sysgrok/edge-net
- `khal-std`: https://github.com/dimforge/khal
- `ruvix-net`: https://crates.io/crates/ruvix-net
