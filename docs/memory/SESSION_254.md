# SESSION_254 — Crash ip=0 com loader 4GB: stack do Limine não reservada + heap lazy (2026-08-09)

**Escopo:** Diagnóstico do `#PF ip=0x0` que só ocorria com BITNET2B via QEMU-loader em 4GB+.
**Status:** ✅ Fechada — 2 fixes (stack reserve + heap lazy AIOS) — 1 commit `8901d97` — 0 erros.

---

## 1. Contexto

Boot QEMU com `-device loader` (BITNET2B 577MB @0x100000000) crashava com
`#PF ip=0x0000000000000000 err=0x10` (fetch em NULL) em T+1971, logo após o publish
`[BOOT:DriverInit]`. Sem o loader, boot passava. Reproduzido em HEAD **e** em
`704a176` (pai do bughunt) — bug pré-existente, NÃO regressão do bughunt.

---

## 2. Diagnóstico

### 2.1 Bisect

- `704a176` (pai do bughunt 0c55327) + loader: crasha (T+1094, `#PF ip=0xffffffff808dc5e0 err=0x11` — jump para endereço de dado, NX).
- `704a176` sem loader: boot OK (scheduler vivo T+2555).
- HEAD sem loader: boot OK (teste usuário).

→ Crash pré-existente, loader-dependente, manifesta como **jump para lixo** (return
address / function pointer corrompido). Ponto de manifestação muda com o layout
do build (T+1094 vs T+1971).

### 2.2 Verificações que EXONERARAM o caminho direto

- `publish_boot_phase` (main.rs:3986-3999): slog → log_quiet → EVENT_BUS.publish → drain. Tudo safe.
- EventBus (event-bus/src/bus.rs): BTreeMap + VecDeque, sem unsafe.
- `load_bge` (k_ai/memory_systems.rs:17-73): parse bounded, retorna false no BITNET2B.
- Scan BGE (main.rs:2143-2193): somente leitura, size_hint 512KB bound.
- `buffer_log` (boot_logger.rs:73): Vec bounded.

### 2.3 Causa raiz (oracle + confirmação empírica)

**A stack de 2MB que o Limine aloca (StackSizeRequest) nunca era reservada no frame
allocator.** O `StackSizeResponse` do kernel nem tinha o campo `address` (ABI
incompleto). A stack fica logo abaixo da imagem do kernel (~2.44GB). O frame
allocator tratava essa região como livre → com o loader, as alocações extras do
boot (scan BGE, buffers FAT, model copy) empurram o watermark até a região da
stack → frames da PRÓPRIA stack do kernel são entregues ao heap → return address
corrompido → `ip=0x0`.

**A gordura do heap piorava:** `resize_bump_heap(1536)` mapeava ~1.5GB eager no
T+0, subindo o watermark do allocator para perto da stack. Menos alocação eager =
menor janela de crash.

---

## 3. Fixes (commit `8901d97`)

### 3.1 Stack do Limine reservada (root cause do ip=0)

- `crates/k_nano/src/limine.rs`: `StackSizeResponse` ganha campo `address` (ABI real do Limine: revision + address).
- `crates/neural-kernel/src/limine_boot.rs`: `LIMINE_STACK` vira `pub(crate)`.
- `crates/neural-kernel/src/main.rs`: `reserve_range(resp.address, 2MB)` após a reserva da imagem.

### 3.2 Heap lazy AIOS (premissa 4 — auto-adaptar)

- Remove `resize_bump_heap(1024)` eager (main.rs:1357) e o resize para
  `heap_initial_mb` (1536MB) → piso `heap_initial_mb.min(512)`.
- O `LazyBumpAllocator` já chama `grow_bump_auto` (256MB/passo) no OOM
  (allocator.rs:46) — agora é ele que cresce sob demanda.

---

## 4. Validação (QEMU TCG, `-m 6G`, BITNET2B + HWEXPRT4 + hw_expert_v4)

| Run | Resultado |
|---|---|
| HEAD pré-fix + loader | `#PF ip=0x0` T+1971 (crash) |
| `704a176` + loader | `#PF ip=0xffffffff808dc5e0` T+1094 (crash) |
| `704a176` sem loader | Boot OK (scheduler vivo T+2555) |
| HEAD pós-fix + loader (2×) | **MVP-C demo OK** — crash ausente |
| HEAD pós-fix+heap-lazy + loader | **MVP-C demo OK, LLM LOADED h=2560 L=30 577MB, auto-grow 512→768→1024MB** |

Prova do heap lazy funcionando (AIOS):
```
heap piso=512MB (RAM=7168MB; grow_bump_auto sob demanda até budget 1536MB)
auto-grow 512 MB → 768 MB (need=512MB, 65536 páginas, AIOS)
auto-grow 768 MB → 1024 MB (need=768MB, 65536 páginas, AIOS)
QEMU loader: copying 590680KB -> heap (leak backing) then load_model...
LLM LOADED file=llama8b.bin size=590680KB RAM=7168MB airllm=false
```

Nota: `layer 0/30` no fim = FWD do generator h=2560×30 em TCG soft-float (lento,
não travado — CPU sobe). BPE ausente → fallback CHAR OK (`bpe=0`), não era o bloqueio.

---

## 5. Lições

1. **Stack do bootloader é memória viva, não reservada:** qualquer frame allocator
   que trata toda a RAM como livre pode entregar a própria stack do kernel. O
   `StackSizeResponse` do Limine tem `address` — usar.
2. **Reserve a stack ANTES de alocar:** a ordem é: init frame allocator → reserve
   kernel image → reserve stack → reserve arena → heap.
3. **Heap eager = janela de crash:** mapear 1.5GB no T+0 não é "pronto", é
   dívida — sobe o watermark e expõe bugs de reserva. Piso modesto + auto-grow é
   o comportamento AIOS (premissa 4) e funciona (2B carregou sem reserva eager).
4. **`cargo check --release` 0 erros obrigatório** — feito antes do commit.
