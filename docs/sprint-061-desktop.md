# Sprint 61 — Desktop Environment (SmileyOS Patterns)

**v0.61.0** — Based on SmileyOS patterns (IDEA #279). Port of the SmileyOS UX to neural-os-core. Compositor, window manager, shell, theming, Hermes as desktop app, WASM app sandbox, LLM-generated icons. **Mouse como agente (Agent/Skill-First).**

---

## Legenda
| Símbolo | Significado |
|---|---|
| 🟢 Fácil | < 100 LOC, sem dependências externas |
| 🟡 Médio | 100-300 LOC, depende de 1-2 módulos existentes |
| 🔴 Pesado | 300-700 LOC, módulo novo ou pesquisa |
| ⚫ Bloqueado | Depende de HW ou ambiente externo |

---

## Sub-Sprint 61.0 — MouseAgent (PS/2 + Skills)

**Target:** v0.61.0 | **LOC:** ~300 | **Dependências:** HwBridgeAgent (IRQ12 bridge), EventBus

### Feature: MouseAgent — Agente de Mouse
- LOC: ~300
- Dependencies: HwBridgeAgent (IRQ12 → scancode bridge, extendido para mouse), EventBus, PCI (PS/2 controller detect)
- Implementation:
  1. Create `agents/mouse_agent.rs`:
     - `MouseAgent` struct — `{ x: u16, y: u16, buttons: u8, pkt_buf: [u8; 3], pkt_count: u8 }`
     - `tick()`: ler IRQ12 via HwBridge → montar pacote PS/2 (3 bytes: [Y_OVF+X_OVF+sign, X_delta, Y_delta]) → atualizar (x, y) → publicar eventos no EventBus
  2. PS/2 mouse protocol (3-byte packet):
     - Byte 0: bit7=Y_overflow, bit6=X_overflow, bit5=Y_sign, bit4=X_sign, bit3=always1, bit2=M, bit1=R, bit0=L
     - Byte 1: X delta (signed)
     - Byte 2: Y delta (signed, negate for screen coords)
  3. EventBus topics:
     - `MOUSE_MOVED` — payload: `(x: u16, y: u16, dx: i16, dy: i16)`
     - `MOUSE_CLICK` — payload: `(button: u8, x: u16, y: u16)`
     - `MOUSE_DRAG` — payload: `(from_x, from_y, to_x, to_y)`
     - `MOUSE_SCROLL` — payload: `(delta: i8)`
  4. Skills expostas:
     - `"mouse_position"` → retorna (x, y)
     - `"mouse_wait_click"` → bloqueia até próximo clique, retorna (button, x, y)
  5. MouseAgent registrado como A-021 no boot
- Files to create/modify:
  - `crates/neural-kernel/src/agents/mouse_agent.rs` (new)
  - `crates/neural-kernel/src/agents/mod.rs` (add pub mod)
  - `crates/neural-kernel/src/agents.rs` (register no boot)
  - `crates/neural-kernel/src/display/compositor.rs` (subscribe MOUSE_MOVED/CLICK)

### Feature: HwBridgeAgent — Extensão IRQ12
- LOC: ~80
- Dependencies: IDT (vetor 44 = IRQ12), PIC/APIC routing
- Implementation:
  1. HwBridgeAgent já subscreve IRQ1 (teclado). Adicionar IRQ12:
     - `hw_bridge.rs`: rotear IRQ12 → `MOUSE_PACKET` no EventBus
     - MouseAgent subscreve `MOUSE_PACKET`
  2. Habilitar PS/2 mouse na porta 0x64: comando 0xA8 (enable), 0x20/0x60 (set compaq status bit1)
- Files to modify:
  - `crates/neural-kernel/src/agents/hw_bridge.rs` (IRQ12 handler)
  - `crates/neural-kernel/src/interrupts.rs` (vetor 44 config)

---

## Sub-Sprint 61.1 — Theming Engine

**Target:** v0.61.1 | **LOC:** ~350 | **Dependências:** DisplayAgent, HermesAgent

### Feature: Theme System
- LOC: ~350
- Dependencies: DisplayAgent (framebuffer), HermesAgent (command parsing), BootTrustAgent (persist)
- Implementation:
  1. Create `display/theme.rs` — `Theme` struct with 7 slots (bg, fg, accent, secondary, error, success, terminal_bg)
  2. Define 5 built-in themes: `HermesDark` (default), `HermesLight`, `Matrix`, `Dracula`, `Solarized`
  3. `ThemeRegistry` — BTreeMap of named themes, `apply(name)` hot-swaps
  4. `ConsoleRegion` stores `(fg, bg)` pairs — each region can be re-rendered on theme change
  5. `/theme` command in HermesAgent — `theme list`, `theme <name>`, `theme current`
  6. Persist active theme via BootTrustAgent's config store (or hardcoded fallback)
- Files to create/modify:
  - `crates/neural-kernel/src/display/theme.rs` (new)
  - `crates/neural-kernel/src/display/mod.rs` (add pub mod)
  - `crates/neural-kernel/src/display/agent.rs` (theme_swap hook)
  - `crates/neural-kernel/src/display/console.rs` (region color update)
  - `crates/neural-kernel/src/hermes/commands.rs` (Theme command)
  - `crates/neural-kernel/src/hermes/agent.rs` (handler dispatch)

### Feature: Font Rendering for High-Res
- LOC: ~200
- Dependencies: Framebuffer (existing), Theme system
- Implementation:
  1. Create `display/font.rs` — 8×16 VGA font bitmap (embedded, 256 glyphs × 16 bytes)
  2. `render_char(fb, x, y, glyph, fg, bg)` — draws character to framebuffer
  3. `render_string(fb, x, y, text, fg, bg)` — line-aware string renderer
  4. Integrate with ConsoleRegion for framebuffer text output
- Files to create/modify:
  - `crates/neural-kernel/src/display/font.rs` (new)
  - `crates/neural-kernel/src/display/mod.rs`
  - `crates/neural-kernel/src/display/console.rs`

---

## Sub-Sprint 61.2 — Window Manager / Compositor

**Target:** v0.61.2 | **LOC:** ~800 | **Dependências:** 61.1 (Theming), Framebuffer

### Feature: Compositor Multi-Window
- LOC: ~500
- Dependencies: DisplayAgent framebuffer, ThemeEngine, HwBridgeAgent (mouse IRQ)
- Implementation:
  1. Create `display/compositor.rs` — `Compositor` struct:
     - `windows: Vec<Window>` — ordered z-index list
     - `cursor: (x, y)` — mouse cursor position
     - `focus: Option<usize>` — active window index
  2. `struct Window` — `{ id, title, x, y, w, h, z_index, minimized, buffer: [u8; W*H*4] }`
  3. `compositor::render()` — back-to-front compositing: each window draws to its buffer, then compositor blends to fb
  4. Mouse IRQ → `compositor::handle_click()` → focus change, drag detection
  5. Double-buffer: `compositor::commit()` → swap fb2 → fb1
  6. Dock bar: 40px bottom region with window buttons + clock
  7. Menu bar: 20px top region with app menu entries
- Files to create/modify:
  - `crates/neural-kernel/src/display/compositor.rs` (new)
  - `crates/neural-kernel/src/display/window.rs` (new)
  - `crates/neural-kernel/src/display/agent.rs` (compositor tick)
  - `crates/neural-kernel/src/display/mod.rs`
  - `crates/neural-kernel/src/display/framebuffer.rs` (swap_buffers)

### Feature: Taskbar / Dock
- LOC: ~200
- Dependencies: Compositor, ThemeEngine
- Implementation:
  1. `display/taskbar.rs` — renders bottom 40px region
  2. Each window gets a button: `[icon] Title` — click to focus/raise
  3. Clock display from `time_utils::datetime()`
  4. System tray area: profile indicator, theme indicator, network status
- Files to create/modify:
  - `crates/neural-kernel/src/display/taskbar.rs` (new)
  - `crates/neural-kernel/src/display/mod.rs`

### Feature: Mouse Cursor + Drag
- LOC: ~100
- Dependencies: HwBridgeAgent (PS/2 mouse IRQ if available), or keyboard-only focus
- Implementation:
  1. `compositor.rs` — cursor rendering (simple arrow bitmap)
  2. Mouse click → hit-test windows → focus + raise
  3. Drag: mousedown on titlebar → track delta → move window
- Files to modify:
  - `crates/neural-kernel/src/display/compositor.rs`

---

## Sub-Sprint 61.3 — SmileyOS Shell (40+ Commands)

**Target:** v0.61.3 | **LOC:** ~500 | **Dependências:** 61.2, HermesAgent

### Feature: Shell Command Port
- LOC: ~500
- Dependencies: HermesAgent command parser, Compositor (for terminal window)
- Implementation:
  1. Port 40+ SmileyOS commands into `hermes/commands.rs`:
     - **File ops (10):** `ls`, `cat`, `touch`, `mkdir`, `rm`, `cp`, `mv`, `pwd`, `cd`, `find`
     - **Process (6):** `ps`, `kill`, `top`, `uptime`, `dmesg`, `crashes`
     - **System (8):** `uname`, `date`, `hostname`, `env`, `shutdown`, `reboot`, `clear`, `help`
     - **Network (6):** `ping`, `ip`, `netstat`, `dns`, `dhcp`, `fetch`
     - **Agent (6):** `agents`, `skills`, `trust`, `profile`, `logs`, `inspect`
     - **Theme (3):** `theme`, `font`, `wallpaper`
     - **Debug (4):** `meminfo`, `cpuinfo`, `pci`, `backtrace`
  2. Each command implemented as a `CommandHandler` trait — `execute(args: &[&str]) -> Result<String>`
  3. Shell prompt `> ` with tab-completion for command names
  4. Pipe support: `ls | grep foo` — simple string filter via temporary buffer
- Files to create/modify:
  - `crates/neural-kernel/src/hermes/commands.rs` (expand existing)
  - `crates/neural-kernel/src/hermes/shell.rs` (new — shell parser/executor)
  - `crates/neural-kernel/src/hermes/mod.rs`
  - `crates/neural-kernel/src/hermes/agent.rs` (shell integration)

---

## Sub-Sprint 61.4 — First Desktop Apps (3 App-Agents)

**Target:** v0.61.4 | **LOC:** ~500 | **Dependências:** 61.2 (Compositor), 61.3 (Shell), Theming

### Feature: Hermes App Agent — Chatbot + Shell
- LOC: ~250
- Dependencies: Compositor, CortexAgent, HermesAgent, Shell (61.3)
- Agent manifest:
  ```rust
  AgentManifest {
      name: "hermes_app",
      kind: AgentKind::App,
      icon_hint: "chat bubble with neural nodes",
      schedule: ScheduleKind::Continuous,
      auto_start: true,
  }
  ```
- Implementation:
  1. Create `apps/hermes_app.rs`:
     - Janela dividida: 70% historico do chat, 30% terminal shell
     - Aba "Chat": input text → Enter → EventBus `CHAT_INPUT` → HermesAgent → resposta
     - Aba "Shell": prompt `> ` + 40+ comandos via Shell (61.3)
     - Histórico compartilhado entre abas
  2. `/hermes` abre a janela. Alt+Tab alterna.
  3. Dock icon: `[🤖] Hermes`
- Skills: `chat_send`, `chat_history`, `shell_exec`, `shell_complete`
- Files to create/modify:
  - `crates/neural-kernel/src/apps/hermes_app.rs` (new)
  - `crates/neural-kernel/src/apps/mod.rs`
  - `crates/neural-kernel/src/display/compositor.rs` (app lifecycle, dock)

### Feature: Settings App Agent — Tema + Sistema
- LOC: ~120
- Dependencies: Theming (61.1), ProfileManager, MouseAgent cursor config
- Agent manifest:
  ```rust
  AgentManifest {
      name: "settings_app",
      kind: AgentKind::App,
      icon_hint: "gear with sliders",
      schedule: ScheduleKind::Oneshot,
  }
  ```
- Implementation:
  1. Create `apps/settings_app.rs`:
     - `/settings` abre janela
     - Seção "Theme": lista de temas com preview, clique aplica `/theme <nome>`
     - Seção "Profile": `[Gamer] [Engineer] [Student] ...` — clique altera perfil
     - Seção "Display": resolução (futuro), cursor size (futuro)
  2. Cada opção é um `Button` na janela:
     - Desenha retângulo com texto
     - MouseAgent `MOUSE_CLICK` → hit-test vs botões → executa ação
  3. Fecha com ESC ou botão X no canto
- Skills: `get_themes`, `apply_theme`, `get_profiles`, `apply_profile`, `get_system_info`
- Files:
  - `crates/neural-kernel/src/apps/settings_app.rs` (new)
  - `crates/neural-kernel/src/apps/mod.rs`

### Feature: Power App Agent — Desligar + Reiniciar
- LOC: ~80
- Dependencies: ACPI shutdown (existing), triple-fault reboot (existing)
- Agent manifest:
  ```rust
  AgentManifest {
      name: "power_app",
      kind: AgentKind::App,
      icon_hint: "power button circle",
      schedule: ScheduleKind::Oneshot,
  }
  ```
- Implementation:
  1. Create `apps/power_app.rs`:
     - `/power` abre janela com 2 botões grandes:
       - `[🔴 Shutdown]` → ACPI poweroff
       - `[🔄 Reboot]` → triple fault (ou 8042 reset)
     - Confirmation dialog: clique Shutdown → "Are you sure? [Yes] [No]"
  2. Botões grandes (120×40px) com cor:
     - Shutdown: vermelho (COL_RED)
     - Reboot: amarelo (COL_YELLOW)
  3. Operações reais:
     - Shutdown: `acpi::power_off()` (já existe via ACPI)
     - Reboot: `outb(0x64, 0xFE)` — 8042 reset
- Skills: `shutdown`, `reboot`, `cancel`
- Files:
  - `crates/neural-kernel/src/apps/power_app.rs` (new)
  - `crates/neural-kernel/src/apps/mod.rs`

### Feature: App Management — AppRegistry + Dock Icons
- LOC: ~100
- Dependencies: All app agents above
- Implementation:
  1. `AppRegistry` — static `BTreeMap<&str, AppEntry>` registrado no boot
  2. Cada entrada: `{ agent_name, manifest, icon_16x16, window: Option<WindowId> }`
  3. Dock bar renderiza ícones de todas as apps registradas
  4. Clique no ícone da dock → se janela existe, foca; se não, cria
  5. LLM icon generation (61.5) preenche `icon_16x16` a partir de `icon_hint`
- Files:
  - `crates/neural-kernel/src/apps/registry.rs` (new)

### Dock Bar Layout (v0.61.4)
```
┌──────────────────────────────────────────────────────┐
│ [🤖 Hermes] [⚙️ Settings] [🔌 Power]  │  10:32 AM  │
│                                                      │
└──────────────────────────────────────────────────────┘
      app icons (click to focus/launch)        clock
```

### Feature: Hermes Chat Window
- LOC: ~300
- Dependencies: Compositor, CortexAgent, HermesAgent
- Implementation:
  1. Create `apps/hermes_chat.rs` — a `Window`-managed app:
     - Conversation history: scrollable text area (top 80% of window)
     - Input field: single line at bottom with cursor
     - Send button (or Enter key)
  2. Chat protocol:
     - User types → EventBus `CHAT_INPUT` → HermesAgent → CortexAgent → response
     - Response appended to conversation history buffer
     - History stored in ring buffer (last 50 exchanges)
  3. `/chat` command opens the chat window
  4. Alt+Tab cycles between chat window and terminal window
- Files to create/modify:
  - `crates/neural-kernel/src/apps/hermes_chat.rs` (new)
  - `crates/neural-kernel/src/apps/mod.rs` (new)
  - `crates/neural-kernel/src/lib.rs` (add mod apps)
  - `crates/neural-kernel/src/display/compositor.rs` (app lifecycle)

---

## Sub-Sprint 61.5 — LLM-Generated Icons

**Target:** v0.61.5 | **LOC:** ~250 | **Dependências:** 61.2, CortexAgent

### Feature: Dynamic Icon Generation
- LOC: ~250
- Dependencies: Compositor, CortexAgent (LLM), AgentRegistry
- Implementation:
  1. Agent manifest stores `icon_hint: &str` — text hint like "network globe" or "shield lock"
  2. On agent registration, `icon_hint` → LLM query: "Generate a 16×16 bitmap icon for: {hint}"
  3. LLM outputs compact format: 16 rows of 16 hex nibbles (256 bits per color plane)
  4. `IconCache` — `BTreeMap<&str, [u8; 256]>` — stores 16×16 BGRA32 icons
  5. Cache miss → generate via LLM → cache → render
  6. Fallback: geometric shape based on `AgentKind` (System=square, Driver=circle, Network=diamond, etc.)
- Files to create/modify:
  - `crates/neural-kernel/src/display/icons.rs` (new)
  - `crates/neural-kernel/src/display/mod.rs`
  - `crates/agent-core/src/lib.rs` (add icon_hint to AgentManifest)
  - `crates/neural-kernel/src/display/taskbar.rs` (icon render)

---

## Sub-Sprint 61.6 — WASM App Sandbox

**Target:** v0.61.6 | **LOC:** ~400 | **Dependências:** 61.2 (Compositor window lifecycle)

### Feature: WASM App SDK (Lightweight)
- LOC: ~400
- Dependencies: Compositor (Window trait), EventBus, SkillRegistry, AgentRegistry
- Implementation:
  1. Extend existing WasmSandbox stub (`wasm.rs`) with:
     - `load(wasm_bytes) -> Result<AppId>`
     - `execute(app_id, event: &[u8]) -> Result<Vec<u8>>`
  2. `App` trait for native apps (no WASM needed for first-party):
     - `trait App: Send { fn id() -> &'static str; fn on_event(&mut self, event: &[u8]) -> Result<Vec<u8>>; fn render(&self) -> &[u8]; }`
  3. `AppRegistry` — `BTreeMap<&str, Box<dyn App>>` — registered at boot
  4. Each app gets a Window in the Compositor
  5. WASM sandbox (future): `wasmi` embedder for third-party apps with linear memory pool
- Files to create/modify:
  - `crates/neural-kernel/src/wasm.rs` (extend existing stub)
  - `crates/neural-kernel/src/apps/registry.rs` (new)
  - `crates/neural-kernel/src/apps/mod.rs`

---

## Summary

| Sub-Sprint | Feature | LOC | Prioridade | Dependências |
|---|---|---|---|---|
| 61.0 | **MouseAgent** (PS/2 + habilidades) | ~380 | 🔴 Crítica | HwBridge IRQ12, EventBus |
| 61.1 | Theming Engine + Fonts | ~550 | 🔴 Crítica | 61.0, DisplayAgent |
| 61.2 | Compositor + Window Manager | ~800 | 🔴 Crítica | 61.0, 61.1, Framebuffer |
| 61.3 | SmileyOS Shell (40+ cmds) | ~500 | 🟡 Alta | 61.2, HermesAgent |
| 61.4 | **3 Desktop Apps** + Dock + Registry | ~550 | 🟡 Alta | 61.2, 61.3, Theming |
| 61.5 | LLM-Generated Icons | ~250 | 🟢 Normal | 61.2, CortexAgent |
| 61.6 | WASM App Sandbox | ~400 | 🟢 Normal | 61.2 |
| **Total** | **7 features, 3 app agents** | **~3430 LOC** | | |

### Os 3 App-Agents do Desktop

| App | Agente | Comando | Ícone | Skills |
|---|---|---|---|---|
| **Hermes** | `hermes_app` | `/hermes` | `🤖` chat + neural | chat_send, shell_exec, history |
| **Settings** | `settings_app` | `/settings` | `⚙️` gear | apply_theme, apply_profile |
| **Power** | `power_app` | `/power` | `🔌` power | shutdown, reboot |

### Implementation Order
```
61.0 (MouseAgent) ─→ 61.1 (Theming) ─→ 61.2 (Compositor) ─→ 61.3 (Shell)
                                                            └→ 61.4 (3 Apps + Dock) → 61.5 (Icons) → 61.6 (WASM)
```

61.0 MouseAgent é o primeiro — sem ponteiro não há clique em botões. 61.1 Theming + Fontes. 61.2 Compositor com dock, menu, drag. 61.3 Shell com 40+ comandos. **61.4 entrega os 3 apps: Hermes (chat+shell), Settings (tema+perfil), Power (shutdown+reboot)**, mais dock e AppRegistry. 61.5 LLM icons, 61.6 WASM sandbox.

### Dock Bar (v0.61.4)
```
┌──────────────────────────────────────────────────────┐
│ [🤖 Hermes] [⚙️ Settings] [🔌 Power]  │  10:32 AM  │
│                                                      │
└──────────────────────────────────────────────────────┘
```
