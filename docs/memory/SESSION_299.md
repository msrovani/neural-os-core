# SESSION_299 — Boot Audit + ATA TCG Fix + #PF Demand-Page

**Data:** 2026-08-31
**Sprint:** v1.9.99-s299
**Commits:** 456afab, bb90042
**Goal:** Deep audit of boot + k_nano, fix ATA in TCG, fix #PF, improve slog visibility

---

## 1. Auditoria Profunda do Boot

Leitura completa de:
- `main.rs` (4777 LOC) — kernel_boot(), model loading, gates N3/N4/N5
- `limine_boot.rs` — entry, handoff, HHDM
- `memory.rs` — BitmapFrameAllocator, PT pool, heap budget
- `allocator.rs` — LazyBumpAllocator, grow_bump_auto, TALC
- `ata.rs` — PIO driver, probe, identify
- `fat32.rs` — MBR/GPT parser, Fat32Reader
- `storage_probe.rs` — probe_storage_drivers
- `storage_bw.rs` — benchmark skip
- `boot_observe.rs` — k_ai boot plan
- `slog.rs` — severity mapping (Trace/Ok/Warn/Fail)
- `demand_page.rs` — lazy page fault handler
- `interrupts_ext.rs` — #PF handler chain

### Comparação com Redox OS
- Redox: `kstart()` (~500 LOC) → `kmain()` → userspace bootstrap → initfs/init.rc → storage → filesystem → login
- Redox: drivers = userspace processes (scheme file descriptors)
- Neural OS: **TUDO monolítico** no `kernel_boot()` (~3000 LOC inline)

### Comparação com AIOS (agiresearch)
- AIOS: kernel layer separado do agent layer
- AIOS: LLM management isolado em módulo
- Neural OS: LLM loading, BPE, TTS, greeting, Trinity MoE — tudo inline

---

## 2. Fixes Implementados

### Fix 1: ATA Probe em TCG (456afab)

**Problema:** `storage_bw::skip_measure()` retornava `true` em TCG → `AtaDriver::probe()` retornava `None` → zero storage em QEMU TCG → sem BOOT.LOG, sem cross-boot NSGDB, sem FAT32.

**Causa raiz:** SESSION_243 documentou que TCG PIO trava boot. Mas o problema era a **benchmark** (256 setores), não o **probe** (identify + 1 setor MBR). A distinção nunca foi feita.

**Fix:**
- `storage_bw.rs`: novo `allow_probe()` → sempre retorna `true`
- `ata.rs`: usa `allow_probe()` em vez de `skip_measure()` para probe
- `boot_observe.rs`: ATA SEMPRE no plano de storage

**Resultado:** FAT32 mount funciona em TCG (`mount bps=512 spc=1 root=2`)

### Fix 2: Slog Visibility (456afab)

**Problema:** Mensagens de diagnóstico do model probe usavam sub `"ramdisk"`, `"Asset"`, `"BGE"` que mapeiam para `Sev::Trace` (hidden by default). Quando FALCON3 não carregava, o operador via apenas `llm=ABSENT` sem motivo.

**Fix:** Mensagens críticas mudadas de Trace para `ok`/`warn`:
- `"Asset" "ramdisk"` → `"Asset" "ok"` (Probe 4GB)
- `"Asset" "ramdisk"` → `"Asset" "warn"` (region truncation)
- `"HWEXPERT" "info"` → `"HWEXPERT" "ok"` (model loaded)
- `"MODEL" "info"` → `"MODEL" "ok"` (hub status)
- `"RAMDISK" "info"` → `"LLM" "ok"` (LLM loaded @0x120000000)

### Fix 3: #PF Demand-Page Both Heap Ranges (bb90042)

**Problema:** `try_fault_in_heap` só verificava `HEAP_START` (0x_4000_0000_0000 — TALC heap pós-boot). Mas o bump allocator (boot + runtime) usa `HEAP_BUFFER` no endereço do **linker** (`0xffffffff80200000`+). Quando ATA+FAT32 montavam e alocavam Vec, as páginas além do mapeamento do Limine davam #PF loop (11 faults → hlt).

**Causa raiz:** O `.kheap` section (NOLOAD no linker) contém HEAP_BUFFER (512MB). Limine mapeia PT_LOAD segments, mas as páginas no final do `.kheap` podem não estar mapeadas. O bump allocator growPages via `map_page_direct`, mas se a página já foi acessada antes do grow (via Vec allocation), o #PF não é curado.

**Fix:**
```rust
let bump_start = HEAP_BUFFER.as_mut_ptr();  // linker address
let bump_end = bump_start + HEAP_SIZE;
let in_bump = cr2 >= bump_start && cr2 < bump_end;
// + check in_talc for TALC range
if !in_talc && !in_bump { return false; }
```

**Resultado:** ZERO #PFs, boot limpo até Phase 5 com ATA+FAT32

---

## 3. Lições Aprendidas

### L1: skip_measure ≠ skip_probe (crítico)
A distinção entre "skip benchmark" e "skip probe" é fundamental. `storage_bw::skip_measure()` foi usado para AMBOS, bloqueando storage inteiro em TCG. Regra: benchmarks que travam ≠ probes que funcionam.

### L2: slog severity mapping é contrato de visibilidade
Sub como `"ramdisk"`, `"Asset"`, `"BGE"` mapeiam para `Sev::Trace` (hidden). Mensagens de diagnóstico que o operador PRECISA ver devem usar sub `"ok"` ou `"warn"`. Regra: se a mensagem responde "por que X falhou?", deve ser visível.

### L3: Heap dual-range demand-page
O kernel tem DOIS ranges de heap:
1. `HEAP_START` (0x_4000_0000_0000) — TALC allocator (pós-boot)
2. `HEAP_BUFFER` linker address — bump allocator (boot + runtime)

O #PF handler DEVE cobrir AMBOS. O `.kheap` NOLOAD pode ter páginas não mapeadas.

### L4: QEMU TCG é o MAIS limitante
- ATA PIO funciona mas é lento (~16ms per identify)
- SMP é flaky (AP timeout)
- Modelo 770MB copia lenta
- WHPX quebra neste HW (#GP OVMF)
- O caminho real de validação é HW real ou corrigir TCG

### L5: main.rs monolito = dívida técnica
4777 LOC em uma função. Cada SESSION adiciona 50-100 linhas. Extrair em módulos é o investimento de manutenção mais valioso.

---

## 4. Estado Atual do Boot

| Métrica | Antes (s298) | Depois (s299) |
|---|---|---|
| ATA probe TCG | **SKIP** | **FUNCIONA** |
| FAT32 mount TCG | **AUSENTE** | **OK** (bps=512 spc=1) |
| #PF count | **11** (loop → hlt) | **0** |
| Boot phases | 0-4 | **0-5** |
| slog visibility | Trace (hidden) | **ok/warn** (visible) |
| Log output | ~9KB | **~10KB** (1 core) |
| Cross-boot NSGDB | Impossível | **Desbloqueado** (precisa BOOT.LOG persist) |

---

## 5. Próximos Passos

1. **Boot completo em TCG** — Fases 6-8 lentas mas funcionais (scheduler + agents)
2. **Cross-boot NSGDB** — Com ATA habilitado, BOOT.LOG grava no FAT32. Segunda instância lê via `ingest_bootlog()`
3. **Extrair kernel_boot em módulos** — Reduzir de 3000 LOC para ~500 por módulo
4. **Gate QEMU smokes** — Feature flag para smokes não-essenciais
5. **Falcon3-3B v6 loading** — Modelo no disco FAT32, load via ATA (agora funciona)

---

## 6. Referências

- Redox OS boot: https://doc.redox-os.org/book/boot-process.html
- AIOS (agiresearch): https://github.com/agiresearch/AIOS
- ADR-0088: AIOS-first premissa máxima
- ADR-0101: Falcon3-3B cognitive lab
- SESSION_243: TCG PIO hang (base do skip_measure)
- SESSION_252: demand-page e bump heap
- SESSION_293: OVMF pflash, Falcon3 1.58-bit
