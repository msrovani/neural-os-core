# Sprint 60 — Plano de Correção Estrutural

**v0.60.0** — 7 itens do fluxo recomendado, decompostos em sub-sprints.

---

## Legenda
| Símbolo | Significado |
|---|---|
| 🟢 Fácil | < 100 LOC, sem dependências externas |
| 🟡 Médio | 100-300 LOC, depende de 1-2 módulos existentes |
| 🔴 Pesado | 300-700 LOC, módulo novo ou pesquisa |
| ⚫ Bloqueado | Depende de HW ou ambiente externo |

---

## Sub-Sprint 60.1 — UX Imediata

**Target:** v0.60.1 | **LOC:** ~200 | **Dependências:** Nenhuma

### 60.1a — Double Buffering Framebuffer
- `display/fb.rs`: Segundo buffer `fb2` em memória reservada
- `display/console.rs`: Renderiza em `fb2`, `swap_buffers()` copia ou troca ponteiro
- `display/agent.rs`: Trigger swap via `TIMER_TICKS` ou vsync flag
- **LOC:** ~120 | **Status:** 🟢

### 60.1b — Prompt `>` interativo
- `display/agent.rs`: Renderizar prompt `> ` no final do console
- `hermes.rs` / `vga_buffer.rs`: Garantir que `>` aparece em VGA e framebuffer
- **LOC:** ~30 | **Status:** 🟢

---

## Sub-Sprint 60.2 — Segurança em Runtime

**Target:** v0.60.2 | **LOC:** ~200 | **Dependências:** Nenhuma

### 60.2a — Security Pipeline → EventBus
- `security.rs`: `SecurityAgent` subscribe `NET_EVENT` e `SYSTEM_EVENT`
- `security.rs`: Publicar `SECURITY_ALERT` no EventBus com payload estruturado
- `hermes.rs`: Handler para `SECURITY_ALERT` — log + notify usuário
- **LOC:** ~150 | **Status:** 🟢

### 60.2b — Path Confinement + Mask Secrets
- `security.rs`: `PathPolicy` e `MaskPolicy` structs
- Skill executor verifica `PathPolicy` antes de I/O
- `MaskPolicy::apply()` regex-replace em payloads de skill
- **LOC:** ~120 | **Status:** 🟡 (depende de saber onde skills fazem I/O)

---

## Sub-Sprint 60.3 — Drivers HW Real

**Target:** v0.60.3 | **LOC:** ~700 | **Dependências:** PCI scan (existente)

### 60.3a — e1000 Driver
- PCI class 0x020000 (Ethernet)
- BAR0/1: MMIO registers, BAR2/3: I/O ports (optional)
- Init: reset, RCTL, TCTL, TDT/TDH rings
- TX: descritor ring de 64 entradas
- RX: descritor ring de 64 entradas
- Interrupt mask (sem IRQ绑定 — polling first)
- **LOC:** ~400 | **Status:** 🟡 (pattern = RTL8139 + ADR-0017 fix)

### 60.3b — USB-MSC BOT Driver
- `usb_msc.rs`: Bulk-Only Transport (BOT) protocol
- CBW (31 bytes) → Data → CSW (13 bytes)
- `usb_msc_read_sectors()` / `usb_msc_write_sectors()`
- Integrar com `fat.rs` para FAT12 persistente em HW real
- **LOC:** ~400 | **Status:** 🔴 (precisa entender xHCI endpoints)

---

## Sub-Sprint 60.4 — LLM Speed

**Target:** v0.60.4 | **LOC:** ~200 | **Dependências:** cpu::features

### 60.4a — WHPX Detection
- `main.rs` ou boot: detectar WHPX via CPUID
- README/docs: comando QEMU com `-accel whpx`
- **LOC:** ~10 | **Status:** 🟢 (documentação)

### 60.4b — AVX2 BitNet Kernel
- `tensor.rs`: `matmul_avx2()` usando `core::arch::x86_64`
- Fallback para `matmul()` atual se AVX2 ausente
- `cortex.rs`: `generate_text()` usar kernel otimizado
- **LOC:** ~150 | **Status:** 🟡 (intrinsics AVX2)

---

## Sub-Sprint 60.5 — Model Training

**Target:** v0.60.5 | **LOC:** ~0 Rust / ~300 Python | **Dependências:** NVIDIA GPU

### 60.5a — Treino 100M params
- `tools/train_large_model.py`: Config para 100M (6 camadas, 8 heads, dim 512)
- `tools/colab_training.ipynb`: Notebook testado
- Converter .pt → .bitnet via `tools/convert_to_bitnet.py`
- **LOC:** ~0 Rust | **Status:** ⚫ (precisa GPU/Colab)

### 60.5b — Treino 1.5B params
- `tools/train_large_model.py`: Config para 1.5B (24 camadas, 16 heads, dim 1024)
- Requer GPU ≥12GB VRAM
- **LOC:** ~0 Rust | **Status:** ⚫ (precisa GPU)

---

## Sub-Sprint 60.6 — Modelos Maiores

**Target:** v0.60.6 | **LOC:** ~500 | **Dependências:** Heap expansion

### 60.6a — GGUF Loader Research
- `tools/gguf_research.md`: Formato GGUF documentado
- Headers: magic, tensor_count, metadata_kv
- Tensors: name, type (Q4_0, Q4_1, etc), offset
- **LOC:** ~0 (doc) | **Status:** 🟡

### 60.6b — GGUF Loader Mínimo
- `cortex.rs`: `load_gguf()` parser
- Suporte Q4_0 (4-bit block quantization)
- `TENSOR_TYPE_Q4_0`: `block_size=32`, `weights_per_block=32`
- **LOC:** ~500 | **Status:** 🔴 (pesquisa + implementação)

### 60.6c — Heap 5GB+
- `memory.rs`: Heap expansion para modelos grandes
- QEMU: `-m 6G` necessário
- **LOC:** ~50 | **Status:** 🟢

---

## Sub-Sprint 60.7 — Skills de Terceiros

**Target:** v0.60.7 | **LOC:** ~1000 | **Dependências:** Nenhuma

### 60.7a — WASM Sandbox Stub
- `wasm.rs`: `WasmSandbox` struct com `load()` e `execute()`
- Sem runtime WASM real (stub para quando `wasmi` estiver disponível)
- Validar entrada/saída via EventBus
- **LOC:** ~100 | **Status:** 🟢 (stub)

### 60.7b — TV-DSL Co-processor
- `tv_dsl.rs`: `parse_tv_dsl()` — parser de expressões matemáticas
- Suporte: `add`, `sub`, `mul`, `div`, `sin`, `cos`, `sqrt`, `pow`
- `execute_tv_dsl(expr: &str, vars: &[f32]) -> Result<f32>`
- Integrar com `cortex::generate_text()`: se output contém `[TV-DSL: ...]`, executar
- **LOC:** ~250 | **Status:** 🟡 (parser + integração)

---

## Sub-Sprint 60.8 — MHI LLM-Otimizado

**Target:** v0.60.8 | **LOC:** ~350 | **Dependências:** UsageTracker + MHI existentes

### 60.8a — AllocProfile: per-allocation metadata
- `mhi.rs`: `AllocProfile { tier, access_count, last_access, avg_latency, size, owner }`
- `MhiRegistry`: BTreeMap de `AllocProfile` indexado por `PhysAddr`
- **LOC:** ~80 | **Status:** 🟢

### 60.8b — LLM tier suggestion
- `mhi.rs`: `suggest_tier(profile) -> AllocTier` — heurística baseada em:
  - `access_count > THRESHOLD` → DRAM ou VRAM (quente)
  - `avg_latency > 1000` → DRAM (requer baixa latência)
  - `size > 1MB` → NVMe ou HDD (frio)
  - Fallback para LLM: `"alocar 500KB para skill X: qual tier?"`
- **LOC:** ~100 | **Status:** 🟢

### 60.8c — migrate_to_tier() + auto-migration
- `mhi.rs`: Copiar dados entre tiers, atualizar page tables
- `optimizer.rs`: A cada 1000 ticks, scanear profiles quentes/frias e sugerir migração
- **LOC:** ~170 | **Status:** 🟡 (page table manipulation)

---

## Sub-Sprint 60.9 — UserProfile System

**Target:** v0.60.9 | **LOC:** ~300 | **Dependências:** Nenhuma

### 60.9a — Perfis de usuário
- `profile.rs`: 6 perfis (Gamer, Engineer, Student, Office, Browsing, Multimedia)
- Cada perfil: `resource_weights(CPU,GPU,IO)`, `mhi_tier()`, `theme_colors()`, `power_profile()`
- `ProfileManager`: singleton atômico, `get()`, `set()`, `list()`, `detect_from_usage()`
- **LOC:** ~180 | **Status:** 🟢

### 60.9b — Integração com display
- `console.rs`: Status bar mostra perfil atual + cores do tema
- `profile.rs`: `theme_colors()` → (bg, accent, fg) muda paleta do NeuralConsole
- **LOC:** ~30 | **Status:** 🟢

### 60.9c — Comando /profile
- `hermes.rs`: `Command::Profile` + parse `/profile <nome>`
- `agents.rs`: Handler mostra perfil atual + lista + alteração
- **LOC:** ~50 | **Status:** 🟢

---

## Summary

| Sub-Sprint | Itens | LOC | Prioridade | Status |
|---|---|---|---|---|
| 60.1 | Double buffer + Prompt | ~200 | 🔴 Crítica | ✅ |
| 60.2 | Security Pipeline + Path | ~270 | 🟡 Alta | ✅ |
| 60.3 | e1000 + USB-MSC BOT | ~700 | 🟡 Alta | 🟡 Médio |
| 60.4 | WHPX + AVX2 BitNet | ~160 | 🟢 Normal | 🟡 Médio |
| 60.5 | Model Training 100M/1.5B | ~300 Python | 🟢 Normal | ⚫ Bloqueado |
| 60.6 | GGUF + Heap 5GB | ~550 | 🟢 Normal | 🔴 Pesado |
| 60.7 | WASM stub + TV-DSL | ~350 | 🟢 Normal | 🟡 Médio |
| 60.8 | MHI LLM-Otimizado | ~350 | 🟡 Alta | ✅ |
| **60.9** | **UserProfile System** | **~260** | **🟡 Alta** | **✅** |
| **Total** | **17 items** | **~3.140 LOC** | | **6 ✅ / 4 🟡 / 2 🔴 / 1 ⚫** |
