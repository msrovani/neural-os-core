# ADR-0032: WASM Agent Apps — O Formato Nativo do Neural AIOS

**Data:** 2026-07-03
**Status:** Draft
**Sprint Target:** 76-77

---

## 1. Visão

```
┌──────────────────────────────────────────────────────────────┐
│                      COMO NASCE UM APP                        │
│                                                               │
│   Dev escreve      cargo build      Drop .wasm      PRONTO   │
│   Rust normal   ──▶ wasm32-wasi  ──▶ em /agents/  ──▶ app    │
│                                                               │
│   Sem bare-metal. Sem linker script. Sem reboot.              │
│   Um `cargo build` e o agente está vivo no AIOS.             │
└──────────────────────────────────────────────────────────────┘
```

O Neural AIOS adota **WebAssembly (WASM)** como formato nativo para aplicações.
Cada arquivo `.wasm` é um **agente completo**: tem identidade, ciclo de vida,
acesso controlado a skills do kernel, e executa em sandbox isolada.

Isto substitui o modelo tradicional de SO onde apps são binários ELF/PE
linkados a syscalls. Aqui, o app é um **agente** e as syscalls são **skills**.

## 2. Por que WASM?

| Fator | ELF/PE nativo | WASM |
|---|---|---|
| Compilação | Precisa cross-compiler + linker script | `cargo build --target wasm32-wasi` |
| Segurança | Precisa de ring isolation (complexo) | Sandbox built-in (memória + CPU) |
| Crash | Pode travar o kernel | Crash isolado, kernel continua |
| Hot-reload | Requer reboot | Substitui .wasm, recarrega |
| Debug | QEMU GDB | wasmi debug hooks |
| Ecossistema | Só devs bare-metal | Qualquer dev Rust no mundo |
| Tamanho | Linkado no kernel (~15 MB) | Arquivo externo (~50-500 KB) |

## 3. Arquitetura

```
┌──────────────────────────────────────────────────────────────────┐
│                        NEURAL AIOS KERNEL                          │
│                                                                    │
│  ┌────────────────┐  ┌──────────────┐  ┌───────────────────────┐  │
│  │ AgentRegistry  │  │ wasmi 0.42+  │  │ WASI → Skill Bridge   │  │
│  │ (já existe)    │  │ (intérprete) │  │ (20 syscalls)         │  │
│  └───────┬────────┘  └──────┬───────┘  └───────────┬───────────┘  │
│          │                  │                       │              │
│          │    ┌─────────────▼───────────────────────▼──────────┐  │
│          │    │              WasmAgent                          │  │
│          │    │                                                 │  │
│          │    │  pub struct WasmAgent {                         │  │
│          │    │      instance: wasmi::Instance,                 │  │
│          │    │      store: wasmi::Store<AgentContext>,         │  │
│          │    │      manifest: AgentManifest,  // do .wasm      │  │
│          │    │      fuel: u64,                // instr/tick    │  │
│          │    │      memory_limit: usize,      // 256 KB        │  │
│          │    │  }                                              │  │
│          │    │                                                 │  │
│          │    │  impl Agent for WasmAgent {                     │  │
│          │    │      fn tick(&mut self, t, tc) {                │  │
│          │    │          self.instance.call("tick", t, tc)      │  │
│          │    │      }                                          │  │
│          │    │  }                                              │  │
│          │    └──────────────────────┬──────────────────────────┘  │
│          │                           │                             │
│  ┌───────▼───────────────────────────▼──────────────────────────┐  │
│  │                     Skill Registry                            │  │
│  │                                                               │  │
│  │  DiskAgent  │  NetAgent  │  TimeAgent  │  SysAgent  │  ...   │  │
│  │  .read()    │  .http()   │  .now()     │  .random() │        │  │
│  │  .write()   │  .socket() │  .sleep()   │  .log()    │        │  │
│  │  .stat()    │            │             │  .args()   │        │  │
│  └───────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.1 Ciclo de vida de um WasmAgent

```
1. DISCOVERY:  DiskAgent lista /agents/*.wasm → AgentRegistry.notify()
2. VALIDATE:   wasmi::Module::validate(&wasm_bytes) → Ok/Err
3. LOAD:       wasmi::Module::new(&engine, &wasm_bytes)
4. INSTANTIATE: linker.instantiate(&mut store, &module)
5. EXTRACT:    chama instance.get_export("manifest") → AgentManifest
6. REGISTER:   AgentRegistry.register(Box::new(WasmAgent { ... }))
7. EXECUTE:    scheduler chama agent.tick() → instance.call("tick", t, tc)
8. TEARDOWN:   se AgentTickResult::Done/Crashed → drop instance, libera mem
```

### 3.2 Sandbox

```rust
pub struct WasmSandbox {
    fuel_per_tick: u64,      // 100_000 instruções WASM por tick
    memory_pages: u32,       // 64 páginas = 256 KB heap
    max_instances: usize,    // 32 agents WASM simultâneos
    capability_token: u64,   // token mínimo para acessar skills
}
```

Se o .wasm exceder fuel → `AgentTickResult::Crashed("fuel exhausted")`
Se tentar acessar memória fora dos limites → trap → `Crashed("out of bounds")`
Se chamar skill sem capability token → `PermissionDenied`

O kernel **nunca** crasha por causa de um .wasm mal comportado.

## 4. Contrato do Desenvolvedor

### 4.1 Funções que TODO .wasm DEVE exportar

```rust
//! meu_agente.rs — compilar com: cargo build --target wasm32-wasi --release

use alloc::string::String;
use alloc::vec::Vec;

/// Chamada UMA VEZ pelo AgentRegistry durante o registro.
/// Retorna ponteiro + tamanho de um JSON AgentManifest.
#[no_mangle]
pub extern "C" fn manifest() -> u64 {
    // O AgentRegistry lê a string JSON do linear memory do WASM
    // e constrói o AgentManifest
    let json = r#"{
        "name": "MeuAgente",
        "kind": "User",
        "schedule": "PollEvery(100)",
        "auto_start": true,
        "persist": false,
        "description": "Monitora S.M.A.R.T. do disco e alerta",
        "required_tokens": [1]
    }"#;
    // Retorna (ptr << 32) | len
    let ptr = json.as_ptr() as u64;
    let len = json.len() as u64;
    (ptr << 32) | len
}

/// Chamada pelo AgentScheduler a cada tick (ou conforme schedule).
/// tick: contador global de ticks do kernel
/// tick_count: quantos ticks este agente já executou
/// Retorna: 0 = Pending, 1 = Done, 2 = Crashed
#[no_mangle]
pub extern "C" fn tick(tick: u64, tick_count: u64) -> u32 {
    // A cada 1000 ticks, verificar S.M.A.R.T.
    if tick % 1000 == 0 {
        // Chamar skill do kernel via WASI import
        // (ver seção Skills Disponíveis)
    }
    1 // Done
}

/// Chamado quando o agente é removido ou o sistema desliga.
/// Oportunidade para salvar estado, fechar conexões, etc.
#[no_mangle]
pub extern "C" fn teardown() {
    // Cleanup
}
```

### 4.2 AgentManifest (formato JSON)

```json
{
  "name": "string (max 64 chars, unique)",
  "kind": "System | Driver | Inference | Network | Console | User",
  "schedule": "Oneshot | Continuous | PollEvery(N) | EventDriven(topic)",
  "auto_start": true,
  "persist": false,
  "description": "descrição curta do que o agente faz",
  "required_tokens": [1],
  "version": "1.0.0",
  "author": "github.com/seu-user",
  "icon": "📊"
}
```

### 4.3 Skills Disponíveis para WASM Agents

Cada skill é importada como função WASI (namespace `neural_aios`):

| Skill | Assinatura WASM | Descrição |
|---|---|---|
| `disk_read` | `(fd: i32, buf: i32, len: i32) -> i32` | Lê bytes de um arquivo |
| `disk_write` | `(fd: i32, buf: i32, len: i32) -> i32` | Escreve bytes em um arquivo |
| `disk_stat` | `(path_ptr: i32, path_len: i32) -> i64` | Info de arquivo (tamanho, tipo) |
| `disk_list` | `(dir_ptr: i32, dir_len: i32) -> i32` | Lista diretório → JSON |
| `smart_query` | `(disk_ptr: i32, disk_len: i32) -> i64` | Atributos S.M.A.R.T. |
| `net_http_get` | `(url_ptr: i32, url_len: i32) -> i64` | HTTP GET → (status << 32) \| body_len |
| `net_http_post` | `(url_ptr, url_len, body_ptr, body_len) -> i32` | HTTP POST → status |
| `time_now` | `() -> i64` | Tick atual do kernel |
| `time_sleep` | `(ticks: i64)` | Suspende agente por N ticks |
| `random` | `(buf: i32, len: i32)` | Preenche buffer com bytes aleatórios |
| `log` | `(level: i32, msg_ptr: i32, msg_len: i32)` | Escreve no log do kernel |
| `publish_event` | `(topic_ptr, topic_len, payload_ptr, payload_len) -> i32` | Publica no EventBus |
| `subscribe_event` | `(topic_ptr, topic_len) -> i32` | Inscreve no EventBus |
| `skill_call` | `(skill_ptr, skill_len, payload_ptr, payload_len) -> i64` | Chama skill do kernel |
| `mhi_query` | `(addr: i64) -> i64` | Consulta tier MHI de um endereço |

### 4.4 Como chamar uma skill do WASM

```rust
// Importa a skill do kernel
#[link(wasm_import_module = "neural_aios")]
extern "C" {
    fn disk_read(fd: i32, buf: i32, len: i32) -> i32;
    fn time_now() -> i64;
    fn log(level: i32, msg_ptr: i32, msg_len: i32);
}

// Exemplo: ler /data/config.json
fn read_config() -> Option<String> {
    let path = b"/data/config.json";
    let path_ptr = path.as_ptr() as i32;
    
    // Abrir arquivo (fd = disk_open retornaria um fd)
    let fd = 3; // fds 0-2 = stdin/stdout/stderr, 3+ = user
    
    let mut buf = vec![0u8; 4096];
    let buf_ptr = buf.as_mut_ptr() as i32;
    let bytes_read = unsafe { disk_read(fd, buf_ptr, 4096) };
    
    if bytes_read > 0 {
        buf.truncate(bytes_read as usize);
        String::from_utf8(buf).ok()
    } else {
        None
    }
}

// Exemplo: log
fn info(msg: &str) {
    unsafe {
        log(1, msg.as_ptr() as i32, msg.len() as i32);
    }
}
```

## 5. Como Construir, Testar e Publicar

### 5.1 Setup do projeto

```bash
cargo new meu-agente --lib
cd meu-agente
```

```toml
# Cargo.toml
[package]
name = "meu-agente"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[profile.release]
opt-level = "s"     # tamanho mínimo
lto = true
strip = true
```

```rust
// src/lib.rs
#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

// ... implementar manifest(), tick(), teardown()
```

### 5.2 Compilar

```bash
rustup target add wasm32-wasi
cargo build --target wasm32-wasi --release
# → target/wasm32-wasi/release/meu_agente.wasm (~50 KB)
```

### 5.3 Testar localmente

```bash
# Opção 1: wasmtime (runtime standalone)
wasmtime run meu_agente.wasm

# Opção 2: QEMU com o AIOS
cp meu_agente.wasm /path/to/neural-os/agents/
python tools/build_image_QEMU.py
qemu-system-x86_64 ...
# → [WASM] Loading /agents/meu_agente.wasm
# → [WASM] Agent 'MeuAgente' registered
```

### 5.4 Publicar

```bash
# Publicar no marketplace (futuro)
neural-cli publish meu_agente.wasm \
    --name "MeuAgente" \
    --description "Monitora S.M.A.R.T. do disco" \
    --tags "disk,monitoring,smart" \
    --token YOUR_API_TOKEN
```

## 6. Segurança e Isolamento

### 6.1 Modelo de ameaça

| Ameaça | Mitigação |
|---|---|
| .wasm consome toda CPU | Fuel metering: 100K instr/tick, depois preempt |
| .wasm acessa memória do kernel | Sandbox: linear memory isolada, sem acesso ao espaço de endereço do kernel |
| .wasm chama skill privilegiada | Capability tokens: cada skill tem token mínimo (0=root, 1=user, 2=guest) |
| .wasm faz loop infinito | Fuel acaba → `AgentTickResult::Crashed("fuel exhausted")` |
| .wasm aloca memória demais | Memory limit: 64 páginas WASM = 256 KB |
| .wasm vaza dados entre instâncias | Instâncias isoladas: cada WasmAgent tem seu próprio Store |

### 6.2 Capability Tokens

```rust
pub struct CapabilityToken(u64);

// Tokens herdados do kernel:
// 0 = Kernel (acesso total — agentes kernel-only)
// 1 = User   (skills de usuário: disk, net, time)
// 2 = Guest  (skills limitadas: log, time, random)
// 3+ = Custom (granularidade fina por agente)

impl WasmAgent {
    fn can_call_skill(&self, skill_token: u64) -> bool {
        self.capability_token <= skill_token
    }
}
```

## 7. BitNet IDE — Desenvolvimento Assistido por IA

```
┌──────────────────────────────────────────────────────────────────┐
│  BitNet IDE v1.0                                                  │
│  ┌─────────┐ ┌────────────────────────────────────────────────┐  │
│  │ Files   │ │  1  //! Monitor de saúde de disco               │  │
│  │         │ │  2  #![no_std]                                  │  │
│  │ 📁 src/ │ │  3  extern crate alloc;                         │  │
│  │  lib.rs │ │  4                                               │  │
│  │         │ │  5  #[no_mangle]                                 │  │
│  │         │ │  6  pub extern "C" fn tick(t: u64, tc: u64) {   │  │
│  │         │ │  7      if t % 1000 == 0 {                      │  │
│  │         │ │  8          let data = smart_query("sda"); //❌  │  │
│  │         │ │  9      }                                       │  │
│  └─────────┘ └────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────┐│
│  │ 💡 Cortex: "A função smart_query() precisa de 2 argumentos:  ││
│  │    disk_name_ptr e disk_name_len. Exemplo:                   ││
│  │    let name = b\"sda\";                                       ││
│  │    let result = smart_query(name.as_ptr(), name.len());       ││
│  │    O resultado é (realloc << 32) | pending, use:              ││
│  │    let realloc = (result >> 32) as u32;"                      ││
│  └──────────────────────────────────────────────────────────────┘│
│                                                                   │
│  [🔨 Compilar]  [🧪 Testar]  [📦 Publicar]  [📖 Skill Docs]      │
└──────────────────────────────────────────────────────────────────┘
```

### Como o HOWTO funciona:

1. O editor envia o código-fonte para o `CortexAgent` via LLM_REQUEST
2. Cortex classifica erros por tipo (import, type, skill_not_found)
3. Para cada classe de erro, existe um template de resposta
4. O template é preenchido com:
   - Nomes reais dos agentes/skills disponíveis (do AgentRegistry)
   - Assinaturas corretas das funções WASI
   - Exemplos de código funcional
5. O resultado é exibido no painel Cortex

Com o **GGUF loader + modelo 1.5B BitNet** (futuro), o LLM geraria o código completo, não só templates.

## 8. Marketplace de Agents (Futuro)

```
┌────────────────────────────────────────────┐
│         NEURAL AIOS MARKETPLACE             │
│                                             │
│  🔍 [________________] Search              │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │ 📊 DiskMonitor    ★4.8  1.2K dl    │   │
│  │ Monitora S.M.A.R.T., alerta falha  │   │
│  │ @alicedev · v2.1.0 · 48 KB         │   │
│  │ [Install] [Details]                 │   │
│  └─────────────────────────────────────┘   │
│  ┌─────────────────────────────────────┐   │
│  │ 🌐 RSS Reader     ★4.2  856 dl     │   │
│  │ Lê feeds RSS, exibe no console     │   │
│  │ @bobcodes · v1.0.0 · 72 KB         │   │
│  │ [Install] [Details]                 │   │
│  └─────────────────────────────────────┘   │
│  ┌─────────────────────────────────────┐   │
│  │ 🎮 Snake Game     ★4.9  3.4K dl    │   │
│  │ Snake no framebuffer! WASM + FB     │   │
│  │ @game_dev · v3.2.0 · 24 KB          │   │
│  │ [Install] [Details]                 │   │
│  └─────────────────────────────────────┘   │
│                                             │
│  [Publish your agent]  [My agents]          │
└────────────────────────────────────────────┘
```

O marketplace é um **agente HTTP no AIOS** (`MarketplaceAgent`):

```rust
// Comando no shell do AIOS:
> marketplace search "disk monitor"
  📊 DiskMonitor v2.1.0 — @alicedev — ★4.8 — 1.2K downloads
  💾 DiskCleaner v1.5.0  — @sysadmin — ★4.5 — 340 downloads

> marketplace install DiskMonitor
  📥 Baixando... [████████████████] 48 KB
  ✅ Instalado em /agents/DiskMonitor.wasm
  ✅ Agente registrado: DiskMonitor
  ▶️ Iniciado: tick 0
```

## 9. Kernel Agents vs WASM Agents

| | Kernel Agent (Tier 0-2) | WASM Agent (Tier 3) |
|---|---|---|
| **Linguagem** | Rust `no_std` + `no_main` | Rust normal |
| **Compilação** | x86_64-unknown-none, linkado no kernel | wasm32-wasi, arquivo separado |
| **Acesso HW** | Direto (MMIO, I/O ports, DMA) | Indireto (via skills do kernel) |
| **Performance** | 100% nativo | 50-80% nativo (intérprete) |
| **Latência** | ~100ns | ~1-10µs (IPC via linker) |
| **Crash** | Pode travar o kernel | Isolado, kernel continua |
| **Hot-reload** | Requer reboot | Substitui .wasm, recarrega |
| **Debug** | QEMU + GDB | wasmi debug hooks + logs |
| **Segurança** | Trust total (compilado junto) | Sandbox + capability tokens |
| **Exemplos** | DiskAgent, NetAgent, Display | Monitor, RSS, Snake, Plugins |
| **Quem escreve** | Time core do kernel | Qualquer dev Rust |

**Regra de ouro:** Se o agente precisa de acesso direto a hardware → kernel agent.
Se é lógica de aplicação → WASM agent.

## 10. Roteiro de Implementação

### Fase 1 — Runtime Básico (Sprint 76)
- [x] ADR-0032 (este documento)
- [ ] Integrar wasmi 0.42+ como dependência `no_std`
- [ ] `WasmAgent` struct + `Agent` trait impl
- [ ] WASI→Skill bridge: 5 skills iniciais (log, time, random, disk_stat, disk_read)
- [ ] `manifest()` parser JSON do linear memory WASM
- [ ] `tick()` dispatcher com passagem de argumentos
- [ ] Fuel metering configurável
- [ ] Memory sandbox (64 páginas = 256 KB)

### Fase 2 — Skills e Segurança (Sprint 77)
- [ ] 15 skills WASI completas
- [ ] Capability token gating por skill
- [ ] EventBus integrado (publish_event, subscribe_event)
- [ ] `teardown()` hook
- [ ] Crash recovery: se .wasm morre, loga e remove

### Fase 3 — Tooling (Sprint 78)
- [ ] CLI: `neural-cli build` (cargo wrapper)
- [ ] Agent template: `cargo generate neural-os/agent-template`
- [ ] Hot-reload: `neural-cli deploy --watch`

### Fase 4 — Marketplace (Pós-MVP)
- [ ] MarketplaceAgent: HTTP search + install
- [ ] Verificação Ed25519 de pacotes .wasm
- [ ] Versionamento semântico
- [ ] Ratings + reviews (via EventBus)

### Fase 5 — IDE (Pós-MVP)
- [ ] BitNet IDE no framebuffer
- [ ] Integração Cortex HOWTO
- [ ] Debug visual: fuel, mem, stack

## 11. Estimativas

| Componente | LOC | Dependências |
|---|---|---|
| WasmAgent (runtime) | ~400 | wasmi crate |
| WASI→Skill bridge | ~350 | AgentRegistry, SkillRegistry |
| Fuel + Sandbox | ~100 | wasmi Config |
| manifest() parser | ~80 | serde_json_wasm? ou manual |
| CLI tooling | ~300 | cargo, clap |
| MarketplaceAgent | ~400 | NetAgent (B-01) |
| **Total Fase 1-2** | **~930 LOC** | |

## 12. Exemplo Completo: DiskMonitor Agent

```rust
//! DiskMonitor — Alerta quando S.M.A.R.T. detecta falha iminente
//! Compilar: cargo build --target wasm32-wasi --release
//! Tamanho: ~48 KB

#![no_std]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

// Importa skills do kernel AIOS
#[link(wasm_import_module = "neural_aios")]
extern "C" {
    fn log(level: i32, msg_ptr: i32, msg_len: i32);
    fn smart_query(disk_ptr: i32, disk_len: i32) -> i64;
    fn publish_event(topic_ptr: i32, topic_len: i32, payload_ptr: i32, payload_len: i32) -> i32;
    fn time_now() -> i64;
}

fn info(msg: &str) {
    unsafe { log(1, msg.as_ptr() as i32, msg.len() as i32); }
}

fn check_disk(name: &str) -> Option<(u32, u32)> {
    let result = unsafe { smart_query(name.as_ptr() as i32, name.len() as i32) };
    if result == 0 { return None; }
    let realloc = (result >> 32) as u32;
    let pending = (result & 0xFFFFFFFF) as u32;
    Some((realloc, pending))
}

#[no_mangle]
pub extern "C" fn manifest() -> u64 {
    let json = r#"{"name":"DiskMonitor","kind":"User","schedule":"PollEvery(1000)",
        "auto_start":true,"persist":false,"description":"S.M.A.R.T. disk health monitor",
        "required_tokens":[1],"version":"1.0.0","author":"@alicedev","icon":"📊"}"#;
    ((json.as_ptr() as u64) << 32) | (json.len() as u64)
}

#[no_mangle]
pub extern "C" fn tick(tick: u64, _tick_count: u64) -> u32 {
    if tick % 1000 != 0 { return 1; }

    for &disk in &["sda", "sdb", "nvme0n1"] {
        if let Some((realloc, pending)) = check_disk(disk) {
            if realloc > 50 || pending > 5 {
                let alert = alloc::format!(
                    "⚠ DISK HEALTH: {} realloc={}, pending={}", disk, realloc, pending
                );
                info(&alert);
                
                let topic = b"DISK_HEALTH_ALERT";
                unsafe {
                    publish_event(
                        topic.as_ptr() as i32, topic.len() as i32,
                        alert.as_ptr() as i32, alert.len() as i32,
                    );
                }
            }
        }
    }
    1 // Done
}

#[no_mangle]
pub extern "C" fn teardown() {
    info("DiskMonitor: shutting down.");
}

// Precisa de um panic_handler mínimo para no_std
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
```

## 13. Conclusão

WASM Agents transformam o Neural AIOS de um **sistema fechado** (só o time core pode estender) em uma **plataforma aberta** (qualquer dev Rust pode criar apps). Isto é o que diferencia um OS de um ecossistema.

A stack completa de adoção:

```
Dev descobre   →   Lê ADR-0032   →   cargo generate template
    ↓
Implementa     →   cargo build   →   .wasm de 50 KB
    ↓
Testa          →   wasmtime / QEMU
    ↓
Publica        →   marketplace   →   outros instalam
    ↓
AIOS detecta   →   /agents/*.wasm → AgentRegistry.load()
    ↓
Executa        →   WasmAgent.tick() → sandbox + skills
```

**O futuro do AIOS é uma plataforma, não um produto.**
