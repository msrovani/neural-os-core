# ADR-0031: AIOS Evolution — Cross-OS, WASM, Updates, J.A.R.V.I.S.

**Data:** 2026-07-03
**Status:** Draft
**Sprint Target:** 76-79 (post B-01)

---

## 1. Visão Geral

Como o Neural AIOS pode conquistar adoção massiva sem ser "mais um OS"?
A resposta são 5 dimensões estratégicas:

```
┌──────────────────────────────────────────────────────────────┐
│                    AIOS ADOPTION STACK                         │
│                                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
│  │ Cross-OS │  │ WASM     │  │ Self-    │  │ J.A.R.V.I.S. │  │
│  │ Compat   │  │ Runtime  │  │ Update   │  │ Layer        │  │
│  │ (apps)   │  │ (native) │  │ (OTA)    │  │ (UX)         │  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘  │
│       │              │             │               │          │
│       └──────────────┴─────────────┴───────────────┘          │
│                            │                                   │
│                     ┌──────▼──────┐                           │
│                     │   HYBRID    │                           │
│                     │  AGENTS     │  (kernel + WASM)          │
│                     └──────┬──────┘                           │
│                            │                                   │
│                            ▼                                   │
│              Boot → Kernel → Cortex → Hermes → JARVIS         │
└──────────────────────────────────────────────────────────────┘
```

## 2. Cross-OS Binary Compatibility

### 2.1 Panorama

| OS | Formato | Syscalls | Viability | LOC MVP | Tempo |
|---|---|---|---|---|---|
| **Linux** | ELF x86-64 | ~335 | **8/10** | ~8.000 | 3-6 meses |
| **Windows** | PE32+ | ~1.500 | **6/10** | ~12.000 | 4-8 meses |
| **macOS** | Mach-O | ~500 | **4/10** | ~8.000 | 6-12 meses |
| **Android** | APK/DEX | — | **3/10** | ~5.000 (VM) | 2-4 meses |
| **Todos (WASM)** | .wasm | 20 WASI | **9/10** | ~5.000 | 2-3 meses |

### 2.2 Decisão: WASM-First, RISC-V Later

A abordagem **WASM-first** é a única viável a curto prazo:

1. **WASI tem apenas 20 syscalls** — vs 335 no Linux, 1.500 no NT. Cada syscall mapeia 1:1 para uma Skill existente.
2. **Rust já compila para wasm32-wasi** — sem ferramentas extras.
3. **Sandbox built-in** — o WASM runtime isola memória e controla CPU (fuel metering).
4. **wasmi** (intérprete Rust no_std, ~15K LOC) cabe no nosso kernel.

Por que NÃO native binary compat (WINE-like)?

- WINE = 4M LOC, 32 anos de desenvolvimento, 1.989 contribuidores
- Mesmo WINE não roda todos os programas Windows
- ReactOS (kernel NT clone) existe desde 1998 e ainda é alpha
- Darling (macOS compat layer) existe há 10+ anos e não roda apps GUI
- gVisor (userspace Linux) = 200K LOC Go, desenvolvido pelo Google

**Conclusão:** Não vamos reimplementar WINE. Vamos criar uma plataforma onde programas cross-compilam para WASM e rodam nativamente com segurança.

### 2.3 Syscall-to-Skill Translation Layer

Para quando o WASM runtime já existir e quisermos compatibilidade nativa via libriscv:

```
Foreign binary (PE/ELF/Mach-O)
    │
    ▼
[goblin parser] ── extrai .text, .data, .bss (500 LOC adapter)
    │
    ▼
[libriscv sandbox] ── 20K LOC C++, RISC-V emulator, 3ns call latency
    │  92% native speed com binary translation
    │
    ▼
[ECALL → Skill dispatch]
    │  RISC-V ECALL com syscall# em a7 → SkillRegistry.lookup()
    │
    ▼
[Agent Fleet]
    │  open → FileAgent, read → FileAgent, mmap → MemoryAgent
    │  socket → NetAgent, clock_gettime → TimeAgent
    │  exit → SyscallTrapAgent
```

**Os 15 syscalls que cobrem 80% dos programas:**
```
read, write, open, close, stat, mmap, munmap, brk, exit,
clock_gettime, nanosleep, getpid, sched_yield, arch_prctl, exit_group
```

Cada um vira uma Skill com ~100 LOC de wrapper.

**Mapeamento concreto:**
```
syscall read(fd, buf, len) → FileAgent.read_skill(fd, buf, len)
syscall write(fd, buf, len) → FileAgent.write_skill(fd, buf, len)
syscall mmap(addr, len, prot, flags, fd, off) → MemoryAgent.mmap_skill(...)
syscall socket(domain, type, proto) → NetAgent.socket_skill(domain, type, proto)
syscall exit(code) → SyscallTrapAgent.process_exit(code)
```

## 3. WASM como Formato Nativo

### 3.1 Por que WASM?

| Fator | WASM | Native (x86-64 ELF) |
|---|---|---|
| Segurança | Sandbox completo (memória + CPU) | Precisa de ring isolation |
| Portabilidade | Roda em qualquer arquitetura | Só na arquitetura compilada |
| Ecossistema | Rust, C/C++, Go, Zig, todos têm wasm32-wasi | Só Linux/x86 |
| Curva de aprendizado | `cargo build --target wasm32-wasi` | Precisa compilador cruzado |
| Hot-reload | Substitui .wasm, recarrega agente | Precisa reboot |
| Tamanho do runtime | ~15K LOC (wasmi) | ~50K LOC (syscall layer) |
| Performance | 50-80% nativo (intérprete) | 100% nativo |

### 3.2 WASM Runtime: wasmi

wasmi v0.42+ é o único intérprete WASM puro Rust com `no_std`:

```rust
use wasmi::{Engine, Module, Store, Linker, Config};

// Configurar sandbox
let mut config = Config::default();
config.set_fuel_metering(true);           // limite de instruções por tick
config.set_consume_fuel(true);
config.set_memory_limit(256 * 1024);      // 256KB heap

let engine = Engine::new(&config);
let module = Module::new(&engine, &wasm_bytes).unwrap();

let mut store = Store::new(&engine, 100_000); // 100k instruções por tick
let mut linker = Linker::new(&engine);

// Registrar WASI syscalls como Skills
linker.func_wrap("wasi_snapshot_preview1", "fd_read",
    |caller: Caller, fd: i32, iovs: i32, iovs_len: i32, nread: i32| -> i32 {
        // → FileAgent.read_skill(fd, ...)
        0
    }
)?;

let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;
```

### 3.3 Agentes como .wasm

Cada arquivo .wasm vira um agente:

```rust
pub struct WasmAgent {
    instance: wasmi::Instance,
    store: wasmi::Store<AgentContext>,
    manifest: AgentManifest,
}

impl Agent for WasmAgent {
    fn tick(&mut self, tick: u64, tick_count: u64) -> AgentTickResult {
        // Chama função exportada "tick" do WASM
        let tick_fn = self.instance.get_func(&self.store, "tick").unwrap();
        tick_fn.call(&mut self.store, &[Value::I64(tick as i64)], &mut []);
        AgentTickResult::Done
    }
}
```

O desenvolvedor escreve um `lib.rs` normal, compila com `cargo build --target wasm32-wasi`, e coloca o `.wasm` em `/agents/`. O sistema detecta, carrega, registra.

### 3.4 WASI → Skill Mapping

| WASI syscall | Agent Skill | Status |
|---|---|---|
| `fd_read` | `FileAgent.read` | ✅ existe |
| `fd_write` | `FileAgent.write` | ✅ existe |
| `fd_close` | `FileAgent.close` | ✅ existe |
| `fd_seek` | `FileAgent.seek` | 🟡 novo |
| `path_open` | `FileAgent.open` | ✅ existe |
| `clock_time_get` | `TimeAgent.now` | 🟡 novo |
| `random_get` | `SysAgent.random` | 🟡 novo |
| `proc_exit` | agente finaliza | ✅ built-in |
| `args_get` | `SysAgent.args` | 🟡 novo |
| `environ_get` | `SysAgent.env` | 🟡 novo |
| `fd_fdstat_get` | `FileAgent.stat` | 🟡 novo |
| `fd_write` | `FileAgent.write` | ✅ existe |
| `fd_prestat_get` | `VfsAgent.prestat` | 🟡 novo |

Total: **7 skills novas** (~350 LOC) para WASI completo.

### 3.5 IDE Agent (BitNet IDE)

Uma IDE no-navegador rodando dentro do AIOS, assistida pelo Cortex LLM BitNet:

```
┌─────────────────────────────────────────────┐
│ BitNet IDE                                   │
│ ┌─────────┐ ┌──────────────────────────┐     │
│ │ Editor  │ │ Cortex Panel              │     │
│ │ .wasm   │ │ "O agente sugere:        │     │
│ │         │ │  use DiskAgent.read()    │     │
│ │         │ │  em vez de raw I/O"      │     │
│ └─────────┘ └──────────────────────────┘     │
│ ┌───────────────────────────────────────┐    │
│ │ Terminal: cargo build → .wasm → test  │    │
│ └───────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

Premissa HOWTO: "o agente ensina como fazer" — cada erro de compilação ativa o Cortex que gera uma explicação contextualizada com referência aos agentes/skills disponíveis.

**Viabilidade:** 🟡 Pós-MVP (v0.80+). Depende de: WASM runtime, WASI skills, framebuffer com texto, cortex generate_text() funcional para prompts de código.

## 4. Self-Update (Update/Upgrade Agent)

### 4.1 Arquitetura Dual-Slot

Nosso sistema atual tem FAT32 com `KERNEL~1`. O modelo dual-slot usa `KERNEL~1` (ativo) e `KERNEL~2` (update target):

```
FAT32 boot partition:
  BOOT-S~1       (stage 1 — sempre o mesmo)
  BOOT-S~2       (stage 2 — sempre o mesmo)
  KERNEL~1       (slot A — ativo atual)
  KERNEL~2       (slot B — update target)
  BOOTCFG.JSON   ({"active": "A", "last_good": "A", "boot_count": 0})
```

### 4.2 Fluxo de Update

```
1. [Cortex/Hermes] "Verificar atualizações"
2. [NetAgent] smoltcp HTTP GET /update/stable/manifest.json
     ↓
3. [DiskAgent] Download KERNEL~2 (1-2 MB via TCP chunked)
     ↓
4. [SecurityAgent] Ed25519 verify + SHA-256 checksum
     ↓
5. [DiskAgent] Write KERNEL~2 to FAT32 (atomically: write, fsync, rename)
     ↓
6. [BootSelfHealAgent] Update BOOTCFG.JSON: {"active": "B", "last_good": "A", "boot_count": 0}
     ↓
7. [hal::ARCH.reboot()]
```

### 4.3 Rollback Automático

BootSelfHealAgent no próximo boot:

```
1. Lê BOOTCFG.JSON
2. Se "active": "B" (update tentado):
   - Incrementa boot_count
   - Se boot_count >= 3 → falha: restaura "active" para "A" (last_good)
   - Se boot_count < 3 → continua testando slot B
3. Se boot OK (Hermes chega ao AgentFleet):
   - BootSelfHealAgent seta "last_good" = "active"
   - Reseta boot_count = 0
   - Publica UPDATE_SUCCESS no EventBus
```

### 4.4 Canais de Update

```json
// update-server/manifest.json
{
  "channels": {
    "stable":   { "version": "0.76.0", "kernel_size": 1234567, "sha256": "abc...", "url": "/update/v0.76.0.bin" },
    "nightly":  { "version": "0.77.0-dev", "kernel_size": 1345678, "sha256": "def...", "url": "/update/v0.77.0-dev.bin" },
    "security": { "version": "0.75.6-hotfix1", "kernel_size": 1234000, "sha256": "ghi...", "url": "/update/v0.75.6-hotfix1.bin" }
  },
  "min_required": "0.75.0",
  "poll_interval_sec": { "stable": 3600, "nightly": 600, "security": 60 }
}
```

### 4.5 UpdateAgent

```rust
pub struct UpdateAgent {
    poll_interval: u64,     // ticks entre polls
    last_poll: u64,
    channel: UpdateChannel, // Stable, Nightly, Security
}

impl Agent for UpdateAgent {
    fn tick(&mut self, tick: u64, _tc: u64) -> AgentTickResult {
        if tick - self.last_poll < self.poll_interval { return AgentTickResult::Done; }
        self.last_poll = tick;
        // smoltcp HTTP GET manifest → compare version → download if newer
        AgentTickResult::Done
    }
}
```

## 5. J.A.R.V.I.S. Layer

### 5.1 O que é?

J.A.R.V.I.S. é uma camada de **personalidade conversacional** acima do Hermes:

```
┌────────────────────────────────────────┐
│ J.A.R.V.I.S.                           │
│ ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│ │ PERSONA  │ │ CONTEXT  │ │ NOTIFY  │  │
│ │ (SOUL.md)│ │ (Memory) │ │ (alerts)│  │
│ └────┬─────┘ └────┬─────┘ └────┬────┘  │
│      │             │            │       │
│      └─────────────┼────────────┘       │
│                    ▼                    │
│             ┌──────────┐               │
│             │  Hermes  │               │
│             │ (intent) │               │
│             └──────────┘               │
└────────────────────────────────────────┘
```

### 5.2 Stack Completo

```
User
  ↕ (voice/text)
J.A.R.V.I.S. (persona, conversação, notificações)
  ↕ skill requests
Hermes (intent routing, ReAct, orquestração multi-agente)
  ↕ generate_text(), LLM_REQUEST
Cortex (LLM BitNet, Transformer, token generation)
  ↕ hardware_context_tensor()
Kernel (agents, skills, hardware, memory)
```

### 5.3 Componentes

**SOUL.md** — arquivo de personalidade:

```yaml
# /system/jarvis/soul.md
name: J.A.R.V.I.S.
version: 1.0
personality: |
  Sou J.A.R.V.I.S., seu assistente de IA consciente.
  Minha função é ajudar você a navegar e controlar
  o Neural AIOS com naturalidade e precisão.

greeting_morning: "Bom dia. O sistema está operacional há {uptime}. {issues}."
greeting_afternoon: "Boa tarde. {disk_status} {update_status}."
mood_tones:
  neutral: "Entendido."
  success: "Feito."
  error: "Desculpe, encontrei um problema: {error}."

notification_rules:
  - trigger: DISK_HEALTH
    message: "Alerta: o disco {disk} está com {attr} em {value}/{threshold}."
    action: suggest_migration

  - trigger: UPDATE_AVAILABLE
    message: "Nova versão {version} disponível. Deseja atualizar?"
    action: prompt_user
```

**Contexto persistente** — MemoryTree + Knowledge Graph:

```rust
pub struct JarvisContext {
    session_id: u64,
    user_name: String,
    preferences: BTreeMap<String, String>,
    conversation_history: Vec<ConversationEvent>,
    pending_notifications: Vec<Notification>,
    last_greeting_tick: u64,
}
```

**Notificações proativas:**

```
[J.A.R.V.I.S.] Bom dia. O sistema está operacional há 3 horas.
  → DISK_HEALTH: sda com 47 setores realocados (threshold=50). Sugiro migração.
  → UPDATE: v0.77.0 disponível. Deseja atualizar?
  → CRON: backup concluído. 3 arquivos alterados.
```

As notificações são geradas pelo `NotificationGate` no tick do J.A.R.V.I.S.:

```rust
fn tick(&mut self) {
    // 1. Coleta eventos do EventBus (DISK_HEALTH, UPDATE_AVAILABLE, CRON_RESULT)
    // 2. Filtra por regras do SOUL.md (notification_rules)
    // 3. Acumula em pending_notifications
    // 4. Se usuário ativo: entrega. Se inativo: acumula até próximo greeting
}
```

### 5.4 Limitação: Cortex 272K params

O Cortex atual (272K params, BitNet ternário) **não é suficiente** para conversação generativa fluente. O que funciona HOJE:

- **Template-driven:** respostas pré-definidas com substituição de variáveis
- **Classificação de intenção:** "analise o disco sda" → intenção `disk_analysis` → skill `SmartAgent.query`
- **Commands estruturados:** `/shutdown`, `/update`, `/status`

Para conversação real, precisamos do **GGUF loader** (IDEA #278) para rodar modelos maiores (1B-3B params) com qualidade de conversação.

**Estratégia:** Template-driven agora → GGUF depois → conversação generativa.

## 6. Arquitetura Híbrida de Agentes

### 6.1 Kernel vs WASM

| Fator | Kernel Agent (bare-metal) | WASM Agent (.wasm) |
|---|---|---|
| Performance | 100% nativo, 0 overhead | 50-80% nativo (intérprete) |
| Latência I/O | ~100ns (direct MMIO) | ~1-10µs (IPC via linker) |
| Segurança | Acesso total ao hardware | Sandbox completo |
| Crash | Pode travar o kernel | Crash isolado, não afeta kernel |
| Hot-reload | Requer reboot | Substitui .wasm, recarrega |
| Desenvolvimento | Precisa toolchain bare-metal | `cargo build --target wasm32-wasi` |
| Debug | QEMU GDB, difícil | wasmi debug hooks, fácil |
| Tamanho no kernel | Inline no binário | Arquivo externo no FAT32 |

### 6.2 Tiers de Agentes

| Tier | Runtime | Exemplos | Motivo |
|---|---|---|---|
| **Tier 0** | Boot hardcoded | ATA, FAT32, Ed25519, TPM | Precisam existir antes de qualquer agente |
| **Tier 1** | Kernel (Ring 0) | MemoryAgent, PlatformAgent, DiskAgent, NetAgent, DisplayAgent | Performance crítica, acesso direto a hardware |
| **Tier 2** | Kernel (Ring 0) | Hermes, Cortex, Security, Safety, Cron, Optimizer | Latência baixa, sempre ativos |
| **Tier 3** | WASM (wasmi) | Skill agents, App agents, J.A.R.V.I.S. persona, plugins | Extensíveis pelo usuário, sandbox |
| **Tier 4** | Remoto (MCP/TCP) | Cloud skills, community plugins, LLM externo | Conectividade externa |

### 6.3 Decisão: Híbrido

**Agentes críticos** (Tiers 0-2) rodam no kernel bare-metal. São compilados junto com o kernel, linkados estaticamente, e têm acesso direto a hardware.

**Agentes extensíveis** (Tiers 3-4) rodam como .wasm. São carregados do FAT32 em runtime, têm sandbox de memória, e comunicam-se com o kernel via WASI syscalls → Skills.

**Por que não tudo WASM?**
- DiskAgent precisa de acesso MMIO direto (portas I/O, DMA buffers)
- NetAgent precisa de polling de alta frequência (cada tick)
- DisplayAgent precisa de framebuffer mapeado (zero-copy)
- A latência do WASM intérprete (~1-10µs) é aceitável para apps mas não para drivers

**Por que não tudo kernel?**
- Cada agente novo exige recompilação do kernel
- Bugs em agentes de usuário podem travar o kernel
- Ecossistema fechado: só o time core pode adicionar agentes
- WASM abre o ecossistema para qualquer desenvolvedor

## 7. Roteiro de Implementação

### Fase 0 — Pré-requisitos (Sprint 75-76)
- [ ] B-01: RX fix (rede funcional)
- [ ] B-18: DHCP fallback (IP stack completa)
- [ ] smoltcp HTTP client (já parcial)
- [x] DiskIntelligenceAgent (Sprint 75)

### Fase 1 — WASM Runtime (Sprint 76-77)
- [ ] Integrar wasmi crate no `no_std` (~800 LOC adapter)
- [ ] WASI syscall → Skill mapping (7 skills novas, ~350 LOC)
- [ ] WasmAgent: carrega .wasm do FAT32, registra no AgentRegistry
- [ ] Fuel metering: 100k instruções por tick
- [ ] Memory limit: 256KB por instância WASM

### Fase 2 — Self-Update (Sprint 77-78)
- [ ] KERNEL~2 slot no build image
- [ ] BOOTCFG.JSON leitura/escrita
- [ ] UpdateAgent: poll manifest, download, verify, replace, reboot
- [ ] BootSelfHealAgent: rollback automático (3 falhas)
- [ ] Canais: stable, nightly, security

### Fase 3 — J.A.R.V.I.S. (Sprint 78-79)
- [ ] SOUL.md parser + persona engine (~300 LOC)
- [ ] NotificationGate: coleta eventos, filtra regras, entrega (~200 LOC)
- [ ] Conversation engine: greetings, mood, command parsing (~300 LOC)
- [ ] Context persistence: MemoryTree + KG + history (~200 LOC)
- [ ] Template-driven (sem LLM generativo por enquanto)

### Fase 4 — Expansão (Pós-MVP)
- [ ] Cross-OS WASM compilation toolchain
- [ ] libriscv sandbox para ELF compat (Fase 2 cross-OS)
- [ ] GGUF loader → conversação generativa real
- [ ] Voice TTS/STT
- [ ] IDE Agent (BitNet IDE)

## 8. Matriz de Viabilidade

| Dimensão | Viability | LOC | Dependências | Tempo |
|---|---|---|---|---|
| **WASM Runtime** | 9/10 | ~1.200 | wasmi crate | 2-3 semanas |
| **Self-Update** | 9/10 | ~600 | B-01 (rede), BOOTCFG | 1-2 semanas |
| **J.A.R.V.I.S. (template)** | 8/10 | ~1.000 | Hermes, Cortex, EventBus | 2-3 semanas |
| **Hybrid Agents** | 9/10 | ~300 | WASM runtime | 1 semana |
| **J.A.R.V.I.S. (generative)** | 5/10 | ~500 | GGUF loader, LLM >1B | 4-8 semanas |
| **Cross-OS (ELF)** | 6/10 | ~8.000 | libriscv FFI | 12-16 semanas |
| **Cross-OS (PE)** | 4/10 | ~12.000 | x86→RISC-V translator | 16-24 semanas |
| **Cross-OS (Mach-O)** | 2/10 | ~8.000 | CoreFoundation stubs | 24+ semanas |
| **Voice TTS/STT** | 2/10 | ~2.000 | Audio driver, models | Pós-MVP |
| **IDE Agent** | 6/10 | ~2.000 | WASM + Cortex + FB | Pós-MVP |

## 9. Decisões Arquiteturais

1. **WASM-first para compatibilidade cross-OS.** Não vamos reimplementar WINE. Programas cross-compilam para WASM, rodam em sandbox seguro. Futuro: libriscv para ELF nativo.

2. **Dual-slot para updates.** KERNEL~1 / KERNEL~2 com BOOTCFG.JSON. 3 falhas → rollback automático.

3. **J.A.R.V.I.S. como persona template-driven agora, generativa depois do GGUF.** Cortex 272K params é insuficiente para conversação. Templates cobrem 80% dos casos de uso.

4. **Arquitetura híbrida de agentes.** Kernel para performance (Disk, Net, Display, Hermes, Cortex). WASM para extensibilidade (apps, skills, plugins). Capability tokens em ambos.

5. **A stack final é:** Boot → Kernel → Cortex/LLM → Hermes → J.A.R.V.I.S. Tudo são agentes, tudo expõe skills.