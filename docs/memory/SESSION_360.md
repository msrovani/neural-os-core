# SESSION_360 — Mesh WHPX lab 6-node + fail/warn honesty + UI orb

## Goal
Lab mesh 6 QEMU WHPX (3G/3c + 2G/2c + 4×1G/1c) com L2 hub; orb por role/MESH_HEALTH; Hub Health = SystemInfo aberto; HDA em QEMU; fechar fails/warns falsos no dmesg (TLSPINS, Trust, CapGate) e tick lento (boot_log/mouse).

## Lab / UI
| Item | Evidência / Fix |
|---|---|
| Mesh STATIC 10.0.3.x + hub L2 | `tools/qemu_l2_hub.py` + relaunch script; FRAG skip em 1G |
| Orb cores / roles | `MESH_HEALTH` periódico → SoulMirror/OrbSignals; palette por role |
| Dual SystemInfo + OFF sob orb | remove demo cards Sistema; HubHealth default open; power dialog always-on-top |
| Node A Master peers≤4 (sem B) | B hang PS/2 mouse re-init — soft F4 |
| Log A spam BEI/Hermes | residual; mesh tick throttle com UI live |

## Fail/warn honesty (ADR-0092)
| Sintoma | Causa | Fix |
|---|---|---|
| `[GGUF] fail FAT write OK` (TLSPINS) | slog sev errado no sucesso | `gguf::write_fat_file` → sev `ok` |
| Trust `diagnostic` deny → SystemAgent **Crashed** | Contain sem `trust_allow`; deny=fatal | `BootTrust`+SystemAgent `trust_allow(diagnostic/echo)`; deny→skip/`warn` |
| CapGate DENY `warn` spam (PoC) | deny-by-default esperado = alarme falso | `k_hal::cap_gate` + `WRITE_FB` → sev `ok` |
| Tick lento boot_log / mouse | Continuous FAT walk; PS/2 reset+E9 100k spins | `PollEvery` + analyze 1×; soft F4 + timeout 8k |

## Verify
- `cargo check --release -p neural-kernel` — 0 erros (warnings Known)
- Mesh relaunch pós-fix — AWAITING operador

## Lições
1. **Sucesso com sev `fail` = mentira de dmesg** (mesma classe timeout→Ok).
2. **Contain + Legacy(1) ≠ auto-allow** — skills de boot precisam `trust_allow`; Crashed em SystemAgent por skill opcional é pior que skip.
3. **CapGate PoC DENY ≠ regressão** — sev `ok` (visível) não `warn` (alarme).
4. **Boot já init PS/2** — MouseAgent reset/E9 no tick engasga 1c; só F4 stream.
