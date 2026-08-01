# crates/k_hal/src/net/ — Net/WiFi MMIO Backends (R1)

**Responsibility**: WiFi BE MMIO drivers (ADR-0041 H3) — Qualcomm ath10k bring-up
(CE/BMI → HTC/WMI → scan/assoc), Intel iwlwifi ucode load + scan, and a generic
register-map engine (`generic_wifi`) over per-chipset `HardwareRegisterMap` tables
(Intel AX, Realtek, Atheros, Broadcom, Ethernet fallback). The smoltcp stack is NOT
here — it lives in hermes/bin; k_hal only owns the BARs and raw frames.

**Key symbols**: `mod.rs::{register_net_bound, set_link_up}`; `ath10k_ce_bmi::CeBmi`
(BMI exchange/LZ download), `ath10k_htc_wmi::a4_htc_wmi_bringup`,
`ath10k_wmi_scan::a5_start_scan`, `ath10k_wmi_assoc::a6_try_assoc`,
`wifi_ath10k::{Ath10kDevice, a3_bringup, try_assoc, last_verdict}`;
`ath10k_fw::resolve_ath10k_fw`, `iwl_fw::resolve_iwl_fw`; `wifi_iwlwifi::IwlWifi`
(load_ucode/send_cmd/scan); `generic_wifi::{WifiChipset, AgnosticWifiEngine,
runtime_probe_and_bind, detect_wifi}`; `wifi_softmac::{enable_if_associated,
push_rx_eth/pop_tx_eth}`; `wifi_crypto::inject_wpa2_key`; `wifi_msix::setup_msix`.

**Integration**: hermes consumes directly — `wifi_agent.rs` (ath10k verdicts/assoc,
`wifi_msix::set_rx_inject` → netstack, `wifi_softmac::enable_if_associated`),
`wifi_protocol.rs` (WPA2 key injection), and `hermes::lib.rs` re-exports the wifi
modules. Grants `unlock_dag` tokens (`WifiFwAlive`, `WifiAssociated`) consumed by
hermes and `hw_gate` (ATH10K gates).
