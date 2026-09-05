# HW flash — usb_hw.img (gerado 2026-09-03, HEAD e3b0075)

## Artefato
- `target/usb_hw.img` — **6271 MB** (alias `disk_hw_unified.raw`)
- Layout: MBR dados `0x0C` + ESP `0xEF`; GPT ESP + NEURAL-OS FAT32
- Pack: `PACK_LLM=falcon3` → FALCON3.V6 (~770MB) + Piper + BGE + HW Expert + firmware NVIDIA/i915/ath10k + LEGO recipes + BOOT.LOG + NSGDB
- Kernel: `fat-boot-log` ON (via crate `boot`)

## Rufus (Windows)
1. Pendrive **≥8 GB**
2. Rufus → selecionar `target\usb_hw.img` → **Imagem DD** (não ISO)
3. Gravar (apaga o stick)
4. Notebook: **Secure Boot OFF**, boot **UEFI** no USB
5. Windows deve montar volume **NEURAL-OS** com `BOOT.LOG`

## Aceite metal (fechar AWAITING / caminhar v2.0)

| Item | Log / critério no metal |
|------|-------------------------|
| SMP K23 | `online==madt-1`, `smp_online=N`, roles ∝ N; sem freeze ICR |
| Ring3 H2 | `ring3_mark_hw_gate_passed` / `register_native_ring` só se metal PASS |
| xHCI HID | mouse/teclado USB real (QEMU EnableSlot falha) |
| HDA | playback/capture real (QEMU ICW stub) |
| GPU/fw | `fw_gpu` / canary CE se NVIDIA/Intel presente; senão honesty ABSENT |
| WiFi RF | ath10k/iwlwifi scan se HW + firmware no FAT |
| UAC | `[UAC-HW]` deixa de ser AWAITING_REAL_HW |
| BOOT.LOG | flush no stick (MSC/ATA); Notepad lê sem BOM lixo |
| LLM | FALCON3.V6 load → CURRENT_MODEL / TTS greeting |
| NSGDB | ingest/persist não-RAM |
| VFS+FS | pós-fix: `AtaAgent` não re-probe; NeuralFS skip ATA no live USB |

### Fix 2026-09-03/04 — hang `BOOT: self-tests...`
Causa: `boot_ckpt` → `try_flush_ramlog` → flush falha → `heal` re-probe xHCI/MSC = hang.
Fix: live USB sem MSC não flush/heal; `storage_available` respeita SKIP_*; FB `BOOT: K33 …` por passo; trainer skip no live USB.

### Run Alienware 2026-09-04 (Core 7 240H / 16c / RTX 3050)
- Operador: chegou **desktop UI** → **freeze**.
- Stick `E:\BOOT.LOG` = **placeholder**; `E:\NSGDB.BIN` = **zeros** (sem MSC → sem persist).
- Relatório: `logs/hw_alienware_s310/REPORT.md`.
- **OK (imagem pré-pull s310 / ~`8573fa0`)**: boot metal até **desktop/UI** no Alienware 240H.
- Reteste com HEAD atual + fix MSC multi-porta (webcam≠stick) para gravar BOOT.LOG/NSGDB.

### Fix 2026-09-04 — BOOT.LOG/NSGDB no pendrive (urgente)
Causa: `bringup_boot_msc` pegava só a **1ª porta CCS** (webcam/BT) → stick sem MSC.
Fix: varrer todas as portas CCS (SS>HS); SCSI fail → skip porta; SysInfo promove FileFlash/NSGDB após FAT_READY.

**Não fecha só com stick:** WiFi RF sem rádio, GPU sem silicon, TLS PKI, EEVDF, gate v2.0.0 sem OK maintainer + residual defer.

## Não usar
- Esta imagem como disco único no QEMU (continuar `uefi.img` + `disk_qemu.raw`)
- Rufus modo ISO / “escrever em partições”
