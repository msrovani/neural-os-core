---
name: neural-aicore
description: >-
  Develop cortex (BitNet/MoE/Trinity tensor engine), hermes (agent runtime,
  WASM, networking, skills), or jarbas (UI compositor, audio, GPU, persona).
  no_std bare-metal Rust. Use when implementing inference, agents, UI cards,
  skills, network stacks, or personality for Neural OS.
---

# Neural AICore — K³CHJ Crates

## Núcleo

**Goal:** o que o crate precisa entregar em termos de funcionalidade, não o
que o prompt diz. Prompt é palpite. O crate compila? O agente spawna? O
framebuffer renderiza? Isso é Goal.

**Toda crate é `#![no_std]` + `extern crate alloc;`.** Sem std, sem POSIX,
sem sistema de arquivos padrão. Vec, String, Box vêm de `alloc`. Toda
alocação pode falhar — não tem `std::alloc::handle_alloc_error` elegante.

**Antes de escrever qualquer módulo novo, ler os existentes.** O projeto já
tem 180+ arquivos Rust. O que você vai criar provavelmente já tem padrão em
algum lugar.

---

## 1. Visão Geral das Crates

### Crates AICore

| Crate | Caminho | Função | Depende de | Tamanho |
|-------|---------|--------|------------|---------|
 | **cortex** | `crates/cortex/src/` | BitNet ternário, MoE, Trinity, tensor ops, decode, tokenização, GGUF, modelo hub | k_nano | ~31 módulos |
 | **hermes** | `crates/hermes/src/` | Runtime de agentes, WASM/wasmi, rede (smoltcp), skills, orquestração, MCP, memória | k_nano + cortex + k_ia | ~98 módulos |
 | **jarbas** | `crates/jarbas/src/` | Compositor de display, cards (embedded-graphics), áudio, GPU, persona Jarbas | k_nano + cortex + hermes | ~14 módulos |

### Build

```powershell
cargo build --release -p neural-kernel  # compila tudo, incluindo as crates
cargo check --release                   # lint canônico: 0 erros
cargo clean -p neural-kernel            # antes de check --release quando sumirem erros
```

**⚠️ Build incremental mascara erros.** Se um erro some misteriosamente,
rode `cargo clean -p neural-kernel` antes. Cache incremental esconde.

---

## 2. Cortex — Engine de Inferência

### O que é

Motor neural puro: tensor ops, transformer layers (BitLinear), tokenização
(BPE), decodificação (speculative decoding com NgramSpeculator / Medusa),
MoE (Mixture of Experts), Trinity (LLM + router + experts), GGUF loading,
model hub, HNSW (memória vetorial), cellular/evolution (algoritmos
genéticos para auto-evolução), structured decode, compute dispatch.

### Módulos Principais

| Módulo | Função | Padrões |
|--------|--------|---------|
 | `tensor.rs` | Tensor, PackedTernaryTensor (2-bit packing: 4 pesos/byte) | ADD/SUB apenas (BitNet ternário). Sem FPU em matmul. |
 | `nn.rs` | BitLinear, silu, rms_norm | Camadas transformer com pesos ternários |
  | `trinity.rs` | Trinity MoE: LLM + router + experts | Router_weight treinável, 7 kind (2 wired HWEXPRT/RUSTCDR, 4 keyword→Generator) VOCAB=99 HIDDEN=64 |
 | `moe.rs` | Int8Router, expert forwarding | Router i8 com scores f32 |
 | `decode.rs` | Autoregressive decode loop | Cache KV, speculative decoding |
 | `bpe.rs` | Tokenização BPE | Vocab 99 tokens (pequeno, otimizado) |
 | `gguf.rs` | Carregamento de modelos GGUF | Leitura de arquivo FAT → tensor |
 | `model_hub.rs` | Download e cache de modelos | HTTP GET via hermes::net |
 | `compute.rs` | Compute dispatch (SMP/GPU/NPU) | AP pollable gate, fallback BSP |
 | `bitnet_avx2.rs` | Matmul AVX2 otimizado | Só se hypervisor != TCG |
 | `cellular.rs` | Algoritmo celular para auto-evolução | Regras de vizinhança |
 | `evolution.rs` | Evolução de pesos | Mutação + seleção |
 | `hnsw.rs` | HNSW para memória vetorial | Busca aproximada por similaridade |
 | `kv_h2o.rs` | Heavy Hitter Oracle (KV cache pruning) | Reduz cache KV mantendo accuracy |
 | `structured_decode.rs` | Decodificação estruturada (ponytail: disabled) | Grammar/JSON/Code modes |

### Regras Cortex

1. **Ternário primeiro, f32 depois.** BitNet ternário (ADD/SUB) sobre f32.
   Só usar f32 onde ternário não alcança (router scores, rms_norm).
2. **AVX2 disponível apenas se não TCG.** WHPX + `-cpu host` = AVX2 nativo.
   QEMU TCG = bloquear AVX2. Usar `bitnet_avx2.rs` + fallback SSE.
3. **PackedTernaryTensor: 4 pesos/byte.** packing 2-bit, unpack na hora do matmul.
4. **Speculative decoding aceita drafts incorretos.** NgramSpeculator pode errar
   — o verify aceita ou rejeita. O custo de errar é baixo (alguns tokens extras).
5. **Model Hub: download via hermes::net.** cortex não faz HTTP direto.
   Usar `hermes::net_bridge` ou `neural-kernel::net::resolve_and_http_get_safe`.
6. **GGUF loading: ler de FAT (firmware.rs).** Modelos estão em `/mnt/neural/`
   como `MODEL_.BIN`. Usar `FatFile::open` do k_nano.
7. **Compute dispatch gated por ap_pollable().** AP workers só disponíveis
   quando `k_nano::smp::ap_pollable()` = true (ADR-0057 WS-F). BSP faz matmul
   se APs não estiverem prontos.
8. **Cuidado com `unsafe` em tensor ops.** Toda operação que mexe em memória
   raw precisa de `write_volatile`/`read_volatile` para evitar UB com `#[repr(C, packed)]`.

---

## 3. Hermes — Runtime de Agentes

### O que é

O cérebro orquestrador. Gerencia agentes (50+ nativos + variáveis HW + 147
EventDriven), roteamento de intenções (ReAct 7 fases), runtime WASM (wasmi),
stack de rede (smoltcp), ciclo de vida de skills (geração, loading,
manifest, market, observer), MCP client/server, self-evolução, segurança,
safety, shell, cron, browser_agent, wifi, netfs, memory store, app factory,
orquestração, plugin hub, approval, hitl_ui, e mais.

### Módulos Principais

| Módulo | Função | Padrões |
|--------|--------|---------|
 | `orchestrator.rs` | Orquestração de agentes e workflows | Schedules: Continuous, EventDriven, PollEvery, Oneshot |
 | `agents/` | Agentes nativos registrados | agent-*.rs com manifests, capabilities, lifecycle |
 | `intent_bus.rs` | Roteamento de intenções (ReAct) | TOPIC_USER_INTENT, TOPIC_HERMES_RESPONSE |
 | `skill_gen.rs` | Geração de skills via LLM | Prompt → skill WASM |
 | `skill_loader.rs` | Loading de skills do VFS/FAT | Manifesto + código WASM |
 | `wasmi_rt.rs` | Runtime WASM real (wasmi, no_std, fuel) | Sandbox, fuel limit, host imports `aios::*` |
 | `app_factory.rs` | App factory: wasmi (A) / cranelift (B) / nativo (C) | A=default, B/C gated por ring de isolamento |
 | `mcp.rs` | MCP client para tools externas | stdio, tool discovery, call |
 | `mcp_server.rs` | MCP server expondo tools do Hermes | Tool catalog, call routing |
 | `net.rs` | Network bridge + smoltcp | TCP, UDP, DNS, HTTP |
 | `net_bridge.rs` | Bridge entre hermes::net e neural-kernel::net | Registrar no boot |
 | `netstack.rs` | smoltcp interface + poll | Ethernet, IPv4, ARP |
 | `netfs.rs` | Network filesystem (HTTP GET → VFS) | Fetch remoto → cache local |
 | `self_evolve.rs` | Auto-evolução do Hermes | Meta-learning, otimização de rotas |
 | `memory.rs` | Memory store (SGDB + RAG) | Episodic, semantic, procedural |
 | `safety.rs` | Safety invariants I1-I4 | Sempre verificar antes de ação destrutiva |
 | `security.rs` | Security pipeline (5 detectores) | Cadeia de análise |
 | `shell.rs` | Shell remoto | Comandos via intent |
 | `cron.rs` | Cron scheduler | Eventos periódicos |
 | `approval.rs` | Approval system (HITL) | Human-in-the-loop |
 | `hal_offer.rs` | HalOffer client | Bind de device capabilities |
 | `elf_loader.rs` | ELF loader para bins nativos | Carregamento de código nativo |
 | `dynskill.rs` | Dynamic skill loading | Hot-reload de skills |
 | `membrane.rs` | Membrane isolation | Isolamento entre agentes |
 | `cognitive_bridge.rs` | Ponte cognitiva | Cortex → Hermes → Jarbas |
 | `hub.rs` | Agent hub | Registry, lifecycle, descoberta |

### Regras Hermes

1. **Agent manifesto explícito.** Nome, tipo, schedule, trust tokens — nada
   implícito. Usar `AgentManifest` com `agent_type`, `schedule`, `capabilities`.
2. **NUNCA chamar `netstack` lock dentro de `bootstrap_early`.** Deadlock
   garantido. Smoke test só após return do bootstrap.
3. **Skills não são hardcoded no enum Intent.** O fluxo: usuário → WakeWord
   → Hermes → Chat → LLM → gera skill → SkillObserver registra → executa.
4. **wasmi é default (A) seguro.** Cranelift (B) e nativo (C) são **GATED**
   por ring de isolamento (ADR-0041 Ring3) + HITL forte.
5. **Sempre registrar `hermes::net_bridge` no boot.** Sem ele, Browser/Search
   usam `hermes::net` vazio (NETSTACK não conectado).
6. **MCP tools:** documentar com exemplos. ACI: toda tool tem description
   + parâmetros tipados + retorno `{status, data?, error?}`.
7. **EventBus topics são strings estáticas.** Usar `TOPIC_*` constants.
   Tópicos são `&str`, não enum.
8. **Schedule tipos definem ativação.** Continuous = sempre pollando.
   EventDriven = só quando evento. PollEvery(N) = a cada N ticks. Oneshot =
   uma vez no boot. Continuous não-essencial >5% ticks por 1000 ticks →
   rebaixado para EventDriven.

---

## 4. Jarbas — UI, Áudio & Personalidade

### O que é

A camada de interação com o usuário. Display compositor (framebuffer BGRA32),
UI declarativa com cards (embedded-graphics `DrawTarget`), avatar/emoção,
audio pipeline (HDA playback + capture + STT + TTS), GPU drivers (VirtIO-GPU,
NVIDIA), persona Jarbas (SOUL.md / PERSONA.md), visão (UVC), clipboard, IDE.

### Módulos Principais

| Módulo | Função | Padrões |
|--------|--------|---------|
 | `display/` | Compositor, cards, console, dock, decorations, theme, tiling | Z-ordem baseada em Layer enum |
 | `display/card.rs` | UiDeclaration, UiRenderer (Text/KeyValue/Gauge/Button/Panel) | embedded-graphics sobre DoubleBuffer |
 | `display/eg.rs` | FbTarget: DrawTarget sobre DoubleBuffer | embedded-graphics 0.8 |
 | `display/compositor.rs` | DoubleBuffer, Layer compositing | OrbBackground < Overlay < Windows < Dock |
 | `display/fb.rs` | Framebuffer probe + init | `bytes_per_pixel` dinâmico, não hardcoded |
 | `display/avatar.rs` | Avatar (orb) responsivo | Reage a FFT audio (16 bins Goertzel) |
 | `display/theme.rs` | Tema visual (COSMIC_DARK etc.) | `const Theme::new()` — const fn |
 | `display/chat_window.rs` | Chat overlay semi-transparente | Hermes CLI integrado |
 | `display/gauges.rs` | Status gauges (HUD) | CPU, mem, rede |
 | `audio/` | HDA driver, pipeline, STT, TTS | Pipeline PCM → STT, TTS → PCM → HDA |
 | `gpu/` | GPU backend | VirtIO-GPU, NVIDIA PUSH_BUFFER, GPU compute |
 | `jarvis.rs` | Persona Jarbas, emoção, soul | SoulProfile, Emotion enum |
 | `virtio_gpu.rs` | VirtIO GPU driver | GPU paravirtualizada |
 | `uvc_driver.rs` | UVC camera driver | Captura de vídeo USB |
 | `vision_agent.rs` | Agente de visão | Processamento de frames |
 | `screensaver.rs` | Screensaver | Detecção de inatividade |
 | `ide.rs` | IDE integrada | Editor de código no OS |
 | `cards/` | Card components | UI declaration hierarchy |

### Regras Jarbas

1. **Framebuffer bpp:** usar `info.bytes_per_pixel` (pode ser 3 ou 4).
   **NUNCA** hardcodar 3. QEMU/OVMF reporta 4. Calcular `fb_stride =
   info.stride * bytes_per_pixel`.
2. **Z-ordem:** Layer enum: `OrbBackground < HermesOverlay < AppWindows <
   DockBar`. Respeitar ordem. Cada layer é um buffer separado no DoubleBuffer.
3. **embedded-graphics 0.8 compila em `x86_64-unknown-none` soft-float.**
   Implementar `DrawTarget` para `DoubleBuffer` (`eg.rs`). Fontes são
   **ASCII 0x20–0x7E** — evitar acentos/-- nos títulos de cards.
4. **`const Theme` com `const fn`.** `const COSMIC_DARK: Theme =
   Theme::new(...)` evita `&temporary` pattern (E0515).
5. **Free function > method para render com borrow conflitante.**
   `draw_window_fb(fb, win, theme)` separa borrow de `fb` (mutable) de
   `win` (immutable), evitando E0502.
6. **Cards são gerados por LLM (#412 `card_json_schema_hint`) ou skill
   WASM.** Não hardcodar cards novos — criar UI_SPEC via intent.
7. **Avatar orb reage a áudio.** 16 bins Goertzel da FFT. Se áudio não
   disponível, orb fica em estado neutro.
8. **Persona carregada de PERSONA.md > SOUL.md > default_jarbas().** Fallback
   seguro se VFS não montou ainda.
9. **GPU compute só se canário Ready.** NVIDIA PUSH_BUFFER só funciona se
   firmware ACR carregado (VRAM P8 mode). VirtIO-GPU sempre disponível.

---

## 5. Padrões Cross-Crate

### Comunicação entre Crates

| Padrão | Mecanismo | Exemplo |
|--------|-----------|---------|
 | **EventBus** | Tópicos `&str` com payload | `cortex::TOPIC_LLM_REQUEST` → hermes escuta |
 | **pub use** | Bin re-exporta globals das crates | `k_nano::SKILL_REGISTRY` (nunca redeclarar no bin) |
 | **Agent Manifests** | Hermes registra, cortex/jarbas expõem capabilities | Agent-*-rs no `hermes::agents/` |
 | **MCP** | Hermes serve tools, cortex/jarbas consomem | `mcp_server.rs` expõe, `mcp.rs` chama |
 | **Intent Bus** | Hermes roteia intenções | `intent_bus::TOPIC_USER_INTENT` |
 | **HalOffer** | k_hal oferece HW, hermes consome | `hal_offer.rs` + `device_recipe.rs` |
 | **Memory Store** | hermes::memory_store, cortex::hnsw, jarbas::display | Episodic + semantic + procedural + vetorial |
 | **VFS (FAT)** | k_nano gerencia, hermes/jarbas leem | Modelos, firmware, PERSONA.md |

### Atenção: Shadow de Globals

O erro mais comum e mais caro desta codebase:

```
// ❌ ERRADO: bin redeclara SKILL_REGISTRY como lazy_static privado
// O pub use da crate base fica invisível para hermes/k_ia
lazy_static! { static ref SKILL_REGISTRY: ... }

// ✅ CERTO: bin usa o singleton da crate base
use k_nano::SKILL_REGISTRY;
```

**Sempre que for usar um global (SKILL_REGISTRY, GLOBAL_ALLOCATOR,
PHYS_MEM_OFFSET, event_bus), verificar se ele já existe em `k_nano` ou
outra crate.** Se existir, `use`. Não redeclarar.

### no_std Checklist

| Item | O que usar |
|------|-----------|
 | Alocação | `extern crate alloc;` — `Vec`, `String`, `Box`, `format!` |
 | Spinlocks | `spin::Mutex`, `spin::RwLock` |
 | Lazy init | `lazy_static` com `spin_no_std` |
 | Sem std | sem `std::thread`, `std::fs`, `std::net`, `std::time` |
 | Math | `libm` (f32 sqrt, sin, etc.) |
 | IO | MMIO com `write_volatile`/`read_volatile` |
 | Collections | `alloc::collections::BTreeMap`, `Vec`, `BinaryHeap` |
 | Erros | `core::result::Result`, sem `anyhow`/`thiserror` |

### Build & Test

```powershell
# Check canônico (0 erros, 1 warning conhecido de import não usado)
cargo clean -p neural-kernel; if ($?) { cargo check --release }

# Build da crate individual (útil para testar mudanças localizadas)
cargo check --release -p cortex
cargo check --release -p hermes
cargo check --release -p jarbas

# Build completo
cargo build --release -p neural-kernel
```

---

## 6. Lições Críticas (do SESSION_INDEX)

| Lição | Contexto | Onde aplicar |
|-------|----------|-------------|
 | **Shadow de SKILL_REGISTRY** | SESSION_217: bin redeclarava lazy_static privado | Nunca redeclarar globals. Sempre `use` da crate base. |
 | **Drift de tipo struct** | SESSION_217: impl Trait for Struct no bin não via impl da crate | Verificar com `fc.exe /A` antes de mover. Tipos distintos não compartilham impls. |
 | **`return` em match arm** | SESSION_217: `return String` dentro de fn que retorna `AgentTickResult` | Usar `Result<String,String>` para early-exit. |
 | **`ToString` em no_std** | SESSION_217: método não encontrado para &str | Importar `use alloc::string::ToString;`. |
 | **framebuffer bpp** | SESSION_139: hardcoded 3 em BGR framebuffer de 32 bits | Usar `info.bytes_per_pixel`. |
 | **e1000 TX aliases QEMU** | SESSION_149: TDBAL/TDT em 0x0420/0x0438 são aliases não funcionais | Usar `0x3800/0x3818`. |
 | **Deadlock NETSTACK** | SESSION_152: chamar tcp_exchange dentro de NETSTACK.lock() | Smoke test só após return do bootstrap. |
 | **AP sem IDT trava** | SESSION_163: AP sem lidt + hlt sem trabalho | Só usar APs quando `ap_pollable()` = true. |
 | **Build incremental** | Sessões múltiplas: erro some, volta, some | `cargo clean -p neural-kernel` antes de check --release. |



---

**Versão:** 1.0.0  
**Crates:** cortex (31 módulos), hermes (98 módulos), jarbas (14 módulos)  
**Baseada em:** neural-os-core AGENTS.md + SESSION_INDEX + superdev-rules v4.0.0
