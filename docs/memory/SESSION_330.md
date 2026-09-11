# SESSION_330 — Metal: freeze da UI (deadlock) + timer morto (x2APIC) + cadência adaptativa (ADR-0104) — 2026-09-10

**Objetivo:** resolver os 2 problemas persistentes no HW real (escrita USB e freeze da UI Jarbas), diagnosticar a lentidão e tornar a cadência de tick adaptativa (AIOS-First).

**Fonte de verdade:** metal (Alienware) via fotos do FB/BOOT SCREEN; QEMU mascarava os bugs (nunca exercita os branches de HW).

---

## 1. Freeze da UI — RESOLVIDO (deadlock determinístico)

**Causa:** `crates/hermes/src/agents.rs:430` segurava `SKILL_STORAGE` (TicketLock **não-reentrante**) ao chamar `cortex_system_prompt`, que re-locka o mesmo `SKILL_STORAGE` em `memory_store::skills_l0_gated` (`memory_store.rs:220`) → **self-deadlock** no tick do `cortex_llm`, exatamente no estágio `S4` (`agents.rs:396` "classify feito") → `S5` (`:433`).

**Fix (commit `c174c504`):** montar o prompt sem segurar o lock (`cortex_system_prompt` fora; `find_skill_hint` com guard curto); removido `build_system_prompt_for` (footgun `&self`-sob-guard); clamp UTF-8-safe em `memory_store` (evitava panic em `&s[..57]` fora de fronteira de char).

**Validado no metal:** UI **viva** (não congela); `S` sobe e pisca `7↔9` (scheduler ciclando); HUD com `T` (renders) avançando.

## 2. USB no metal — instrumentado; AINDA ABERTO

O boot screen revela a causa: `USB: ramlog sem linhas hub/MSC (probe nao chegou?)` → `BOOT: skip models (no MSC)` → **nenhum modelo carregado** (LLM offline). No pendrive, `E:\BOOT.LOG` = **placeholder** e `E:\NSGDB.BIN` = **8MB de zeros** → o kernel não gravou nada. Confirmado independente de IRQ (MSC é polled): provável `should_probe_usb_host()` pulando o USB, ou o xHCI `PP=0/CCS=0` (SESSION_316). Instrumentação xHCI adicionada (`3bcb49ed`: ownership/PPC/PP-settle/RxDetect) — precisa de linha no FB p/ leitura (o log agora aparece na tela de boot).

## 3. Timer morto no metal (`TIMER_TICKS=0`) — RESOLVIDO

**Causa:** `USING_X2APIC` era saturado de firmware/CPUID **sem read-back**; se o `WRMSR` EN|EXTD não confirmasse, todos os acessos LAPIC (SVR/LVT/INIT/EOI) iam por MSR com a LAPIC real em MMIO → timer nunca armado. QEMU nunca exercita esse branch (hv gate).

**Fix (commit `36c9d1a6`):** `enable_x2apic_this_cpu()` lê `IA32_APIC_BASE` de volta e só liga x2APIC com EN+EXTD; `init_apic` deriva o modo do read-back e cai p/ MMIO se não confirmar; `+lapic_timer_diag()` no FB/BOOT.LOG; removido `apic_eoi()` espúrio no `ap_entry`; `SKIP_PIT` só para `MicrosoftHv` real (CPUID bit 31).

**Validado no metal:** `T` (TIMER_TICKS) **passa a avançar**.

## 4. Lentidão (~1 Hz → 1 fps) — calibração + adaptação (ADR-0104)

**Causa:** `LAPIC_TIMER_INIT_COUNT=0x800000` fixo (÷16) → ~1 disparo/s neste HW; `calibrate_timer_hz`/`estimate_timer_hz` tinham **zero callers**.

**Fix:** calibração em runtime (mede LAPIC counts/s via `CURRENT_COUNT`×TSC; `INIT = counts/target`) e **ADR-0104**: alvo deixa de ser constante.
- **R0 `k_nano`** mede (LAPIC/TSC/jitter/alive) + `set_tick_hz` (ponto único).
- **R1 `k_hal::timer_cap`** sanciona a faixa + `request_tick_hz` (rails `{30,60,120}`, dwell 5s, confirmação 2 janelas, rollback `LAST_GOOD`); auto só mantém/desce; subir > default = **HITL**.
- **R3 `jarbas`** objetivo de frame (`target_frame_ticks`/`frame_cost_us`).
- **Bin** dirige a política + `/tick` + `CONFIG.TXT TICK_HZ=`.
- **Testes host:** 10 em `k_hal::timer_cap` (clamps, untrusted→default, throttled→default, rails, dwell). `cargo test -p k-hal` = 34 pass.

Desenho completo: `docs/architecture/0104-cadencia-tick-adaptativa-timercap.md`. Motivo de não ser um PID: `TIMER_HZ` alimenta todos os timeouts tick-based (mesh 2000, backoff do `offer.rs`) → adaptação rara e quantizada.

## 5. Pendências abertas (registradas)

1. **USB/MSC no metal** (§2) — `probe nao chegou`; falta linha de diagnóstico xHCI no FB + investigar `should_probe_usb_host()`/PP=0.
2. **Mouse não responde** no metal (provável USB-HID atrás do mesmo xHCI morto; ou IOAPIC GSI12/dest).
3. **Modelos não carregados** (`skip models (no MSC)`) → sem LLM/TTS real no metal.
4. **Licença** AGPL vs MIT (IDEA #555) — decisão do maintainer.

## 6. Lições

- **QEMU mascara branches de HW** (x2APIC por hv gate; PP=1 mantido) — medir/validar no metal.
- **TicketLock não-reentrante**: nunca chamar função que re-locka o mesmo lock com o guard em mão.
- **`T=` (TIMER_TICKS) = discriminador**: avança = vivo; `0` = timer não dispara (não confundir com freeze).
- **Doc é cache**: medir o repo (SESSION_329).
