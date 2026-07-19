# SESSION_159 — WiFi S0 honesty + prep S1 (sem ALIVE)

**Data:** 2026-07-18  
**Log:** `logs/boot_wifi_s0_20260718_200725.txt`

---

## Goal

Opção 2: **S0** (sem “Conectado!” falso) + **prep S1** (DID→blob/FAT short names) sem claim `[IWL] ucode alive`.

## Evidência serial (QEMU WHPX `-cpu qemu64`)

```
[IWL] fw_resolve=SKIP reason=no_wifi_pci
[WIFI-HW] step=boot_probe status=UNSUPPORTED detail=no_wifi_pci
[WIFI-HW] VERDICT=AWAITING_REAL_HW reason=no_wifi_radio_onda7
```

- Sem `Conectado a`
- Sem `ucode alive`
- Probe no `WifiAgent::new()` (AgentFleet) — evidência antes de #PF e1000 pós-NetAgent tick (ruído WHPX conhecido; não bloqueia S0)

## Código

| Peça | Onde |
|------|------|
| S0 `do_connect` | `hermes/wifi_agent.rs` — Failed + VERDICT; sem NETWORK_CONFIGURED/DHCP |
| Scan demo copy | prefixo “DEMO AP list (nao e RF…)” |
| Boot probe | `WifiAgent::new` → detect + VERDICT |
| Short FAT | `tools/mkfat32.py` → `FW_CC77.BIN` / `FW_SOGF` / `FW_SOHR` / `FW_TYGF` / `FW_QUHR` |
| DID→fw | `k_hal/src/net/iwl_fw.rs` — resolve + `fw_fat` FOUND/MISSING/NO_DISK; header check only |
| Wire Intel bind | `generic_wifi.rs` → `probe_iwl_fw_for_did` |

## Residuais

- S1 ALIVE real (HW Intel)
- `.pnvm` ausente no repo
- Scan RF / assoc / WPA2 (S2–S3)
- Rebuild `disk_qemu.raw` com short names antes de testar FOUND em HW

## Aceite

✅ S0 + prep S1 fechados neste log.
