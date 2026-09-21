# SESSION_380 — hermes residuals (pós-s379)

**Sprint:** v1.9.99-s380 TEST  
**Data:** 2026-09-21  
**Foco:** Fechar residuais Med/Low + IDEA #604 (aios_net/fs wire + orphans)

## Premissas

- Cap + bridge/VFS → I/O real; sem bridge → Deny+trap (não Ok stub)
- Orphan ≠ Continuous no fleet (AGENTS honesty)
- WPA2 demo ≠ ReadyForTraffic sem EAPOL
- wasmi 2.x / smoltcp 0.14 / spin 0.12 = IDEA (não bump cego)

## Aplicado

| ID | Fix |
|----|-----|
| #604 | `aios_net::http_get` / `aios_fs::{read,write}` → net_bridge + VFS; RiskLevel Auto só se ready |
| TLS | `log_tls_status_boot` BRIDGE_OK/PENDING (ready⇔bridge) |
| WPA2 | Msg4 → `HandshakingWpa` (não ReadyForTraffic) |
| GPU | GpuDriverAgent reporta `k_hal::compute_state` |
| IPC | `capgate_boot_smoke` token0 aceite = FAIL |
| InferenceFs | ephemeral RAM; `no_model` sem texto fake |
| Decode | Card → `Err(card_ir_pending)` |
| AtaAgent | reuse `ATA_DRIVER` / skip live USB — sem re-probe |
| llm_gate | doc Legacy(1) only |
| Safety | I1/I2/I4 default off; I3 on; orphan header |
| Orphans | delete DEAD (shell, graph_engine, …); wire `ipc_bus`+`wpa2_hs`; AGENTS A-019/020/023 ORPHAN |
| Deps | resign_imported→pending + miniz/ed25519 bumps (follow-up s379) |
| slog | self_update / FS register ADR-0092 |

## Verificação

- `cargo test -p hermes --lib` → **205/205**

## Residual aberto (IDEA)

- #599 wasmi 2.x
- #600 DynSkill CapGate token
- #597/#602 smoltcp/spin workspace
