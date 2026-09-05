# STATE — neural-os-core v1.9.99-s315 — UI liveness + anti-black-screen metal

#   PISTA ATIVA: P0 Alienware — desktop vivo + BOOT.LOG no stick (ADR-0103 S1)
#   SESSION_315: soft-halt 18Hz; HID defer sem MSC; PIC IRQ12; urgency voz
#   SESSION_315b: tela preta pós-Limine → budget 3s MSC + TSC timeouts EP0/reset
#                 + boot_progress_line no FB (boot_ckpt = ramlog só)
#   SESSION_314: k_hal::usb hub→MSC; S0 multi_user/hnsw; freeze Ring3/S2+
#   SESSION_313: xHCI RTSOFF+0x20, TRB IOC/CC, takeover metal
#   FREEZE até aceite: Ring3 Onda 6, 0103 S2–S6, Falcon3 sprint, 0089
#   HW: target/usb_hw.img 6271MB READY (PACK_LLM=falcon3, kernel sha 8dbde4ea822aca58)
#       flash: docs/memory/HW_FLASH_s314.md | report: logs/hw_alienware_s314/REPORT.md
#       Aceite: FB linhas BOOT: early USB… + orb/relógio/mouse; E:\BOOT.LOG real
#   Não declarar v2.0.0
