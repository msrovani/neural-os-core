# SESSION_439 — real-HW unlock (GPU VRAM / VMD / USB-MSC) + forja WASM a quente

**Data:** 2026-10-03
**Máquina real (ROCEKT):** Intel Core 7 240H (Raptor Lake-H) · 16GB · SSD ~477GB
(NVMe **atrás do VMD** `8086:a77f`) · **NVIDIA RTX 3050 6GB** (`10de:25a5`, Ampere
GA107) + Intel iGPU · **MediaTek MT7925** WiFi (`14c3:7925`) · sem NIC com fio.

> Continuação HW-real da SESSION_438 (que fez clean build + QEMU 8c/8GB + 10 fixes
> de #PF). Esta sessão trabalha no **boot real** e nos **drivers ausentes**.

## Boot real: OK
8 fases, Runtime, UI/HUD vivos, `fault: none` (zero #PF até T+249s), 70 agentes,
heap 82/512M, tick 30Hz lapic. **O kernel com os 10 fixes roda no metal.**

## Fixes desta sessão

1. **GPU VRAM — bug de papel de BAR** (`k_hal/gpu/detect.rs`):
   a detecção só considerava os pares `(bar0,bar1)(bar2,bar3)(bar4,bar5)` como
   bases. A **VRAM NVIDIA está em BAR1** (índice ímpar) → nunca medida →
   `vram_size=0` → `vram: n/a` e o lane **BAR Compute (ADR-0112) off** (a RTX 3050
   não servia os 6GB de VRAM). Fix: varre os **6 dwords** de BAR, **pula o _high_
   de BARs 64-bit** e escolhe o **maior BAR ≥64MB** como VRAM (MMIO=BAR0, salvo
   quando BAR0 é a aperture → BAR5).

2. **Forja de skills WASM a quente LIGADA** (era **órfã** — só testes chamavam):
   - `self_evolve::auto_generate_pending` publica **`TOPIC_SKILL_GEN_REQUEST`**
     (elo antes morto).
   - `HermesAgent` consome → prompt op-IR (`structured_decode::model_skill_prompt`)
     → `TOPIC_LLM_REQUEST` (marcado `PENDING_WASM_SKILL`) → na resposta chama a
     forja **real** `evolve::promote_model_text_to_wasm` (proveniência **model-born**,
     **wasmi-only**).
   - `hw_pnp.rs` e `bei.rs::PromoteSkill` **deixaram de carimbar dummy**
     (`I32Const(0)`) → publicam o pedido de geração (honestidade: dummy ≠ solução).
   - **Acoplamento honesto:** sem modelo carregado, o Cortex responde `NO_MODEL_MSG`
     e a forja recusa (sem skill falsa). A cadeia é
     `storage→modelos→inferência→LLM op-IR→forja→skill/agente a quente`.

3. **VMD/NVMe diagnosticado** (`k_nano/vmd.rs` + HUD): `VMD_STAGE`
   (0=n/a, 2=found, 7=init OK, 8=sem NVMe 01:08, 11=probe OK; `0x8X`=falha no
   passo X) e a linha `storage` do HUD passa a incluir **NVMe** (antes ignorava) +
   `no dev vmd{N}`.

4. **USB warm port reset** (`xhci/bringup.rs`): budget 100→**500ms** para Speed≥4
   (evidência: `port 2 reset FAIL` num stick SuperSpeed `P2 CCS=1 PED=1 speed=4`;
   o UEFI habilitou a porta e o warm reset não completava em 100ms) + **último
   Command Completion Code** (`LAST_USB_CC`) + timeouts (`USB_CMD_TIMEOUTS`) no
   HUD + log do `PORTSC` no timeout. Linha `vram` do HUD mostra
   `off {vendor} {isa} ap{N}M`.

## Método
QEMU-monitor (`info registers` por vCPU) + `llvm-nm` para simbolizar + HUD
instrumentado (como no USB/VMD) para tornar o metal diagnosticável sem serial.

## Gates
`cargo build -p boot` **0 erros**; hermes **282/283** (1 fail **pré-existente**
`permission_gate::test_risk_level_classify` — poluição de VFS por
`app_factory::lane_b_tests`, confirmado via `git stash`, fora do escopo).

## Commit
`cb51890d` (main) — 9 arquivos de código. `20be8109` (s438) + `cb51890d` pushed.

## AWAITING / residual
- **WiFi MT7925** (`14c3:7925`): sem driver **mt76** + firmware no repo → 🔴 AWAITING.
- **NVMe via VMD**: `vmd.rs` não bindou no metal; o `VMD_STAGE` no HUD aponta o passo.
- **dGPU BAR lane**: só serve a VRAM se a aperture couber os pesos (1B ~369MB;
  BAR1 de laptop **sem ReBAR** ~256MB → off honesto; com **ReBAR** → 6GB → cabe).
- **GoAL 1h sem crash/#PF**: continua bloqueado pelo wild-write (SESSION_438).

## Nota de colisão (frente concorrente)
Os docs compartilhados (`CHANGELOG.md`, `docs/memory/STATE.md`,
`docs/memory/SESSION_INDEX.md`, `TODO.md`) foram editados **ao vivo** por outra
frente (watchdog `[SILENCE]` **s436**, `SESSION_436.md`) na mesma árvore. Para não
subir WIP alheio, os registros desta sessão foram deixados **no working tree** (não
commitados) e este `SESSION_439.md` é o registro owned; o `CHANGELOG` já contém a
seção `[1.9.99-s439]`. Nunca sobrescrever o número/edits da outra frente
(lição SESSION_264/277/346).

**Arquivos de código:** `k_hal/gpu/detect.rs`, `k_nano/vmd.rs`,
`k_nano/xhci/{bringup,mod}.rs`, `jarbas/display/gauges.rs`,
`hermes/{self_evolve,agents,hw_pnp,bei}.rs`.
