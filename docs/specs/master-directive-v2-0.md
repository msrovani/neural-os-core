# MASTER DIRECTIVE — HERMES vNEURAL OS v2.0 (ULTIMATE SYSTEM DIRECTIVE)

> **Arquivo de trabalho:** este documento é o *prompt mestre* pronto para colar na IDE
> (Cursor / Windsurf / Claude Code / Copilot Agent). Ele foi **calibrado contra o código
> real do repositório** (clone `neural-os-core-latest`, HEAD `6b0e4f5`, v1.9.9-s257+41):
> cada item marca ✅ **JÁ IMPLEMENTADO** (não refazer — só verificar) ou ⚠️ **PENDENTE**
> (implementar com a referência indicada). Os exemplos de código usam **as APIs reais**
> das crates — nenhum símbolo inventado.

---

## 0. CONTEXTUALIZAÇÃO — COMO OS LOGS DO KERNEL INFORMARAM ESTA ARQUITETURA

A análise anatômica dos logs de boot (`boot_whpx`, `boot_pf_fix`, `boot_6g`, `boot_control`,
`boot_mesh`, `boot_hweval`, `boot_only_hwexpert`) revelou que o sistema **já roda a cadeia
completa** (8 fases de boot → Runtime → tick) e que a maioria dos "gargalos" clássicos de
kernel bare-metal já foi resolvida em sessões anteriores. Os logs informaram os seguintes
fatos que **ancoram** esta diretiva:

| Evidência no log / auditoria | Conclusão |
|---|---|
| `smoke_level2=OK` no boot | Inserção massiva no B-tree do NeuralFS está estável (não é bug ativo) |
| `PHYS_MEM_OFFSET` gravado em `main.rs` **antes** de IDT/heap/drivers | Risco de `#PF` por ponteiro físico puro já eliminado (T+0 ✅) |
| Frame allocator agora **recusa dealloc de frame não-entregue** (bitmap `delivered`) | Classe exata do `hash_mismatch` do OTA (frame vivo liberado p/ DMA) corrigida |
| `HalOffer` com backoff exponencial 50→3200 ticks por `DeviceClass` | Poluição do EventBus por polling de `video`/`Absent` silenciada (✅ implementado) |
| `PromoteSkill` era código morto (veredito só logava) | Promoção ephemeral→WASM **wireada** no laço do supervisor (✅) |
| `MpmcQueue` (Vyukov/CAS) já existe em `k_nano/mpmc.rs` | **NÃO criar** um ring "lock-free" caseiro inseguro — usar o existente |
| `wake_aps_sequential` (IPI direcionado + retry 3× + stacks per-AP) | Timeout do AP LAPIC 1 já mitigado (SESSION_163); calibração TSC/HPET = melhoria opcional |
| 7 experts do Trinity registrados **sem peso** (`weight: None` em todo o código) | O "Efeito Matrix" real = implementar `get_or_mmap_expert` (gap verdadeiro do 2.4) |

**Regra de ouro:** antes de escrever qualquer código novo, `grep` o nome da primitiva no
repo. Este projeto já tem MPMC, SPSC, work-stealing deque, arena, CapGate, runtime wasmi e
loader de modelos — o trabalho é **wiring e completude**, não reinvenção.

---

# ═══════════════════════════════════════════════════════════════════════════════
# MASTER PROMPT (COLE NA IDE)
# ═══════════════════════════════════════════════════════════════════════════════

# ==============================================================================
# MASTER DIRECTIVE: HERMES vNEURAL OS — VERSION 2.0 FULL REFACTOR & BRINGUP
# TARGET ARCHITECTURE: x86_64 Bare-Metal / Limine HHDM / Rust #![no_std]
# INSPIRATION: Redox OS, Theseus OS, seL4, Marvel MCU JARVIS & The Matrix "Trinity"
# ROLE: Chief Operating System Architect & Lead Bare-Metal Rust AI Engineer
# ==============================================================================

---

## 1. VISÃO GERAL E FILOSOFIA DO SISTEMA

Você é o Arquiteto Principal do **Hermes vNeural OS v2.0**. O objetivo é promover o
protótipo atual (que já roda com fallbacks *por design*) para um **Sistema Operacional
Neural de Produção Bare-Metal**: Full SMP, memória gerida por IA, execução nativa de
skills, orquestração lock-free e interface executiva sem latência.

**Princípio AIOS-First (ADR-0088):** IA não é feature — é o modo de operar. Toda decisão
é HITL, auto-adaptável e registrada (IDEA → ADR → SESSION). Nada é bypassado: fallback
exige análise e gera busca ativa por solução.

**⚠️ Gate da versão:** o repositório **proíbe** declarar "v2.0.0" sem review formal de ADR
+ zerar demandas `por_fazer` + OK explícito do maintainer. Todo trabalho neste prompt
promove itens **rumo ao gate**; jamais rotule commits como "v2.0 completo".

---

## 2. MAPA DE CRATES E ANÉIS (dependências reais)

```
k_nano (R0 — fundação)  ←  k_hal (R1)  ←  cortex (R2)          ←  (bin neural-kernel = wire)
                                ├← k_ai (R2)
                                └← hermes (R3) ← jarbas (R3)
```

| Crate | Anel | Função | Arquivos-chave |
|---|---|---|---|
| `k_nano` | R0 | mem, IRQ, PCI, APIC/SMP, FAT32, NeuralFS, mpmc/spsc/work-stealing, frame allocator | `memory.rs`, `fat32.rs`, `smp/mod.rs`, `smp/work_stealing.rs`, `smp/spsc.rs`, `mpmc.rs`, `neural_fs/*` |
| `k_hal` | R1 | DeviceCap, HalOffer, CapGate, drivers (e1000/xHCI/HDA), ACPI | `offer.rs`, `cap_gate.rs`, `e1000.rs`, `acpi.rs` |
| `k_ai` | R2 | SelfHeal, Trust, SGDB, memória episódica | `sgdb/*`, `memory_systems.rs` |
| `cortex` | R2 | LLM BitNet, Trinity MoE, tensores, arena | `trinity.rs`, `arena.rs`, `global_arena.rs`, `bitnet_avx2.rs` |
| `hermes` | R3 | Orquestração, wasmi, skills, mesh P2P, net bridge | `executive.rs`, `wasmi_rt.rs`, `evolve.rs`, `skill_opt.rs`, `bei_init.rs` (no bin) |
| `jarbas` | R3 | Display FE, persona, áudio | `display/*`, `audio/*` |
| `neural-kernel` | bin | Integração/residuals — **só wire/`pub use`**, lógica nova nas crates | `main.rs`, `bei_init.rs` |

**Regras estruturais:** `no_std` + `no_main`; anéis são organização lógica (tudo roda em
Ring 0 real; isolamento efetivo = wasmi + Ring3 gated); build soft-float com
`#[target_feature]` para SIMD; target dirs isolados sob `target/`.

---

## 3. REQUISITOS TÉCNICOS POR PILAR

### PILAR 2.1 — FULL SMP REAL & WORK-STEALING SCHEDULER

**Estado real:** wake de APs já usa IPI **direcionado** + retry INIT-SIPI-SIPI 3× + stack
e PerCpu **por-AP** (`k_nano::smp::mod::{wake_aps, wake_aps_sequential, init_smp}`,
SESSION_163; `-smp 4` → 3 APs). `WorkStealingDeque` (Chase-Lev) + `WorkStealingPool`
existem em `k_nano::smp::work_stealing` (já usados por `cortex::parallel_*` gated).

- [ ] **✅ Wake multi-AP — já implementado.** Verificar apenas: `wake_aps_sequential`
      acorda TODOS os LAPIC; nenhum "AP LAPIC N timeout" novo no boot.
- [ ] **⚠️ Calibração TSC/HPET do SIPI (melhoria opcional):** substituir delays fixos da
      sequência INIT-SIPI-SIPI por calibração `rdtsc` (ou HPET), eliminando dependência de
      timing do QEMU. Não reescrever a sequência — só o timer.
- [ ] **⚠️ Scheduler multi-core (projeto grande):** o `AgentScheduler` global ainda é
      single-core. Alvo: um `WorkStealingPool` por core + `MpmcQueue` para a fila global de
      agentes prontos; APs ociosos fazem `try_steal_global(worker_id)` em vez de `hlt`.
      **Pré-requisito:** APs com IDT compartilhada + reschedule-IPI (ADR-0057 WS-F — hoje
      `ap_pollable()` é false; sem isso AP em `hlt` trava).
- [ ] **⚠️ Lazy FPU/SIMD context switch (novo):** setar `CR0.TS` no switch-out e restaurar
      no handler `#NM` (vetor 7) — elimina salvar 256/512 bits de registradores XMM/YMM/ZMM
      a cada tick. Padrão em §4.5.

### PILAR 2.2 — GESTÃO DE MEMÓRIA NATIVA GERIDA POR IA

**Estado real:** `PHYS_MEM_OFFSET` é gravado em T+0 (`main.rs` antes de IDT/heap; store em
`k_nano::memory.rs:411`). Frame allocator tem **ownership check** (bitmap `delivered` —
dealloc de frame reservado/duplo é recusado; 3 testes host). Arena Cortex:
`cortex::arena::CORTEX_ARENA_VIRT = 0x4800_0000_0000` (512MB).

- [ ] **✅ HHDM/`PHYS_MEM_OFFSET` T+0 — já implementado.** Usar sempre
      `k_nano::memory::phys_to_virt`/`virt_to_phys`; nunca dereferenciar ponteiro físico.
- [ ] **✅ Zero-copy FAT32 → frames/arena (IMPLEMENTADO — `fat32.rs` + `gguf_mmap.rs`):**
      novo `Fat32Reader::read_file_range_into(name, offset, size, dst)` grava setores
      alinhados **direto no destino** (PIO do controlador → página do frame/arena, sem
      `Vec` intermediário); `read_file_into` cobre o arquivo inteiro capado em `dst.len()`.
      O parser agora é `dyn` sobre `Fat32Io` (trait `io_read_sectors`/`io_write_sectors`)
      — host-testável com `MemoryDisk` (round-trip FAT32 completo em host) e o ATA real
      em bare-metal. O bin `gguf_mmap.rs` carrega o modelo direto nos frames (HHDM),
      detectando o magic com leitura de 4 bytes. Falta para 100% do 2.2: NVMe/zero-copy
      DMA (controller write) — hoje o PIO passa pela CPU (leitura direta no destino).
- [ ] **✅ Pool dedicada de page tables CoW/Ring3 (IMPLEMENTADO — `memory.rs`):**
      `BitmapFrameAllocator.init_pt_pool` reserva 256 frames (1 MiB) do alocador geral
      (nunca vazam para DMA); `alloc_pt_frame`/`dealloc_pt_frame` com fallback no geral
      (exaustão da pool NUNCA bloqueia isolamento); 2 testes host. `AddressSpace::
      clone_current()`/CoW (bin) usa a pool. O desbloqueio do Ring3 segue GATED pelo
      ADR-0060, mas a pool já está pronta.
- [ ] **✅ FAT32 4Kn (IMPLEMENTADO — `fat32.rs`):** parser 100% `bytes_per_sector`-driven
      (scans de diretório, entradas FAT, dados — zero 512 hardcoded no caminho);
      `format_fat32_bps(dev, start, sectors, bps, spc)` generalizado (wrapper
      `format_fat32_esp` = 512/1); teste host round-trip com device de 4096B
      (`fat32_4kn_roundtrip`, 512MB). Device layer 4Kn (ATA/NVMe) = AWAITING_HW.
- [ ] **⚠️ Paging preditivo por IA (stretch):** o kernel pode usar a saída do LLM/HW Expert
      para pré-mapear páginas de modelos conhecidos no boot (ex: `hwexpert_v6.bitnet`).
      Não implementar antes do zero-copy.

### PILAR 2.3 — WASMI EM TEMPO DE EXECUÇÃO & HOT-PLUGGING DE SKILLS

**Estado real:** runtime **wasmi real** no_std com fuel + CapGate em `hermes::wasmi_rt`
(self-test `add(2,3)=5` PASS). Promoção ephemeral→WASM **wireada** (`executive.rs`
`tick_observe` chama `check_skill_promotion`; veredito `PromoteSkill` age em `bei_init.rs`;
gatilho ≥3 runs/≥70% em `skill_opt.rs`; 3 testes host).

- [ ] **✅ Runtime wasmi — já implementado.** APIs: `run_wasm(wasm, func, args, caps)`,
      `register_wasm_skill(bytecode, name, desc)`, `run_i32_2`, `self_test()`.
- [ ] **✅ Promoção automática ephemeral→WASM — wireada agora.** Verificar:
      `check_skill_promotion` roda no tick; skills `EphemeralPython` com ≥3 runs/≥70%
      promovem; nenhuma re-promoção por estágio.
- [ ] **✅ Compilação REAL da skill efêmera na promoção (F5) — implementada.**
      `wasm_build::compile_expression(source)` (parser da gramática op-IR constrangida:
      ids → `LocalGet` por ordem de 1º uso ou `pN` explícito; números → `I32Const`;
      `+ - * ( )`; rejeita o resto) + `build_run_module(n, ops)` montam o wasm REAL a
      partir do `EVOLVING.source` da skill — sem placeholder demo quando o source
      parseia (fallback demo honesto quando não). `wasmi_rt::sandbox_validate_and_run`
      testa no sandbox de forma arity-agnóstica (run/_start/main, 0..=4 params).
- [ ] **✅ Persistência da skill promovida no VFS — implementada.** `evolve::
      promote_ephemeral_to_wasm` grava `/mnt/neural/ecosystem/skills/<name>.wasm` via
      `globals::write_vfs` (NeuralFS §12, árvore `ecosystem/skills` criada no boot);
      falha de FS é não-fatal (runtime segue). **Bônus:** a promoção agora marca o estágio
      `WasmPersistent` no `EVOLVING` — sem isso o supervisor re-propunha a mesma skill a
      cada tick (loop de re-promoção); 3 testes host (source real compila+roda,
      fallback demo, nome inválido) + 4 testes do compilador op-IR.
- [ ] **⚠️ ELF nativo Ring 3 (GATED):** modos B/C (Cranelift JIT / nativo) ficam **gated**
      por isolamento de Ring 3 (ADR-0060, `TRY_ENTER_RING3=false` — triple-fault histórico;
      bugs do SESSION_233 corrigidos). Não desbloquear sem review de ADR.

### PILAR 2.4 — TRINITY MoE ("EFEITO MATRIX" — INJEÇÃO INSTANTÂNEA DE CONHECIMENTO)

**Estado real:** `cortex::trinity::TrinityRouter` (690 linhas) tem 7 experts registrados
(`ExpertKind`: HwIdentify, HwControl, RustCoder, DiskDiag, Security, Generator,
SpeechSynth), router MoE treinado (`load_router_from_file`, chamado no boot), e
`classify_intent(text) -> &Expert`. **Porém nenhum expert tem pesos** (`weight: Some` não
existe em lugar nenhum) e **não existe `get_or_mmap_expert`** — as peças (arena, wasmi,
CapGate) existem, mas o fluxo unificado de injeção não.

- [ ] **✅ `get_or_mmap_expert(kind)` + loader na arena (IMPLEMENTADO — `trinity.rs`):**
      `expert_weight_source(kind)` mapeia ExpertKind→FAT (HWEXPRT.V6→HwIdentify,
      RUSTCDR.BITNET→RustCoder); o loader target aloca no bump da arena
      (`global_arena::with_arena`) e lê o arquivo **zero-copy** (`gguf::read_fat_into`,
      cap 16MB); `parse_expert_weights` decodifica v6 (model_type=1→embed hwexpert,
      model_type=2→router) para `PackedTernaryTensor`; `set_expert_weight` não re-injeta;
      `expert_resident_bytes(kind)` telemetria. 4 testes host (parse embed/router, injeção
      única, fallback sem fonte, rejeição de formato).
- [ ] **✅ Fonte de pesos por expert — definida** (`expert_weight_source`): HwIdentify e
      RustCoder têm fonte; demais experts seguem por skill/roteamento. Sem fonte,
      `get_or_mmap_expert` retorna `None` graciosamente.
- [ ] **✅ Fluxo de injeção orquestrado no hermes — IMPLEMENTADO (`trinity_inject.rs`):**
      `inject_capability(kind, class, wasm)` faz os 3 passos: (1) `ensure_expert_resident`
      garante pesos na arena via **bridge instalado pelo bin** (`install_trinity_mmap_bridge`
      → router REAL de `init_trinity`; o `globals::TRINITY` do hermes é vazio — mesmo
      padrão do bridge VFS/net, evita duplicar o registro dos 7 experts); (2)
      `register_wasm_skill` registra a skill no SKILL_REGISTRY com o nome canônico
      (`cortex::trinity::expert_kind_name`, fonte única); (3) `grant_fe(fe_for_class)` no
      CapGate real. `InjectOutcome::Injected{cap, expert, bytes}` vs `Degraded` (sem fonte
      de pesos — skill+cap ainda valem; honesto). 4 testes host (degraded com cap+skill,
      bridge→Injected, classe sem FE, nomes batem com o registry).
- [ ] **⚠️ Chamada real no runtime:** `inject_capability` ainda não é chamado quando um
      agente enfrenta tarefa inédita (HW PnP / protocolo novo) — o hook no supervisor é o
      próximo passo (ligar ao `hw_pnp`/`PromoteSkill`).
- [ ] **✅ Hot-swap de experts — infraestrutura pronta** (`expert_lifecycle`, arena);
      validar <10ms só em runtime com pesos reais.

### PILAR 2.5 — HERMES ORQUESTRADOR SUPREMO

**Estado real:** budget BEI (`ProceedWithBudget`) existe no supervisor; **HalOffer backoff
implementado** (`k_hal::offer.rs`: `ABSENT_BACKOFF_BASE=50` → dobra até `CAP=3200` ticks
por `DeviceClass`, reset no bind/release, silencioso no meio; 2 testes host); mesh P2P com
expurgo dinâmico (s242: `cleanup_stale_nodes` >30s + `cleanup_peer_health_ttl` +
`capacity_weighted_assign` health-aware).

- [ ] **✅ Backoff do HalOffer — já implementado.** Efeito direto no agente `vision`:
      classe `video` Absent não polui mais logs/ticks (intervalo efetivo 50→3200 ticks,
      que em ticks de ~20ms ≈ 1s→64s — a escala pedida "1s, 2s, 4s..." está coberta).
- [ ] **✅ Mesh P2P pruning — já implementado.** Verificar: nós inativos (ex: 4 e 5) são
      removidos da tabela CRDT após heartbeats perdidos; dispatch de `matmul` cai para
      local/Master sem stall.
- [ ] **⚠️ EventBus MPMC (PENDENTE — wiring, não primitiva):** a primitiva correta **já
      existe** — `k_nano::mpmc::MpmcQueue<T: Copy + Default>` (Vyukov: sequências
      `AtomicU64` por slot + CAS; `try_enqueue`/`try_dequeue` non-blocking; usada em
      `cortex/cellular.rs`). Substituir `BoundedChannel` (TicketLock+VecDeque) onde o
      throughput importar. **NUNCA** escrever um ring "lock-free" caseiro com
      `buffer[tail]` sem CAS — não é MPMC sound (ver §4.2).

### PILAR 2.6 — JARBAS = JARVIS ENGINE (INTERFACE MCU & OS)

**Estado real:** desktop 1280x800@32bpp com double-buffer + compositor + mouse PS/2
(`jarbas/display/*`, ADR-0058 cards); Piper TTS é o **default** de voz; formant synth é
fallback **runtime-condicional** (só quando PIPER.BIN ausente/corrompido — não é stub);
mesh dashboard JSON integrado no DisplayAgent.

- [ ] **✅ HUD/desktop — já implementado.** Manter fluidez 60Hz; eventos PS/2 + xHCI.
- [ ] **✅ Voz Piper TTS — default real.** Mapear `PIPER.BIN` via HHDM na arena (zero-copy
      do 2.2 ajuda); **não remover** o fallback de formantes (violaria a premissa AIOS de
      graceful degradation).
- [ ] **⚠️ Assistente pró-ativo (parcial):** `metrics_agent`/`soul_mirror` existem;
      completar a sugestão de ações corretivas a partir dos logs do SelfHeal no HUD.

---

## 4. EXEMPLOS PRÁTICOS DE CÓDIGO (APIs REAIS DO REPO)

### 4.1 — `PHYS_MEM_OFFSET` em T+0 (já existe — padrão a seguir)

```rust
// crates/k_nano/src/memory.rs (real)
pub static PHYS_MEM_OFFSET: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

// Gravado no boot (main.rs) ANTES de IDT/heap/drivers:
//   PHYS_MEM_OFFSET.store(physical_memory_offset, Ordering::Release);

// Uso correto em drivers — NUNCA dereferenciar físico puro:
let va = k_nano::memory::phys_to_virt(phys);   // phys + offset (Relaxed load)
```

### 4.2 — Barramento MPMC lock-free (JÁ EXISTE — usar, não reescrever)

⚠️ O padrão `push` com `Relaxed` + escrita crua em `buffer[tail]` **sem CAS** (snippet
clássico de "lock-free ring") é **race condition entre produtores** e UB de aliasing
(falta `UnsafeCell`). Use a implementação Vyukov do repo:

```rust
use k_nano::mpmc::MpmcQueue;

// Fila de eventos do Hermes (T: Copy + Default obrigatório)
static EVENTS: spin::Mutex<Option<MpmcQueue<u64>>> = spin::Mutex::new(None);

fn publish_event(ev: u64) {
    let q = EVENTS.lock();
    if let Some(q) = q.as_ref() {
        q.try_enqueue(ev); // non-blocking; false = cheia (política: drop/log)
    }
}

// Consumidor (qualquer core):
//   while let Some(ev) = q.try_dequeue() { dispatch(ev); }
```

### 4.3 — SPSC ring corrigido (se precisar de fila por núcleo sem heap)

O snippet original (um produtor, um consumidor) só é sound como **SPSC** — com `UnsafeCell`,
`Send/Sync` justificados, cheque de cheia antes de escrever e `take()` no consumidor:

```rust
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscRing<T, const CAP: usize> {
    buffer: [UnsafeCell<Option<T>>; CAP], // UnsafeCell OBRIGATÓRIO (escreve via &self)
    head: AtomicUsize,  // consumidor
    tail: AtomicUsize,  // produtor
}
unsafe impl<T: Send, const CAP: usize> Send for SpscRing<T, CAP> {}
unsafe impl<T: Send, const CAP: usize> Sync for SpscRing<T, CAP> {}

impl<T, const CAP: usize> SpscRing<T, CAP> {
    pub const fn new() -> Self {
        SpscRing {
            buffer: [const { UnsafeCell::new(None) }; CAP],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
    /// Produtor (ÚNICO). Err(value) se cheia — não bloqueia.
    pub fn push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = tail.wrapping_add(1);
        if next_tail.wrapping_sub(self.head.load(Ordering::Acquire)) >= CAP {
            return Err(value);
        }
        unsafe { *self.buffer[tail % CAP].get() = Some(value); }
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }
    /// Consumidor (ÚNICO).
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) { return None; }
        let slot = unsafe { (*self.buffer[head % CAP].get()).take() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        slot
    }
}
```

> Nota: `k_nano::smp::spsc::SpscQueue` (heap) e `work_stealing::WorkStealingDeque`
> (Chase-Lev, stack) também já existem — prefira-os a criar uma terceira.

### 4.4 — Trinity `get_or_mmap_expert` (A IMPLEMENTAR — template com API real)

```rust
// crates/cortex/src/trinity.rs — gap real do Pilar 2.4
// Hoje: 7 experts com `weight: None`; este método preenche na arena sob demanda.
use crate::arena::CORTEX_ARENA_VIRT;

impl TrinityRouter {
    /// Efeito Matrix: localiza/carrega os pesos do expert na Cortex Arena.
    /// Retorna None graciosamente se a fonte de pesos não existe (nunca derruba boot).
    pub fn get_or_mmap_expert(&mut self, kind: ExpertKind) -> Option<&Expert> {
        let idx = self.experts.iter().position(|e| e.kind == kind)?;
        if self.experts[idx].weight.is_some() {
            return Some(&self.experts[idx]); // já residente na arena
        }
        // 1. Fonte de pesos por ExpertKind (a criar — ver §2.4):
        //    ex: HwIdentify -> blob de `hwexpert_v6.bitnet` (loader v6 JÁ existe)
        let blob = crate::weights::find_expert_weights(kind)?; // TODO: criar
        let packed = PackedTernaryTensor::load_into_arena(&blob, CORTEX_ARENA_VIRT)?; // TODO: criar
        self.experts[idx].weight = Some(packed);
        Some(&self.experts[idx])
    }
}
```

O fluxo de **injeção de capacidade** (skill WASM + CapGate) pertence ao **hermes** (R3),
que depende de k_hal e cortex:

```rust
// crates/hermes/src/ (orquestração) — shape correto:
//   1. hermes::wasmi_rt::register_wasm_skill(&bytecode, name, desc)?;
//   2. k_hal::cap_gate::grant_fe(HalCap::Net);      // API real: grant_fe(HalCap)
//   3. validação runtime: k_hal::cap_gate::check_fe(caller_ring, cap, has_cap)
```

### 4.5 — Lazy FPU/SIMD context switch via CR0.TS (A IMPLEMENTAR — padrão x86_64)

Elimina salvar XMM/YMM/ZMM a cada tick (custo alto para inferência):

```rust
use x86_64::registers::control::{Cr0, Cr0Flags};

// No switch-out do contexto (scheduler):
Cr0::update(|flags| flags.insert(Cr0Flags::TASK_SWITCHED));

// Handler #NM (vetor 7) — restaura o estado FPU do novo contexto e limpa TS:
extern "x86-interrupt" fn nm_handler(_: x86_64::structures::idt::InterruptStackFrame) {
    x86_64::instructions::fpu::FxSave::restore(&CURRENT_FPU_STATE); // a criar
    Cr0::update(|flags| flags.remove(Cr0Flags::TASK_SWITCHED));
}
```

---

## 5. PROTOCOLO DE REFATORAÇÃO PASSO A PASSO

**Fase 0 — Varredura de stubs (auditoria):** `grep -rn` por `CpuFallback|FORMANT_SYNTH|
backend=RAM|TRY_ENTER_RING3=false|TODO: criar`. Classificar cada ocorrência como
(1) fallback runtime-condicional legítimo (premissa AIOS) ou (2) stub morto a eliminar.
**Não** remover fallback legítimo.

**Fase 1 — Core (`k-nano`):** verificar T+0 ✅ → implementar **zero-copy DMA disco→arena**
(2.2) → testar com modelo real no FAT32. Não tocar no frame allocator (ownership check já
feito).

**Fase 2 — HAL & SMP (`k-hal`, `k-nano/smp`):** calibração TSC/HPET do SIPI (opcional) →
`ap_pollable()` + IDT compartilhada p/ APs trabalharem (pré-requisito do scheduler) →
lazy FPU/CR0.TS. Validar com `-smp 4` no QEMU.

**Fase 3 — Cortex & Trinity:** fonte de pesos por `ExpertKind` → `get_or_mmap_expert` →
fluxo de injeção no hermes (`register_wasm_skill` + `grant_fe`). Medir tempo de injeção
(<10ms alvo) com log.

**Fase 4 — Hermes & Jarbas:** EventBus → `MpmcQueue` onde o throughput importar (manter
`BoundedChannel` como mailbox) → persistência da skill promovida no VFS →
`metrics_agent` pró-ativo no HUD.

---

## 6. CHECKLIST DE VERIFICAÇÃO (OBRIGATÓRIO POR ITEM FECHADO)

- [ ] `cargo clean -p neural-kernel` (regra: incremental mascara erros) seguido de
      `cargo check --release` → **0 erros** (warnings dead-code = política Known Warnings)
- [ ] Testes host: `cargo test -p k-nano --lib` (125/125), `cargo test -p cortex --lib`
      (28/28), `cargo test -p k-hal` (7/7), `cargo test -p hermes` (56/56),
      `cargo test -p k_ai --lib` (22/22); novos testes por feature implementada
- [ ] `tools/check_duplication.py` — nenhum `.rs` não-facade duplicado entre crates
- [ ] `tools/update_tecnologias.py` se tecnologia nova entrou
- [ ] Lógica nova nas crates K³CHJ; bin só wire/`pub use` (regra emagrecer bin)
- [ ] Evidência de boot registrada (smoke logs em docs/evidence) — validação QEMU WHPX +
      HW real; `AWAITING_HW` explícito quando não testável
- [ ] Docs: AGENTS.md + SESSION + IDEA_BANK atualizados por item fechado
- [ ] Gate v2.0.0 **NÃO** declarado sem review de ADR + OK do maintainer

---

## 7. ORDEM DE EXECUÇÃO RECOMENDADA (por valor/risco)

1. **Zero-copy FAT32→frames/arena** (2.2) — ✅ implementado (leitura direta no destino;
   NVMe DMA real = follow-up)
2. **`get_or_mmap_expert` + fonte de pesos** (2.4) — destrava o Efeito Matrix de verdade
3. **Persistência da skill promovida no VFS** (2.3) — ✅ implementado (op-IR→WASM real +
   `/mnt/neural/ecosystem/skills/`; próximo: bind da skill WASM no hermes via
   `register_wasm_skill` + `grant_fe(HalCap)` p/ fechar o Efeito Matrix)
4. **EventBus MPMC com `MpmcQueue`** (2.5) — concorrência sem locks onde importa
5. **APs como workers (IDT + `ap_pollable`) + scheduler work-stealing** (2.1) — projeto
   grande, requer QEMU `-smp 4` e validação em HW real
6. **Lazy FPU/CR0.TS** (2.1/2.2) — ganho de ciclos para inferência
