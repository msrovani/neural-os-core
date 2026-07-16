# ADR-0044: Análise edge-python — Padrões para VM Sandbox + Compilador SSA

**Status:** Finalizado — Análise Completa (código fonte + docs)
**Data:** 2026-07-15
**Autores:** AI Agent (OpenCode)
**Contexto:** Análise de viabilidade e extração de padrões do projeto [dylan-sutton-chavez/edge-python](https://github.com/dylan-sutton-chavez/edge-python) v0.2.0 (1.193 commits, 63 arquivos Rust no compilador, ~16.600 LOC). Código fonte clonado localmente para análise aprofundada.

---

## 1. Escopo

Esta ADR documenta os resultados da análise do edge-python — um compilador SSA single-pass e VM stack threaded-code para um subconjunto de Python com sandbox. O projeto compila para WASM (~200 KB) e roda no navegador. A análise identificou **15 padrões concretos** transferíveis para o neural-os-core, especialmente para o pipeline de compilação de skills (Hermes) e VM WASM (Hermes/jarbas).

---

## 2. Arquitetura Geral

```
┌──────────────────────────────────────────────────────────────────────┐
│                        edge-python (Cargo workspace)                 │
├──────────────────────────────────────────────────────────────────────┤
│   compiler/ (63 .rs, 16.609 LOC)   │   wasm-abi/ (codec FFI)       │
│   ┌──────────────────────────────┐ │   ┌──────────────────────────┐│
│   │  Parser (SSA single-pass):   │ │   │  NaN-boxing constants,   ││
│   │  lexer/ → parser/ → types.rs │ │   │  HandleTable, ErrorStash ││
│   │  stmt.rs → expr.rs → control │ │   │  encode/decode codec     ││
│   │  ↓                           │ │   └──────────────────────────┘│
│   │  VM (threaded-code stack):   │ │   wasm-pdk/ (plugin toolkit) │
│   │  dispatch.rs → ops.rs → cache│ │   ┌──────────────────────────┐│
│   │  gc.rs → types/ → handlers/  │ │   │  guest_env, module_fixed ││
│   │  builtins/ → optimizer.rs    │ │   │  _pool macro             ││
│   └──────────────────────────────┘ │   └──────────────────────────┘│
├──────────────────────────────────┬───────────────────────────────────┤
│  runtime/ (JS — host runtime)    │  cli/ (edge CLI)                  │
│  std/ (stdlib modules)           │  docs/ (edgepython.com)           │
└──────────────────────────────────┴───────────────────────────────────┘
```

---

## 3. Padrões Identificados

### E-01: Single-Pass SSA sem IR Intermediário

**Localização:** `compiler/src/modules/parser/mod.rs:79-150`

**O que é:** O parser emite SSA form diretamente durante a análise sintática, sem construir uma AST ou IR intermediário. Cada variável tem um contador de versão (`ssa_versions: HashMap<String, u32>`). O parser emite `LoadName slot_N` / `StoreName slot_N+1` diretamente.

```rust
// Padrão: versionamento inline durante parse
fn parse_name(&mut self, name: &str) -> SSAValue {
    let ver = self.ssa_versions.entry(name.to_string()).or_insert(0);
    *ver += 1;
    let slot = alloc_slot(format!("{}_{}", name, ver));
    SSAValue::Slot(slot)
}
```

**Por que é relevante:** Nossa compilação de skills (Hermes gera código via LLM) atualmente não tem pipeline definido — skills são strings interpretadas ou WASM. Um compilador SSA single-pass permitiria compilar skills para bytecode eficiente sem o custo de uma AST.

**Adoção:** Usar o padrão para compilar scripts de agente (Hermes skills, regras de automação) diretamente para bytecode VM. Evita AST, reduz memória.

---

### E-02: Inline Cache Dual (Scalar + Instance-Dunder)

**Localização:** `compiler/src/modules/vm/cache.rs:33-39, 142-158`

**O que é:** Dois níveis ortogonais de cache por sítio de opcode:

1. **Scalar fast path** (`cache.rs:180-194`): Registra par `(type_tag_a, type_tag_b)`. Após 4 hits com o mesmo par, promove para `FastOp` (AddInt, AddFloat, SubInt, etc.) — pula toda a lógica de dunder lookup.
2. **Instance-dunder fast path** (`cache.rs:142-158`): Registra `(class_heap_idx, method_u64, arity)`. Após 4 hits com a mesma classe+método, promove para chamada direta.

```rust
struct CacheSlot {
    type_key: u8,       // (ta<<4)|tb — type pair
    hits: u8,           // promove em QUICK_THRESH=4
    fast: Option<FastOp>,
    inst: Option<InstanceCache>,
}
```

**Por que é relevante:** Nosso sistema de dispatch de agentes/skills (247+ agentes) faz lookup por nome + trust token em toda chamada. Um cache dual (tipo + instância) eliminaria o overhead de lookup para agentes chamados com frequência.

**Adoção:** Implementar `SkillCacheSlot` no Hermes: (1) cache por tipo de skill (scalar: io, compute, network), (2) cache por instância de agente específico. Promoção após N hits, deopt em type change.

---

### E-03: Pure-Function Template Memoization

**Localização:** `compiler/src/modules/vm/cache.rs:197-275`

**O que é:** Funções puras memoizadas por hash dos argumentos (Fowler-Noll-Vo, 64-bit). Promove após 2 hits. Guarda contra argumentos mutáveis (listas/dicts/sets/instâncias excluídos). Resultados cacheados como bits crus de `Val` — custo zero no hit.

```rust
fn template_hash(args: &[Val]) -> u64 {
    let mut h = FNV_OFFSET;
    for v in args {
        h = h.wrapping_mul(FNV_PRIME) ^ v.bits();
    }
    h
}
```

**Por que é relevante:** Nosso pipeline de inferência (Cortex) tem operações determinísticas: `rms_norm(x)`, `silu(x)`, `softmax(x)`. Se chamadas repetidas com mesmos shapes/valores podem ser memoizadas, especialmente no decode loop (mesma sequência de ops para cada token).

**Adoção:** Adicionar `TemplateCache` no Cortex para operações de inferência determinísticas. FNV hash é zero-allocation e cabe em ~50 LOC.

---

### E-04: Super-Instruction Fusion em Runtime

**Localização:** `compiler/src/modules/vm/cache.rs:277-297`

**O que é:** O runtime funde sequências de opcodes em super-instructions no cache, sem modificar o bytecode original. `fuse_method_calls()` detecta `LoadAttr + Call(0)` e transforma em `CallMethod + CallMethodArgs` — eliminando o passo de materializar o atributo.

```rust
fn fuse_method_calls(instructions: &[Instruction]) -> Vec<Instruction> {
    // LoadAttr(a, "method") + Call(0) → CallMethod(a, "method") + CallMethodArgs
}
```

**Por que é relevante:** Nosso decode loop de inferência (Medusa: 3 heads, mesma sequência de matmul/attention/ffn) tem padrões repetidos que poderiam ser fundidos em operações compostas.

**Adoção:** Implementar `FusedOp` para sequências comuns no inference graph: `rms_norm + silu + matmul` → `FusedGateProjection`, reduzindo dispatch overhead.

---

### E-05: NaN-Boxing com 47-bit Inteiro Inline

**Localização:** `compiler/src/wasm-abi/src/lib.rs:11-22`, `compiler/src/modules/vm/types/mod.rs:101-104`

**O que é:** Tagged value representation usando os bits NaN de IEEE 754 para armazenar tipos não-float dentro de 64 bits:

```
QNAN = 0x7FFC_0000_0000_0000
SIGN = 0x8000_0000_0000_0000
TAG_UNDEF   = QNAN              (0x7FFC...)
TAG_NONE    = QNAN | 1
TAG_TRUE    = QNAN | 2
TAG_FALSE   = QNAN | 3
TAG_INT     = QNAN | SIGN       (0xFFFC...)
TAG_HEAP    = QNAN | 4
INT_PAYLOAD_MASK = 0x0000_FFFF_FFFF_FFFF  (47-bit signed)
```

Qualquer padrão não-QNAN = IEEE 754 float (preserva todos os bits float, incluindo NaN payloads exceto canonical NaN). Inteiros de 47-bit cabem inline; overflow promove para `HeapObj::LongInt(i128)`.

**Por que é relevante:** Nosso `Tensor` armazena `Vec<f32>`. Para um sistema de tipos unificado (P-08 da ADR-0043), NaN-boxing permitiria representar f32, int32, e tipos de baixa precisão (E2M1 4-bit) no mesmo slot de 64 bits.

**Adoção:** Implementar `PackedValue` de 64 bits para nosso type system: 4 tipos inline (f32, i32, bool, null) + heap pointer para tipos complexos. Especialmente útil para o inference engine onde misturamos f32 (ativações) com i32 (tokens, índices).

---

### E-06: Mark-Sweep GC com Intern Pool

**Localização:** `compiler/src/modules/vm/types/mod.rs:403-601`, `gc.rs:1-60`

**O que é:** Garbage collector mark-sweep baseado em arena com interning de strings (≤128 bytes), bytes, LongInt, Type, Ellipsis, NotImplemented. Worklist-based mark, sweep com remoção de intern entries para slots liberados.

```rust
struct HeapPool {
    slots: Vec<HeapSlot>,
    free_list: Vec<u32>,
    live: usize,
    gc_threshold: usize,     // começa em 512, dobra após sweep
    strings: HashMap<String, u32>,     // intern pool (<=128 bytes)
    longints: HashMap<i128, u32>,
    // ...
}
```

**Por que é relevante:** Nosso sistema não tem GC hoje — usa `Vec` com drop. Para um VM de scripts de agente, precisaríamos de coleta de lixo. O design do edge-python é minimalista (~60 LOC para o core GC, ~200 LOC com o pool) e cabe em no_std.

**Adoção:** Implementar `AgentHeap` com mark-sweep simplificado para alocações de objetos de script. O intern pool de strings ≤128 bytes é diretamente aplicável ao nosso sistema de nomes de agentes/skills.

---

### E-07: Budget Sandbox (Op Count, Call Depth, Heap Limit)

**Localização:** `compiler/src/modules/vm/types/mod.rs:18-23`

**O que é:** Três limites de segurança para scripts:

```rust
struct Limits { calls: u32, ops: u32, heap: u32 }
// sandbox() = 256 calls, 100M ops, 100K heap
```

- **calls**: profundidade máxima de chamada de função (previne stack overflow)
- **ops**: contagem total de operações (previne loops infinitos)
- **heap**: alocação máxima de heap (previne exaustão de memória)

Back-edge charging em backward jumps, verificação por alocação de heap.

**Por que é relevante:** Nosso Hermes executa scripts de agente e código WASM. Atualmente não há budget — um agente malicioso ou com bug pode consumir CPU/memória infinita.

**Adoção:** Adicionar `ScriptBudget` ao HermesAgent para execução de skills: `max_ops`, `max_heap`, `max_call_depth`. Skills que excedem são pausados (não mortos) — o scheduler pode retomá-los no próximo tick.

---

### E-08: Atomics-Free Cooperative Coroutine Scheduler

**Localização:** `compiler/src/modules/vm/types/scheduler.rs:1-13`, `compiler/src/modules/vm/mod.rs:29-53`

**O que é:** State machine de corrotinas single-threaded com `Pending` side-channel (7 razões de yield). `top_loop` seleciona a corrotina executável com menor deadline. Zero atomics, zero locks — state machines puras.

```rust
enum CoroState { Running, WaitingForChildren, WaitingTimer, WaitingFrame,
                 WaitingEvent, WaitingHostCall, Done }
struct Pending {
    host_frame_request: Option<...>,
    event_wait_request: Option<...>,
    host_call_request: Option<...>,
    waiting_for_children: bool,
}
```

**Por que é relevante:** Nosso `AgentScheduler` em Hermes usa polling com `PollEvery` — agents são verificados em intervalo fixo. O padrão de corrotinas com yield-explícito permite scheduling mais eficiente: agents só rodam quando têm trabalho.

**Adoção:** Implementar `CooperativeAgent` que faz yield explícito (`AgentState::Yield(reason, deadline)`). O scheduler só acorda agents com trabalho pendente, reduzindo polling overhead.

---

### E-09: Phi Elimination como Pós-SSA

**Localização:** `compiler/src/modules/vm/optimizer.rs:39-53`

**O que é:** Pós-processamento SSA que detecta `phi(x, x) → no-op` e elimina a instrução phi. Compacta as instruções restantes com remapeamento de jump targets. Não há passe separado de SSA destruction — o phi é eliminado durante a otimização.

```rust
fn eliminate_trivial_phis(chunk: &mut SSAChunk) {
    for phi in &chunk.phi_map {
        let (src_a, src_b) = chunk.phi_sources[phi];
        if src_a == src_b {
            // phi(x, x) = x — remover
            chunk.instructions.retain(|inst| inst.op != OpCode::Phi);
        }
    }
}
```

**Por que é relevante:** Se implementarmos compilação SSA para skills, a eliminação de phis triviais é uma otimização simples que reduz o bytecode em ~5-10% (loops e if/else simples geram phis redundantes).

**Adoção:** Incluir `eliminate_trivial_phis` no pipeline de pós-compilação de skills.

---

### E-10: RAII VmGuard para Estado Global Seguro

**Localização:** `compiler/src/main/mod.rs:115-136`

**O que é:** `VmGuard` publica um `NonNull<VM>` no static `WasmRuntime` durante a execução; `Drop` limpa. Toda função `host_edge_*` roteia por `with_vm()` que lê o guard e retorna `None` se fora de `run()`. Nenhum dangling pointer sobrevive a panic.

```rust
struct VmGuard(NonNull<VM>);
impl Drop for VmGuard { fn drop(&mut self) { WASM_RUNTIME.store(null(), Release); } }
fn with_vm<F, R>(f: F) -> Option<R> where F: FnOnce(&mut VM) -> R {
    let ptr = WASM_RUNTIME.load(Acquire);
    if ptr.is_null() { return None; }
    Some(unsafe { f(&mut *ptr) })
}
```

**Por que é relevante:** Nosso `CURRENT_BACKEND` e `VRAM_BUDDY` usam `spin::Mutex<Option<T>>` — locking em toda chamada. O padrão VmGuard permite acesso global sem lock para dados que só são acessados em contexto conhecido (ex: dentro de uma interrupção ou tick de agente).

**Adoção:** Implementar `ScopedGlobal<T>` que usa `NonNull<T>` + `AtomicUsize` em vez de `Mutex`. O escopo é garantido por RAII, sem custo de lock para leituras dentro do mesmo contexto.

---

### E-11: Content-Hashed Dict/Set com Ordem de Inserção

**Localização:** `compiler/src/modules/vm/types/mod.rs:244-359`

**O que é:** `DictMap` usa `Vec<(Val, Val)>` + `HashTable<usize>` index. Iteração preserva ordem de inserção, lookup é O(1) médio. `ValSet` usa `HashTable<Val>` diretamente. Igualdade por valor (não ponteiro) para chaves dict/set.

```rust
struct DictMap {
    entries: Vec<(Val, Val)>,
    index: HashTable<usize>,
}
```

**Por que é relevante:** Nosso cache de inferência (KV cache, resultados de memoização) usa `HashMap<u64, Tensor>` — semântica de igualdade por ponteiro. Um dicionário content-hashed permitiria cache por valor de tensor, não por referência.

**Adoção:** Implementar `ContentHashCache<K, V>` para o inference engine. Especialmente útil para memoização de `rms_norm` e `silu` onde mesma entrada → mesma saída.

---

### E-12: `#[repr(u8)]` Opcode + Jump-Table Dispatch

**Localização:** `compiler/src/modules/parser/types.rs:12`, `compiler/src/modules/vm/dispatch.rs:383-569`

**O que é:** 52 opcodes em 1 byte (`#[repr(u8)]`). Dispatch é um único `match` que LLVM compila para jump table. VM stack-based significa operandos são u16 inline — sem register allocator, sem spilling.

```rust
#[repr(u8)]
enum OpCode {
    LoadConst, LoadName, StoreName, LoadAttr, StoreAttr,
    Add, Sub, Mul, Div, FloorDiv, Mod, Pow,
    Call, CallMethod, Return, Jump, JumpIf, Phi,
    MakeClass, MakeFunction, MakeList, MakeDict,
    // ... 52 variantes
}
```

**Por que é relevante:** Se implementarmos um bytecode VM para skills de agente, o padrão `#[repr(u8)]` + jump-table é o mais eficiente possível em Rust. 1 byte por opcode = bytecode compacto, dispatch O(1).

**Adoção:** Usar `#[repr(u8)]` para nosso `SkillOpCode` se implementarmos VM própria. Alternativa: reutilizar o design para nosso inference graph IR (P-09 da ADR-0043).

---

### E-13: `s!()` Macro para Formatação no_std

**Localização:** `compiler/src/util/fstr.rs`

**O que é:** Macro de formatação de string em tempo de compilação:

```rust
// s!("prefix ", str_var, " suffix")
// vs alloc::format!("prefix {} suffix", str_var)
```

Evita `alloc::format!` e sua alocação de `fmt::Arguments`. Usada pervasivemente em mensagens de erro.

**Por que é relevante:** Nosso logging de kernel (`serial_println!`, `kjson!`) usa `alloc::format!` internamente. Uma macro `s!()` reduziria alocações no hot path de logging.

**Adoção:** Implementar `s!()` ou simplificar `serial_println!` para evitar `alloc::format!` no hot path (interrupções, critical sections).

---

### E-14: lol_alloc + linked_list_allocator para WASM

**Localização:** `compiler/Cargo.toml:36` (dependência lol_alloc), PDK usa `module_fixed_pool!`

**O que é:** Duas estratégias de alocador para WASM:
- **lol_alloc**: Allocator WASM mínimo (~200 LOC) com free-list ou leaking-page
- **linked_list_allocator**: Allocator de lista ligada para fixed pools no PDK

Ambos são `no_std` e produzem binários minúsculos.

**Por que é relevante:** Nosso runtime WASM em Hermes (`crates/hermes/src/wasm_rt.rs`) usa o alocador padrão do kernel. Poderíamos usar `lol_alloc` para módulos WASM isolados, prevenindo fragmentação do heap do kernel.

**Adoção:** Usar `lol_alloc` como alocador para módulos WASM hospedados. O fixed pool do PDK é útil para alocações de tamanho conhecido (buffers de áudio, frames de display).

---

### E-15: Coverage-Guided Fuzzing com AFL

**Localização:** `compiler/fuzz-afl/` (workspace separado)

**O que é:** Setup de fuzzing usando AFL (`afl = "0.18.2"`) que testa o pipeline completo `lex → parse → VM::with_limits(ops=100_000)`. Compilado com `debug-assertions = true`, `overflow-checks = true`. Limite de 100K ops previne hangs.

```rust
fn main() {
    afl::fuzz!(|data: &[u8]| {
        if let Ok(src) = std::str::from_utf8(data) {
            let _ = edge_python::compile_and_run(src, Limits::sandbox());
        }
    });
}
```

**Por que é relevante:** Não temos fuzzing para nosso pipeline de skills. O LLM pode gerar skills com bugs, e o parser/compilador pode ter vulnerabilidades. Fuzzing é essencial.

**Adoção:** Configurar `cargo fuzz` ou `afl` para o pipeline de compilação de skills em Hermes. Fuzz targets: (1) parser de skill, (2) executor WASM, (3) inference graph optimizer.

---

## 4. Mapa de Adoção

| # | Padrão | Esforço | Ganho | Quando |
|---|--------|---------|-------|--------|
| **E-07** | Budget sandbox (ops/call/heap) | ~80 LOC | Segurança contra agentes maliciosos/bugados | Imediato |
| **E-10** | RAII VmGuard (lock-free global) | ~60 LOC | Elimina spin::Mutex em 3 globals críticos | Imediato |
| **E-03** | Template memoization (FNV) | ~50 LOC | Cache de ops de inferência determinísticas | Imediato |
| **E-02** | Inline cache dual | ~150 LOC | Dispatch de skills 2-5× mais rápido | Próximo sprint |
| **E-13** | `s!()` macro no_std format | ~30 LOC | Remove alloc::format! do hot path | Próximo sprint |
| **E-05** | NaN-boxing (PackedValue 64-bit) | ~200 LOC | Type system unificado (f32 + i32 + E2M1) | Médio prazo |
| **E-04** | Super-instruction fusion | ~200 LOC | Fused ops no decode loop (Medusa) | Médio prazo |
| **E-08** | Cooperative coroutine scheduler | ~300 LOC | Scheduling eficiente sem polling | Médio prazo |
| **E-06** | Mark-sweep GC + intern pool | ~300 LOC | Coleta de lixo para scripts de agente | Médio prazo |
| **E-01** | Single-pass SSA compiler | ~500 LOC | Compilação de skills sem AST | Longo prazo |
| **E-09** | Phi elimination pós-SSA | ~80 LOC | Otimização de bytecode de skills | Longo prazo |
| **E-12** | `#[repr(u8)]` opcode + jump-table | ~100 LOC | Bytecode VM próprio para skills | Longo prazo |
| **E-11** | Content-hashed dict | ~150 LOC | Cache de inferência por valor | Longo prazo |
| **E-14** | lol_alloc para WASM | ~50 LOC | Alocador isolado para módulos WASM | Longo prazo |
| **E-15** | Fuzzing coverage-guided | ~100 LOC | Segurança do pipeline de skills | Longo prazo |

---

## 5. Tecnologias que NÃO São Aproveitáveis

| Tecnologia | Razão |
|-----------|-------|
| `hashbrown` (0.17) | Equivalente funcional ao nosso `hashbrown` já usado. Padrão de uso importa, não a crate. |
| `libm` (0.2) | Já usamos `libm` para funções matemáticas no_std. |
| `afl` (0.18) | Fuzzing só roda em std/host. Não aplicável em bare-metal, apenas em testes de compilação cruzada. |
| `serde` / `serde_json` | std-only. Usado apenas para testes. |
| `wasm-abi` completo | O codec FFI do edge-python é específico para Python ↔ WASM. Nosso FFI é bare-metal ↔ hardware. |
| `runtime/` (JavaScript) | Runtime JS para browser. Não aplicável em bare-metal. |

---

## 6. Conclusão

Edge Python é um exemplo notável de engenharia de software compacta e eficiente: 16.600 LOC para um compilador SSA + VM completa com GC, inline caching, e fuzzing, tudo rodando em ~200 KB WASM sem std.

Os padrões de **maior impacto imediato** para o neural-os-core são:

1. **E-07 (Budget sandbox)** — nossos agentes executam skills sem limites; um budget de ops/call/heap previne DoS acidental ou malicioso. ~80 LOC.
2. **E-10 (RAII VmGuard)** — nossos globals (`CURRENT_BACKEND`, `VRAM_BUDDY`) usam `spin::Mutex` mesmo em contextos single-threaded. VmGuard elimina o lock. ~60 LOC.
3. **E-03 (Template memoization)** — FNV hash + cache de resultados para ops determinísticas (rms_norm, silu). ~50 LOC.

O padrão de **maior valor estratégico** é **E-01 (Single-pass SSA)**: um compilador de skills sem AST permitiria ao Hermes compilar scripts de agente gerados por LLM para bytecode eficiente, sem o overhead de construir e walkar uma AST. Este é o pré-requisito para um pipeline de skills maduro.

A integração como dependência não é viável — edge-python é um interpretador Python completo, e nós precisamos de um pipeline de compilação de skills específico para nosso domínio (agentes, não Python). Mas **os padrões arquiteturais são diretamente transferíveis**.

---

## 7. Repositório de Referência

- **Repositório:** [dylan-sutton-chavez/edge-python](https://github.com/dylan-sutton-chavez/edge-python)
- **Clone local:** `C:\Users\msrov\AppData\Local\Temp\opencode\edge-python\` (branch `main`, depth 1, 63 arquivos Rust no compilador, ~16.600 LOC)
- **Versão:** v0.2.0 (1.193 commits, 248 stars)
- **Crates:** compiler, wasm-abi, wasm-pdk, wasm-pdk/macros, wasm-pdk/example
- **License:** MIT OR Apache-2.0
- **Site:** https://edgepython.com/
