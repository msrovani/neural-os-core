# SESSION_163 — Compute Dispatch (ADR-0057) + Generative Card Desktop (ADR-0058)

**Data:** 2026-07-21
**Contexto:** setup do ambiente Cursor Cloud (Linux/QEMU) → investigação de multicore/GPU/NPU → planejamento → implementação SMP + camada de compute + UI/desktop do Jarbas.
**Status:** v1.9.1 TEST / NÃO ESTÁVEL — gate v2.0.0 permanece fechado.

---

## 1. Ambiente (Cursor Cloud Linux)

- Toolchain: `nightly-2026-07-05` (rustc 1.98.0) via `RUSTUP_TOOLCHAIN` (o `rust-toolchain.toml` fixa canal Windows e quebra no Linux). Update script = `rustup toolchain install nightly-2026-07-05 -c rust-src -c llvm-tools-preview -t x86_64-unknown-none`.
- QEMU UEFI/OVMF `-accel tcg`; disco `python3 tools/build_image.py`; boot 8 fases + scheduler vivo. Detalhes em AGENTS.md `## Cursor Cloud specific instructions`.

## 2. ADR-0057 — Compute Dispatch SMP+GPU+NPU

**Causa-raiz do não-wake (evidência empírica):** SIPI broadcast (`all-excl-self`, `apic.rs`) + stack real/32b/64b compartilhadas do trampoline + GS.base = BSP → com ≥2 APs eles corrompem a stack na transição de modo. `-smp 2`→1 AP; `-smp 3/4`→0.

**WS-A (✅ evidenciado):** IPI direcionado (`send_init_ipi_to`/`send_sipi_to`) + wake **sequencial** por LAPIC ID + **stack + PerCpu por-AP** (`AP_PCPU`) + **retry INIT-SIPI-SIPI 3x** (robustez TCG). Bin reusa `k_nano::smp::ap_entry` → **unifica `AP_ENTRY_COUNTER`** (que `parallel_matmul` lê) e **emagrece** `neural-kernel::smp`. QEMU `-smp 4` → **APs acordados: 3**, `CorePools r0=1 r1=2 r2=1`.

**WS-B/C (✅ wired):** `cortex::compute::dispatch_ternary` — choke point único (`NPU→GPU→CPU-SMP→AVX2→scalar`) em `matmul_hybrid`/`Tensor::matmul`. `parallel_ternary_matmul` particiona **colunas** (decode m=1 escala). **Gated por `ap_pollable`** (hoje false) → deadlock-proof (APs em `hlt` sem IDT não podem ser workers; BSP faz o matmul).

**WS-D/E (✅ honesto):** GPU registra só se `BackendState::Ready` (canário silício); `k_hal::npu` detecta XDNA/Intel por PCI + `[NPU-HW] VERDICT=SOFTWARE` + fallback software. Kernel GPU W2A8 + driver NPU + on-demand AP-worker (IDT/reschedule-IPI) = **Layer S/HW**.

**WS-F (✅ parcial):** wake robusto + `hlt` idle + gate `ap_pollable` + seam `install_wake_fn`/`wake_aps`. **WS-G #412 (✅):** `cortex::decode` structured decoding (máscara de tokens antes do argmax), default no-op; self-test de boot PASS.

## 3. ADR-0058 — Generative Card Desktop (UI/Jarbas) — S1–S4 ✅

- **S1:** dep `embedded-graphics` (MIT/Apache; **compila no `x86_64-unknown-none` soft-float**) + `DrawTarget` adapter (`FbTarget`, `display/eg.rs`) sobre `DoubleBuffer` BGRA32. Self-test PASS.
- **S2:** `UiDeclaration` + parser JSON no_std + `UiRenderer` (`display/card.rs`): Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel. Self-test PASS.
- **S3:** `CardWindow` retido no compositor + `UI_SPEC` spawn/close + mouse: **close (X), mover (título), redimensionar (canto inf-dir), botão→`CARD_ACTION`, foco**. **Orb responsivo e barra de relógios/HUD preservados.**
- **S4:** cards demo (Sistema/Clima "amanhã"/Chamada de Vídeo com Atender/Microfone/Alto-falante/Encerrar) + `card_json_schema_hint()` p/ #412.
- Cards gerados como **dados** por Hermes/Trinity/Cortex ou por **skill WASM** (RustCoder/Codex, ADR-0052) + Cron. Supersede parcial ADR-0047-HMI (H3 ❌); ADR-0036 persona inalterada. kolibri = conceitos (adotar quando amadurecer).
- **Evidência QEMU:** 3 cards renderizados; clique fechou card; card "Clima" redimensionado via mouse; self-tests PASS; sem panics.

## 4. Lacunas honestas (registradas, não implementadas)

- **`AppWindow` legada resize:** método existe, não wired (aposentado pelos cards).
- **Tocar vídeo:** sem decoder/codec/pipeline; UVC = captura de câmera, não playback. `Panel` = superfície. Feed de câmera ao vivo no Panel é viável (não wired).
- **Spotify/música:** UI do player 100% expressável em card; áudio bloqueado por (a) TLS/HTTPS parcial (`tls_not_ready`), (b) sem codecs comprimidos (só PCM raw HDA), (c) DRM Widevine.

## 5. Lições críticas

- **SMP wake:** broadcast SIPI só acorda 1 AP se stack/PerCpu forem compartilhados; wake **sequencial + recursos por-AP + retry** é o caminho robusto (e TCG é flaky, exige retry).
- **APs sem IDT:** `hlt` com IF=0 e sem trabalho trava o AP; usar APs como workers vivos exige IDT compartilhada + reschedule-IPI (Layer S). Até lá, `parallel_*` é gated por `ap_pollable` para não deadlockar.
- **embedded-graphics** compila limpo em bare-metal soft-float — bom seam (`DrawTarget`) sem puxar std/GPU.
- **Sem fake:** GPU/NPU/vídeo/Spotify gated honestamente (canário/verdict/AWAITING) — nunca fingir Ready.

## 6. Artefatos / ADRs

- ADRs: `docs/architecture/0057-*.md`, `0058-*.md`; INDEX + IDEA_BANK (#467, #468) + TODO + `.md` da raiz atualizados.
- Código: `crates/k_nano/src/{apic,smp/*}`, `crates/neural-kernel/src/{apic,smp/mod,main}.rs`, `crates/cortex/src/{compute,decode,parallel_matmul,bitnet_avx2,tensor}.rs`, `crates/k_hal/src/{npu,gpu/compute_dispatch}.rs`, `crates/jarbas/src/display/{eg,card,compositor,agent}.rs`.
