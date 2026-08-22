# SESSION_283 — Onda 0–1 ADR-0100 (`BOOT_AI` + bandwidth TSC)

**Objetivo:** T-001–T-016 (exceto aceite metal T-017).

**Código:** `boot_report::BootAiCounts` + `BOOT_AI` no EventBus/serial; Escalate ≠ act; `parse_boot_ai_line` testes 3/3. `HardwareInfo` congelado + `hw_cpu_*`. `storage_bw` TSC 16 setores, skip TCG. Plano k_ai omite ATA em TCG. SGDB `/hw/storage|gpu|net`, wifi só se device.

**Testes:** k-nano boot_report 3, storage_bw 2, hw_cpu 1; k_ai `boot_ai_line_roundtrip` PASS.

**Próximo:** T-017 USB + K23 no metal (i5 / 240H).
