# ADR-0065: Cosmic-like Window Manager + GPU Render 2D Multi-Vendor — Arquitetura Unificada

**Data:** 2026-07-25  
**Status:** Proposed — planejamento consolidado das FASES 0a/0b/0c + ADR-0062 + contexto neural-os-core  
**Lifecycle (INDEX):** `planejamento`  
**Estende:** ADR-0041 (Capability Rings), ADR-0057 (Compute Dispatch), ADR-0058 (Generative Card Desktop), ADR-0059 (Runtime App Factory), ADR-0062 (ClaudioOS vs Neural-AIOS), ADR-0063 (TicKV), ADR-0064 (RAG DB)  
**IDEA_BANK:** #479–#494 (novas), #495–#510 (derivadas)  
**Sprints alvo:** v1.9.x → v2.0.0 gate

---

## 1. Contexto e Problema

O neural-os-core v1.8.6 tem:
- **Compositor framebuffer CPU** em `crates/jarbas/src/display/` (DoubleBuffer BGRA32, 4 layers fixos, 60Hz, `embedded-graphics` DrawTarget)
- **UI declarativa** via `UiDeclaration`/`UiRenderer` (cards: Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel)
- **Orb afetivo + HUD** (gauges CPU/MEM/GPU/HD, FFT audio)
- **Input**: mouse PS/2 + USB xHCI (clique/drag/resize/close/focus), keyboard F1-F11 hardcoded
- **GPU backends multi-vendor** em `k_hal/src/gpu/` (Intel Gen9/Arc, NVIDIA Pascal, AMD RDNA1-4, VirtIO) — **compute-only**, display via UEFI GOP
- **SMP** com APs acordados mas **sem IDT própria** → `ap_pollable()=false` → work-stealing inativo
- **Async executor** stub em `k_nano/src/async_rt.rs` — 0 chamadas no bin

**Gaps para "cosmic-like" (pop!_OS COSMIC DE):**
1. **WM**: sem tiling, workspaces, focus model, decorations, dock rico, notifications, atalhos Super+*
2. **GPU render**: CPU memcpy no framebuffer GOP — sem blit 2D acelerado, sem page-flip HW, sem cursor HW
3. **Responsividade**: LLM/inferência trava UI (BSP single-threaded), I/O síncrono bloqueia tick

**Decisão:** atacar os 3 eixos em paralelo (FASE 0), depois implementar em fases incrementais (FASE 1/2).

---

## 2. Síntese das FASES 0 (Planejamento Concluído)

### 2.1 FASE 0a — RENDER-2D Multi-Vendor (@oracle ora-5/ora-6)

**Achado crítico:** **Intel BCS (Blitter Command Streamer) JÁ EXISTE** em `k_hal/src/gpu/intel.rs:330` — `BcsRing::blit` com `XY_SRC_COPY_BLT` completo, probeado no boot (`backend.rs:192`). NVIDIA/AMD não têm blit 2D; VirtIO GPU é stub.

| Vendor | Blit 2D | Page-flip HW | Cursor HW | Display Engine | Esforço Fase 1 |
|--------|---------|--------------|-----------|----------------|----------------|
| **Intel iGPU** | ✅ BCS pronto | ❌ DSPCNTR pendente | ❌ CUR_* pendente | Heurística só | **1 sprint** (trait + glue) |
| NVIDIA dGPU | ❌ (compute shader possível) | ❌ EVO 6+ meses | ❌ EVO | Não | YAGNI |
| AMD dGPU | ❌ (compute shader possível) | ❌ DCN 6+ meses | ❌ DCN | Não | YAGNI |
| VirtIO GPU | ❌ stub | ❌ SET_SCANOUT | ❌ UPDATE_CURSOR | Paravirt | 1-2 sprints (QEMU only) |

**Estratégia recomendada:** **Intel iGPU first** — maior ROI, reusa BCS existente, iGPU já é display owner (`display_coex.rs` policy). NVIDIA/AMD ficam compute-only. VirtIO como teste secundário em QEMU.

**Seam de integração:** trait `BlitBackend` em `k_nano` (R0) com impls:
- `CpuBlit` (fallback, sempre)
- `IntelBcsBlit` (registrada se BCS probe OK + canário PASS)
- `VirtIoGpuBlit` (Fase 3, opcional)
- `NvidiaComputeBlit` / `AmdComputeBlit` (Fase 4, opcional)

`jarbas` consome via `k_nano::blit_backend()` — **zero acoplamento R3→R1**, idêntico ao padrão `gpu_matmul` em `backend.rs:338`.

**Canário blit:** blit gradiente 64×64 → compara com golden CPU → `CapToken::GpuBlitReady` no `unlock_dag`.

---

### 2.2 FASE 0b — WM Cosmic-like no Jarbas (@designer ses_0647718f4ffeVHbIRF0pPWg6Xx)

**Documento completo entregue** com 10 seções + protótipos Rust compiláveis. Resumo:

| Componente | Arquivo novo | Conceito COSMIC adaptado |
|------------|--------------|--------------------------|
| **Tiling WM** | `tiling.rs` | Binary split (bsp) `Group{Horizontal/Vertical, children[2], sizes}` + `Window{id}` + `Placeholder` |
| **Workspaces** | `workspaces.rs` | `Workspace{tiling_root, floating_windows[]}` + `Workspaces{vec, active, previously_active, max=9}` |
| **Focus Model** | `focus.rs` | `FocusStack` MRU per-seat, policy `FollowsMouse`/`ClickToFocus`, border color focus (accent laranja) |
| **Decorations (SSD)** | `decorations.rs` | Title bar 28px, botões [×][□][─], rounded corners 8px (`RoundedRectangle`), resize handles 8 cantos (10px) |
| **Dock/Panel** | `dock.rs` | Bottom 48px, launchers + running indicators + clock + system tray, sempre visível |
| **Notifications** | `notifications.rs` | Queue top-right, urgency Low/Normal/Critical, timeout, actions → EventBus `NOTIFICATION_ACTION` |
| **Shortcuts** | `shortcuts.rs` | Tabela estática `KeyCombo→WmAction`: Super+1-9 workspace, Super+H/V split, Super+Shift+Arrows resize, Alt+Tab cycle, Super+Q close, Super+M max, Super+Enter launcher |
| **Design Tokens** | `theme.rs` (rewrite) | Paleta COSMIC dark/light (bg `#0F0F12`, accent `#FF8C00`), spacing 4/8/16/24/32, radius 4/8/12, header 48px, nav 56px |
| **Integração** | — | Unifica `AppWindow`+`CardWindow` → `Window` enum, `JarvisDesktop` ganha `workspaces`, `tiling_enabled`, `floating_windows` |
| **Phasing** | — | **F1**: workspaces+focus+shortcuts+dock (1 sprint) → **F2**: tiling engine+decorations (1 sprint) → **F3**: notifications+tokens+polish (0.5 sprint) |

**Constraints bare-metal:** sem GPU → `embedded-graphics` primitives (`RoundedRectangle`, `PrimitiveStyle`), sem TTF runtime → pré-rasterizar fontes no build (`tools/convert_ttf_to_bitmap.py`), ASCII only (0x20-0x7E).

---

### 2.3 FASE 0c — P13 (APs com IDT) + P16 (Async Executor) (@oracle ora-7/ora-8)

**Achados críticos (código já existe, só falta wiring):**
- IPI reschedule **já instalado**: vector 0x80, `send_ipi_reschedule()` em `apic.rs:571`, handler em `interrupts.rs:486`
- `parallel_matmul` **já chama** `send_ipi_reschedule()` (`cortex/src/parallel_matmul.rs:99,201`)
- `ap_entry` **não faz `lidt` nem `sti`** — APs sobem IF=0, sem IDT → `hlt` trava
- `AsyncExecutor` é **stub**: `process_wakes` vazio, `register_future` não armazena future, 0 chamadas no bin
- TSS/IST **único** (BSP) — APs precisam TSS próprio + 3 IST stacks (16KB cada) pra #DF/#PF/#GP não corromperem

**Arquitetura decidida:**
- **IDT compartilhada** (uma `lazy_static` em `k_nano/interrupts.rs`) — APs chamam `IDT.load()` + `sti`
- **TSS por-AP** + GDT compartilhada com TSS descriptors extras (selector 0x40+index)
- **IST stacks por-AP**: 3 × 16KB = 48KB/AP (alocar em `init_smp` region)
- **Gate `ap_pollable()`**: vira `true` **runtime** após `AP_IDT_READY == ap_woke` (barrier atômico)
- **Work-stealing**: **não** — fila central `ap_work` + IPI reschedule basta (YAGNI `WorkStealingPool` 222 LOC)
- **Async executor no BSP**: `process_wakes()` no scheduler tick (`agent-core/src/lib.rs:286`), futures `static` (sem alloc), `TimerFuture` demo
- **Nenhum future em AP** — APs são workers síncronos; futures vivem no BSP, offload compute via `ap_work::enqueue` + IPI

**Phasing:**
| Fase | Escopo | LOC est. | Gate |
|------|--------|----------|------|
| **F1** | TSS/IST por-AP + `ap_entry` lidt/sti + `AP_IDT_READY` barrier + `set_ap_pollable(true)` | ~250 | `parallel_matmul` roda em N cores TCG `-smp 4`, sem #DF/#GP |
| **F2** | `AsyncExecutor` storage + `process_wakes` real + `TimerFuture` + hook no scheduler tick | ~200 | `TimerFuture` acorda em N+100 ticks, scheduler não deadlock |
| **F3** | TLS/SSE async + DisplayAgent em AP fixo (opcional) | TBD | TLS handshake não bloqueia scheduler; render FPS estável em LLM longo |

**Risco WHPX/TCG:** SMP flaky em WHPX ("VP exit code 4") — **validar em TCG primeiro**, HW real depois. Feature flag runtime `set_ap_pollable(false)` pra fallback.

---

## 3. Código Pronto / Reutilizável (Inventário)

### 3.1 Jarbas Display (já compila, roda 60Hz)
```
crates/jarbas/src/display/
├── fb.rs           831 LOC  GpuDevice, DoubleBuffer, probe_uefi_framebuffer
├── eg.rs            96 LOC  FbTarget (DrawTarget adapter)
├── compositor.rs   667 LOC  JarvisDesktop, Layer enum, AppWindow, CardWindow, render()
├── card.rs         472 LOC  Widget enum, UiDeclaration, render_card, JSON parser
├── gauges.rs       265 LOC  GaugeSnapshot, draw_status_gauges
├── agent.rs        633 LOC  DisplayAgent, EventBus subs, F1-F11 hardcoded
├── theme.rs         53 LOC  5 temas hardcoded (NÃO wired)
├── ttf_engine.rs   160 LOC  TTF pré-rasterizado, FontManager
├── font.rs          70 LOC  VGA 8x16 bitmap ASCII 32-126
├── avatar.rs       120 LOC  Orb afetivo + partículas
├── console.rs       80 LOC  NeuralConsole (chat overlay)
└── mod.rs           26 LOC  module declarations
```

### 3.2 GPU Backends (k_hal/src/gpu/ — 37 arquivos, ~15k LOC)
```
├── detect.rs              524 LOC  PCI multi-vendor, GpuArch enum
├── backend.rs             455 LOC  GpuAccel enum, init_backend, map_bars_uc, CapToken gate
├── display_coex.rs        142 LOC  GpuAssignment (iGPU display + dGPU compute)
├── intel.rs               400 LOC  IntelRing (RCS), BcsRing::blit (XY_SRC_COPY_BLT) ✅
├── intel_gen9.rs          380 LOC  Gen9 specifics
├── intel_arc.rs           320 LOC  Arc/Alchemist
├── intel_guc.rs           280 LOC  GuC firmware bring-up
├── intel_gtt.rs           220 LOC  GGTT pin/unpin (GgttPin)
├── nvidia_pascal.rs       693 LOC  D2/D3/D4 (GMMU PTE, pushbuffer, runlist, QMD, fence)
├── nvidia_pascal_acr.rs   556 LOC  WPR/LSB/HS SEC2, AcrStage enum
├── nvidia_pascal_qmd.rs   410 LOC  QMD v01_07
├── nvidia_pascal_sw.rs    180 LOC  SW context
├── amd.rs                 280 LOC  IP discovery, PSP bring-up
├── amd_psp.rs             340 LOC  PSP firmware
├── amd_mes.rs             290 LOC  MES scheduler (stub)
├── amd_kiq.rs             260 LOC  KIQ compute queue
├── amd_discovery.rs       150 LOC  RDNA discovery table
├── firmware.rs            319 LOC  preload_blob, load_firmware_file (FAT 8.3 + VFS)
├── vram.rs                215 LOC  Buddy allocator power-of-2
├── canary.rs              327 LOC  run_vector_add_canary_nv/intel/amd
├── compute_dispatch.rs    280 LOC  register_compute_if_ready, gpu_ternary
├── kernel_pack.rs         374 LOC  NKP1 signed envelope
└── ... (xqueue, work_queue, msched, bench, sasos, pipeline_g5, direct_storage, kv_dma, ring)
```

### 3.3 SMP + Async (k_nano)
```
k_nano/src/smp/
├── mod.rs              180 LOC  init_smp, wake_aps_sequential, AP_PCPU, AP_STACK_SIZE
├── work_stealing.rs    222 LOC  WorkStealingDeque, WorkStealingPool (YAGNI)
├── ap_work.rs          340 LOC  ap_idle_loop, enqueue/try_dequeue, send_ipi_reschedule
└── percpu.rs           120 LOC  PerCpu struct, AP_PCPU array

k_nano/src/async_rt.rs  420 LOC  AsyncExecutor (stub), SPSC WakerQueue, Future trait próprio
k_nano/src/interrupts.rs 580 LOC  IDT lazy_static, IPI vector 0x80, IST stacks (BSP only)
```

### 3.4 ADR-0062 Itens Já MVP (P1/P2/P3/P11 ✅)
| Item | ADR-0062 | Status Real | Path |
|------|----------|-------------|------|
| P1 TLS 1.3 | ✅ MVP | ✅ WIRED | `neural-kernel/src/tls_client.rs` |
| P2 VFS + BlockDevice | ✅ MVP | ✅ WIRED | `k_nano/src/storage_bus.rs`, `fs_driver.rs` |
| P3 AHCI + NVMe | ✅ MVP | ✅ WIRED | `k_nano/src/storage/nvme.rs` (admin + 1 I/O q) |
| P11 USB Storage | ✅ MVP | ✅ WIRED | `k_nano/src/xhci/` + `usb_msc` (BOT+SCSI) |

---

## 4. Dificuldades e Riscos (Honestos)

### 4.1 Riscos Técnicos

| Risco | Probabilidade | Impacto | Mitigação |
|-------|---------------|---------|-----------|
| **Intel display engine (DSPCNTR) brickar tela** | Média (Fase 2) | Alto (tela preta = brick visual) | Fase 1 **não toca display engine** — só BCS blit em surfaces GTT. Fase 2 opcional, fallback GOP. |
| **BCS ring hang** | Baixa | Médio | BCS já probeado no boot, canário vector_add usa indiretamente. Timeout + reset no blit. |
| **TSS/IST por-AP mal formado → #GP no `ltr`** | Média (Fase 1 P13) | Alto (AP morre silencioso) | Copiar estrutura TSS do BSP, só mudar IST bases. Testar `-smp 2` primeiro. |
| **IPI reschedule perdido (race `ap_pollable`)** | Baixa | Médio | Barrier `AP_IDT_READY` atômico antes de `set_ap_pollable(true)`. |
| **Async executor reentrante (future spawn dentro de poll)** | Média | Alto | `process_wakes` sem lock — array estático + CAS. Futures `static` (sem alloc). |
| **DMA cross-GPU (NVIDIA VRAM → GOP sysmem) lento** | Alta (se Fase 4) | Médio | **Não fazer Fase 4** — Intel BCS no iGPU já resolve. NVIDIA/AMD compute-only. |
| **VirtIO GPU não emulado no HW real** | Certa | Baixo | Só QEMU. Fase 3 opcional. |
| **WHPX + SMP = VP exit code 4** | Alta (Windows host) | Bloqueador teste | TCG only para dev. HW real pra validação final. |

### 4.2 Dívida Técnica (Ponytail)

- `work_stealing.rs` (222 LOC) **morto** — remover se Fase 1 confirmar que `ap_work` fila central basta
- `theme.rs` (53 LOC) **não wired** — rewrite completo na Fase 3
- `ttf_engine.rs` **não wired aos cards** — pré-rasterizar `FONT_12X20` e usar
- `AppId` enum hardcoded coexiste com `CardWindow` — unificar na Fase 1
- `DisplayAgent` F1-F11 hardcoded → migrar pra tabela `shortcuts.rs`
- `async_rt.rs` trait `Future` próprio vs std `Future` — **não unificar** (YAGNI), conviver

---

## 5. Migrações Necessárias

### 5.1 Jarbas → WM Cosmic-like (Fase 1-3)

| Migração | De | Para | Risco |
|----------|-----|------|-------|
| Compositor layers | 4 fixas (Orb/Hermes/Apps/Dock) | Dinâmicas: Workspace tiling + Floating + Dock + Notifications + Orb/Hermes overlay | Baixo (adição, não quebra) |
| Janelas | `AppWindow` (enum hardcoded) + `CardWindow` | `Window` enum unificado: `Legacy(AppId) \| Card(UiDeclaration) \| Tiled(TilingNode) \| Floating(FloatingWindow)` | Médio (touch points: compositor, agent, render) |
| Input | F1-F11 + mouse cards | Tabela `KeyCombo→WmAction` + mouse hit-test unificado (tiling + floating + dock + notifications) | Baixo (adição) |
| Tema | 5 hardcoded não wired | Paleta COSMIC dark/light/high-contrast wired em todo render | Baixo |
| Fontes | VGA 8x16 + FONT_6X10/9X15 | + `FONT_12X20` pré-rasterizada | Baixo (build script) |

### 5.2 GPU Render 2D (Fase 1-2)

| Migração | De | Para |
|----------|-----|------|
| Compositor blit | `DoubleBuffer::fill_rect`/`set_pixel` CPU memcpy | `k_nano::blit_backend().blit(src, dst, w, h, bpp)` → `IntelBcsBlit` ou `CpuBlit` |
| Page-flip | `DoubleBuffer::swap()` (memcpy back→front) | `blit_backend.page_flip(fb_ptr, w, h, stride)` → Intel DSPCNTR (Fase 2) ou memcpy (Fase 1) |
| Cursor | `draw_mouse_cursor` CPU no compositor | `blit_backend.cursor_set(image, x, y)` → Intel CUR_* (Fase 2) ou CPU blit |

### 5.3 SMP/Async (Fase 1-2 P13/P16)

| Migração | De | Para |
|----------|-----|------|
| AP entry | `hlt` com IF=0, sem IDT | `lidt` + `lgdt`/`ltr` (TSS próprio) + `sti` → `ap_idle_loop` com `hlt` acordável |
| `ap_pollable()` | `false` const | Runtime `true` após barrier `AP_IDT_READY` |
| Matmul | BSP only (`cortex::parallel_*` gated) | `parallel_matmul` distribui linhas nos APs via `ap_work::enqueue` + IPI |
| Async | 0 chamadas | `global_executor().process_wakes()` no scheduler tick + `TimerFuture` demo |

---

## 6. Desenvolvimento — Fases de Implementação

### FASE 1 — WM-CORE (Jarbas) — **2 sprints**

| Sprint | Entregável | Arquivos novos/modificados | Gate QEMU |
|--------|------------|----------------------------|-----------|
| **1.1** | Workspaces + Focus + Shortcuts + Dock básico | `workspaces.rs`, `focus.rs`, `shortcuts.rs`, `dock.rs` (novo), `compositor.rs` (integração), `agent.rs` (migrar F1-F11 → shortcuts) | Super+1-4 troca workspace, Alt+Tab cicla, border laranja foco, dock mostra apps + clock |
| **1.2** | Tiling engine + Decorations (SSD) | `tiling.rs`, `decorations.rs` (novo), `compositor.rs` (layout tiling + floating), `floating.rs` (extraído de cards) | 3 apps abrem → tiling auto 3 painéis, Super+H/V split, Super+Shift+Arrows resize, × fecha, drag title move, 8 handles resize |

### FASE 2 — WM-GPU (Render 2D Acelerado) — **1.5 sprints**

| Sprint | Entregável | Arquivos | Gate |
|--------|------------|----------|------|
| **2.1** | Trait `BlitBackend` + `CpuBlit` + `IntelBcsBlit` + canário + hook no compositor | `k_nano/src/blit_backend.rs`, `blit_cpu.rs`, `k_hal/src/gpu/intel_blit.rs`, `k_hal/src/gpu/canary.rs` (blit), `k_hal/src/unlock_dag.rs` (CapToken), `jarbas/compositor.rs` (usa blit) | Boot log `BlitBackend=IntelBCS`, canário blit PASS, blit 1920×1080 ≥2× CPU memcpy |
| **2.2** | Page-flip HW Intel (DSPCNTR) + Cursor HW (CUR_*) — **opcional** | `k_hal/src/gpu/intel_display.rs`, `intel_blit.rs` estende page_flip/cursor, canários | Screenshot HW: page-flip sem tearing, cursor visível |

### FASE 3 — P13/P16 (SMP + Async) — **1.5 sprints** (paralelo com Fase 1/2)

| Sprint | Entregável | Arquivos | Gate |
|--------|------------|----------|------|
| **3.1** | TSS/IST por-AP + `ap_entry` lidt/sti + `AP_IDT_READY` barrier + `set_ap_pollable(true)` | `k_nano/src/smp/percpu.rs`, `mod.rs` (ap_entry), `interrupts.rs` (ap_load_idt/gdt), `neural-kernel/src/smp/mod.rs` | TCG `-smp 4`: `parallel_matmul` 4× speedup, sem #DF/#GP em 100 iterações |
| **3.2** | `AsyncExecutor` storage + `process_wakes` real + `TimerFuture` + hook no scheduler | `k_nano/src/async_rt.rs`, `agent-core/src/lib.rs` (tick loop) | `TimerFuture` acorda em N+100 ticks, scheduler continua tickando |

---

## 7. Necessidades (Pré-requisitos)

### 7.1 Hardware / Validação
- **Intel iGPU real** (notebook Skylake+ / Gen9) — **obrigatório** pra Fase 2 (BCS blit + display engine). QEMU não emula BCS nem display engine Intel.
- **GTX 1050 (Pascal)** — já validado pra compute (canário PASS). Não necessário pra Fase 2.
- **TCG QEMU com `-device virtio-gpu` — pra Fase 3 VirtIO (opcional).

### 7.2 Tooling / Build
- `tools/convert_ttf_to_bitmap.py` — estender pra gerar `FONT_12X20` (títulos dock/notifications)
- `cargo check --release` — 0 erros em cada gate
- `target/agent-<nome>` dirs isolados pra builds paralelos

### 7.3 Documentação / ADRs
- ADR própria pra **Fase 1** (WM-CORE) — supersede ADR-0058 parcial
- ADR própria pra **Fase 2** (GPU Blit) — estende ADR-0057 WS-D
- ADR própria pra **Fase 3** (P13/P16) — estende ADR-0057 WS-A/WS-F

---

## 8. Alcance (Scope)

### Dentro do Escopo (v1.9.x → v2.0.0 gate)
✅ WM cosmic-like completo: tiling (bsp), workspaces (4 default, 9 max), focus (follows-mouse/click), SSD decorations, dock bottom 48px, notifications top-right, shortcuts Super+*, design tokens COSMIC  
✅ GPU blit 2D Intel iGPU via BCS (reusa código existente) + fallback CPU  
✅ Page-flip HW Intel (DSPCNTR) + Cursor HW (CUR_*) — **opcional, Fase 2.2**  
✅ SMP work-stealing ativado: APs com IDT/TSS/IST próprios, IPI reschedule, `ap_pollable=true`  
✅ Async executor wired: `process_wakes` no tick, `TimerFuture` demo, base pra TLS/SSE futuro  
✅ Integração limpa: `k_nano` (R0) traits `BlitBackend`/`AsyncExecutor`, `k_hal` (R1) impls, `jarbas` (R3) consome via R0

### Fora do Escopo (YAGNI / Pós-v2.0)
❌ NVIDIA/AMD blit 2D via compute shader + DMA cross-GPU  
❌ EVO display engine NVIDIA / DCN AMD (6+ meses cada)  
❌ VirtIO GPU driver completo (só QEMU)  
❌ Work-stealing entre APs (`WorkStealingPool` — remover)  
❌ Async mutex / futures em APs  
❌ DisplayAgent em AP dedicado (Fase 3 opcional)  
❌ TTF runtime / text shaping / Unicode completo  
❌ Animações fluidas (workspace switch, window minimize) — cut direto por enquanto  
❌ Multi-monitor real (output_id placeholder só)  
❌ Wayland/X11 compat — não é o paradigma

---

## 9. Resultado Esperado (Definition of Done)

### v1.9.0 "Cosmic WM + GPU Blit" (após Fase 1 + 2.1)
- Boot QEMU/TCG: compositor roda 60Hz, **Super+1-4 troca workspace**, **Alt+Tab cicla janelas**, **Super+H/V split tiling**, **Super+Shift+Arrows resize**, **×/□/─ botões funcionam**, **drag title bar move**, **dock bottom mostra apps+clock**, **border laranja foco**
- Boot HW real (Intel iGPU): **blit 2D via BCS** ≥2× CPU memcpy, canário PASS, `CapToken::GpuBlitReady` granted
- `cargo check --release` = 0 erros, 0 warnings novos

### v1.9.5 "Page-flip + Cursor HW" (após Fase 2.2 — opcional)
- HW Intel: **page-flip atômico no vblank** (sem tearing), **cursor HW** sobreposto automaticamente
- Fallback GOP se display engine falhar

### v2.0.0 "SMP + Async Ready" (após Fase 3)
- TCG `-smp 4`: **`parallel_matmul` 3.5-4× speedup** vs single-core, APs vivos com IDT/TSS/IST
- Scheduler tick roda `process_wakes()` — **`TimerFuture` acorda no tick certo**
- Base pronta pra TLS handshake async + SSE streaming sem travar UI

---

## 10. Goal Final

> **Entregar um OS bare-metal com WM cosmic-like funcional (tiling, workspaces, focus, dock, notifications, shortcuts, design system), render 2D acelerado na iGPU Intel via BCS existente, e fundação SMP/Async sólida (APs vivos + executor cooperativo) — tudo sem quebrar o que já funciona, em ~4.5 sprints, validável em QEMU TCG + HW real Intel iGPU + GTX 1050.**

**Próximo passo imediato:** despachar `@fixer` pra **Fase 1.1** (workspaces + focus + shortcuts + dock) — arquivos alvo: `crates/jarbas/src/display/{workspaces,focus,shortcuts,dock,compositor,agent}.rs`. O designer já entregou protótipos Rust compiláveis em cada seção do documento FASE 0b.

---

## Apêndice A — Referências Cruzadas

| Documento | Conteúdo |
|-----------|----------|
| ADR-0041 | Capability Rings (P0-P9 PoC) — base pra `CapToken::GpuBlitReady` |
| ADR-0057 | Compute Dispatch SMP+GPU+NPU — WS-A/WS-F (SMP), WS-D (GPU gate) |
| ADR-0058 | Generative Card Desktop (S1-S4 ✅, S5 residual) — base do `jarbas` |
| ADR-0059 | Runtime App Factory (wasmi + Cranelift + Rust-subset) — futuro skills |
| ADR-0062 | ClaudioOS vs Neural-AIOS — adoção seletiva (P1-P11 MVP ✅) |
| ADR-0063 | TicKV + NoProto + Índices IA (SGDB) — persistência |
| ADR-0064 | RAG DB in-kernel — vector search |
| SESSION_176 | SGDB Memory Quality (SleepCycle ckpt + recall L4 BQ + Tickv V-flag + ART SIMD) |
| SESSION_163 | SMP wake multi-AP fix (SIPI direcionado + stack/PerCpu por-AP) |
| SESSION_152 | Deadlock NETSTACK (nunca await dentro de lock) |
| AGENTS.md | Lições críticas, workflow, regras de build |

---

## Apêndice B — IDEA_BANK Novas (derivadas deste ADR)

| # | Ideia | Destino | Status |
|---|-------|---------|--------|
| #495 | Trait `BlitBackend` em `k_nano` | ADR própria Fase 2 | ⏳ |
| #496 | `CapToken::GpuBlitReady` no `unlock_dag` | ADR própria Fase 2 | ⏳ |
| #497 | Canário blit 2D (gradiente 64×64 vs golden) | ADR própria Fase 2 | ⏳ |
| #498 | TSS/IST por-AP + `AP_IDT_READY` barrier | ADR própria Fase 3 | ⏳ |
| #499 | `AsyncExecutor` storage + `TimerFuture` | ADR própria Fase 3 | ⏳ |
| #500 | `WorkStealingPool` remoção (222 LOC morto) | Ponytail cleanup | ⏳ |
| #501 | `theme.rs` rewrite COSMIC tokens | Fase 3 WM | ⏳ |
| #502 | `FONT_12X20` pré-rasterizada | Build script | ⏳ |
| #503 | Unificar `AppWindow` + `CardWindow` → `Window` enum | Fase 1 WM | ⏳ |
| #504 | Migrar F1-F11 hardcoded → `shortcuts.rs` tabela | Fase 1 WM | ⏳ |
| #505 | Page-flip HW Intel DSPCNTR | Fase 2.2 opcional | ⏳ |
| #506 | Cursor HW Intel CUR_* | Fase 2.2 opcional | ⏳ |
| #507 | VirtIO GPU driver (QEMU only) | Fase 3 opcional | ⏳ |
| #508 | DisplayAgent em AP fixo | Fase 3 opcional | ⏳ |
| #509 | Multi-monitor `output_id` real | Pós-v2.0 | ❌ |
| #510 | Animações workspace/window (EaseInOutCubic 200ms) | Pós-v2.0 | ❌ |

---

**Fim do ADR-0065.** Próxima ação: despachar `@fixer` para Fase 1.1 com os protótipos do designer em mãos.