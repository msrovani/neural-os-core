# ADR-0100: Backlog unificado K³CHJ — custo × anel (pós-0089)

- **Status:** Proposed (plano operacional; ADRs temáticas **não** são substituídas)
- **Lifecycle:** `fazendo`
- **Ideia:** #538
- **Sprint:** contínuo até gate v2.0.0 + Layer S/HW
- **Data:** 2026-08-22
- **Depende de:** ADR-0088 (premissa máxima — analisar **primeiro**), INDEX 2026-08-22
- **Não substitui:** 0055, 0057, 0077, 0078, 0081–0089. Esta ADR **ordena e filtra** o que ainda tem valor.

**Lacuna 0090–0099:** IDs não alocados. Salto para **0100** é deliberado (plano-mestre), não um buraco de decisão. Não criar 0090–0099 só para preencher tabela. **Exceção 2026-08-24:** ADR-0092 (`0092-boot-observability.md`, IDEA #539) — contrato de observabilidade de boot. 0090/0091/0093–0099 continuam vazios.

---

## 0. Premissa de filtro (o que entra)

Cada residual da revisão INDEX/TODO entra só se **as quatro** forem verdade:

1. **Valor** — fecha gap observável (boot, metal, honesty, I/O, SMP, HITL).
2. **Aderência** — cabe no anel certo (`k_nano`→`k_hal`→`k_ai`/`cortex`→`hermes`→`jarbas`; bin = wire).
3. **Viabilidade** — no_std, HITL, sem teto-de-crate como política, QEMU ≠ aceite metal.
4. **Melhoria** — 10% mensurável (latência, cores online, I/O, segurança) **sem** degradar Trust.

**Fora do plano (não TODO aqui):** BitTorrent; ZK/TPM Quote; WASI-P2 pleno; NTFS/EXT **write**; Vulkan; Linux ABI; Git HTTPS nativo; SigLIP/8B/6-slots como sprint próximo; WireGuard; XDNA 💰; SmileyOS/Cube; `MAX_APS`/8 slots GDT como teto; dual RAG in-kernel (0064).

---

## 1. Eixos de ordenação

| Eixo | Valores | Uso |
|------|---------|-----|
| **Custo** | S ≤1d / M 2–5d / L 1–3 sem / XL HW+sprint | ordem primária |
| **Anel** | R0 `k_nano` → R1 `k_hal` → R2 `k_ai`/`cortex` → R3 `hermes`/`jarbas` → bin wire | desempate: anel mais baixo primeiro |
| **Dependência** | evidência metal SMP **antes** de runqueue/mesh 2c/Ring3 produção | nunca inverter |

K³CHJ: código novo na crate do anel; `neural-kernel` só `pub use` / boot / residual.

```
Onda 0  S   R2/bin   honesty
Onda 1  S–M R0       I/O + HardwareInfo
Onda 2  S   evidência metal SMP (código já no tree)
Onda 3  S–M R3       OTA/install refinements
Onda 4  M–L R0       AP workers + runqueue
Onda 5  M   R0/R3    mesh 2c + CRDT
Onda 6  L   R0/bin   Ring3 produção + PIN_DMA
Onda 7  M–L R2       W2A8 + 0078 Fase 1 só
Onda 8  XL  R1       golden GPU / SDMA
Onda 9  M   R3       cards S5 / A/V
Onda 10 L   R2       AirLLM DMA residual
```

---

## 2. Veredito por origem (item → entra / corta / defer)

| Origem | Veredito | Onda |
|--------|----------|------|
| 0055/0057 metal K23 | **entra** (aceite, não reescrever ICR) | 2 |
| 0055 BSS 511 / `ap_pollable` | **entra** | 4 |
| 0089 runqueue | **entra** após 4.1 | 4 |
| #513 `measure_bandwidth` + BMIDE | **entra** | 1 |
| 0082 `/hw/gpu|storage|net` | **entra** | 1 |
| 0083 métrica IA boot | **entra** | 0 |
| 0086 A2 A3 A7 A4 A8 | **entra** | 3 |
| 0086 A5 | **verify** (SESSION_253 UI disco) | 3 |
| 0086 A9 mini default | **HITL maintainer** | 3 |
| 0086 A1 Ed25519/TPM update | **defer** público/mesh | — |
| 0081 mesh 2c SIPI | **entra** após Onda 2 | 5 |
| 0081 SemanticRouter | **defer** (lexical até slot) | 5 nota |
| 0081 merkle piece | **fora** (BitTorrent ❌) | — |
| 0081 CRDT merge | **entra** se no_std já tem tipos | 5 |
| 0077 WHPX/HW + `register_native_ring` | **entra** após SMP metal estável | 6 |
| 0076 SYS_PIN_DMA | **entra** após 6 | 6 |
| 0076 F1–F17 resto | **corta** (já ✅ s224) | — |
| 0084 F4 W2A8 | **entra** gated HW | 7 |
| 0078 Fase 1 threshold | **entra** (PPL, barato vs 6 slots) | 7 |
| 0078 Fase 2–4 8B/SigLIP | **defer** RAM/boot metal 2B primeiro | — |
| 0048–50 golden | **AWAITING_HW** | 8 |
| 0087 F6 AMD SDMA | **AWAITING_HW** | 8 |
| 0058 S5 + A/V | **entra** pós-desktop estável | 9 |
| 0046 DMA prefetch | **AWAITING** | 10 |
| 0040 NTFS write | **fora** (risco disco alheio) | — |

---

## 3. Ondas — subitens e TODOs enumerados

Numeração global **T-001…**. Aceite de onda = todos os T da onda `[x]` **ou** residual explícito no STATE.

### Onda 0 — Honesty cognitiva (custo S · R2 `k_ai` + bin wire)

**Por quê:** ADR-0083 residual; AIOS-first sem métrica é telemetria cega.

#### 0.1 Contador de decisões no boot
- **T-001** Emitir no `boot_report` (ou tópico EventBus `BOOT_AI`) `observe/plan/act/verify` com counts do plano `k_ai` (SESSION_272).
- **T-002** Distinguir `Auto` vs `Escalate` (HITL) — nunca contar Escalate como “IA executou”.
- **T-003** Log uma linha serial: `BOOT_AI observe=N plan=N act=N escalate=N`.
- **T-004** Teste host: parse da linha / struct (sem QEMU).

#### 0.2 HardwareInfo: congelar MVP
- **T-005** Não adicionar campos em `HardwareInfo` (0082 checklist). Novos dados → SGDB `/hw/...`.
- **T-006** Wrappers `hw_cpu_*` que leem cache (early-boot) — sem duplicar CPUID.

**Aceite 0:** serial mostra `BOOT_AI`; struct HardwareInfo congelada.

---

### Onda 1 — I/O self-adaptive + inventário (custo S–M · R0 `k_nano` · R1 `k_hal` só probe)

**Por quê:** #513; TCG hang PIO; HardwareInfo ondas GPU/storage.

#### 1.1 `measure_bandwidth` (#513)
- **T-007** API `BlockDevice::measure_bandwidth` (ou helper em `k_nano::storage`) — bytes/tempo TSC, sem `f32` se possível (u64 B/s).
- **T-008** Medir NVMe (PRP) quando o plano inclui NVMe.
- **T-009** Medir AHCI PRDT quando o plano inclui AHCI.
- **T-010** BMIDE 0xC8: implementar **ou** skip honesto `VERDICT=UNSUPPORTED` (não PIO eterno).
- **T-011** TCG lento: log `CRÍTICO` + boot sem disco se medida < limiar; **nunca** travar (0088).
- **T-012** `k_ai` plano de storage usa medida (reordena / skip) — bin só executa.

#### 1.2 SGDB `/hw/*` (0082)
- **T-013** StorageBus → `/hw/storage/<id>/…` pós-probe.
- **T-014** GPU detect → `/hw/gpu/<id>/…` (BAR roles já medidos s252).
- **T-015** NetAgent → `/hw/net/<nic>/…` (sem WiFi se rádio ausente).
- **T-016** WifiAgent: só se device presente; senão skip (não fingir).

**Aceite 1:** TCG NoDisk ou disco lento não freeze; `/hw/storage` ou `/hw/gpu` visível pós-Fase 6.

---

### Onda 2 — Aceite SMP metal (custo S evidência · R0 já no tree s281)

**Por quê:** código ICR+GDT feito; aceite ≠ QEMU.

#### 2.1 Imagem e dois silícios
- **T-017** `cargo build --release -p boot` + `build_image.py --hw --unified` (sem modelos se K22 ainda for o gate).
- **T-018** i5 7ª: serial/FB `K23` e `online == madt_enabled - 1`.
- **T-019** Core 7 240H: mesmo critério (sem freeze N×SIPI).
- **T-020** Confirmar log: ICR sem bits 12–19; sem INIT deassert em x2APIC.
- **T-021** Se falhar: SESSION + hipótese (não BSP-only como destino).

**Aceite 2:** dois notebooks **ou** um + segundo defer explícito HITL.

---

### Onda 3 — Install/OTA refinements (custo S–M · R3 `hermes`/`jarbas` · `k_ai` provision)

**Por quê:** 0086 A* com valor de ciclo de vida; A1 fora do gate.

#### 3.1 Evidência de ciclo (A2)
- **T-022** Script QEMU: `serve_update.py` → guest install/provision/update (Ato 1–3).
- **T-023** Evidência em `docs/evidence/` (serial + HTTP 200). Hash SHA-256 guest já FIPS s252.

#### 3.2 Provision no 1º boot (A3)
- **T-024** Hook `NET_READY` + `SELF.STATE first_boot` → ModelProvisioner (não só shell).
- **T-025** HITL se download grande; log honesto se net down.

#### 3.3 Telemetria periódica (A7)
- **T-026** Cron/LogAgent POST `/api/logs` (já existe path) sem spam (backoff s269).

#### 3.4 A5 seleção de disco
- **T-027** Verificar card SESSION_253 no tree; fechar TODO A5 **ou** listar gap UI restante.

#### 3.5 Menu live/install (A4)
- **T-028** UI `[L]ive` timeout ~5s / `[I]nstall`; `BOOT_MODE` já existe.
- **T-029** Default Live (não formatar HD sem tecla).

#### 3.6 Rollback tries (A8)
- **T-030** `tries` 1→3 + `last_good` (ChromeOS-like); teste host da máquina de estados.

#### 3.7 A9 mini default
- **T-031** Só se maintainer confirmar: `--hw --unified` default `MODELS_SOURCE=network`.

#### 3.8 A1 (não nesta onda)
- **T-032** Documentar defer: FNV-1a agora; Ed25519+PCR[8] quando update **público**.

**Aceite 3:** A2 evidência **ou** A3+A7 no QEMU; A4 sem wipe acidental.

---

### Onda 4 — APs como workers + runqueue (custo M–L · R0 `k_nano` · R2 `cortex` matmul · `agent-core`)

**Por quê:** 0057 WS-F residual; 0089; depende Onda 2.

#### 4.1 `ap_pollable` verdadeiro
- **T-033** Barreira `AP_IDT_READY` após IST heap + TSS por CPU (já GDT s281).
- **T-034** `sti` no AP só com IST mapeado (já regra s281).
- **T-035** `cortex::parallel_*` deixa de no-op no BSP quando `ap_pollable()`.
- **T-036** Self-test matmul 2+ cores (QEMU TCG `-smp 2` pós-fix 2c).

#### 4.2 Dívida BSS `MAX_APS=511`
- **T-037** PerCpu/IST: heap/`Vec` limitado a `madt_enabled`, **não** array 511 no `.bss`.
- **T-038** Teste: boot 1c não reserva 511 TSS.

#### 4.3 Runqueue (ADR-0089, feature `smp-runqueue`)
- **T-039** Ligar feature no bin **só** após T-033.
- **T-040** BSP `dispatch_to_core`; ring0 agents nunca migram.
- **T-041** `steal_agent` min-1 (starvation).
- **T-042** IPI reschedule vetor dedicado; `wake_core_if_needed`.
- **T-043** Testes host: `CPU_COUNT` explícito + `TEST_LOCK` + `clear_all_queues()`.
- **T-044** Telemetria `pending` por core no HUD (jarbas lê static — desenho no `render()`).

**Aceite 4:** QEMU `-smp 2` matmul AP; agents R1/R2 em ≥2 filas **ou** feature off + log honesto.

---

### Onda 5 — Mesh (custo M · R0 transporte · R3 consumo)

**Por quê:** SESSION_280 2c hang; CRDT útil; SemanticRouter caro.

#### 5.1 Dual-QEMU 2 cores
- **T-045** Reproduzir hang SIPI **com** tree s281; se PASS, atualizar SESSION_280 nota.
- **T-046** WHPX OVMF `#GP`: **não** gastar sprint no firmware; TCG/HW.
- **T-047** Memória 4G teto (host) — documentar no script.

#### 5.2 CRDT merge
- **T-048** Merge no_std dos tipos já em mesh (`CRDT\0`) — conflito visível, não silent overwrite.
- **T-049** Teste host merge LWW/multi-value (padrão neural-sgdb).

#### 5.3 SemanticRouter
- **T-050** **Não** implementar transformer no path mesh. Manter lexical/intent_bus até slot Cortex dedicado (0078 defer).

**Aceite 5:** mesh 2c TCG PASS **ou** hang com causa (SIPI) fechada; CRDT teste host.

---

### Onda 6 — Ring3 produção (custo L · R0 GDT/AS · bin `user_mode`/`isolation_ring` · R3 `hermes` seam)

**Por quê:** 0077; B/C gated; TCG P6 ≠ produto.

#### 6.1 Não-TCG
- **T-051** WHPX: separar `#GP` OVMF (s280) de `#GP` kernel Ring3.
- **T-052** Metal: iretq+CPL3 **um** notebook; fault-containment.

#### 6.2 Liberação B/C
- **T-053** Checklist ADR-0077 §6 completo em HW.
- **T-054** `register_native_ring` + HITL Escalate para nativo.
- **T-055** `isolation_ring_available()==true` só então.
- **T-056** Soft-float: JIT sem SSE default (`#UD` SESSION_278).

#### 6.3 0076 SYS_PIN_DMA
- **T-057** Pin/unpin frames **após** T-055; CapGate deny DMA a CPL=3.

**Aceite 6:** wasmi permanece default IA não-confiável até T-055.

---

### Onda 7 — Cortex (custo M–L · R2 `cortex` · tools host)

#### 7.1 W2A8 (0084 F4)
- **T-058** Kernel só se `allow_avx2` / WHPX/HW — nunca TCG fingindo GPU.
- **T-059** Golden GTX 1050 (AWAITING até placa).
- **T-060** Paridade ref quantizada (lição s249b).

#### 7.2 0078 **somente Fase 1**
- **T-061** `f32_to_ternary_packed_adaptive` + teste host.
- **T-062** `tools/convert_gguf_to_bitnet.py` (ou estender existente).
- **T-063** Um GGUF pequeno (1B ou menor) carrega QEMU e gera ≥1 token.
- **T-064** PPL vs threshold fixo (critério 0078 Fase 1).

#### 7.3 Fase 2–4
- **T-065** Explicitamente **fora** desta ADR até metal 2B/Falcon3 boot estável + RAM.

**Aceite 7:** T-061–T-064 **ou** W2A8 AWAITING_HW com canário.

---

### Onda 8 — Golden silício GPU (custo XL · R1 `k_hal` · ▶️)

- **T-066** NVIDIA ACR/GSP canário (0048) — AWAITING_HW.
- **T-067** AMD SDMA F6 (0087) — AWAITING_HW; BAR roles já medidos.
- **T-068** Intel GuC/walkers (0050) — AWAITING_HW.
- **T-069** NPU XDNA — 💰 sponsor; fallback software permanece.

**Aceite 8:** log `AWAITING_REAL_HW` honesto; nenhum `gpu_ok` falso.

---

### Onda 9 — Desktop (custo M · R3 `jarbas`)

- **T-070** 0058 S5: um widget (tema **ou** TTF) — não os dois de uma vez.
- **T-071** A/V: HDA playback path já existe; UVC AWAITING; desenho só no `render()`.
- **T-072** Não reintroduzir cards no `tick()` (SESSION_261).

**Aceite 9:** 1 entregável S5 visível QEMU **ou** defer.

---

### Onda 10 — AirLLM residual (custo L · R2 `cortex` · ▶️)

- **T-073** Prefetch DMA GGUF — AWAITING path storage Onda 1.
- **T-074** Stream-to-disk grande e2e — evidência hash (sha256 guest).
- **T-075** K-quants avançados: SESSION_253 já Q2/Q3/Q5; só documentar residual real.

**Aceite 10:** um e2e GGUF grande **ou** AWAITING explícito.

---

## 4. Mapa crate × onda (K³CHJ)

| Crate | Ondas | Proibido |
|-------|-------|----------|
| `k_nano` | 1, 2 evidência, 4, 5 transporte, 6 GDT/AS | lógica Jarbas/Hermes |
| `k_hal` | 1 GPU probe, 8 engines | scheduler agents |
| `k_ai` | 0 métrica, 1 plano I/O, 3 provision | MMIO |
| `cortex` | 4 matmul AP, 7 | Ring3 iretq |
| `hermes` | 3 OTA, 5 CRDT FE, 6 seam B/C | SMP wake |
| `jarbas` | 3 menu, 4 HUD cores, 9 | ICR |
| `neural-kernel` | wire boot, evidência serial | cópia de driver |

---

## 5. Gate v2.0.0 vs este plano

**No gate (se maintainer não deferir):** Onda 0, 1 (pelo menos T-011), 2 (um metal), 3 (A2 ou A3).

**Pós-gate / Layer S:** Ondas 4–10, A1, 0078 Fase 2–4.

**AWAITING_HW não bloqueia** o gate com defer na tabela STATE.

---

## 6. Relação com ADRs

| Tema | ADR dona | 0100 |
|------|----------|------|
| Premissa | 0088 | filtro §0 |
| SMP | 0055/0057/0089 | Ondas 2, 4 |
| I/O | 0087/0062 | Onda 1 |
| HW registry | 0082 | 0.2, 1.2 |
| OTA | 0086 | Onda 3 |
| Mesh | 0081 | Onda 5 |
| Ring3 | 0077 | Onda 6 |
| BitNet | 0084/0078 | Onda 7 |
| GPU | 0048–50 | Onda 8 |
| UI | 0058 | Onda 9 |
| AirLLM | 0046 | Onda 10 |

---

## 7. Inventário T-001–T-075 (checklist mestre)

Copiar para `TODO.md`. Status inicial: todos `[ ]` exceto onde o tree já fechou (anotar SESSION).

Onda 0: T-001…T-006  
Onda 1: T-007…T-016  
Onda 2: T-017…T-021  
Onda 3: T-022…T-032  
Onda 4: T-033…T-044  
Onda 5: T-045…T-050  
Onda 6: T-051…T-057  
Onda 7: T-058…T-065  
Onda 8: T-066…T-069  
Onda 9: T-070…T-072  
Onda 10: T-073…T-075  

**Próximo passo operacional:** T-001 (métrica boot) **em paralelo** a T-017 (USB metal) — anéis diferentes, sem conflito de crate.
