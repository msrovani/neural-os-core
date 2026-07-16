# SESSION 112 — ADR-0042 N2 CLOSED

**Data:** 2026-07-16  
**Versão:** v1.7.4 (docs + runtime N2)  
**Pista:** ADR-0042 N2 → **✅ CLOSED** (critérios funcionais); próximo N3→N5

## Objetivo

Fechar N2 (k-ai HW-AI + SelfHeal): evidência serial QEMU, HEALTH_ISSUE/honest noop, inventário VID coerente, Trust `(token,agent,skill)`, residual hermes, docs. Wire crate `k_ai` no bin = **N2.5** (allocator).

## Entregas

| Item | Status | Nota |
|------|--------|------|
| Heal/noop + `[N2-SELFHEAL]` | ✅ | boot `BootSelfHealAgent` + `run_vid_gated_scan` |
| Trust allow serial | ✅ | Trust **antes** SelfHeal; `[TRUST] allow (token,agent,skill)=…` |
| HEALTH_ISSUE | ✅ | path heal (log 131655) **ou** honest noop `fw_gated=0` (131837) |
| VID gate | ✅ | `device_needs_fw` — Intel 8086 net exclui Ethernet `subclass==0x00` (e1000 ≠ iwlwifi); NVIDIA 10DE:03 intacto |
| hermes residual | ✅ | gate N2 em `hermes/src/agents.rs` (usa `k_ai`); `cargo check -p hermes` 0e |
| Espelho / N2.5 | ⏳ documentado | bin monólito espelha; clash `#[global_allocator]` k_nano |
| `cargo nk` | ✅ 0 erros | soft-float alias |

## Evidência QEMU (WHPX short)

**Log canônico:** `logs/boot_n2_20260716_131837.txt`

```text
[TRUST] allow (token,agent,skill)=(1,self_heal,recover)
[TRUST] allow (token,agent,skill)=(1,self_heal,inventory_vid)
[N2-SELFHEAL] inventory pci=5 fw_gated=0 trust=OK
[N2-SELFHEAL] HEALTH_ISSUE: honest noop (fw_gated=0 - no known VID needs FW)
[N2-SELFHEAL] done scanned=5 noop=5 heal=0 HEALTH_ISSUE=0
[N2-SELFHEAL] gate complete heal=0 noop=5 HEALTH_ISSUE=0 (k_ai policy mirror)
```

**Path HEALTH_ISSUE (pré fine-gate):** `logs/boot_n2_20260716_131655.txt` — heal `8086:100E` I3+I4 (e1000 falso positivo → corrigido no fine-gate).

**Ops:** `cargo nk` → `bootloader_linker -u -o target build …/neural-kernel` → QEMU 6G + loaders + pflash OVMF; kill após gate N2 (~25s). Sem `cargo build -p boot` (hang conhecido).

## Decisão N2.5

Não bloquear N3 por link crate. Critérios funcionais N2 satisfeitos via espelho no bin + `k_ai` API + hermes wired. Revisitar allocator quando migrar `global_allocator` para um único dono.

## Próximo

- ADR-0042 **N3** (cortex generate/TTS)
- Sprint Sound = voz only (não bloqueia)
- Sem push; sem tag `v2.0.0`
