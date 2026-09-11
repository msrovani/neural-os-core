# ADR-0104: Cadência de tick adaptativa — TimerCap (R1) + política limitada AIOS-First

**Data:** 2026-09-10
**Status:** Proposed
**Lifecycle (INDEX):** `fazendo`
**IDEA:** **#556**
**Sprint / enquadramento:** continuidade da **ADR-0088** (Premissa Máxima AIOS-First: detectar→medir→decidir→otimizar→versionar) no eixo de tempo/cadência; uso dos anéis **ADR-0042** (R0 mede, R1 sanciona, R3 consome) e do padrão de observabilidade **ADR-0092**. Não abre Onda no **ADR-0100** (é correção de bypass, não T-* novo).
**Evidência:** metal s328-s330 — `TIMER_TICKS=0` por transição x2APIC não verificada (corrigido), e depois LAPIC `INIT_COUNT=0x800000` fixo → **~1 tick/s → 1 fps**. Frequência medida em runtime (`estimate_timer_hz`/`calibrate_lapic_timer`), alvo ainda constante `TARGET_TIMER_HZ=60`. QEMU nunca exercitou o branch x2APIC (hv gate), mascarando o bug.

**Não substitui:** ADR-0088 (premissa; esta ADR a aplica), ADR-0055/0057 (SMP/compute), ADR-0041 (HalOffer/DeviceCap — `TimerCap` é um irmão de DeviceCap), ADR-0092 (boot log/BOOT SCORE), ADR-0103 (k_nano modular — `TimerCap` vive em `k_hal`), ADR-0100 (backlog).
**Corrige:** o alvo de tick como constante hardcoded — bypass da premissa máxima (detecta mas não decide).

---

## 0. Decisão (ler primeiro)

1. **O alvo de tick deixa de ser constante.** Passa a ser uma **decisão limitada** dentro de uma **faixa sancionada** por medição, não um número mágico.
2. **Split por anel (não "k-hal → jarbas" direto):** **R0 `k_nano` mede** (LAPIC counts/s, TSC, jitter, alive) e é o **único ponto de mutação** (`set_tick_hz`); **R1 `k_hal` sanciona a faixa** (`TimerCap`) e aplica a política com clamp/rails/histerese (`request_tick_hz`); **R3 `jarbas` declara o objetivo** de cadência (frame) e reporta custo; o **bin** dirige a política e é dono do HITL/pin.
3. **Adaptação é rara e quantizada — não é um controlador contínuo.** Rails `{30,60,120}` Hz + dwell ~5 s + confirmação em 2 janelas + passo de no máximo um rail. Motivo: `TIMER_HZ` alimenta **todos** os timeouts contados em tick (mesh 2000, backoff do `offer.rs`, soft-tick) — mudar a taxa muda o significado real deles.
4. **Auto só mantém ou desce.** Subir acima do default (60) **é HITL** (Escalate ≠ Auto): altera power/thermal e a duração real dos timeouts → é **proposto**, aplicado só por pin do operador.
5. **Toda decisão é medida, logada (`ok`/`warn`), versionada** (SGDB `/hw/timer`) e sujeita a **rollback** (`LAST_GOOD`).
6. **Fatos medidos não são anuláveis:** mesmo o pin do operador é clampado à faixa de hardware.

---

## 1. Contexto

O timer LAPIC era armado com `INIT_COUNT=0x800000` fixo (÷16), o que neste HW dá ~1 disparo/s. A frequência passou a ser **medida** (`CURRENT_COUNT` decrementando contra o TSC) e o `INIT` derivado — mas o **alvo** permanecia `TARGET_TIMER_HZ = 60` hardcoded. Isso é o padrão "detecta mas não decide" que a premissa máxima rejeita.

Ao mesmo tempo, tratar o tick como knob cosmético seria pior: vários subsistemas contam *ticks* como tempo. Logo a decisão precisa ser **bounded** e **auditável**, não um PID em cima de um relógio de 60 Hz.

---

## 2. Split por anel

| Anel | Responsabilidade | Onde |
|---|---|---|
| **R0 `k_nano`** | Medir (LAPIC counts/s, TSC Hz, jitter, alive) e programar o `INIT`; **fonte única** do valor programado | `crates/k_nano/src/apic.rs`, `interrupts.rs` |
| **R1 `k_hal`** | **Sancionar** a faixa viável (`TimerCap`) e ser o **único ponto de mutação** (`request_tick_hz`) com clamp/rail/histerese; publicar no EventBus + `boot_report` | `crates/k_hal/src/timer_cap.rs` |
| **R3 `jarbas`** | Declarar o **objetivo** de cadência (frame) e reportar custo de frame | `crates/jarbas/src/display/{compositor,agent}.rs` |
| **Bin** | Dirigir a política periodicamente (como `cpufreq::ondemand_tick`) + HITL/pin/config | `crates/neural-kernel/src/{main,shell}.rs` |

**Não** colocar a decisão no caminho do IRQ do timer. **Não** adicionar campos a `HardwareInfo` (congelado por ADR-0100 T-005) — a telemetria vai via `boot_report` + SGDB `/hw/timer`.

---

## 3. Medido vs política

| | Dono | Exemplos |
|---|---|---|
| **Medido** | R0 | LAPIC counts/s, TSC Hz, `INIT`, `TIMER_TICKS` delta vs TSC (`alive`), jitter inter-tick (ppm), x2APIC readback, soft-tick ativo, `cpufreq::actual_ratio()` |
| **Faixa sancionada** | R1 (derivada, não objetivo) | `hz_min=max(30, counts/u32::MAX)`, `hz_max=min(240, counts/INIT_FLOOR)`; penalidade de jitter/confiança |
| **Política** | R2/R3 (escolha dentro da faixa) | rails `{30,60,120}`, postura thermal/carga, pin do operador, escalada acima do default |

`trusted=false` quando: medição = 0, TSC caiu no fallback conservador (2 GHz), jitter > 5%, ou timer não vivo.

---

## 4. Algoritmo limitado (sem runaway)

```
RAILS = [30, 60, 120]; DEFAULT = 60; MIN = 30; MAX = 240
INIT_FLOOR = 1000         // mantém overhead/jitter do IRQ <~1%
DWELL = 5 s               // tempo mínimo entre mudanças
CONFIRM = 2 janelas        // decisão precisa repetir

sanction(m) -> (hz_min, hz_max)
recommend(m, frame_cost_us, throttled, prev) -> hz   // puro
apply(hz):
    if hz > DEFAULT && !pinned: escalate_hitl(hz); return current   // Escalate ≠ Auto
    if confirmado (2x) && now-last_change >= DWELL:
        k_nano::apic::set_tick_hz(hz)          // clamp de novo, programa INIT, seta TIMER_HZ
        log old->new + inputs; LAST_GOOD = new
```

- **Default seguro:** qualquer falha/incerteza → 60.
- **Histerese:** confirmação em 2 janelas + dwell + quantização em rails (não oscila 58↔62).
- **Thermal:** `cpufreq::actual_ratio()` (APERF/MPERF), gate de hv como o `probe_and_init`.

---

## 5. Governança

- **Log (visível):** `slog_nano!("TIMER","ok"|"warn", "tick_hz {old}->{new} src={auto|operator|rollback} meas=… tsc=… jitter_ppm=… frame_us=… reason=…")`. `ok/warn`, não `trace` (lição SESSION_289/299).
- **Versão/memória:** gravar `{hz, source, inputs, policy_version, ts}` em SGDB **`/hw/timer`** e reidratar no próximo boot (loop detectar→medir→decidir→versionar).
- **Rollback:** `LAST_GOOD_HZ`. Se dentro do dwell a mudança regredir (timer não vivo / jitter spike / frame-starvation) → reverter (`src=rollback`, `warn`), uma vez.
- **HITL:** auto mantém/desce; subir acima do default = proposto (card / `HEALTH_ISSUE`), aplicado por pin. Pin via `/tick <hz>` (shell) ou `TICK_HZ=` em `CONFIG.TXT` (`src=operator`), sempre honrado, clampado à faixa.

---

## 6. Fallback (degradação graciosa)

| Condição | Comportamento |
|---|---|
| `LAST_MEASURED_LAPIC_HZ == 0` | `set_tick_hz` é **no-op**; mantém `INIT` fixo; sem adaptação |
| `trusted=false` | `recommend → DEFAULT`; `request` recusa mudanças; loga `warn` |
| Timer IRQ não avança | `source=Soft`; soft-tick inalterado; UI usa `wall_ticks()`; sem adaptação |
| x2APIC readback false | LAPIC MMIO; ainda mensurável → adaptação permitida |
| Erro de programação | mantém `INIT`/alvo anterior — nunca deixa o timer desarmado |

O soft-tick usa `1_000_000 / tick_hz` µs (respeita o rail escolhido).

---

## 7. API / superfícies

- **R0 `apic.rs`:** `TICK_HZ_MIN/DEFAULT/MAX`, `INIT_FLOOR`, `TICK_TARGET_HZ: AtomicU64`; `set_tick_hz(hz)->u64` (clamp + programa + TIMER_HZ), `tick_hz()`, `TimerMeasured`, `timer_measured()`.
- **R0 `interrupts.rs`:** `timer_jitter_ppm()`, `timer_alive()`.
- **R1 `k_hal::timer_cap`:** `TickSource`, `TimerCap`, `detect()/cap()`, `current_tick_hz()`, `recommend()`, `request_tick_hz()`, `pin_tick_hz()`, `note_frame_cost_us()`, `TOPIC_TIMER_CAP`.
- **R3 `jarbas`:** `target_frame_ticks()`, `frame_cost_us()`.
- **Bin:** `detect()` pós-`init_apic`; política no loop; `/tick`; `CONFIG.TXT TICK_HZ=`.

---

## 8. Verificação

- Teste host de `sanction`/`recommend`: rails, clamp, `trusted=false`→default, `throttled`→default, passo de um rail, dwell.
- Boot log mostra `TIMER` `tick_hz` com `meas/jitter/frame_us`; BOOT SCORE/HUD inclui a cadência.
- QEMU `-d int` na cadência escolhida; **metal** confirma `TIMER_TICKS` avançando à taxa alvo (não ~1 Hz).

---

## 9. YAGNI (não construir agora)

- Sem PID/controlador a cada tick; sem servo sobre 60 Hz.
- Sem política AC/bateria (não há driver); `throttled` (APERF/MPERF) é o proxy.
- Sem taxa por-núcleo/por-AP.
- Sem campos novos em `HardwareInfo`; telemetria em SGDB `/hw/timer`.
- Sem TimerAgent nem LLM no loop — a decisão é determinística; a IA lê telemetria e propõe override (HITL).
- **Não** migrar os timeouts tick-based existentes para microssegundos nesta ADR (dívida separada; flag, não bundle).

---

## 10. Riscos / notas

1. **Timeouts acoplados ao Hz.** Rails quantizados + dwell + não-subir (auto) limitam o drift semântico. Longo prazo: timeouts novos usam TSC.
2. **Cadência de frame usa `tick_id`, não `TIMER_TICKS`** — `TARGET_TIMER_HZ≈fps` é otimista; o gate por wall-clock (M4) corrige o acoplamento.
3. **Duas fontes de calibração** (uma morta) foram consolidadas antes de empilhar política.
4. **ADR-0104 é livre** — INDEX ia até 0103.
